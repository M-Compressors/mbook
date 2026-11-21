//! `FictionBook` 2.
//!
//! The document is read with `quick-xml` into a small tree of elements and
//! written as Markdown element by element, because XML gives a tree, not
//! text. Elements and attributes are matched by their local name: `l:href`,
//! `xlink:href` and `href` are one attribute, and elements may carry any
//! namespace prefix.
//!
//! - **Sections.** One section of the draft per top-level `section` of the
//!   main body, named `s1`, `s2`, and so on after their position. What the
//!   main body has in front of its first section (a title, epigraphs, a
//!   picture) is the section `front`; what it has between or after sections
//!   is `t1`, `t2`, and so on. A body named `notes` or `comments` is the
//!   source of footnote texts and produces no section. The main body is the
//!   first body that is not such a body. A body or a table without content
//!   is empty, not an error; a book without any content is
//!   [`Error::Empty`](crate::Error::Empty).
//! - **Order.** Everything a section holds, its own blocks and its
//!   subsections, is written in the order of the document.
//! - **Text.** Titles are headings of the depth of their section, a
//!   `subtitle` is a heading one level below it. Epigraphs and citations
//!   are block quotes, a poem is a paragraph for each stanza with a hard
//!   line break after every verse, a table is a table of the dialect.
//! - **White space.** As in XML mixed content, a run of white space in text
//!   is one space. A space stands between two inline elements, or between
//!   text and an inline element, exactly when the source has white space
//!   there; white space at either end of a block is dropped. Spaces at the
//!   ends of emphasis are written outside its markers.
//! - **Unknown elements.** What the format does not have is not lost. Inside
//!   a line an unknown element is transparent and its text stays. At the
//!   level of blocks it is a container when it holds blocks and a paragraph
//!   otherwise.
//! - **Links and notes.** A link to an element of the book leads to the
//!   section that holds the element; every element with an `id` is marked
//!   in front of its block. A link of type `note` is a footnote, defined at
//!   the end of the section that refers to it.
//! - **Pictures.** Every `binary` is a picture, named by its `id`; the image
//!   of the cover page is the cover. A `binary` without content is reported
//!   as dropped, like one whose base64 is damaged.
//! - **Metadata.** Values are taken as written, trimmed: a genre is the text
//!   of its element, the number of a sequence is the text of the attribute
//!   (an empty one gives no position), and the core validates them.
//! - **Containers.** A source that starts like a ZIP archive is read as a
//!   `.fb2.zip`: the first entry whose name ends in `.fb2` is the book.
//!
//! The text of the document is decoded from the encoding of its byte order
//! mark or its declaration, `windows-1251`, `koi8-r` and UTF-16 among them.
//! Malformed XML and a root element other than `FictionBook` are source
//! errors. Elements nested more deeply than 256 levels are a limit error,
//! and so is a picture above the limits of the options.

mod meta;
mod text;
mod tree;

use crate::draft::{Draft, PictureData, Section, SourcePicture};
use crate::error::{Error, Result};
use crate::format::Format;
use crate::options::ConvertOptions;
use crate::report::{Warning, WarningKind};
use base64::Engine as _;
use base64::alphabet::STANDARD;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use mbook_core::Limits;
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};
use text::Renderer;
use tree::{Binary, Document, Element};
use zip::ZipArchive;

/// Names of bodies that hold the texts of notes and not the book.
const NOTE_BODIES: [&str; 2] = ["notes", "comments"];

/// The name of the root element.
const ROOT: &str = "FictionBook";

/// Decodes the base64 of a picture, with or without padding.
const LENIENT: GeneralPurpose = GeneralPurpose::new(
    &STANDARD,
    GeneralPurposeConfig::new()
        .with_decode_padding_mode(DecodePaddingMode::Indifferent)
        .with_decode_allow_trailing_bits(true),
);

/// Reads a `FictionBook` 2 document, plain or in a ZIP archive, into a draft.
pub(crate) fn read(source: &[u8], options: &ConvertOptions) -> Result<Draft> {
    let limits = &options.limits.book;
    let unpacked;
    let xml = if source.starts_with(b"PK\x03\x04") {
        unpacked = unpack(source, limits)?;
        unpacked.as_slice()
    } else {
        source
    };
    let document = tree::parse(xml, limits)?;
    if document.root.name != ROOT {
        return Err(Error::source(
            Format::Fb2,
            format!("the root element is `{}`, not `{ROOT}`", document.root.name),
        ));
    }
    Ok(convert(&document))
}

