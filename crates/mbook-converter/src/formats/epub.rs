//! EPUB 2 and 3.
//!
//! The package is read by `rbook`; the entries of the archive that hold text
//! and pictures are unpacked here, within the limits of the options, so that
//! no size a file declares is trusted.
//!
//! - **Sections.** One per item of the spine, in spine order, `linear="no"`
//!   items included at their place. An XHTML item is converted by
//!   [`crate::xhtml`] as a whole, not cut at its headings. The section is
//!   named by the path of the item in the archive, with `#` and `:` replaced
//!   by `_`. An image in the spine is a section that shows the picture as a
//!   figure. An SVG in the spine is converted like XHTML. A spine item that
//!   is missing from the archive, has no manifest entry or has a media type
//!   that is neither text nor a picture is left out, as is one that
//!   converts to nothing.
//! - **Text.** XHTML is UTF-8 unless a byte order mark says UTF-16; bytes
//!   that are not text become U+FFFD.
//! - **Links.** The `href` of a link is resolved against the folder of the
//!   file it is written in. A link to a spine item leads to its section, and
//!   to an element of it when the element exists; a link to an element that
//!   does not exist leads to the section. An address with a scheme is
//!   written as it is, and anything else, a file that is not a section
//!   included, is left to the assembler, which drops it with a warning.
//! - **Pictures.** Every manifest item with an image media type is a picture,
//!   named by its path in the archive, with the bytes as the archive has them.
//! - **Cover.** The cover image of the manifest (`cover-image` of EPUB 3, or
//!   `<meta name="cover">` of EPUB 2) is the cover. A spine item that shows
//!   that picture and nothing else is left out, so the cover is not shown
//!   twice.
//! - **Contents.** The navigation document of EPUB 3, otherwise the NCX of
//!   EPUB 2, is the outline. An entry without an address, and an entry that
//!   leads to a spine item that was left out because it is the cover or has
//!   no text, is skipped.
//! - **Landmarks.** The `landmarks` navigation of EPUB 3, otherwise the
//!   `<guide>` of EPUB 2, gives the landmarks. An entry is a landmark when it
//!   has an address and its `epub:type` (the first of several values that
//!   maps) or its guide `type` maps to a kind of RFC 0001: `bodymatter`,
//!   `text` and `start` to `start`, `toc` to `contents`, `titlepage` to
//!   `title-page`, `loi` and `lot` to `list-of-figures` and `list-of-tables`,
//!   `copyright-page` to `copyright`, the notes types to `notes`, and the
//!   others to the kind of the same name (see `landmarks::kind_of`). The
//!   cover and a type without a kind give no landmark and no warning. The
//!   title is the trimmed label, the target is resolved like an entry of the
//!   contents, and an entry that leads to a left out spine item is skipped.
//!   The assembler keeps one `start` and drops a target that leads nowhere
//!   with a warning.
//! - **Metadata.** `dc:title`, `dc:language`, the unique `dc:identifier`,
//!   `dc:creator`, `dc:contributor` with the role `trl`, `dc:publisher`, the
//!   publication `dc:date`, `dc:subject`, `dc:description` as plain text,
//!   `belongs-to-collection` with `group-position` or `calibre:series` with
//!   `calibre:series_index`, and the identifiers that are ISBNs. See
//!   [`meta`].
//!
//! A book with `META-INF/rights.xml`, an encrypted entry of the archive or an
//! `encryption.xml` that names anything but the obfuscation of fonts is
//! protected. Entries are unpacked through the limits whatever size their
//! headers declare; the package files that `rbook` reads are bounded by the
//! limit of a resource through their declared sizes.

mod archive;
mod landmarks;
mod meta;
mod paths;

use crate::draft::{Draft, OutlineEntry, PictureData, Section, SourcePicture};
use crate::error::{Error, Result};
use crate::format::Format;
use crate::markdown;
use crate::options::ConvertOptions;
use crate::report::Warning;
use crate::xhtml;
use ::markdown::mdast::Node;
use ::markdown::{ParseOptions, to_mdast};
use archive::Archive;
use rbook::Epub;
use std::collections::{HashMap, HashSet};
use std::io::Cursor;

/// What a spine item holds, by its media type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// XHTML, HTML or SVG: converted to Markdown.
    Text,
    /// A raster picture: shown as a figure.
    Picture,
    /// Anything else: left out.
    Other,
}

