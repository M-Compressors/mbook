//! The error type of the application.

use std::io;

/// A failure of the application.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A file or a directory could not be read or written.
    #[error("a file could not be read or written")]
    Io(#[from] io::Error),
    /// `mbook` refused a book or a change of one.
    #[error("the book is not valid")]
    Book(#[from] mbook::Error),
    /// A screen could not be drawn, or its picture could not be written.
    #[cfg(feature = "visual")]
    #[error("a screen could not be rendered: {0}")]
    Render(String),
}

/// The result of a call that can fail with an [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[test]
    fn an_io_error_is_kept_as_the_source() {
        let error = Error::from(io::Error::other("disk"));
        assert_eq!(error.to_string(), "a file could not be read or written");
        assert_eq!(error.source().unwrap().to_string(), "disk");
    }

    #[test]
    fn a_book_error_is_kept_as_the_source() {
        let cause = mbook::Book::new("", "en").unwrap_err();
        let expected = cause.to_string();
        let error = Error::from(cause);
        assert_eq!(error.to_string(), "the book is not valid");
        assert_eq!(error.source().unwrap().to_string(), expected);
    }
}
