//! Saving a copy of a library book in another format.
//!
//! The desktop asks where to save it. Android has no save dialog a path comes
//! back from: the copy is made in the folder of shared files and offered to
//! the share sheet, whose targets include saving to a folder.

use std::fs::File;
#[cfg(not(target_os = "android"))]
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
#[cfg(not(target_os = "android"))]
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
#[cfg(not(target_os = "android"))]
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

/// Makes a copy of the book `key`, called `title`, in `format` on the
/// background executor, and offers it to the share sheet of the platform.
/// A failure is announced; a shown sheet is not.
#[cfg(target_os = "android")]
pub(super) fn share(key: BookKey, title: &str, format: Format, window: &Window, cx: &App) {
    let roots = &Store::global(cx).roots;
    let source = roots.book(key);
    let folder = roots.shared();
    let name = file_name(title, format);
    let title = title.to_owned();
    let executor = cx.background_executor().clone();
    window
        .spawn(cx, async move |cx| {
            let staged = executor
                .spawn(async move { stage(&source, &folder, &name, format) })
                .await;
            cx.update(|window, cx| {
                let message = match staged {
                    Ok(path) => match super::share::present(&path, &title) {
                        Ok(()) => return,
                        Err(error) => {
                            format!("Could not export {title}. {}.", error_chain(&error))
                        }
                    },
                    Err(error) => format!("Could not export {title}. {}.", error_chain(&error)),
                };
                announce(message, window, cx);
            })
            .ok();
        })
        .detach();
}

/// Saves the book at `source` in `format` as `name` in a new folder under
/// `folder`, and returns its path. Each export has a folder of its own, as
/// each share does.
#[cfg(any(target_os = "android", test))]
fn stage(source: &Path, folder: &Path, name: &str, format: Format) -> Result<PathBuf, ExportError> {
    std::fs::create_dir_all(folder)?;
    let own = tempfile::Builder::new()
        .prefix("export-")
        .tempdir_in(folder)?
        .keep();
    let path = own.join(name);
    save(source, &path, format)?;
    Ok(path)
}

fn file_name(title: &str, format: Format) -> String {
    format!("{}.{}", base_name(title), format.extension())
}

/// A file name without extension made of `title` that every platform accepts,
/// or `Book` when nothing of the title is left.
pub(super) fn base_name(title: &str) -> String {
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
    if safe.is_empty() || reserved {
        "Book".to_owned()
    } else {
        safe.to_owned()
    }
}

#[cfg(not(target_os = "android"))]
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
    fn a_staged_export_has_a_folder_of_its_own_under_the_shared_folder() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("book.mbook");
        let shared = dir.path().join("cache/share");
        let mut book = Book::new("The Example", "en").unwrap();
        book.add_chapter("one.md", "Text.\n").unwrap();
        book.save(&source).unwrap();

        let first = stage(&source, &shared, "The Example.txt", Format::Text).unwrap();
        let second = stage(&source, &shared, "The Example.txt", Format::Text).unwrap();

        assert_ne!(first, second);
        for path in [&first, &second] {
            assert!(path.starts_with(&shared));
            assert_eq!(path.file_name().unwrap(), "The Example.txt");
            assert_eq!(
                std::fs::read_to_string(path).unwrap(),
                "The Example\n\nText.\n"
            );
        }
    }

    #[test]
    fn a_staged_export_of_an_unreadable_book_fails_without_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("share");

        let error = stage(
            &dir.path().join("missing.mbook"),
            &shared,
            "a.txt",
            Format::Text,
        )
        .unwrap_err();

        assert!(matches!(error, ExportError::Book(_)));
        let left: Vec<_> = walk(&shared);
        assert!(left.iter().all(|path| path.is_dir()), "{left:?}");
    }

    fn walk(folder: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                found.extend(walk(&path));
            }
            found.push(path);
        }
        found
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
