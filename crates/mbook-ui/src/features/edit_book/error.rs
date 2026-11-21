//! What can go wrong in an editing session.

use std::error::Error as _;

/// Why a book could not be opened, kept in a form that can be given again.
///
/// An [`mbook::Error`] is not `Clone`; the session answers every command with
/// the same failure, so it keeps the message of the whole chain.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub(crate) struct OpenFailure {
    /// The message of the error and of every source below it.
    message: String,
}

impl OpenFailure {
    /// Describes the error `Book::open` returned.
    pub(crate) fn new(error: &mbook::Error) -> Self {
        let mut message = error.to_string();
        let mut source = error.source();
        while let Some(cause) = source {
            message.push_str(": ");
            message.push_str(&cause.to_string());
            source = cause.source();
        }
        Self { message }
    }
}

/// Why a command or a request of an editing session failed.
///
/// A failed command leaves the book as it was and does not end the session.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum EditError {
    /// The book could not be opened when the session started; every command
    /// of the session gives this error.
    #[error("the book could not be opened")]
    Open(#[source] OpenFailure),
    /// `mbook` refused the change: an invalid or used path, a text it cannot
    /// store, an unusable picture, an invalid metadata value or a position
    /// past the end.
    #[error("the book refused the change")]
    Book(#[source] mbook::Error),
    /// The book could not be written; the file on disk is as it was.
    #[error("the book could not be saved")]
    Save(#[source] mbook::Error),
    /// Neither a chapter nor a picture of the book has this path, or not the
    /// kind the command needs.
    #[error("nothing in the book has the path {0:?}")]
    NoSuchPath(String),
    /// The task that owns the book is gone: the future of the session was
    /// dropped or has ended.
    #[error("the editing session has ended")]
    Ended,
}

impl EditError {
    /// Where in the text of a chapter the book refused it, in bytes after
    /// line endings were normalised; `None` for every other failure.
    pub(crate) fn text_offset(&self) -> Option<usize> {
        match self {
            Self::Book(mbook::Error::InvalidText(error)) => Some(error.offset),
            _ => None,
        }
    }
}

impl From<mbook::Error> for EditError {
    /// A refusal of the book; a path that names nothing becomes
    /// [`EditError::NoSuchPath`].
    fn from(error: mbook::Error) -> Self {
        match error {
            mbook::Error::NoSuchPath(path) => Self::NoSuchPath(path),
            other => Self::Book(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EditError, OpenFailure};
    use std::error::Error as _;
    use std::io;

    fn missing() -> mbook::Error {
        mbook::Error::Io(io::Error::new(io::ErrorKind::NotFound, "gone"))
    }

    #[test]
    fn an_open_failure_keeps_the_message_of_the_whole_chain() {
        let failure = OpenFailure::new(&missing());
        assert_eq!(failure.to_string(), "I/O failed: gone");
    }

    #[test]
    fn an_open_error_names_the_opening_and_exposes_the_failure_as_source() {
        let error = EditError::Open(OpenFailure::new(&missing()));
        assert_eq!(error.to_string(), "the book could not be opened");
        let source = error.source().map(ToString::to_string);
        assert_eq!(source.as_deref(), Some("I/O failed: gone"));
    }

    #[test]
    fn a_refusal_of_the_book_is_the_source_of_the_error() {
        let error = EditError::from(mbook::Error::DuplicatePath("a.md".into()));
        assert_eq!(error.to_string(), "the book refused the change");
        let source = error.source().map(ToString::to_string);
        assert_eq!(source.as_deref(), Some("the path \"a.md\" is already used"));
    }

    #[test]
    fn a_missing_path_of_the_book_becomes_no_such_path() {
        let error = EditError::from(mbook::Error::NoSuchPath("a.md".into()));
        assert!(matches!(&error, EditError::NoSuchPath(path) if path == "a.md"));
        assert_eq!(
            error.to_string(),
            "nothing in the book has the path \"a.md\""
        );
        assert!(error.source().is_none());
    }

    #[test]
    fn a_failed_save_names_the_save_and_keeps_its_source() {
        let error = EditError::Save(missing());
        assert_eq!(error.to_string(), "the book could not be saved");
        assert_eq!(
            error.source().map(ToString::to_string).as_deref(),
            Some("I/O failed")
        );
    }

    #[test]
    fn an_ended_session_says_so() {
        assert_eq!(
            EditError::Ended.to_string(),
            "the editing session has ended"
        );
        assert!(EditError::Ended.source().is_none());
    }

    #[test]
    fn the_offset_of_a_refused_text_is_reachable() {
        let mut book = mbook::Book::new("T", "en").expect("a book");
        let refused = book
            .add_chapter("a.md", "ab\u{1}")
            .expect_err("a control character");
        assert_eq!(EditError::from(refused).text_offset(), Some(2));
    }

    #[test]
    fn other_errors_have_no_text_offset() {
        assert_eq!(EditError::Ended.text_offset(), None);
        let error = EditError::from(mbook::Error::DuplicatePath("a.md".into()));
        assert_eq!(error.text_offset(), None);
    }
}
