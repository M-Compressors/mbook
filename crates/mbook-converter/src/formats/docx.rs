//! Word documents (`.docx`).
//!
//! The document is read by `undoc`, which turns it into a model of blocks
//! and renders that model as Markdown. This module chooses the rendering
//! options, then reshapes the result into a draft:
//!
//! - **Markdown.** Tables are pipe tables, never HTML, and the front matter
//!   and the page breaks of `undoc` are off. Text is escaped by `undoc`.
//!   What it still writes as raw HTML is underline (`<u>`), superscript
//!   (`<sup>`), subscript (`<sub>`) and the line break between the
//!   paragraphs of a table cell (`<br>`); the assembler removes the tags and
//!   reports them.
//! - **Sections.** The Markdown is parsed, and it is cut in front of every
//!   top-level (`#`) heading: the text before the first one is a section,
//!   then there is one section per heading. A document without a top-level
//!   heading is one section. Sections are named `s1`, `s2`, and so on. A
//!   heading is a paragraph whose style has an outline level (or is
//!   `Title`); the cut follows the parsed tree, so a `#` inside a code block
//!   is not a heading.
//! - **Pictures.** Every picture that the text shows is handed over as it is
//!   stored in the document. Its name is the destination `undoc` writes for
//!   it, percent-decoded; names are made unique first. Pictures the text
//!   does not show (a page background, the SVG original of a picture) are
//!   left out.
//! - **Footnotes and endnotes.** `undoc` writes them as GFM footnotes and
//!   collects every definition at the end of the document. Each definition
//!   is moved to the section that refers to it, and copied when several
//!   sections do. A note is plain text: its formatting is lost in `undoc`.
//! - **Lists.** `undoc` reads the numbering of Word and indents an item by
//!   two spaces for each of its levels. Markdown reads an item inside
//!   another only when it is indented as far as the text of that item,
//!   past its marker: two spaces past `- `, three past `1. `, four past
//!   `10. `. So the level of every item is set first to the one whose two
//!   spaces reach the text of its parent, and an item under a number is
//!   not read as an item of the list of its parent. An item that is more
//!   than one level deeper than its parent is nested one level, as
//!   Markdown reads it. A paragraph with text that is not an item, or any
//!   other block, ends the lists before it; an empty paragraph does not.
//! - **Links.** External links stay as `undoc` writes them. A link to a
//!   bookmark is plain text in `undoc`, which does not report where a
//!   bookmark is, so there are no links into the book and no marks.
//! - **Metadata.** The core properties that `undoc` reads: title, creator
//!   (split at `;`), subject and description (the annotation) and keywords
//!   (the genres). The language is `dc:language` of `docProps/core.xml`,
//!   read here because `undoc` does not keep it. Dates are not used.
//! - **Errors.** A password-protected document is an OLE container, and is
//!   [`Error::Protected`]. Anything else `undoc` reports is the source of an
//!   [`Error::Source`](crate::Error::Source).
//! - **Limits.** Before `undoc` unpacks anything, the sizes the archive
//!   declares are bounded: each entry by `max_resource_bytes` and all of
//!   them together by `max_file_bytes`. `undoc` itself refuses an entry that
//!   expands more than a hundredfold.

use crate::draft::{Draft, DraftMeta, PictureData, Section, SourcePicture};
use crate::error::{Error, Result};
use crate::format::Format;
use crate::markdown as md;
use crate::options::ConvertOptions;
use markdown::mdast::Node;
use markdown::{ParseOptions, to_mdast};
use mbook::Limits;
use percent_encoding::percent_decode_str;
use std::collections::HashSet;
use std::io::{Cursor, Read};
use undoc::docx::DocxParser;
use undoc::render::{self, RenderOptions, TableFallback};
use undoc::{Block, Document, ListInfo, ListType, Metadata, Paragraph, ResourceRole, Table};
use zip::ZipArchive;

/// The first bytes of an OLE container: an encrypted document, or a
/// document of Word 97 to 2003.
const OLE_MAGIC: [u8; 4] = [0xD0, 0xCF, 0x11, 0xE0];

/// Stands in for a tilde in the text while `undoc` renders it. A single
/// tilde marks strikethrough in the dialect of RFC 0001, and `undoc` does
/// not escape it, so each one is written as `\~` after the rendering.
const TILDE: char = '\u{FDD0}';

/// The part that holds the core properties.
const CORE_PROPERTIES: &str = "docProps/core.xml";

