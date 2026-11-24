//! One import, from a file to a row of the catalogue.
//!
//! Reading and writing files runs on the background executor; converting
//! a file and making the thumbnail of a cover run on the pool of `compute`,
//! which the executor awaits (mbook-ui docs, "Threads").

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read as _, Write as _};
use std::path::Path;
use std::str::FromStr as _;

use mbook::{Book, BookReader};
use mbook_converter::{ConvertOptions, Converter, Format};
use tempfile::NamedTempFile;

use crate::entities::catalogue::{BookRow, Catalogue, CatalogueError};
use crate::entities::picture;
use crate::shared::compute;
use crate::shared::model::BookKey;
use crate::shared::roots::Roots;

/// The bytes `mbook::is_mbook` needs to tell an mbook file from another.
const PREFIX_LEN: usize = 53;

/// The width of the thumbnail of a cover, in pixels.
const THUMBNAIL_WIDTH: u32 = 400;

/// The largest cover a thumbnail is made from, in pixels: the limit the
/// converter puts on a picture.
const MAX_COVER_PIXELS: u64 = 100_000_000;

/// What an import is doing. The steps follow each other in this order;
/// `Copying` and `Converting` exclude each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// Finding out whether the file is an mbook file.
    Probing,
    /// Copying an mbook file.
    Copying,
    /// Converting a file in another format.
    Converting,
    /// Moving the book into the library and recording it.
    Indexing,
}

/// The result of an import that did not fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Imported {
    /// The book in the library.
    pub(crate) key: BookKey,
    /// `false` when the library had this book already: nothing was added and
    /// `key` names the existing book.
    pub(crate) added: bool,
    /// What the conversion could not keep, one line each.
    pub(crate) warnings: Vec<String>,
}

