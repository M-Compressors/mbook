//! Saving a copy of a library book in another format.

use std::fs::File;
use std::future::Future;
use std::io::{self, BufReader, BufWriter, Write as _};
use std::path::{Path, PathBuf};

use gpui_kit::{App, Window};
use mbook::Book;
use mbook_converter::{Converter, Format};
use tempfile::NamedTempFile;

use crate::entities::catalogue::Store;
use crate::features::import_book::error_chain;
use crate::shared::model::BookKey;
use crate::shared::ui::announce;

pub(super) fn label(format: Format) -> &'static str {
    match format {
        Format::Epub => "EPUB (.epub)",
        Format::Docx => "Word (.docx)",
        Format::Fb2 => "FictionBook (.fb2)",
        Format::Pdf => "PDF (.pdf)",
        Format::Cbz => "Comic archive (.cbz)",
        Format::Html => "HTML (.html)",
        Format::Markdown => "Markdown (.md)",
        Format::Text => "Plain text (.txt)",
        _ => format.extension(),
    }
}

/// Offers the platform save dialog for the selected format.
pub(super) async fn pick(name: String, format: Format) -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Export book")
        .set_file_name(name)
        .add_filter(label(format), &[format.extension()])
        .save_file()
        .await
        .map(|file| file.path().to_path_buf())
}

/// Waits for the selected destination and performs the export in the background.
pub(super) fn choose<F>(
    key: BookKey,
    title: &str,
    format: Format,
    window: &Window,
    cx: &App,
    picker: impl FnOnce(String, Format) -> F,
) where
    F: Future<Output = Option<PathBuf>> + 'static,
{
    let source = Store::global(cx).roots.book(key);
    let selected = picker(file_name(title, format), format);
    let title = title.to_owned();
    let executor = cx.background_executor().clone();
    window
        .spawn(cx, async move |cx| {
            let Some(selected) = selected.await else {
                return;
            };
            let path = with_extension(selected, format);
            let result = executor
                .spawn(async move { save(&source, &path, format) })
                .await;
            cx.update(|window, cx| {
                let message = match result {
                    Ok(()) => format!("Exported {title} to {}.", label(format)),
                    Err(error) => format!("Could not export {title}. {}.", error_chain(&error)),
                };
                announce(message, window, cx);
            })
            .ok();
        })
        .detach();
}

fn file_name(title: &str, format: Format) -> String {
    let mut safe = String::new();
    for character in title.chars() {
        let character = if character.is_control() || "/\\<>:\"|?*".contains(character) {
            '_'
        } else {
            character
        };
        // Keep space for the suffix within common byte and UTF-16 limits.
        if safe.len() + character.len_utf8() > 180 {
            break;
        }
        safe.push(character);
    }
    let safe = safe.trim_matches(|character: char| character == '.' || character.is_whitespace());
    let first = safe
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&first.as_str())
        || ["COM", "LPT"].iter().any(|prefix| {
            first.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        });
    let safe = if safe.is_empty() || reserved {
        "Book"
    } else {
        safe
    };
    format!("{safe}.{}", format.extension())
}

fn with_extension(path: PathBuf, format: Format) -> PathBuf {
    let extension = format.extension();
    if path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|actual| actual.eq_ignore_ascii_case(extension))
    {
        return path;
    }
    let mut path = path.into_os_string();
    path.push(".");
    path.push(extension);
    path.into()
}

#[derive(Debug, thiserror::Error)]
enum ExportError {
    #[error("the library book could not be read")]
    Book(#[from] mbook::Error),
    #[error(transparent)]
    Convert(#[from] mbook_converter::Error),
    #[error("the export file could not be saved")]
    Io(#[from] io::Error),
    #[error("a file with that name already exists")]
    Exists,
}

/// Publishes the finished export by moving a temporary file in the destination
/// directory. A concurrent export or an existing file keeps its name.
fn save(source: &Path, destination: &Path, format: Format) -> Result<(), ExportError> {
    let file = File::open(source).map_err(mbook::Error::from)?;
    let book = Book::read(BufReader::new(file))?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut output = NamedTempFile::new_in(parent)?;
    let mut writer = BufWriter::new(output.as_file_mut());
    Converter::export(&book, format, &mut writer)?;
    writer.flush()?;
    drop(writer);
    output.persist_noclobber(destination).map_err(|error| {
        if error.error.kind() == io::ErrorKind::AlreadyExists {
            ExportError::Exists
        } else {
            error.error.into()
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_suggested_name_is_safe_on_all_platforms() {
        assert_eq!(
            file_name(" story: a/b? ", Format::Markdown),
            "story_ a_b_.md"
        );
        assert_eq!(file_name("..", Format::Epub), "Book.epub");
        assert_eq!(file_name("NUL.notes", Format::Epub), "Book.epub");
        assert!(file_name(&"日".repeat(100), Format::Docx).len() <= 185);
    }

    #[test]
    fn only_a_missing_selected_format_extension_is_appended() {
        assert_eq!(
            with_extension("My book.PDF".into(), Format::Pdf),
            Path::new("My book.PDF")
        );
        assert_eq!(
            with_extension("My book".into(), Format::Pdf),
            Path::new("My book.pdf")
        );
        assert_eq!(
            with_extension("My book.v2".into(), Format::Pdf),
            Path::new("My book.v2.pdf")
        );
    }

    #[test]
    fn saving_exports_the_library_copy_and_keeps_the_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("book.mbook");
        let target = dir.path().join("out.txt");
        let mut book = Book::new("The Example", "en").unwrap();
        book.add_chapter("one.md", "# One\n\nText.\n").unwrap();
        book.save(&source).unwrap();
        let original = std::fs::read(&source).unwrap();

        save(&source, &target, Format::Text).unwrap();

        assert_eq!(
            std::fs::read_to_string(target).unwrap(),
            "The Example\n\nOne\n\nText.\n"
        );
        assert_eq!(std::fs::read(source).unwrap(), original);
    }

    #[test]
    fn exporting_from_the_library_decodes_avif_pictures() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("book.mbook");
        let target = dir.path().join("out.html");
        let mut book = Book::new("The Example", "en").unwrap();
        book.add_chapter("one.md", "Text.\n").unwrap();
        book.set_cover(
            "cover.avif",
            include_bytes!("../../../../mbook/tests/fixtures/plain.avif"),
        )
        .unwrap();
        book.save(&source).unwrap();

        save(&source, &target, Format::Html).unwrap();

        assert!(
            std::fs::read_to_string(target)
                .unwrap()
                .contains("data:image/png;base64,")
        );
    }

    #[test]
    fn saving_never_overwrites_an_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("book.mbook");
        let target = dir.path().join("out.txt");
        let mut book = Book::new("The Example", "en").unwrap();
        book.add_chapter("one.md", "Text.\n").unwrap();
        book.save(&source).unwrap();
        std::fs::write(&target, "Already here").unwrap();

        assert!(matches!(
            save(&source, &target, Format::Text),
            Err(ExportError::Exists)
        ));
        assert_eq!(std::fs::read_to_string(target).unwrap(), "Already here");
    }

    #[test]
    fn a_book_that_cannot_be_read_leaves_no_output() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("broken.mbook");
        let target = dir.path().join("out.epub");
        std::fs::write(&source, "broken").unwrap();

        assert!(matches!(
            save(&source, &target, Format::Epub),
            Err(ExportError::Book(_))
        ));
        assert!(!target.exists());
    }
}
