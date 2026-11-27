//! Why a tool of an agent failed, as the agent is told it.
//!
//! Every error has a `code` the agent can act on and a message in English,
//! the language of the tools; some carry more fields (mbook-ui docs, "Agents
//! (MCP)", "Errors"). A failed call changes nothing.

use serde_json::{Map, Value, json};

use super::types::Kind;
use crate::features::edit_book::error::EditError;

/// Why a tool failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum AgentError {
    /// No book of the library has this key.
    #[error("no book of the library has the key {0:?}")]
    NoSuchBook(String),
    /// Nothing of the kind the call needs has this path.
    #[error("nothing in the book has the path {0:?}")]
    NoSuchPath(String),
    /// A path breaks a rule of paths.
    #[error("invalid path: {0}")]
    InvalidPath(String),
    /// Something already has this path.
    #[error("the path {0:?} is already used")]
    DuplicatePath(String),
    /// A character a chapter may not hold.
    #[error("the character {character:?} is not allowed in a chapter")]
    InvalidText {
        line: usize,
        column: usize,
        character: char,
    },
    /// A value of the metadata breaks a rule.
    #[error("invalid {field}: {message}")]
    InvalidMeta { field: String, message: String },
    /// A landmark breaks a rule.
    #[error("invalid landmarks: {0}")]
    InvalidLandmark(String),
    /// An argument that is not of the form the tool takes.
    #[error("{0}")]
    InvalidArguments(String),
    /// An `old` of `edit_chapter` is not in the text.
    #[error("the old text of edit {index} is not in the chapter")]
    OldNotFound { index: usize },
    /// An `old` of `edit_chapter` is in the text more than once.
    #[error("the old text of edit {index} is in the chapter {count} times")]
    OldAmbiguous { index: usize, count: usize },
    /// The chapter changed after the revision given.
    #[error("the chapter changed after revision {given}; it is at {revision}")]
    Conflict { given: String, revision: String },
    /// The person is typing in this chapter.
    #[error("the person is typing in this chapter; try again in a few seconds")]
    UserTyping,
    /// The picture cannot be read, or is too large.
    #[error("invalid picture: {0}")]
    InvalidPicture(String),
    /// A diagram or formula cannot be drawn.
    #[error("the {} cannot be drawn: {message}", kind_name(*kind))]
    InvalidFigure {
        kind: Kind,
        /// The line of the text it was written on, when it came from one.
        line: Option<usize>,
        message: String,
    },
    /// The person declined in the dialog.
    #[error("the person declined")]
    Declined,
    /// A limit of the application.
    #[error("limit exceeded: {0}")]
    Limit(String),
    /// The book could not be opened.
    #[error("the book could not be opened: {0}")]
    OpenFailed(String),
    /// The book could not be written.
    #[error("the book could not be written: {0}")]
    SaveFailed(String),
    /// A file could not be read or written.
    #[error("{0}")]
    Io(String),
}

/// The name of a kind of figure in a message.
const fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Diagram => "diagram",
        Kind::Formula => "formula",
    }
}

