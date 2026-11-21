//! Why an open book could not be opened or read.

/// A failure to open a book or to read from it.
///
/// The messages say what could not be done; the cause is the `source()`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum BookError {
    /// The file could not be opened as a book.
    #[error("the book could not be opened")]
    Open(#[source] mbook::Error),
    /// A chapter, a picture or a link could not be read from the file.
    #[error("the book could not be read")]
    Read(#[source] mbook::Error),
    /// The book has no chapter with this index.
    #[error("the book has no chapter {0}")]
    NoSuchChapter(usize),
    /// The book has no picture that this reference names.
    #[error("the book has no such picture")]
    NoSuchPicture,
    /// An earlier read panicked while it held the file, so the file cannot be
    /// trusted any more.
    #[error("the file of the book is no longer usable")]
    Unusable,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[test]
    fn opening_failure_keeps_its_cause_as_the_source() {
        let error = BookError::Open(mbook::Error::NotMbook);
        assert_eq!(error.to_string(), "the book could not be opened");
        assert_eq!(error.source().unwrap().to_string(), "not an mbook file");
    }

    #[test]
    fn reading_failure_keeps_its_cause_as_the_source() {
        let error = BookError::Read(mbook::Error::NoSuchResource(7));
        assert_eq!(error.to_string(), "the book could not be read");
        assert!(error.source().unwrap().to_string().contains('7'));
    }

    #[test]
    fn a_missing_chapter_names_its_index_and_has_no_source() {
        let error = BookError::NoSuchChapter(3);
        assert_eq!(error.to_string(), "the book has no chapter 3");
        assert!(error.source().is_none());
    }

    #[test]
    fn a_missing_picture_has_no_source() {
        let error = BookError::NoSuchPicture;
        assert_eq!(error.to_string(), "the book has no such picture");
        assert!(error.source().is_none());
    }

    #[test]
    fn an_unusable_file_has_no_source() {
        let error = BookError::Unusable;
        assert_eq!(
            error.to_string(),
            "the file of the book is no longer usable"
        );
        assert!(error.source().is_none());
    }
}