/// One item of the spine that is a section.
struct Item {
    /// Its path in the archive.
    path: String,
    /// The name of its section.
    name: String,
    kind: Kind,
    /// The decoded text, for `Kind::Text`.
    text: String,
}

/// Where links lead: the sections that were made and the pictures that were
/// read.
struct Targets {
    /// By the path of the spine item in the archive.
    sections: HashMap<String, Target>,
    /// The paths of the pictures.
    pictures: HashSet<String>,
}

/// A section as a link target.
struct Target {
    name: String,
    /// The identifiers of its elements.
    ids: HashSet<String>,
}

impl Target {
    /// The destination of a link to an element of the section, or to the
    /// section when it has no such element.
    fn destination(&self, id: Option<&str>) -> String {
        markdown::destination(&self.name, id.filter(|id| self.ids.contains(*id)))
    }
}

impl Targets {
    /// The destination of the `href` of a link written in the file
    /// `document`; `None` when it leads to no section.
    fn link(&self, document: &str, href: &str) -> Option<String> {
        let reference = paths::resolve(document, href)?;
        let target = self.sections.get(&reference.path)?;
        Some(target.destination(reference.fragment.as_deref()))
    }

    /// The name of the picture that the `src` written in the file `document`
    /// leads to.
    fn picture(&self, document: &str, src: &str) -> Option<String> {
        let reference = paths::resolve(document, src)?;
        self.pictures
            .contains(&reference.path)
            .then_some(reference.path)
    }
}

/// Reads an EPUB into a draft with one section per item of the spine.
///
/// # Errors
///
/// The archive or the package cannot be read ([`Error::Source`]), the book is
/// protected ([`Error::Protected`]), or a limit is exceeded
/// ([`Error::Limit`]).
pub(crate) fn read(source: &[u8], options: &ConvertOptions) -> Result<Draft> {
    let limits = options.limits.book;
    let mut archive = Archive::open(source, limits)?;
    // `rbook` wants a source that it owns.
    let epub = Epub::options()
        .read(Cursor::new(source.to_vec()))
        .map_err(|error| Error::source(Format::Epub, error))?;

    let pictures = read_pictures(&epub, &mut archive)?;
    let picture_names: HashSet<String> = pictures.iter().map(|p| p.name.clone()).collect();
    let cover = epub
        .manifest()
        .cover_image()
        .and_then(|entry| paths::resolve("", entry.href().as_str()))
        .map(|reference| reference.path)
        .filter(|path| picture_names.contains(path));
    let mut items = read_items(&epub, &mut archive, &picture_names, limits.max_resources)?;

    // The first pass finds out which elements every section has and which
    // spine items show nothing but the cover; the links of the second say so.
    let mut targets = Targets {
        sections: HashMap::new(),
        pictures: picture_names,
    };
    let mut skipped = HashSet::new();
    for item in &mut items {
        let first = first_pass(item, &targets, cover.as_deref())?;
        match first {
            Some(ids) => {
                targets.sections.insert(
                    item.path.clone(),
                    Target {
                        name: item.name.clone(),
                        ids,
                    },
                );
            }
            None => {
                skipped.insert(item.path.clone());
            }
        }
    }

    let mut draft = Draft::default();
    for item in &items {
        if skipped.contains(&item.path) {
            continue;
        }
        let markdown = if item.kind == Kind::Picture {
            let figure = markdown::image("", &markdown::encoded_path(&item.path), None);
            format!("{figure}\n")
        } else {
            let document = xhtml::convert(
                Format::Epub,
                &item.text,
                |href| targets.link(&item.path, href),
                |src| Ok(targets.picture(&item.path, src)),
            )?;
            draft.warnings.extend(
                document
                    .warnings
                    .into_iter()
                    .map(|kind| Warning::new(kind, &item.name)),
            );
            document.markdown
        };
        draft.sections.push(Section {
            name: item.name.clone(),
            markdown,
        });
    }
    draft.meta = meta::read(&epub);
    draft.pictures = pictures;
    draft.cover = cover;
    draft.outline = outline(&epub, &targets, &skipped);
    draft.landmarks = landmarks::read(&epub, &targets, &skipped);
    Ok(draft)
}