/// The `.fb2` file of an archive.
fn unpack(source: &[u8], limits: &Limits) -> Result<Vec<u8>> {
    let failed = |error| Error::source(Format::Fb2, error);
    let mut archive = ZipArchive::new(Cursor::new(source)).map_err(failed)?;
    let index = (0..archive.len())
        .find(|&index| {
            archive
                .name_for_index(index)
                .and_then(std::result::Result::ok)
                .is_some_and(|name| name.to_ascii_lowercase().ends_with(".fb2"))
        })
        .ok_or_else(|| Error::source(Format::Fb2, "the archive holds no .fb2 file"))?;
    if archive.by_index_raw(index).map_err(failed)?.encrypted() {
        return Err(Error::Protected(Format::Fb2));
    }
    // The entry is the whole book, with its pictures: it is bounded like
    // everything unpacked together.
    let max = limits.max_file_bytes;
    let too_large = || Error::Limit("the .fb2 file in the archive is larger than the limit");
    let entry = archive.by_index(index).map_err(failed)?;
    if entry.size() > max {
        return Err(too_large());
    }
    let mut data = Vec::new();
    entry
        .take(max.saturating_add(1))
        .read_to_end(&mut data)
        .map_err(|error| Error::source(Format::Fb2, error))?;
    if u64::try_from(data.len()).map_or(true, |length| length > max) {
        return Err(too_large());
    }
    Ok(data)
}

/// Turns a parsed document into a draft.
fn convert(document: &Document) -> Draft {
    let root = &document.root;
    let (pictures, mut warnings) = pictures(&document.binaries);
    let names: HashSet<String> = pictures
        .iter()
        .map(|picture| picture.name.clone())
        .collect();
    let cover = root
        .child("description")
        .and_then(|description| description.child("title-info"))
        .and_then(|info| info.child("coverpage"))
        .and_then(|covers| covers.child("image"))
        .and_then(|image| image.attr("href"))
        .and_then(|href| href.trim().strip_prefix('#'))
        .filter(|id| names.contains(*id))
        .map(str::to_owned);

    let mut notes = HashMap::new();
    for body in root.named("body").filter(|body| is_notes(body)) {
        collect_notes(body, &mut notes);
    }
    let mut renderer = Renderer::new(notes, names);
    let texts = root
        .named("body")
        .find(|body| !is_notes(body))
        .map(|body| renderer.render_body(body))
        .unwrap_or_default();
    let sections = texts
        .into_iter()
        .filter(|(_, markdown)| !markdown.trim().is_empty())
        .map(|(name, markdown)| Section {
            markdown: renderer.resolve(&markdown),
            name,
        })
        .collect();
    warnings.append(&mut renderer.warnings);
    #[cfg_attr(
        not(outlines),
        expect(
            clippy::needless_update,
            reason = "builds with outlines have one more field"
        )
    )]
    let draft = Draft {
        meta: meta::read(root),
        sections,
        pictures,
        cover,
        warnings,
        ..Draft::default()
    };
    draft
}

/// Tells whether a body holds the texts of notes.
fn is_notes(body: &Element) -> bool {
    body.attr("name").is_some_and(|name| {
        NOTE_BODIES
            .iter()
            .any(|note| name.trim().eq_ignore_ascii_case(note))
    })
}

/// The sections of a body of notes that have an identifier, nested ones
/// included.
fn collect_notes<'a>(parent: &'a Element, notes: &mut HashMap<String, &'a Element>) {
    for section in parent.named("section") {
        if let Some(id) = section.attr("id") {
            notes.entry(id.trim().to_owned()).or_insert(section);
        }
        collect_notes(section, notes);
    }
}

/// The pictures of a document, in the order of the document.
///
/// The sizes were checked against the limits while the document was read.
/// A picture whose base64 is damaged, or that has none, is left out, with a
/// warning.
fn pictures(binaries: &[Binary]) -> (Vec<SourcePicture>, Vec<Warning>) {
    let mut pictures = Vec::new();
    let mut warnings = Vec::new();
    for binary in binaries {
        match LENIENT.decode(&binary.data) {
            Ok(data) if !data.is_empty() => pictures.push(SourcePicture {
                name: binary.id.clone(),
                data: PictureData::Encoded(data),
            }),
            _ => warnings.push(Warning::whole_book(WarningKind::PictureDropped(
                binary.id.clone(),
            ))),
        }
    }
    (pictures, warnings)
}
