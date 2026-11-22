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
    /// The list of contents refused the change.
    #[error("the contents were not changed")]
    Contents(#[from] ContentsError),
    /// The path does not identify a resource of the kind the command needs.
    #[error("nothing in the book has the path {0:?}")]
    NoSuchPath(String),
    /// The task that owns the book is gone: the future of the session was
    /// dropped or has ended.
    #[error("the editing session has ended")]
    Ended,
}

/// Why a change of the contents list was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum ContentsError {
    /// The contents follow the headings: the book keeps no list.
    #[error("the contents follow the headings, so there is no list to change")]
    Automatic,
    /// The list has no entry at this position, counted from 0.
    #[error("the list has no entry at position {0}")]
    NoSuchEntry(usize),
    /// The title is empty.
    #[error("an entry needs a title")]
    NoTitle,
    /// The entry leads to something that is neither a chapter of the book
    /// nor a heading of one.
    #[error("the entry {title:?} leads to no chapter or heading of the book")]
    NoSuchPlace {
        /// The title of the entry.
        title: String,
    },
    /// The entry is already one level under the entry above it, or it is
    /// the first.
    #[error("an entry can be only one level under the entry above it")]
    TooDeep,
    /// The entry is at the first level.
    #[error("the entry is at the first level already")]
    TopLevel,
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
    use super::{ContentsError, EditError, OpenFailure};
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
    fn a_refused_change_of_the_contents_keeps_the_reason_as_its_source() {
        let error = EditError::from(ContentsError::NoSuchPlace {
            title: "Lost".into(),
        });
        assert_eq!(error.to_string(), "the contents were not changed");
        assert_eq!(
            error.source().map(ToString::to_string).as_deref(),
            Some("the entry \"Lost\" leads to no chapter or heading of the book")
        );
    }

    #[test]
    fn every_refusal_of_the_contents_says_what_is_wrong() {
        let words = [
            (
                ContentsError::Automatic,
                "the contents follow the headings, so there is no list to change",
            ),
            (
                ContentsError::NoSuchEntry(3),
                "the list has no entry at position 3",
            ),
            (ContentsError::NoTitle, "an entry needs a title"),
            (
                ContentsError::TooDeep,
                "an entry can be only one level under the entry above it",
            ),
            (
                ContentsError::TopLevel,
                "the entry is at the first level already",
            ),
        ];
        for (error, message) in words {
            assert_eq!(error.to_string(), message);
        }
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