/// Reads a Word document into a draft.
pub(crate) fn read(source: &[u8], options: &ConvertOptions) -> Result<Draft> {
    if source.starts_with(&OLE_MAGIC) {
        return Err(refuse_container(source));
    }
    let language = inspect(source, &options.limits.book)?;
    let mut document = DocxParser::from_bytes(source.to_vec())
        .and_then(|mut parser| parser.parse())
        .map_err(|error| Error::source(Format::Docx, error))?;
    let names = name_pictures(&mut document);
    protect_tildes(&mut document);
    nest_lists(&mut document);
    let text = render::to_markdown(&document, &render_options())
        .map_err(|error| Error::source(Format::Docx, error))?
        .replace(TILDE, "\\~");
    let tree = to_mdast(&text, &ParseOptions::gfm())
        .map_err(|message| Error::source(Format::Docx, message.to_string()))?;

    let mut shown = HashSet::new();
    pictures_shown(&tree, &mut shown);
    let pictures = names
        .into_iter()
        .filter(|(name, _)| shown.contains(name))
        .filter_map(|(name, id)| {
            let resource = document.resources.remove(&id)?;
            let data = PictureData::Encoded(resource.data);
            Some(SourcePicture { name, data })
        })
        .collect();

    Ok(Draft {
        meta: meta_of(&document.metadata, language),
        sections: cut(&text, &tree),
        pictures,
        ..Draft::default()
    })
}

/// Says why an OLE container is not a Word document that can be read.
fn refuse_container(source: &[u8]) -> Error {
    match undoc::detect_format_from_bytes(source) {
        Err(undoc::Error::Encrypted) => Error::Protected(Format::Docx),
        Err(error) => Error::source(Format::Docx, error),
        Ok(kind) => Error::source(
            Format::Docx,
            format!("a {kind} is not a Word 2007 document"),
        ),
    }
}

/// How `undoc` renders Markdown for a book.
fn render_options() -> RenderOptions {
    RenderOptions::new()
        .with_table_fallback(TableFallback::Markdown)
        .with_frontmatter(false)
        .with_max_heading(6)
        .with_preserve_breaks(true)
}

/// Bounds the sizes the archive declares and reads the language out of the
/// core properties. A source that is not an archive is left to `undoc`,
/// which tells what is wrong with it.
fn inspect(source: &[u8], limits: &Limits) -> Result<Option<String>> {
    let Ok(mut archive) = ZipArchive::new(Cursor::new(source)) else {
        return Ok(None);
    };
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let size = archive.by_index_data(index).map_or(0, |entry| entry.size());
        if size > limits.max_resource_bytes {
            return Err(Error::Limit(
                "a part of the document is larger than the resource limit",
            ));
        }
        total = total.saturating_add(size);
    }
    if total > limits.max_file_bytes {
        return Err(Error::Limit(
            "the document unpacks to more than the file limit",
        ));
    }
    let mut xml = Vec::new();
    let read = archive.by_name(CORE_PROPERTIES).ok().and_then(|part| {
        part.take(limits.max_resource_bytes)
            .read_to_end(&mut xml)
            .ok()
    });
    Ok(read.and_then(|_| language_in(&String::from_utf8_lossy(&xml))))
}

/// The text of `dc:language` in the XML of the core properties.
fn language_in(xml: &str) -> Option<String> {
    let (_, rest) = xml.split_once("<dc:language>")?;
    let (language, _) = rest.split_once("</dc:language>")?;
    let language = language.trim();
    (!language.is_empty()).then(|| language.to_owned())
}

/// The metadata `undoc` read, and the language read here.
fn meta_of(properties: &Metadata, language: Option<String>) -> DraftMeta {
    let text = |value: &Option<String>| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let annotation: Vec<String> = [&properties.subject, &properties.description]
        .into_iter()
        .filter_map(text)
        .collect();
    DraftMeta {
        title: text(&properties.title),
        languages: language.into_iter().collect(),
        authors: properties
            .author
            .iter()
            .flat_map(|authors| authors.split(';'))
            .map(str::trim)
            .filter(|author| !author.is_empty())
            .map(str::to_owned)
            .collect(),
        genres: properties.keywords.clone(),
        annotation: (!annotation.is_empty()).then(|| annotation.join("\n\n")),
        ..DraftMeta::default()
    }
}

/// Gives every picture of the document a name that is unique and that
/// survives being written as a destination and read back, and returns the
/// names with the identifiers of the pictures.
///
/// `undoc` writes the file name of a picture as its destination. Two
/// pictures of one name (from different folders of the archive) would be
/// one picture in the text, and a name with a space or a percent sign would
/// not be read back as it is, so the name is made unique here and written
/// percent-encoded.
fn name_pictures(document: &mut Document) -> Vec<(String, String)> {
    let mut used = HashSet::new();
    let mut named = Vec::new();
    for (id, resource) in &mut document.resources {
        if resource.role != ResourceRole::Primary || !resource.is_image() {
            continue;
        }
        let original = resource.suggested_filename(id);
        let mut name = original.clone();
        let mut attempt = 1;
        while !used.insert(name.clone()) {
            attempt += 1;
            name = format!("{attempt}-{original}");
        }
        resource.filename = Some(md::encoded_path(&name));
        named.push((name, id.clone()));
    }
    named
}

