//! Error types.

use crate::types::Version;
use std::fmt;

/// The result type of this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Why an operation on a book failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The source or the sink failed.
    #[error("I/O failed")]
    Io(#[from] std::io::Error),
    /// The file has no valid format header.
    #[error("not an mbook file")]
    NotMbook,
    /// The file declares a major version newer than this crate supports.
    #[error("format version {0} is newer than this library supports")]
    UnsupportedVersion(Version),
    /// The container or one of its frames is damaged.
    #[error("the container or a frame is damaged")]
    Container(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// A table breaks a rule of the format.
    #[error("resource {resource} is malformed: {reason}")]
    Malformed {
        /// The number of the offending resource.
        resource: u32,
        /// The rule that is broken.
        reason: &'static str,
    },
    /// A configured limit was exceeded.
    #[error("limit exceeded: {0}")]
    Limit(&'static str),
    /// An id that came from another book was used to read a resource.
    #[error("resource {0} is not a chapter or picture of this book")]
    NoSuchResource(u32),
    /// No chapter or picture of the book has this path.
    #[error("nothing in the book has the path {0:?}")]
    NoSuchPath(String),
    /// A position in the reading order is past its end.
    #[error("index {index} is out of range for {len} chapters")]
    IndexOutOfRange {
        /// The index that was given.
        index: usize,
        /// The number of chapters in the book.
        len: usize,
    },
    /// A path breaks the rules of the format.
    #[error("invalid path {path:?}")]
    InvalidPath {
        /// The path that was given.
        path: String,
        /// The rule that is broken.
        #[source]
        reason: PathError,
    },
    /// The path is already used by a chapter or a picture.
    #[error("the path {0:?} is already used")]
    DuplicatePath(String),
    /// The text of a chapter cannot be stored.
    #[error("invalid chapter text")]
    InvalidText(#[from] TextError),
    /// The bytes given for a picture are not a usable AVIF file.
    #[error("invalid picture: {0}")]
    InvalidPicture(&'static str),
    /// A change to the metadata breaks the rules of the format.
    #[error("invalid metadata")]
    InvalidMeta(#[from] MetaError),
    /// The book as a whole is not valid and was not written.
    #[error("the book cannot be written")]
    Invalid(#[from] Problems),
}

/// Why a path was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PathError {
    /// The path is empty.
    #[error("the path is empty")]
    Empty,
    /// The path starts or ends with `/`, or contains `//`.
    #[error("a segment is empty")]
    EmptySegment,
    /// A segment is `.` or `..`.
    #[error("a segment is `.` or `..`")]
    DotSegment,
    /// The path contains a backslash.
    #[error("the path contains a backslash")]
    Backslash,
    /// The path contains a character below U+0020.
    #[error("the path contains a control character")]
    ControlCharacter,
}

/// A character that a chapter must not contain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("control character U+{:04X} at byte {offset}", u32::from(*.character))]
#[non_exhaustive]
pub struct TextError {
    /// Where the character is, in bytes from the start of the text after
    /// line endings were normalised.
    pub offset: usize,
    /// The character.
    pub character: char,
}

/// Why a change to the metadata was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MetaError {
    /// The key does not match `[a-z0-9]+(-[a-z0-9]+)*`.
    #[error("{0:?} is not a valid key")]
    InvalidKey(String),
    /// The key must be present and cannot be removed.
    #[error("the key {0:?} is required")]
    Required(String),
    /// The key holds one value and already has it.
    #[error("the key {0:?} holds one value")]
    SingleValued(String),
    /// The title is empty.
    #[error("the title is empty")]
    EmptyTitle,
}

/// Why a link or picture destination leads nowhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ResolveError {
    /// The destination has a URI scheme other than `http`, `https` or
    /// `mailto`.
    #[error("the scheme is not allowed")]
    ForbiddenScheme,
    /// The destination contains `?`, starts with `/`, or does not decode to
    /// UTF-8.
    #[error("the destination is malformed")]
    Malformed,
    /// A `..` segment leaves the root of the book.
    #[error("the destination leaves the book")]
    OutsideRoot,
    /// No chapter or picture has the resolved path.
    #[error("nothing in the book has that path")]
    NotFound,
    /// The chapter has no anchor with the name given after `#`.
    #[error("the chapter has no such anchor")]
    UnknownAnchor,
    /// A destination that names a picture has a fragment.
    #[error("a picture cannot have a fragment")]
    FragmentOnPicture,
}

/// Every problem that keeps a book from being written.
///
/// Returned by `Book::check`, and inside [`Error::Invalid`] by the calls that
/// write a book.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the book has {} problem(s)", .0.len())]
pub struct Problems(Vec<Problem>);

impl Problems {
    /// The problems, in the order they were found.
    pub fn iter(&self) -> std::slice::Iter<'_, Problem> {
        self.0.iter()
    }

    /// The number of problems; never zero.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Always `false`: a check that finds nothing returns `Ok`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'a> IntoIterator for &'a Problems {
    type Item = &'a Problem;
    type IntoIter = std::slice::Iter<'a, Problem>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// One problem found when checking a book.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The chapter the problem is in, when it is in one.
    pub path: Option<String>,
    /// Where in that chapter, in bytes from the start of its text.
    pub offset: Option<u32>,
    /// What is wrong.
    pub kind: ProblemKind,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.path, self.offset) {
            (Some(path), Some(offset)) => write!(f, "{path}, byte {offset}: {}", self.kind),
            (Some(path), None) => write!(f, "{path}: {}", self.kind),
            (None, _) => write!(f, "{}", self.kind),
        }
    }
}

/// What is wrong with a book.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ProblemKind {
    /// A link leads to no chapter or anchor of the book.
    #[error("the link to {0:?} leads nowhere")]
    UnresolvedLink(String),
    /// A picture reference names no picture of the book.
    #[error("the picture {0:?} is not in the book")]
    UnresolvedPicture(String),
    /// A link or a picture uses a URI scheme the format does not allow.
    #[error("the destination {0:?} uses a scheme that is not allowed")]
    ForbiddenScheme(String),
    /// The chapter contains raw HTML.
    #[error("raw HTML is not allowed")]
    RawHtml,
    /// The `cover` key names no picture of the book.
    #[error("the cover is not a picture of the book")]
    MissingCover,
    /// A contents item names a chapter or an anchor that does not exist.
    #[error("a contents item leads nowhere")]
    DanglingTocItem,
    /// The levels of the contents items do not form a tree.
    #[error("the levels of the contents do not form a tree")]
    TocLevel,
}