impl AgentError {
    /// The code the agent acts on.
    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::NoSuchBook(_) => "no_such_book",
            Self::NoSuchPath(_) => "no_such_path",
            Self::InvalidPath(_) => "invalid_path",
            Self::DuplicatePath(_) => "duplicate_path",
            Self::InvalidText { .. } => "invalid_text",
            Self::InvalidMeta { .. } => "invalid_meta",
            Self::InvalidLandmark(_) => "invalid_landmark",
            Self::InvalidArguments(_) => "invalid_arguments",
            Self::OldNotFound { .. } => "old_not_found",
            Self::OldAmbiguous { .. } => "old_ambiguous",
            Self::Conflict { .. } => "conflict",
            Self::UserTyping => "user_typing",
            Self::InvalidPicture(_) => "invalid_picture",
            Self::InvalidFigure { .. } => "invalid_figure",
            Self::Declined => "declined",
            Self::Limit(_) => "limit",
            Self::OpenFailed(_) => "open_failed",
            Self::SaveFailed(_) => "save_failed",
            Self::Io(_) => "io",
        }
    }

    /// The error as the agent gets it: `code`, `message` and the fields of
    /// the code.
    pub(crate) fn to_json(&self) -> Value {
        let mut out = Map::new();
        out.insert("code".into(), self.code().into());
        out.insert("message".into(), self.to_string().into());
        let more = match self {
            Self::InvalidText { line, column, .. } => json!({"line": line, "column": column}),
            Self::InvalidMeta { field, .. } => json!({"field": field}),
            Self::OldNotFound { index } => json!({"index": index}),
            Self::OldAmbiguous { index, count } => json!({"index": index, "count": count}),
            Self::Conflict { revision, .. } => json!({"revision": revision}),
            Self::InvalidFigure {
                kind,
                line,
                message,
            } => {
                json!({"kind": kind, "line": line, "renderer": message})
            }
            Self::Limit(limit) => json!({"limit": limit}),
            _ => json!({}),
        };
        if let Value::Object(more) = more {
            out.extend(more);
        }
        Value::Object(out)
    }

    /// An error of `mbook`, with `text` the chapter it concerns, if any, to
    /// give a place in it.
    pub(crate) fn of_book(error: &mbook::Error, text: Option<&str>) -> Self {
        let chain = || chain(error);
        match error {
            mbook::Error::NoSuchPath(path) => Self::NoSuchPath(path.clone()),
            mbook::Error::InvalidPath { .. } | mbook::Error::IndexOutOfRange { .. } => {
                Self::InvalidPath(chain())
            }
            mbook::Error::DuplicatePath(path) => Self::DuplicatePath(path.clone()),
            mbook::Error::InvalidText(error) => {
                let (line, column) =
                    text.map_or((1, 1), |text| line_and_column(text, error.offset));
                Self::InvalidText {
                    line,
                    column,
                    character: error.character,
                }
            }
            mbook::Error::InvalidPicture(why) => Self::InvalidPicture((*why).to_owned()),
            mbook::Error::InvalidDiagram(why) => Self::InvalidFigure {
                kind: Kind::Diagram,
                line: None,
                message: why.to_string(),
            },
            mbook::Error::InvalidFormula(why) => Self::InvalidFigure {
                kind: Kind::Formula,
                line: None,
                message: why.to_string(),
            },
            mbook::Error::InvalidMeta(why) => Self::InvalidMeta {
                field: "meta".into(),
                message: why.to_string(),
            },
            mbook::Error::InvalidLandmark(why) => Self::InvalidLandmark(why.to_string()),
            mbook::Error::Limit(limit) => Self::Limit((*limit).to_owned()),
            _ => Self::Io(chain()),
        }
    }
}

impl From<EditError> for AgentError {
    fn from(error: EditError) -> Self {
        match error {
            EditError::Open(failure) => Self::OpenFailed(failure.to_string()),
            EditError::Book(error) => Self::of_book(&error, None),
            EditError::Save(error) => Self::SaveFailed(chain(&error)),
            EditError::Contents(error) => Self::InvalidArguments(error.to_string()),
            EditError::NoSuchPath(path) => Self::NoSuchPath(path),
            EditError::Ended => Self::OpenFailed("the editing session ended".into()),
            EditError::Conflict => Self::UserTyping,
        }
    }
}

/// The message of an error and of every source below it.
pub(crate) fn chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

