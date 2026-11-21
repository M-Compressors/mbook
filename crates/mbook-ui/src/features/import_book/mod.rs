//! Adding imported and newly written books to the library.

mod import;
mod queue;

pub(crate) use import::{ImportError, Imported, Step, create_book, import, refresh_thumbnail};
pub(crate) use queue::{ImportEntry, ImportEvent, ImportQueue, ImportStatus, error_chain, now_ms};

/// The file extensions offered by the native book picker, including ZIP
/// containers for supported zipped sources.
pub(crate) const BOOK_FILE_EXTENSIONS: &[&str] = &[
    "mbook", "epub", "fb2", "fbz", "zip", "pdf", "cbz", "docx", "html", "htm", "xhtml", "md",
    "markdown", "mkd", "txt", "text",
];

#[cfg(test)]
mod tests {
    use super::BOOK_FILE_EXTENSIONS;
    use mbook_converter::Format;

    #[test]
    fn the_book_picker_offers_mbook_and_all_supported_source_formats() {
        assert!(BOOK_FILE_EXTENSIONS.contains(&"mbook"));
        assert!(BOOK_FILE_EXTENSIONS.contains(&"zip"));
        let offered: Vec<Format> = BOOK_FILE_EXTENSIONS
            .iter()
            .filter(|&&extension| !matches!(extension, "mbook" | "zip"))
            .map(|extension| extension.parse::<Format>().unwrap())
            .collect();
        assert!(offered.iter().all(|format| format.is_enabled()));
        assert!(Format::ALL.iter().all(|format| offered.contains(format)));
    }
}