/// Replaces every tilde of the text of the document by [`TILDE`]. Code and
/// addresses are left as they are: nothing is escaped in a code span, and an
/// address is not text.
fn protect_tildes(document: &mut Document) {
    fn paragraph(found: &mut Paragraph) {
        for run in &mut found.runs {
            if !run.style.code {
                run.text = run
                    .text
                    .replace(TILDE, "\u{FFFD}")
                    .replace('~', &TILDE.to_string());
            }
        }
    }
    fn table(found: &mut Table) {
        for cell in found.rows.iter_mut().flat_map(|row| &mut row.cells) {
            cell.content.iter_mut().for_each(paragraph);
            cell.nested_tables.iter_mut().for_each(table);
        }
    }
    for block in document
        .sections
        .iter_mut()
        .flat_map(|section| &mut section.content)
    {
        match block {
            Block::Paragraph(block) => paragraph(block),
            Block::Table(block) => table(block),
            Block::Note { content, .. } => content.iter_mut().for_each(paragraph),
            _ => {}
        }
    }
}

/// Sets the level of every item of a list to the one `undoc` indents it by
/// as far as Markdown needs to read it inside its parent item.
fn nest_lists(document: &mut Document) {
    for section in &mut document.sections {
        // The items an item can be inside: the level of Word of each and the
        // level its children are given.
        let mut parents: Vec<(u8, u8)> = Vec::new();
        for block in &mut section.content {
            let Block::Paragraph(paragraph) = block else {
                parents.clear();
                continue;
            };
            let Some(item) = &mut paragraph.list_info else {
                if !paragraph.images.is_empty() || !paragraph.plain_text().trim().is_empty() {
                    parents.clear();
                }
                continue;
            };
            while parents.last().is_some_and(|&(level, _)| level >= item.level) {
                parents.pop();
            }
            let level = parents.last().map_or(0, |&(_, children)| children);
            let children = level.saturating_add(marker_width(item).div_ceil(2));
            parents.push((item.level, children));
            item.level = level;
        }
    }
}

/// The width of the marker `undoc` writes in front of an item, with the
/// space after it.
fn marker_width(item: &ListInfo) -> u8 {
    match item.list_type {
        ListType::Bullet => 2,
        ListType::Numbered => {
            let digits = item.number.unwrap_or(1).checked_ilog10().unwrap_or(0) + 1;
            u8::try_from(digits).unwrap_or(u8::MAX).saturating_add(2)
        }
        ListType::None => 0,
    }
}

/// Collects the names of the pictures the text shows, from their
/// destinations.
fn pictures_shown(node: &Node, shown: &mut HashSet<String>) {
    if let Node::Image(picture) = node
        && let Ok(name) = percent_decode_str(&picture.url).decode_utf8()
    {
        shown.insert(name.into_owned());
    }
    for child in node.children().map(Vec::as_slice).unwrap_or_default() {
        pictures_shown(child, shown);
    }
}

/// What a section of the cut is made of.
#[derive(Default)]
struct Part<'a> {
    /// The text of its top-level blocks.
    blocks: Vec<&'a str>,
    /// The footnotes its text refers to.
    notes: HashSet<&'a str>,
}

/// Cuts the rendered text in front of every top-level heading, and puts
/// each footnote definition into the sections that refer to it.
fn cut(text: &str, tree: &Node) -> Vec<Section> {
    let mut parts: Vec<Part<'_>> = Vec::new();
    let mut definitions: Vec<(&str, &str)> = Vec::new();
    for block in tree.children().map(Vec::as_slice).unwrap_or_default() {
        if let Node::FootnoteDefinition(definition) = block {
            definitions.push((&definition.identifier, source_of(text, block)));
            continue;
        }
        let starts_section = matches!(block, Node::Heading(heading) if heading.depth == 1);
        if starts_section || parts.is_empty() {
            parts.push(Part::default());
        }
        if let Some(part) = parts.last_mut() {
            part.blocks.push(source_of(text, block));
            notes_referred(block, &mut part.notes);
        }
    }
    parts
        .into_iter()
        .enumerate()
        .map(|(index, part)| {
            let mut blocks = part.blocks;
            blocks.extend(
                definitions
                    .iter()
                    .filter(|(identifier, _)| part.notes.contains(identifier))
                    .map(|(_, definition)| *definition),
            );
            Section {
                name: format!("s{}", index + 1),
                markdown: format!("{}\n", blocks.join("\n\n")),
            }
        })
        .collect()
}