/// The first look at an item. `Some` with the identifiers of its elements
/// when it is a section, `None` when it shows nothing or only the cover.
fn first_pass(
    item: &Item,
    targets: &Targets,
    cover: Option<&str>,
) -> Result<Option<HashSet<String>>> {
    if item.kind == Kind::Picture {
        return Ok((cover != Some(item.path.as_str())).then(HashSet::new));
    }
    let document = xhtml::convert(
        Format::Epub,
        &item.text,
        |_| None,
        |src| Ok(targets.picture(&item.path, src)),
    )?;
    if document.markdown.trim().is_empty()
        || cover.is_some_and(|cover| shows_only(&document.markdown, cover))
    {
        return Ok(None);
    }
    let ids = document.parts().into_iter().flat_map(|part| part.ids);
    Ok(Some(ids.collect()))
}

/// Tells whether Markdown shows one picture and nothing else: the marks of
/// its elements do not count.
fn shows_only(markdown: &str, picture: &str) -> bool {
    let destination = markdown::encoded_path(picture);
    if !markdown.contains(&destination) {
        return false;
    }
    // Parsing fails only for MDX syntax, which the dialect does not enable.
    let Ok(root) = to_mdast(markdown, &ParseOptions::gfm()) else {
        return false;
    };
    let mut blocks = root
        .children()
        .into_iter()
        .flatten()
        .filter(|node| !matches!(node, Node::Definition(_)));
    let (Some(Node::Paragraph(paragraph)), None) = (blocks.next(), blocks.next()) else {
        return false;
    };
    match paragraph.children.as_slice() {
        [Node::Image(image)] => image.url == destination,
        [Node::Link(link)] => {
            matches!(link.children.as_slice(), [Node::Image(image)] if image.url == destination)
        }
        _ => false,
    }
}

/// Unpacks the pictures of the manifest.
///
/// The number of pictures is bounded by the number of entries of the
/// archive, which [`Archive::open`] has checked against the same limit.
fn read_pictures(epub: &Epub, archive: &mut Archive<'_>) -> Result<Vec<SourcePicture>> {
    let mut pictures = Vec::new();
    let mut seen = HashSet::new();
    for entry in epub.manifest() {
        if !entry
            .media_type()
            .to_ascii_lowercase()
            .starts_with("image/")
        {
            continue;
        }
        let Some(reference) = paths::resolve("", entry.href().as_str()) else {
            continue;
        };
        if !seen.insert(reference.path.clone()) {
            continue;
        }
        if let Some(bytes) = archive.read(&reference.path)? {
            pictures.push(SourcePicture {
                name: reference.path,
                data: PictureData::Encoded(bytes),
            });
        }
    }
    Ok(pictures)
}

/// Unpacks the items of the spine, in spine order, and names their sections.
fn read_items(
    epub: &Epub,
    archive: &mut Archive<'_>,
    pictures: &HashSet<String>,
    max_resources: usize,
) -> Result<Vec<Item>> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let mut used = HashSet::new();
    for (count, entry) in epub.spine().iter().enumerate() {
        if count >= max_resources {
            return Err(Error::Limit("the spine has too many items"));
        }
        let Some(manifest) = entry.manifest_entry() else {
            continue;
        };
        let kind = kind(manifest.media_type());
        let Some(reference) = paths::resolve("", manifest.href().as_str()) else {
            continue;
        };
        let path = reference.path;
        if kind == Kind::Other || !seen.insert(path.clone()) {
            continue;
        }
        let text = match kind {
            Kind::Picture if pictures.contains(&path) => String::new(),
            Kind::Picture | Kind::Other => continue,
            Kind::Text => match archive.read(&path)? {
                Some(bytes) => decode(&bytes),
                None => continue,
            },
        };
        let name = unique_name(&paths::section_name(&path), &mut used);
        items.push(Item {
            path,
            name,
            kind,
            text,
        });
    }
    Ok(items)
}

/// What a media type says an item is.
fn kind(media_type: &str) -> Kind {
    let media = media_type.trim().to_ascii_lowercase();
    if media.starts_with("image/") && !media.contains("svg") {
        Kind::Picture
    } else if media.starts_with("text/") || media.contains("xml") || media.contains("html") {
        Kind::Text
    } else {
        Kind::Other
    }
}

/// A section name that differs from every name before it.
fn unique_name(base: &str, used: &mut HashSet<String>) -> String {
    let mut name = base.to_owned();
    let mut number = 1_u32;
    while !used.insert(name.clone()) {
        number += 1;
        name = format!("{base}~{number}");
    }
    name
}

