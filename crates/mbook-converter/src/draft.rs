//! What a front end hands to the assembler: a book in the terms of its
//! source.
//!
//! A front end parses one format and builds a [`Draft`]. It decides nothing
//! about the mbook file: no chapter paths, no picture paths, no tables. The
//! contract between a front end and the assembler is this:
//!
//! - **Sections.** One [`Section`] per top-level division of the source, in
//!   reading order. Its `name` is unique in the draft, is not empty and does
//!   not contain `#` or `:`.
//! - **Markdown.** The text of a section is Markdown of the dialect of
//!   RFC 0001. Text taken from the source is escaped with
//!   [`markdown::escape`](crate::markdown::escape). Raw HTML that slips
//!   through is removed by the assembler, with a warning.
//! - **Link targets.** An element of the source that a link may lead to is
//!   marked in front of its block with an anchor definition and an empty
//!   line: `format!("{}\n\n", markdown::definition(&markdown::anchor_name(id)))`,
//!   where `id` is the identifier the element has in the source. The
//!   assembler keeps the marks that a link uses and removes the others.
//! - **Links into the book.** `format!("[{text}]{}", markdown::target(&destination, None))`,
//!   where `text` is Markdown already and `destination` comes from
//!   [`markdown::destination`](crate::markdown::destination): the name of
//!   the target section and, for a marked element, its identifier. An
//!   address outside the book is written as it is.
//! - **Pictures.** `markdown::image(alt, &markdown::encoded_path(name), title)`,
//!   where `name` is the `name` of a [`SourcePicture`] of the draft. A
//!   picture alone in a paragraph is a figure, and `title` is its caption.
//!   Pictures are handed over as the source has them; the assembler encodes
//!   them.
//! - **Footnotes.** A note is `[^label]` at its reference and
//!   `[^label]: text` in the same section.
//! - **Outline.** When the source has a table of contents of its own, one
//!   [`OutlineEntry`] per item, its target built like the destination of a
//!   link.
//! - **Losses.** Whatever a front end cannot keep is a
//!   [`Warning`] in the draft, made with `Warning::new(kind, section_name)`.

use crate::report::Warning;

/// A book as its source describes it.
#[derive(Debug, Default)]
pub(crate) struct Draft {
    /// The metadata, unchecked.
    pub(crate) meta: DraftMeta,
    /// The text, in reading order.
    pub(crate) sections: Vec<Section>,
    /// The pictures of the source.
    #[cfg(pictures)]
    pub(crate) pictures: Vec<SourcePicture>,
    /// The `name` of the picture that is the cover.
    #[cfg(pictures)]
    pub(crate) cover: Option<String>,
    /// The table of contents of the source; empty when it has none.
    #[cfg(outlines)]
    pub(crate) outline: Vec<OutlineEntry>,
    /// What the front end could not keep.
    pub(crate) warnings: Vec<Warning>,
}

/// The metadata of a source. A value the format does not allow is left out
/// by the assembler, with a warning.
#[derive(Debug, Default)]
pub(crate) struct DraftMeta {
    pub(crate) title: Option<String>,
    /// Language tags; the first is the main language.
    pub(crate) languages: Vec<String>,
    /// The identifier of the book in its source.
    pub(crate) id: Option<String>,
    pub(crate) authors: Vec<String>,
    pub(crate) translators: Vec<String>,
    pub(crate) series: Option<String>,
    /// The position in the series, as a decimal number.
    pub(crate) series_index: Option<String>,
    pub(crate) publisher: Option<String>,
    /// A date in any form that starts with a year.
    pub(crate) date: Option<String>,
    pub(crate) isbns: Vec<String>,
    pub(crate) genres: Vec<String>,
    /// Plain text; paragraphs are separated by an empty line.
    pub(crate) annotation: Option<String>,
}

/// One top-level division of a source.
#[derive(Debug)]
pub(crate) struct Section {
    /// The source's name for the section.
    pub(crate) name: String,
    /// Its text.
    pub(crate) markdown: String,
}

/// One picture of a source.
#[cfg(pictures)]
#[derive(Debug)]
pub(crate) struct SourcePicture {
    /// The source's name for the picture.
    pub(crate) name: String,
    /// The picture.
    pub(crate) data: PictureData,
}

/// A picture as a front end has it.
#[cfg(pictures)]
#[derive(Debug)]
pub(crate) enum PictureData {
    /// A file in any picture format.
    Encoded(Vec<u8>),
    /// Rendered pixels: four bytes for each, red, green, blue and alpha,
    /// row by row.
    #[cfg(feature = "pdf")]
    Rgba {
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    },
}

/// One entry of the table of contents of a source.
#[cfg(outlines)]
#[derive(Debug)]
pub(crate) struct OutlineEntry {
    /// Depth in the tree, starting at 1.
    pub(crate) level: u8,
    /// The title, as plain text.
    pub(crate) title: String,
    /// Where the entry leads, written like the destination of a link.
    pub(crate) target: String,
}