/// The text a node was parsed from.
fn source_of<'a>(text: &'a str, node: &Node) -> &'a str {
    node.position()
        .and_then(|position| text.get(position.start.offset..position.end.offset))
        .unwrap_or_default()
        .trim_end_matches('\n')
}

/// Collects the identifiers of the footnotes a node refers to.
fn notes_referred<'a>(node: &'a Node, notes: &mut HashSet<&'a str>) {
    if let Node::FootnoteReference(reference) = node {
        notes.insert(&reference.identifier);
    }
    for child in node.children().map(Vec::as_slice).unwrap_or_default() {
        notes_referred(child, notes);
    }
}

#[cfg(test)]
mod tests {
    use super::{cut, language_in, marker_width, meta_of};
    use markdown::{ParseOptions, to_mdast};
    use undoc::{ListInfo, ListType, Metadata};

    /// The names and the texts of the sections of Markdown.
    fn sections(text: &str) -> Vec<(String, String)> {
        let tree = to_mdast(text, &ParseOptions::gfm()).unwrap();
        let sections = cut(text, &tree);
        sections.into_iter().map(|s| (s.name, s.markdown)).collect()
    }

    #[test]
    fn language_is_the_text_of_the_element() {
        let xml = "<cp:coreProperties><dc:language> uk-UA </dc:language></cp:coreProperties>";
        assert_eq!(language_in(xml).as_deref(), Some("uk-UA"));
    }

    #[test]
    fn language_is_missing_when_the_element_is_absent_or_empty() {
        assert_eq!(language_in("<cp:coreProperties/>"), None);
        assert_eq!(language_in("<dc:language> </dc:language>"), None);
        assert_eq!(language_in("<dc:language>en"), None);
    }

    #[test]
    fn authors_are_split_at_semicolons_and_blanks_are_left_out() {
        let properties = Metadata {
            author: Some("Ann Lee; ;Bo Ray".to_owned()),
            title: Some("  ".to_owned()),
            ..Metadata::default()
        };
        let meta = meta_of(&properties, None);
        assert_eq!(meta.authors, ["Ann Lee", "Bo Ray"]);
        assert_eq!(meta.title, None);
        assert_eq!(meta.annotation, None);
    }

    #[test]
    fn subject_and_description_make_the_annotation() {
        let properties = Metadata {
            subject: Some("A subject".to_owned()),
            description: Some("A description".to_owned()),
            ..Metadata::default()
        };
        let meta = meta_of(&properties, Some("en".to_owned()));
        assert_eq!(
            meta.annotation.as_deref(),
            Some("A subject\n\nA description")
        );
        assert_eq!(meta.languages, ["en"]);
    }

    #[test]
    fn text_without_a_top_level_heading_is_one_section() {
        let found = sections("Text.\n\n## Sub\n\nMore.\n");
        assert_eq!(found, [("s1".into(), "Text.\n\n## Sub\n\nMore.\n".into())]);
    }

    #[test]
    fn text_before_the_first_heading_is_a_section_of_its_own() {
        let found = sections("Intro.\n\n# One\n\nA.\n\n# Two\n\nB.\n");
        let texts: Vec<_> = found.iter().map(|(_, text)| text.as_str()).collect();
        assert_eq!(texts, ["Intro.\n", "# One\n\nA.\n", "# Two\n\nB.\n"]);
        assert_eq!(found[2].0, "s3");
    }

    #[test]
    fn a_heading_inside_a_code_block_does_not_cut() {
        let found = sections("# One\n\n```\n# not a heading\n```\n");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_definition_goes_to_the_section_that_refers_to_it() {
        let text = "# One\n\nA[^1].\n\n# Two\n\nB[^2] and C[^1].\n\n[^1]: First.\n\n[^2]: Second.\n\n[^3]: Nobody.\n";
        let found = sections(text);
        assert_eq!(found[0].1, "# One\n\nA[^1].\n\n[^1]: First.\n");
        assert_eq!(
            found[1].1,
            "# Two\n\nB[^2] and C[^1].\n\n[^1]: First.\n\n[^2]: Second.\n"
        );
    }

    #[test]
    fn a_marker_is_as_wide_as_undoc_writes_it_with_its_space() {
        let width = |list_type, number| {
            marker_width(&ListInfo {
                list_type,
                number,
                ..ListInfo::default()
            })
        };
        assert_eq!(width(ListType::Bullet, None), 2);
        assert_eq!(width(ListType::Numbered, None), 3);
        assert_eq!(width(ListType::Numbered, Some(9)), 3);
        assert_eq!(width(ListType::Numbered, Some(10)), 4);
        assert_eq!(width(ListType::Numbered, Some(0)), 3);
        assert_eq!(width(ListType::None, None), 0);
    }

    #[test]
    fn no_text_makes_no_section() {
        assert_eq!(sections(""), []);
    }
}