/// The text of a file: UTF-16 when a byte order mark says so, otherwise
/// UTF-8. Bytes that are not text become U+FFFD.
fn decode(bytes: &[u8]) -> String {
    match bytes {
        [0xff, 0xfe, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xfe, 0xff, rest @ ..] => utf16(rest, u16::from_be_bytes),
        [0xef, 0xbb, 0xbf, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// UTF-16 text with the given byte order; a stray last byte is dropped.
fn utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
    let (pairs, _) = bytes.as_chunks::<2>();
    char::decode_utf16(pairs.iter().copied().map(unit))
        .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// Where an entry of the contents or of the landmarks with this `href`
/// leads, written like the destination of a link; `None` when it leads to a
/// spine item that was left out. An address that leads to no section is
/// passed on as written, for the assembler to drop with a warning.
fn destination(targets: &Targets, skipped: &HashSet<String>, href: &str) -> Option<String> {
    match paths::resolve("", href) {
        Some(reference) if skipped.contains(&reference.path) => None,
        Some(reference) => Some(match targets.sections.get(&reference.path) {
            Some(target) => target.destination(reference.fragment.as_deref()),
            None => href.to_owned(),
        }),
        None => Some(href.to_owned()),
    }
}

/// The table of contents of the book as an outline of the draft.
fn outline(epub: &Epub, targets: &Targets, skipped: &HashSet<String>) -> Vec<OutlineEntry> {
    let Some(contents) = epub.toc().contents() else {
        return Vec::new();
    };
    contents
        .flatten()
        .filter_map(|entry| {
            let target = destination(targets, skipped, entry.href()?.as_str())?;
            Some(OutlineEntry {
                level: u8::try_from(entry.depth()).unwrap_or(u8::MAX).max(1),
                title: entry.label().trim().to_owned(),
                target,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Kind, decode, kind, shows_only, unique_name};
    use std::collections::HashSet;

    #[test]
    fn a_media_type_tells_text_from_pictures() {
        assert_eq!(kind("application/xhtml+xml"), Kind::Text);
        assert_eq!(kind("text/html"), Kind::Text);
        assert_eq!(kind("Image/SVG+xml"), Kind::Text);
        assert_eq!(kind("image/jpeg"), Kind::Picture);
        assert_eq!(kind("application/pdf"), Kind::Other);
        assert_eq!(kind(""), Kind::Other);
    }

    #[test]
    fn names_are_made_unique() {
        let mut used = HashSet::new();
        assert_eq!(unique_name("a.xhtml", &mut used), "a.xhtml");
        assert_eq!(unique_name("a.xhtml", &mut used), "a.xhtml~2");
        assert_eq!(unique_name("a.xhtml", &mut used), "a.xhtml~3");
    }

    #[test]
    fn text_is_decoded_by_its_byte_order_mark() {
        let little: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain("Hé".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        let big: Vec<u8> = [0xfe, 0xff]
            .into_iter()
            .chain("Hé".encode_utf16().flat_map(u16::to_be_bytes))
            .chain([0x00])
            .collect();
        assert_eq!(decode(&little), "Hé");
        assert_eq!(decode(&big), "Hé");
        assert_eq!(decode(b"\xef\xbb\xbfHi"), "Hi");
        assert_eq!(decode("Привет".as_bytes()), "Привет");
        assert_eq!(decode(b"a\xffb"), "a\u{fffd}b");
        assert_eq!(decode(&[0xff, 0xfe, 0x00, 0xd8]), "\u{fffd}");
    }

    #[test]
    fn a_page_of_one_picture_is_recognised_by_its_markdown() {
        let picture = "OEBPS/img/cover 1.jpg";
        let only = "[cover]: #cover\n\n![Cover](OEBPS/img/cover%201.jpg)\n";
        assert!(shows_only(only, picture));
        assert!(shows_only(
            "[![](OEBPS/img/cover%201.jpg)](x.md)\n",
            picture
        ));
        assert!(!shows_only(
            "Title\n\n![Cover](OEBPS/img/cover%201.jpg)\n",
            picture
        ));
        assert!(!shows_only("![A](OEBPS/img/cover%201.jpg) text\n", picture));
        assert!(!shows_only("![A](other.jpg)\n", picture));
        assert!(!shows_only(
            "![A](OEBPS/img/cover%201.jpg)\n\n![B](b.jpg)\n",
            picture
        ));
        assert!(!shows_only("", picture));
    }
}
