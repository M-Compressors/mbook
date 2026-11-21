//! Small value types shared by the reader and the model.

use crate::id::{ChapterId, PictureId};
use std::fmt;

/// The version of the mbook format a file declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Version {
    /// Changes when a file stops being readable by older software.
    pub major: u8,
    /// Changes when something is added that older software can skip.
    pub minor: u8,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// The size of a picture in pixels, as displayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Size {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// A place inside a chapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Position {
    /// The chapter.
    pub chapter: ChapterId,
    /// A count of bytes from the start of the chapter's text.
    pub offset: u32,
}

/// One chapter, as listed by a `BookReader`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChapterEntry<'a> {
    /// The id to read the chapter with.
    pub id: ChapterId,
    /// The path of the chapter inside the book.
    pub path: &'a str,
    /// The length of the text in bytes.
    pub decoded_len: u64,
}

/// One picture, as listed by a `BookReader`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PictureEntry<'a> {
    /// The id to read the picture with.
    pub id: PictureId,
    /// The path of the picture inside the book.
    pub path: &'a str,
    /// The size in pixels, known without decoding the picture.
    pub size: Size,
    /// The length of the AVIF file in bytes.
    pub decoded_len: u64,
}

/// One entry of the table of contents, as stored in a file.
///
/// Entries form a tree listed in pre-order; `level` gives the depth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TocEntry {
    /// Depth in the tree, starting at 1.
    pub level: u8,
    /// The title, as plain text.
    pub title: String,
    /// Where the entry leads.
    pub position: Position,
}

/// A named place inside a chapter, the target of a link fragment.
///
/// A chapter has one anchor for each of its headings, and may have extra
/// anchors for other places: a paragraph, a figure, a line of a table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anchor {
    /// A count of bytes from the start of the chapter's text.
    pub offset: u32,
    /// The name, as written after `#` in a link.
    pub name: String,
}

/// What a link or picture destination in a chapter points to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    /// A link out of the book, with an `http`, `https` or `mailto` scheme.
    External,
    /// A place in a chapter of this book.
    Chapter(Position),
    /// A picture of this book.
    Picture(PictureId),
    /// A named place in this chapter that is not the anchor of a heading.
    ///
    /// The name may be in the chapter's own anchor table, which is not read
    /// when a book is opened. `BookReader::read_resolve` reads it and gives
    /// the position, or tells that the chapter has no such anchor.
    ExtraAnchor(ChapterId),
}

/// What a resource holds, as declared by the resource table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ResourceKind {
    /// One of the five fixed resources at the start of a file.
    Service,
    /// A chapter.
    Text,
    /// A picture.
    Picture,
    /// The anchor table of the chapter that follows it.
    ChapterAnchors,
    /// A kind defined by a later minor version of the format.
    Unknown(u8),
}

impl From<u8> for ResourceKind {
    fn from(code: u8) -> Self {
        match code {
            0 => Self::Service,
            1 => Self::Text,
            2 => Self::Picture,
            3 => Self::ChapterAnchors,
            other => Self::Unknown(other),
        }
    }
}

/// One row of the resource table, for tools that list a whole file.
///
/// Reading applications use [`ChapterEntry`] and [`PictureEntry`] instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resource<'a> {
    /// The number of the resource in the file.
    pub number: u32,
    /// What the resource holds.
    pub kind: ResourceKind,
    /// The path; empty for service resources and anchor tables.
    pub path: &'a str,
    /// The size in pixels; pictures only.
    pub size: Option<Size>,
    /// The decoded length in bytes.
    pub decoded_len: u64,
}