/// The line and column of a byte offset, both from 1; a column counts
/// characters.
pub(crate) fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |at| at + 1);
    (line, before[start..].chars().count() + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_is_counted_in_lines_and_characters() {
        assert_eq!(line_and_column("ab\ncd", 0), (1, 1));
        assert_eq!(line_and_column("ab\ncd", 4), (2, 2));
        assert_eq!(line_and_column("аб\nв", 4), (1, 3));
        assert_eq!(line_and_column("аб", 1), (1, 1));
        assert_eq!(line_and_column("ab", 99), (1, 3));
    }

    #[test]
    fn an_error_carries_its_code_and_fields() {
        let error = AgentError::OldAmbiguous { index: 1, count: 3 };
        assert_eq!(
            error.to_json(),
            json!({"code": "old_ambiguous", "index": 1, "count": 3,
                   "message": "the old text of edit 1 is in the chapter 3 times"})
        );
        let error = AgentError::InvalidFigure {
            kind: Kind::Formula,
            line: Some(4),
            message: "bad".into(),
        };
        assert_eq!(error.to_json()["line"], 4);
        assert_eq!(error.to_string(), "the formula cannot be drawn: bad");
        assert_eq!(AgentError::Declined.to_json()["code"], "declined");
    }

    #[test]
    fn a_refusal_of_the_book_has_the_code_of_its_rule() {
        let text = "ok\n\u{7}";
        let mut book = mbook::Book::new("T", "en").unwrap();
        let error = book.add_chapter("ch01.md", text).unwrap_err();
        assert_eq!(
            AgentError::of_book(&error, Some(text)),
            AgentError::InvalidText {
                line: 2,
                column: 1,
                character: '\u{7}'
            }
        );
        let error = mbook::Error::DuplicatePath("ch01.md".into());
        assert_eq!(AgentError::of_book(&error, None).code(), "duplicate_path");
        let error = mbook::Error::Limit("resources");
        assert_eq!(AgentError::of_book(&error, None).code(), "limit");
    }

    #[test]
    fn every_refusal_of_a_book_has_a_code() {
        let mut book = mbook::Book::new("T", "en").unwrap();
        book.add_chapter("ch01.md", "# One\n").unwrap();
        let codes: Vec<_> = [
            book.add_chapter("../x.md", "").unwrap_err(),
            book.insert_chapter(9, "ch09.md", "").unwrap_err(),
            book.add_picture("a.avif", b"no".to_vec()).unwrap_err(),
            book.add_diagram("d.mmd", " ").unwrap_err(),
            book.add_formula("f.tex", " ").unwrap_err(),
            book.meta_mut().set_title("").unwrap_err(),
            book.set_landmarks(vec![
                mbook::Landmark::new("start", "", "ch01.md"),
                mbook::Landmark::new("start", "", "ch01.md"),
            ])
            .unwrap_err(),
            mbook::Error::NoSuchPath("x".into()),
            mbook::Error::NotMbook,
        ]
        .iter()
        .map(|error| AgentError::of_book(error, None).code())
        .collect();
        assert_eq!(
            codes,
            [
                "invalid_path",
                "invalid_path",
                "invalid_picture",
                "invalid_figure",
                "invalid_figure",
                "invalid_meta",
                "invalid_landmark",
                "no_such_path",
                "io",
            ]
        );
    }

    #[test]
    fn every_error_of_a_session_has_a_code() {
        use crate::features::edit_book::error::{ContentsError, OpenFailure};
        let codes: Vec<_> = [
            EditError::Open(OpenFailure::new(&mbook::Error::NotMbook)),
            EditError::Book(mbook::Error::DuplicatePath("x".into())),
            EditError::Save(mbook::Error::NotMbook),
            EditError::Contents(ContentsError::NoTitle),
            EditError::NoSuchPath("x".into()),
            EditError::Ended,
            EditError::Conflict,
        ]
        .into_iter()
        .map(|error| AgentError::from(error).code())
        .collect();
        assert_eq!(
            codes,
            [
                "open_failed",
                "duplicate_path",
                "save_failed",
                "invalid_arguments",
                "no_such_path",
                "open_failed",
                "user_typing",
            ]
        );
    }

    #[test]
    fn every_error_says_its_code_and_message() {
        for error in [
            AgentError::NoSuchBook("k".into()),
            AgentError::InvalidMeta {
                field: "date".into(),
                message: "bad".into(),
            },
            AgentError::OldNotFound { index: 2 },
            AgentError::Conflict {
                given: "1".into(),
                revision: "2".into(),
            },
            AgentError::Limit("resources".into()),
            AgentError::InvalidText {
                line: 1,
                column: 2,
                character: '\u{7}',
            },
        ] {
            let json = error.to_json();
            assert_eq!(json["code"], error.code());
            assert_eq!(json["message"], error.to_string());
        }
    }
}