/// Why a file was not added.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum ImportError {
    /// The file, or the library, could not be read or written.
    #[error("{}", rust_i18n::t!("import.err.io"))]
    Io(#[from] io::Error),
    /// The file is in no format that can be converted, or the conversion
    /// failed.
    #[error("{}", rust_i18n::t!("import.err.convert"))]
    Convert(#[from] mbook_converter::Error),
    /// The file is an mbook file that cannot be read.
    #[error("{}", rust_i18n::t!("import.err.invalid"))]
    Book(#[from] mbook::Error),
    /// The initial book could not be created or written.
    #[error("{}", rust_i18n::t!("import.err.create"))]
    Create(#[source] mbook::Error),
    /// The book could not be recorded.
    #[error("{}", rust_i18n::t!("import.err.catalogue"))]
    Catalogue(#[from] CatalogueError),
}

/// Adds the file at `source` to the library.
///
/// An mbook file is copied; a file in any other format is converted. The
/// original is neither moved nor changed. A book whose `id` the library has
/// already is not added again.
///
/// `added_ms` is the time recorded for the book, in milliseconds of Unix
/// time. `on_step` is told each step as it begins.
///
/// # Errors
///
/// When the file cannot be read, is in no format that can be converted, or
/// the library cannot be written. A failed import leaves no file in the
/// library and no row in the catalogue.
pub(crate) async fn import(
    roots: &Roots,
    catalogue: &Catalogue,
    source: &Path,
    added_ms: u64,
    on_step: &(dyn Fn(Step) + Sync),
) -> Result<Imported, ImportError> {
    on_step(Step::Probing);
    let mut file = File::open(source)?;
    let is_mbook = starts_as_mbook(&mut file)?;
    // The probe moved the cursor; the copy and the conversion start anew.
    let file = File::open(source)?;

    fs::create_dir_all(roots.incoming())?;
    let mut incoming = NamedTempFile::new_in(roots.incoming())?;
    let (format, warnings) = if is_mbook {
        on_step(Step::Copying);
        copy(file, incoming.as_file_mut())?;
        (None, Vec::new())
    } else {
        on_step(Step::Converting);
        let path = source.to_owned();
        let (report, converted) = compute::spawn(move || {
            let report = convert(&path, file, incoming.as_file_mut());
            (report, incoming)
        })
        .await;
        incoming = converted;
        let report = report?;
        let warnings = report.warnings.iter().map(ToString::to_string).collect();
        (Some(report.format.to_string()), warnings)
    };

    on_step(Step::Indexing);
    let (key, added, cover) = shelve(roots, catalogue, incoming, added_ms, format)?;
    if let Some(avif) = cover {
        write_thumbnail(roots, key, avif).await;
    }
    Ok(Imported {
        key,
        added,
        warnings,
    })
}

/// Creates a book with an empty first chapter and adds it to the library.
///
/// # Errors
///
/// When the title is empty or the book or its catalogue row cannot be stored.
pub(crate) fn create_book(
    roots: &Roots,
    catalogue: &Catalogue,
    title: &str,
    added_ms: u64,
) -> Result<BookKey, ImportError> {
    let mut book = Book::new(title, "und").map_err(ImportError::Create)?;
    book.add_chapter("ch01.md", "")
        .map_err(ImportError::Create)?;
    fs::create_dir_all(roots.incoming())?;
    let mut incoming = NamedTempFile::new_in(roots.incoming())?;
    book.write(incoming.as_file_mut())
        .map_err(ImportError::Create)?;
    incoming.as_file().sync_all()?;
    // A new book has no cover, so there is no thumbnail to make.
    let (key, _, _) = shelve(roots, catalogue, incoming, added_ms, None)?;
    Ok(key)
}

/// Installs a staged mbook file and returns its key, whether it was added,
/// and the cover of a book that was added, for its thumbnail.
fn shelve(
    roots: &Roots,
    catalogue: &Catalogue,
    incoming: NamedTempFile,
    added_ms: u64,
    format: Option<String>,
) -> Result<(BookKey, bool, Option<Vec<u8>>), ImportError> {
    let file_len = incoming.as_file().metadata()?.len();
    let mut reader = BookReader::open(incoming.path())?;
    if let Some(key) = catalogue.find_by_id(reader.meta().id())? {
        return Ok((key, false, None));
    }

    let key = BookKey::generate();
    let row = BookRow::from_reader(key, &reader, file_len, added_ms, format);
    let cover = reader.read_cover()?;
    // The file is renamed next: nothing may hold it open.
    drop(reader);

    let book = roots.book(key);
    incoming.persist(&book).map_err(|error| error.error)?;
    if let Err(error) = catalogue.put_book(&row) {
        // A book without a row would stay on disk unseen.
        let _ = fs::remove_file(&book);
        return Err(error.into());
    }
    Ok((key, true, cover))
}

/// Makes the thumbnail of a cover again, after a book was saved.
///
/// A book without a cover loses the thumbnail it had.
pub(crate) async fn refresh_thumbnail(roots: &Roots, key: BookKey, cover: Option<Vec<u8>>) {
    match cover {
        Some(avif) => write_thumbnail(roots, key, avif).await,
        None => {
            let _ = fs::remove_file(roots.cover(key));
        }
    }
}

/// Whether the file begins like an mbook file.
fn starts_as_mbook(file: &mut File) -> io::Result<bool> {
    let mut prefix = Vec::with_capacity(PREFIX_LEN);
    // `PREFIX_LEN` is a small constant: it fits `u64` on every platform.
    file.take(PREFIX_LEN as u64).read_to_end(&mut prefix)?;
    Ok(mbook::is_mbook(&prefix))
}

/// Copies an mbook file into the file of the import.
fn copy(source: File, sink: &mut File) -> io::Result<()> {
    let mut source = BufReader::new(source);
    let mut sink = BufWriter::new(sink);
    io::copy(&mut source, &mut sink)?;
    sink.flush()
}

/// Converts a file in another format into the file of the import.
fn convert(
    path: &Path,
    source: File,
    sink: &mut File,
) -> Result<mbook_converter::Report, mbook_converter::Error> {
    let mut options = ConvertOptions::default();
    options.format = named_format(path);
    let mut sink = BufWriter::new(sink);
    let report = Converter::with_options(options).convert(BufReader::new(source), &mut sink)?;
    sink.flush()?;
    Ok(report)
}

/// The format a file name gives to a source whose content does not show it.
///
/// Markdown and plain text are never detected from their content, so their
/// extension decides. Every other format is detected: an extension is wrong
/// often enough.
fn named_format(path: &Path) -> Option<Format> {
    let extension = path.extension()?.to_str()?;
    match Format::from_str(extension) {
        Ok(format @ (Format::Markdown | Format::Text)) => Some(format),
        _ => None,
    }
}

/// Writes the thumbnail of a cover. A cover that cannot be decoded, or a
/// cache that cannot be written, costs the thumbnail and nothing else: the
/// shelf draws a cover from the title instead. Decoding and scaling run on
/// the pool of `compute`.
async fn write_thumbnail(roots: &Roots, key: BookKey, avif: Vec<u8>) {
    let made =
        compute::spawn(move || picture::cover_thumbnail(&avif, THUMBNAIL_WIDTH, MAX_COVER_PIXELS))
            .await;
    let Ok(jpeg) = made else {
        return;
    };
    if fs::create_dir_all(roots.covers()).is_ok() {
        let _ = fs::write(roots.cover(key), jpeg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use futures::executor::block_on;

    use mbook::Book;
    use tempfile::TempDir;

    /// A library in a temporary folder.
    struct Library {
        _dir: TempDir,
        roots: Roots,
        catalogue: Catalogue,
        sources: PathBuf,
    }

    fn library() -> Library {
        let dir = tempfile::tempdir().unwrap();
        let roots = Roots::new(dir.path().join("data"), dir.path().join("cache"));
        roots.prepare().unwrap();
        let catalogue = Catalogue::open(&roots.catalogue()).unwrap();
        let sources = dir.path().join("sources");
        fs::create_dir_all(&sources).unwrap();
        Library {
            _dir: dir,
            roots,
            catalogue,
            sources,
        }
    }

    /// An mbook file with one chapter and, when asked, a cover.
    fn mbook_file(library: &Library, name: &str, title: &str, cover: bool) -> PathBuf {
        let mut book = Book::new(title, "en").unwrap();
        book.meta_mut().add_author("A. Writer").unwrap();
        book.add_chapter("ch01.md", "# One\n\nText.\n").unwrap();
        if cover {
            let avif = fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../mbook/tests/fixtures/plain.avif"
            ))
            .unwrap();
            book.set_cover("img/cover.avif", avif).unwrap();
        }
        let path = library.sources.join(name);
        book.save(&path).unwrap();
        path
    }

    fn import_file(library: &Library, source: &Path) -> Result<Imported, ImportError> {
        block_on(import(
            &library.roots,
            &library.catalogue,
            source,
            1_000,
            &|_| {},
        ))
    }

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name != ".incoming")
            .collect();
        names.sort();
        names
    }

    #[test]
    fn an_invalid_title_leaves_no_book_in_the_library() {
        let library = library();
        let result = create_book(&library.roots, &library.catalogue, "", 1_000);
        assert!(matches!(result, Err(ImportError::Create(_))));
        assert_eq!(library.catalogue.books().unwrap(), Vec::<BookRow>::new());
        assert_eq!(files_in(&library.roots.books()), Vec::<String>::new());
    }

    #[test]
    fn a_new_book_can_be_retried_after_a_storage_failure() {
        let library = library();
        let incoming = library.roots.incoming();
        fs::remove_dir(&incoming).unwrap();
        fs::write(&incoming, "").unwrap();

        let result = create_book(&library.roots, &library.catalogue, "A draft", 1_000);
        assert!(matches!(result, Err(ImportError::Io(_))));
        assert_eq!(library.catalogue.books().unwrap(), Vec::<BookRow>::new());
        assert_eq!(files_in(&library.roots.books()), Vec::<String>::new());

        fs::remove_file(incoming).unwrap();
        let key = create_book(&library.roots, &library.catalogue, "A draft", 1_000).unwrap();
        assert!(library.roots.book(key).is_file());
    }

    #[test]
    fn an_mbook_file_is_copied_into_the_library() {
        let library = library();
        let source = mbook_file(&library, "a.mbook", "The Example", false);

        let imported = import_file(&library, &source).unwrap();

        assert!(imported.added);
        assert_eq!(imported.warnings, Vec::<String>::new());
        assert_eq!(
            fs::read(library.roots.book(imported.key)).unwrap(),
            fs::read(&source).unwrap()
        );
    }

    #[test]
    fn the_original_file_is_left_where_it_is() {
        let library = library();
        let source = mbook_file(&library, "a.mbook", "The Example", false);
        let before = fs::read(&source).unwrap();

        import_file(&library, &source).unwrap();

        assert_eq!(fs::read(&source).unwrap(), before);
    }

    #[test]
    fn an_imported_book_has_a_row_with_its_metadata() {
        let library = library();
        let source = mbook_file(&library, "a.mbook", "The Example", false);

        let imported = import_file(&library, &source).unwrap();

        let row = library.catalogue.book(imported.key).unwrap().unwrap();
        assert_eq!(row.title, "The Example");
        assert_eq!(row.authors, ["A. Writer"]);
        assert_eq!(row.language, "en");
        assert_eq!(row.added_ms, 1_000);
        assert_eq!(row.source_format, None);
        assert_eq!(row.file_len, fs::metadata(&source).unwrap().len());
    }

    #[test]
    fn the_cover_of_an_imported_book_gets_a_thumbnail() {
        let library = library();
        let source = mbook_file(&library, "a.mbook", "The Example", true);

        let imported = import_file(&library, &source).unwrap();

        let jpeg = fs::read(library.roots.cover(imported.key)).unwrap();
        assert!(jpeg.starts_with(&[0xFF, 0xD8]), "a JPEG file");
        let row = library.catalogue.book(imported.key).unwrap().unwrap();
        assert!(row.cover.is_some());
    }

    #[test]
    fn a_book_without_a_cover_gets_no_thumbnail() {
        let library = library();
        let source = mbook_file(&library, "a.mbook", "The Example", false);

        let imported = import_file(&library, &source).unwrap();

        assert!(!library.roots.cover(imported.key).exists());
    }

    #[test]
    fn a_markdown_file_is_converted_by_its_extension() {
        let library = library();
        let source = library.sources.join("notes.md");
        fs::write(&source, "# Notes\n\nA paragraph.\n").unwrap();

        let imported = import_file(&library, &source).unwrap();

        assert!(imported.added);
        let row = library.catalogue.book(imported.key).unwrap().unwrap();
        assert_eq!(row.source_format.as_deref(), Some("markdown"));
        let reader = BookReader::open(library.roots.book(imported.key)).unwrap();
        assert_eq!(reader.chapters().len(), 1);
    }

    #[test]
    fn a_plain_text_file_is_converted_by_its_extension() {
        let library = library();
        let source = library.sources.join("story.txt");
        fs::write(&source, "Once upon a time.\n").unwrap();

        let imported = import_file(&library, &source).unwrap();

        let row = library.catalogue.book(imported.key).unwrap().unwrap();
        assert_eq!(row.source_format.as_deref(), Some("text"));
    }

    #[test]
    fn what_a_conversion_could_not_keep_is_reported() {
        let library = library();
        let source = library.sources.join("story.txt");
        fs::write(&source, "Once upon a time.\n").unwrap();

        let imported = import_file(&library, &source).unwrap();

        // Plain text has neither a title nor a language of its own.
        assert_ne!(imported.warnings, Vec::<String>::new());
    }

    #[test]
    fn a_html_document_is_detected_from_its_content() {
        let library = library();
        // The extension says nothing; the content does.
        let source = library.sources.join("page.bin");
        fs::write(
            &source,
            "<!DOCTYPE html><html lang=\"en\"><head><title>A page</title></head>\
             <body><h1>A page</h1><p>Text.</p></body></html>",
        )
        .unwrap();

        let imported = import_file(&library, &source).unwrap();

        let row = library.catalogue.book(imported.key).unwrap().unwrap();
        assert_eq!(row.source_format.as_deref(), Some("html"));
        assert_eq!(row.title, "A page");
    }

    #[test]
    fn a_book_the_library_has_is_not_added_again() {
        let library = library();
        let source = mbook_file(&library, "a.mbook", "The Example", false);
        let first = import_file(&library, &source).unwrap();

        let second = import_file(&library, &source).unwrap();

        assert!(!second.added);
        assert_eq!(second.key, first.key);
        assert_eq!(library.catalogue.books().unwrap().len(), 1);
        assert_eq!(files_in(&library.roots.books()).len(), 1);
    }

    #[test]
    fn two_different_books_are_both_added() {
        let library = library();
        let first = mbook_file(&library, "a.mbook", "One", false);
        let second = mbook_file(&library, "b.mbook", "Two", false);

        let first = import_file(&library, &first).unwrap();
        let second = import_file(&library, &second).unwrap();

        assert_ne!(first.key, second.key);
        assert_eq!(library.catalogue.books().unwrap().len(), 2);
    }

    #[test]
    fn the_steps_of_a_copy_are_told_in_order() {
        let library = library();
        let source = mbook_file(&library, "a.mbook", "The Example", false);
        let steps = Mutex::new(Vec::new());

        block_on(import(
            &library.roots,
            &library.catalogue,
            &source,
            0,
            &|step| steps.lock().unwrap().push(step),
        ))
        .unwrap();

        assert_eq!(
            steps.into_inner().unwrap(),
            [Step::Probing, Step::Copying, Step::Indexing]
        );
    }

    #[test]
    fn the_steps_of_a_conversion_are_told_in_order() {
        let library = library();
        let source = library.sources.join("notes.md");
        fs::write(&source, "# Notes\n").unwrap();
        let steps = Mutex::new(Vec::new());

        block_on(import(
            &library.roots,
            &library.catalogue,
            &source,
            0,
            &|step| steps.lock().unwrap().push(step),
        ))
        .unwrap();

        assert_eq!(
            steps.into_inner().unwrap(),
            [Step::Probing, Step::Converting, Step::Indexing]
        );
    }

    #[test]
    fn a_missing_file_fails_with_the_io_error_as_its_source() {
        let library = library();

        let error = import_file(&library, &library.sources.join("nowhere.epub")).unwrap_err();

        assert!(matches!(error, ImportError::Io(_)));
        assert_eq!(error.to_string(), "a file could not be read or written");
        assert!(error.source().is_some());
    }

    #[test]
    fn a_file_in_no_known_format_fails_and_leaves_nothing() {
        let library = library();
        let source = library.sources.join("noise.bin");
        fs::write(&source, [0x13, 0x37, 0x00, 0x42]).unwrap();

        let error = import_file(&library, &source).unwrap_err();

        assert!(matches!(
            error,
            ImportError::Convert(mbook_converter::Error::UnknownFormat)
        ));
        assert_eq!(error.to_string(), "the file could not be converted");
        assert_eq!(
            error.source().unwrap().to_string(),
            "the format of the source is not recognised"
        );
        assert_eq!(files_in(&library.roots.books()), Vec::<String>::new());
        assert_eq!(files_in(&library.roots.incoming()), Vec::<String>::new());
        assert_eq!(library.catalogue.books().unwrap(), Vec::new());
    }

    #[test]
    fn an_empty_file_fails_and_leaves_nothing() {
        let library = library();
        let source = library.sources.join("empty.epub");
        fs::write(&source, b"").unwrap();

        assert!(import_file(&library, &source).is_err());

        assert_eq!(files_in(&library.roots.books()), Vec::<String>::new());
        assert_eq!(library.catalogue.books().unwrap(), Vec::new());
    }

    #[test]
    fn a_damaged_mbook_file_fails_and_leaves_nothing() {
        let library = library();
        let whole = mbook_file(&library, "a.mbook", "The Example", false);
        let bytes = fs::read(&whole).unwrap();
        let source = library.sources.join("half.mbook");
        // The start says mbook; the directory at the end is gone.
        fs::write(&source, &bytes[..bytes.len() / 2]).unwrap();

        let error = import_file(&library, &source).unwrap_err();

        assert!(matches!(error, ImportError::Book(_)));
        assert_eq!(error.to_string(), "the file is not a valid book");
        assert!(error.source().is_some());
        assert_eq!(files_in(&library.roots.books()), Vec::<String>::new());
        assert_eq!(files_in(&library.roots.incoming()), Vec::<String>::new());
    }

    #[test]
    fn only_markdown_and_text_take_their_format_from_the_name() {
        assert_eq!(named_format(Path::new("a.md")), Some(Format::Markdown));
        assert_eq!(named_format(Path::new("a.TXT")), Some(Format::Text));
        assert_eq!(named_format(Path::new("a.epub")), None);
        assert_eq!(named_format(Path::new("a.unknown")), None);
        assert_eq!(named_format(Path::new("no-extension")), None);
    }

    #[test]
    fn a_thumbnail_is_made_again_after_a_save() {
        let library = library();
        let key = BookKey::generate();
        let avif = fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../mbook/tests/fixtures/plain.avif"
        ))
        .unwrap();

        block_on(refresh_thumbnail(&library.roots, key, Some(avif)));

        assert!(library.roots.cover(key).is_file());
    }

    #[test]
    fn a_book_that_lost_its_cover_loses_its_thumbnail() {
        let library = library();
        let key = BookKey::generate();
        fs::write(library.roots.cover(key), b"old").unwrap();

        block_on(refresh_thumbnail(&library.roots, key, None));

        assert!(!library.roots.cover(key).exists());
    }

    #[test]
    fn a_cover_that_cannot_be_decoded_costs_only_the_thumbnail() {
        let library = library();
        let key = BookKey::generate();

        block_on(refresh_thumbnail(
            &library.roots,
            key,
            Some(b"not a picture".to_vec()),
        ));

        assert!(!library.roots.cover(key).exists());
    }
}
