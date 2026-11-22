//! Read, edit and create mbook files.
//!
//! An mbook file holds a book as Markdown chapters and AVIF pictures, with
//! diagrams in Mermaid notation, formulas in the notation of `KaTeX`, and
//! small tables that describe it. The format is specified by RFC 0001 and this
//! API by RFC 0002, both in the `rfcs` folder of the repository.
//!
//! Two types do the work:
//!
//! - [`BookReader`] reads a book lazily from anything that can seek. Opening
//!   it reads the tables; each chapter, picture, diagram and formula is
//!   decoded when asked for.
//! - [`Book`] is a whole book in memory. It is how a book is created and how
//!   an existing one is changed.
//!
//! A source that cannot seek is served by both: [`BookReader::buffered`] keeps
//! the compressed bytes in memory and still decodes on demand, and
//! [`Book::read`] decodes everything in one pass.
//!
//! # Features
//!
//! | Feature | Default | Enables |
//! | --- | --- | --- |
//! | `read` | yes | [`BookReader`], [`Book::read`], [`Book::open`] |
//! | `write` | yes | [`Book::new`] and everything that changes or saves a [`Book`] |
//!
//! At least one of them must be enabled.
//!
// Links to items of a disabled feature lead to the table above instead. The
// empty doc line above is what lets the definitions start a new block.
#![cfg_attr(
    not(feature = "read"),
    doc = "[`BookReader`]: #features\n[`BookReader::buffered`]: #features\n[`Book::read`]: #features\n[`Book::open`]: #features"
)]
#![cfg_attr(not(feature = "write"), doc = "[`Book::new`]: #features")]

#[cfg(not(any(feature = "read", feature = "write")))]
compile_error!("mbook needs at least one of the `read` and `write` features");

#[cfg(feature = "write")]
mod analysis;
#[cfg(feature = "write")]
mod avif;
mod book;
mod container;
mod error;
mod format;
mod id;
#[cfg(feature = "read")]
mod index;
mod link;
mod meta;
mod options;
mod path;
#[cfg(feature = "read")]
mod reader;
mod tables;
#[cfg(feature = "write")]
mod text;
mod types;

pub use book::{Book, Chapter, Diagram, Formula, Landmark, Picture, Toc, TocItem};
pub use error::{
    Error, LandmarkError, MetaError, PathError, Problem, ProblemKind, Problems, ResolveError,
    Result, SourceError, TextError,
};
pub use id::{ChapterId, DiagramId, FormulaId, PictureId};
pub use meta::{Meta, Series};
pub use options::{Limits, ReadOptions, WriteOptions};
#[cfg(feature = "read")]
pub use reader::BookReader;
pub use types::{
    Anchor, ChapterEntry, Destination, DiagramEntry, FormulaEntry, LandmarkEntry, PictureEntry,
    Position, Resource, ResourceKind, Size, TocEntry, Version,
};

/// Tells whether a file looks like an mbook file from its first bytes.
///
/// This is the detection rule of RFC 0001. It needs the first 53 bytes of the
/// file and returns `false` for a shorter slice. It is a hint for file
/// managers: opening the file is what validates it.
///
/// # Examples
///
/// ```
/// assert!(!mbook::is_mbook(b"PK\x03\x04"));
/// ```
#[must_use]
pub fn is_mbook(prefix: &[u8]) -> bool {
    let format_end = format::PREFIX_OFFSET + format::PREFIX.len();
    prefix.starts_with(format::CONTAINER_MAGIC)
        && prefix.get(format::PREFIX_OFFSET..format_end) == Some(format::PREFIX.as_slice())
}

#[cfg(test)]
mod tests {
    use super::is_mbook;

    /// The first 64 bytes of a version 1.0 file, from RFC 0001.
    const PREFIX: [u8; 64] = [
        0x4d, 0x4f, 0x5a, 0x46, 0x52, 0x4d, 0x01, 0x00, 0x52, 0x53, 0x52, 0x43, 0x00, 0x00, 0x00,
        0x00, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x8e, 0x00, 0x8c, 0xba, 0xdb, 0xa5, 0xb1, 0xd7, 0x03, 0x01, 0x09, 0xf8, 0x01,
        0x01, 0x08, 0x4d, 0x42, 0x4f, 0x4f, 0x4b, 0x00, 0x01, 0x00, 0x52, 0xf8, 0x76, 0xac, 0x1b,
        0x24, 0x43, 0x5c, 0x00,
    ];

    #[test]
    fn is_mbook_accepts_the_prefix_from_the_format_rfc() {
        assert!(is_mbook(&PREFIX));
    }

    #[test]
    fn is_mbook_needs_only_the_first_53_bytes() {
        assert!(is_mbook(&PREFIX[..53]));
    }

    #[test]
    fn is_mbook_rejects_a_prefix_cut_inside_the_signature() {
        assert!(!is_mbook(&PREFIX[..52]));
    }

    #[test]
    fn is_mbook_rejects_another_framed_container() {
        let mut other = PREFIX;
        other[47] = b'X';
        assert!(!is_mbook(&other));
    }
}
