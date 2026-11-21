//! PDF.
//!
//! `unpdf` reads the file. What it makes of it is mapped as follows.
//!
//! - **A document with text** is rendered to Markdown by `unpdf` with Markdown
//!   tables only (never the HTML fallback), without front matter and without
//!   page markers. Its standard cleanup stays on: Unicode normalisation,
//!   ligatures, hyphenation at the end of a line, line breaks inside a
//!   paragraph and white space are repaired, which is what a reader of a book
//!   wants from text that was laid out line by line. Its escaping of Markdown
//!   characters stays on too, and no table is written as HTML, so the text
//!   holds no raw HTML.
//! - **Sections** are cut at the first level of the document outline: one
//!   section for the pages from each entry to the next, and one in front of
//!   the first entry. The pages of a section are rendered on their own. A
//!   document without an outline is cut at its top-level headings, and one
//!   with neither is one section.
//! - **The outline** becomes the table of contents. An entry of the first
//!   level leads to its section. A deeper entry leads to the heading of its
//!   section that has the same text, which gets a mark; without such a
//!   heading it leads to the section.
//! - **Pictures** are the images `unpdf` extracts, as the file has them. The
//!   name of a picture is the identifier `unpdf` writes in the Markdown.
//! - **A scanned document**, one without any extractable text, is one section
//!   in which every page is a figure: each page is painted to pixels at
//!   about 150 dpi, less when the page would have more pixels than the limit
//!   allows. The first page is the cover. Such a document has no table of
//!   contents: its pages are not headings.
//! - **Metadata** is the title, the author (several at `;`), the keywords as
//!   genres (at `,` and `;`) and the subject as the annotation. A date of
//!   creation is not a date of publication, and a PDF has no language or
//!   identifier the format trusts.
//!
//! An encrypted document is [`Error::Protected`]; `unpdf` opens one whose user
//! password is empty, and that is not protection.

use crate::draft::{Draft, DraftMeta, OutlineEntry, PictureData, Section, SourcePicture};
use crate::error::{Error, Result};
use crate::format::Format;
use crate::markdown as md;
use crate::options::{ConvertLimits, ConvertOptions};
use markdown::mdast::Node;
use markdown::{ParseOptions, to_mdast};
use std::ops::Range;
use unpdf::parser::raster::{PageRegion, PageSize, RasterOptions};
use unpdf::render::to_markdown;
use unpdf::{
    Document, Metadata, PageMarkerStyle, PageSelection, ParseOptions as PdfParseOptions, PdfParser,
    RenderOptions, TableFallback,
};

/// The resolution a scanned page is painted at, in dots per inch.
const PAGE_DPI: f32 = 150.0;

/// How many times the resolution of a page is reduced before the page is
/// given up as too large for the limit.
const SHRINK_STEPS: usize = 400;

/// The name of the section that holds the pages of a scanned document.
const SCAN_SECTION: &str = "pages";

/// Reads a PDF into a draft.
///
/// # Errors
///
/// [`Error::Protected`] for an encrypted document, [`Error::Source`] for what
/// `unpdf` reports, [`Error::Limit`] for a scanned document with more pages
/// than the limits allow.
pub(crate) fn read(source: &[u8], options: &ConvertOptions) -> Result<Draft> {
    let parse = PdfParseOptions::new().with_resources(true);
    let parser = PdfParser::from_bytes_with_options(source, parse).map_err(convert_error)?;
    let document = parser.parse().map_err(convert_error)?;
    let meta = metadata(&document.metadata);
    if document.pages.is_empty() {
        return Ok(Draft {
            meta,
            ..Draft::default()
        });
    }
    if has_text(&document) {
        text_document(document, meta, options)
    } else {
        scanned_document(&parser, &document, meta, options)
    }
}

/// What the caller is told of an error of `unpdf`.
fn convert_error(error: unpdf::Error) -> Error {
    match error {
        unpdf::Error::Encrypted | unpdf::Error::InvalidPassword => Error::Protected(Format::Pdf),
        other => Error::source(Format::Pdf, other),
    }
}

/// Tells whether any page of the document has text.
fn has_text(document: &Document) -> bool {
    document
        .pages
        .iter()
        .any(|page| !page.plain_text().trim().is_empty())
}

/// The metadata a PDF has that the format trusts.
fn metadata(meta: &Metadata) -> DraftMeta {
    DraftMeta {
        title: value(meta.title.as_ref()).map(str::to_owned),
        authors: value(meta.author.as_ref())
            .map(|author| split(author, &[';']))
            .unwrap_or_default(),
        genres: value(meta.keywords.as_ref())
            .map(|keywords| split(keywords, &[',', ';']))
            .unwrap_or_default(),
        annotation: value(meta.subject.as_ref()).map(str::to_owned),
        ..DraftMeta::default()
    }
}

/// A value of the metadata, trimmed; `None` when it is absent or blank.
fn value(text: Option<&String>) -> Option<&str> {
    text.map(String::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// The non-empty parts of a text between separators, trimmed.
fn split(text: &str, separators: &[char]) -> Vec<String> {
    text.split(separators)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A scanned document: one section in which every page is a figure.
fn scanned_document(
    parser: &PdfParser,
    document: &Document,
    meta: DraftMeta,
    options: &ConvertOptions,
) -> Result<Draft> {
    let limits = &options.limits;
    if document.pages.len() > limits.book.max_resources {
        return Err(Error::Limit(
            "the document has more pages than the book may have resources",
        ));
    }
    let mut pictures = Vec::with_capacity(document.pages.len());
    let mut figures = Vec::with_capacity(document.pages.len());
    let mut stored: u64 = 0;
    for page in &document.pages {
        let size = parser
            .page_size(page.number, PageRegion::Crop)
            .map_err(convert_error)?;
        let dpi = fitting_dpi(size, limits.max_picture_pixels).ok_or(Error::Limit(
            "a page does not fit the limit of picture pixels",
        ))?;
        let raster = RasterOptions {
            dpi,
            ..RasterOptions::default()
        };
        let painted = parser
            .render_page(page.number, &raster)
            .map_err(convert_error)?;
        let pixels = u64::from(painted.width) * u64::from(painted.height);
        if pixels > limits.max_picture_pixels {
            return Err(Error::Limit("a page has more pixels than the limit"));
        }
        let bytes = u64::try_from(painted.rgba.len()).unwrap_or(u64::MAX);
        if bytes != pixels.saturating_mul(4) {
            return Err(Error::source(
                Format::Pdf,
                "a painted page has pixels of an unexpected size",
            ));
        }
        stored = stored.saturating_add(bytes);
        if stored > limits.book.max_file_bytes {
            return Err(Error::Limit("the painted pages are larger than the limit"));
        }
        let name = format!("page-{}", page.number);
        figures.push(md::image("", &md::encoded_path(&name), None));
        pictures.push(SourcePicture {
            name,
            data: PictureData::Rgba {
                width: painted.width,
                height: painted.height,
                pixels: painted.rgba,
            },
        });
    }
    let cover = pictures.first().map(|picture| picture.name.clone());
    Ok(Draft {
        meta,
        sections: vec![Section {
            name: SCAN_SECTION.to_owned(),
            markdown: figures.join("\n\n") + "\n",
        }],
        pictures,
        cover,
        ..Draft::default()
    })
}

/// The resolution, near [`PAGE_DPI`], at which a page has no more pixels than
/// the limit allows. `None` when no resolution does.
fn fitting_dpi(size: PageSize, max_pixels: u64) -> Option<f32> {
    let mut dpi = PAGE_DPI;
    for _ in 0..SHRINK_STEPS {
        let (width, height) = size.pixels(dpi);
        if u64::from(width) * u64::from(height) <= max_pixels {
            return Some(dpi);
        }
        dpi *= 0.95;
    }
    None
}

/// What `unpdf` is asked to write: Markdown tables, no front matter and no
/// page markers, with its cleanup and its escaping.
fn render_options() -> RenderOptions {
    let mut render = RenderOptions::new()
        .with_table_fallback(TableFallback::Markdown)
        .with_frontmatter(false)
        .with_page_markers(PageMarkerStyle::None)
        .with_image_prefix("")
        .with_standard_cleanup();
    render.escape_special_chars = true;
    render
}

/// A document with text: sections, outline and pictures.
fn text_document(
    mut document: Document,
    meta: DraftMeta,
    options: &ConvertOptions,
) -> Result<Draft> {
    let pictures = pictures(&document, &options.limits);
    let entries = flatten(&document);
    // Form fields are written after the last page; they are rendered once.
    let form_fields = std::mem::take(&mut document.form_fields);
    let render = render_options();
    let mut draft = Draft {
        meta,
        pictures,
        ..Draft::default()
    };
    if entries.is_empty() {
        document.form_fields = form_fields;
        let text = to_markdown(&document, &render).map_err(convert_error)?;
        for (index, part) in split_at_headings(&text).into_iter().enumerate() {
            draft.sections.push(Section {
                name: section_name(index),
                markdown: finish(part, &[]).0,
            });
        }
        return Ok(draft);
    }
    let mut numbers: Vec<u32> = document.pages.iter().map(|page| page.number).collect();
    numbers.sort_unstable();
    let plan = Plan::new(&entries, &numbers);
    // The destination of each deeper entry that has a heading of its own.
    let mut anchored: Vec<Option<String>> = vec![None; entries.len()];
    let last = plan.starts.len().saturating_sub(1);
    for (index, start) in plan.starts.iter().enumerate() {
        let end = plan
            .starts
            .get(index + 1)
            .map_or(u32::MAX, |next| next.saturating_sub(1));
        let mut range = render.clone();
        range.page_selection = PageSelection::Range(*start..=end);
        if index == last {
            document.form_fields.clone_from(&form_fields);
        }
        let text = to_markdown(&document, &range).map_err(convert_error)?;
        let name = section_name(index);
        let wanted: Vec<(usize, String, &str)> = entries
            .iter()
            .enumerate()
            .filter(|(entry, item)| item.level > 1 && plan.sections.get(*entry) == Some(&index))
            .map(|(entry, item)| (entry, format!("o{}", entry + 1), item.title.as_str()))
            .collect();
        let (markdown, marked) = finish(&text, &wanted);
        for ((entry, id, _), marked) in wanted.iter().zip(marked) {
            if let (true, Some(slot)) = (marked, anchored.get_mut(*entry)) {
                *slot = Some(md::destination(&name, Some(id)));
            }
        }
        draft.sections.push(Section { name, markdown });
    }
    // An entry without a heading of its own leads where the entry before it
    // in the same section does, or to the start of the section: the contents
    // would drop an entry that leads back.
    let mut latest: Vec<String> = (0..plan.starts.len())
        .map(|index| md::destination(&section_name(index), None))
        .collect();
    for (index, entry) in entries.iter().enumerate() {
        let section = plan.sections.get(index).copied().unwrap_or_default();
        let Some(place) = latest.get_mut(section) else {
            continue;
        };
        if let Some(Some(target)) = anchored.get(index) {
            place.clone_from(target);
        }
        draft.outline.push(OutlineEntry {
            level: entry.level,
            title: entry.title.clone(),
            target: place.clone(),
        });
    }
    Ok(draft)
}

/// The name of the section with this index.
fn section_name(index: usize) -> String {
    format!("s{}", index + 1)
}

/// The images of the document, as the file has them. An image that is larger
/// than the limits allow is left out; the Markdown that refers to it then
/// refers to a picture the book does not have, and the assembler reports it.
fn pictures(document: &Document, limits: &ConvertLimits) -> Vec<SourcePicture> {
    let mut stored: u64 = 0;
    let mut pictures = Vec::new();
    for (id, resource) in document.resources() {
        let size = u64::try_from(resource.data.len()).unwrap_or(u64::MAX);
        let fits = size <= limits.book.max_resource_bytes
            && stored.saturating_add(size) <= limits.book.max_file_bytes;
        if !resource.is_image() || resource.data.is_empty() || !fits {
            continue;
        }
        stored += size;
        pictures.push(SourcePicture {
            name: id.to_owned(),
            data: PictureData::Encoded(resource.data.clone()),
        });
    }
    pictures
}

/// One entry of the outline of a document.
struct Entry {
    /// Depth in the tree, starting at 1.
    level: u8,
    title: String,
    /// The page it leads to, when `unpdf` could tell which.
    page: Option<u32>,
}

/// The entries of the outline in reading order, without the entries that have
/// no title.
fn flatten(document: &Document) -> Vec<Entry> {
    let mut entries = Vec::new();
    let Some(outline) = &document.outline else {
        return entries;
    };
    let mut pending: Vec<_> = outline.items.iter().rev().map(|item| (item, 1u8)).collect();
    while let Some((item, level)) = pending.pop() {
        let title = item.title.trim();
        if !title.is_empty() {
            entries.push(Entry {
                level,
                title: title.to_owned(),
                page: item.page,
            });
        }
        let below = level.saturating_add(1);
        pending.extend(item.children.iter().rev().map(|child| (child, below)));
    }
    entries
}

/// How the pages of a document are divided into sections by its outline.
struct Plan {
    /// The first page of each section, in increasing order.
    starts: Vec<u32>,
    /// The index of the section of each entry.
    sections: Vec<usize>,
}

impl Plan {
    /// Divides the pages, given by their numbers in increasing order, at the
    /// entries of the first level. An entry that leads nowhere, or backwards,
    /// leads to the section of the entry before it.
    fn new(entries: &[Entry], numbers: &[u32]) -> Self {
        let first = numbers.first().copied().unwrap_or(0);
        let snap = |page: u32| {
            let index = numbers.partition_point(|number| *number < page);
            numbers.get(index).or(numbers.last()).copied().unwrap_or(0)
        };
        let mut pages = Vec::with_capacity(entries.len());
        let mut cuts: Vec<u32> = Vec::new();
        let mut current = first;
        for entry in entries {
            let page = if entry.level == 1 {
                current = entry.page.map_or(current, |page| snap(page).max(current));
                if cuts.last() != Some(&current) {
                    cuts.push(current);
                }
                Some(current)
            } else {
                entry.page.map(snap)
            };
            pages.push(page);
        }
        let mut starts = Vec::with_capacity(cuts.len() + 1);
        if cuts.first() != Some(&first) {
            starts.push(first);
        }
        starts.extend(cuts);
        let mut sections = Vec::with_capacity(entries.len());
        let mut previous = 0;
        for page in pages {
            let section = page.map_or(previous, |page| {
                starts
                    .partition_point(|start| *start <= page)
                    .saturating_sub(1)
            });
            sections.push(section);
            previous = section;
        }
        Self { starts, sections }
    }
}

/// The top-level headings of a Markdown text split it into pieces: the text
/// in front of the first one, if it is not blank, and one piece from each
/// heading to the next. A text without such a heading is one piece.
fn split_at_headings(text: &str) -> Vec<&str> {
    let Ok(tree) = to_mdast(text, &ParseOptions::gfm()) else {
        return vec![text];
    };
    let mut starts: Vec<usize> = tree
        .children()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|node| match node {
            Node::Heading(heading) if heading.depth == 1 => node.position(),
            _ => None,
        })
        .map(|position| position.start.offset)
        .collect();
    let Some(&first) = starts.first() else {
        return vec![text];
    };
    if text
        .get(..first)
        .is_some_and(|front| !front.trim().is_empty())
    {
        starts.insert(0, 0);
    }
    let ends = starts.iter().skip(1).copied().chain([text.len()]);
    starts
        .iter()
        .zip(ends)
        .filter_map(|(start, end)| text.get(*start..end))
        .collect()
}

/// Brings the Markdown of `unpdf` into the terms of the draft: every picture
/// is written with its name as `markdown::image` writes it, and the first
/// heading with the text of each wanted entry gets a mark. `wanted` holds the
/// entry, the identifier of its mark and its title; the result tells which
/// of them got a mark.
fn finish(text: &str, wanted: &[(usize, String, &str)]) -> (String, Vec<bool>) {
    let mut marked = vec![false; wanted.len()];
    let Ok(tree) = to_mdast(text, &ParseOptions::gfm()) else {
        return (text.to_owned(), marked);
    };
    let mut edits = Vec::new();
    pictures_in(&tree, &mut edits);
    let mut headings: Vec<(usize, String, bool)> = tree
        .children()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|node| match (node, node.position()) {
            (Node::Heading(_), Some(position)) => {
                let mut title = String::new();
                plain_text(node, &mut title);
                Some((position.start.offset, normalise(&title), false))
            }
            _ => None,
        })
        .collect();
    for ((_, id, title), marked) in wanted.iter().zip(&mut marked) {
        let title = normalise(title);
        if let Some(heading) = headings
            .iter_mut()
            .find(|(_, text, taken)| !*taken && *text == title)
        {
            heading.2 = true;
            *marked = true;
            let mark = format!("{}\n\n", md::definition(&md::anchor_name(id)));
            edits.push((heading.0..heading.0, mark));
        }
    }
    (apply(text, edits), marked)
}

/// Collects the rewriting of every picture of a tree.
fn pictures_in(node: &Node, edits: &mut Vec<(Range<usize>, String)>) {
    if let (Node::Image(picture), Some(position)) = (node, node.position()) {
        let written = md::image(&picture.alt, &md::encoded_path(&picture.url), None);
        edits.push((position.start.offset..position.end.offset, written));
    }
    for child in node.children().map(Vec::as_slice).unwrap_or_default() {
        pictures_in(child, edits);
    }
}

/// Appends the text of a node without its markup.
fn plain_text(node: &Node, out: &mut String) {
    match node {
        Node::Text(text) => out.push_str(&text.value),
        Node::InlineCode(code) => out.push_str(&code.value),
        other => {
            for child in other.children().map(Vec::as_slice).unwrap_or_default() {
                plain_text(child, out);
            }
        }
    }
}

/// A title as headings and outline entries are compared: white space
/// collapsed, trimmed, and in lower case.
fn normalise(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Replaces the given ranges of a text. A range that starts inside the one
/// before it is skipped.
fn apply(text: &str, mut edits: Vec<(Range<usize>, String)>) -> String {
    edits.sort_by_key(|(range, _)| (range.start, range.end));
    let mut out = String::with_capacity(text.len());
    let mut done = 0;
    for (range, replacement) in edits {
        let Some(kept) = text.get(done..range.start) else {
            continue;
        };
        out.push_str(kept);
        out.push_str(&replacement);
        done = range.end;
    }
    out.push_str(text.get(done..).unwrap_or_default());
    out
}

#[cfg(test)]
mod tests {
    use super::{
        Entry, Plan, apply, convert_error, finish, fitting_dpi, flatten, metadata, normalise,
        pictures, split, split_at_headings,
    };
    use crate::error::Error;
    use crate::options::ConvertLimits;
    use unpdf::parser::raster::PageSize;
    use unpdf::{Document, Metadata, Outline, Page, Resource, model::OutlineItem};

    fn entry(level: u8, page: Option<u32>) -> Entry {
        Entry {
            level,
            title: format!("L{level}"),
            page,
        }
    }

    #[test]
    fn the_outline_is_flattened_in_reading_order_with_depths_from_one() {
        let mut chapter = OutlineItem::new("One", Some(1), 0);
        chapter.add_child(OutlineItem::new("One A", Some(2), 1));
        chapter.add_child(OutlineItem::new("  ", Some(2), 1));
        let mut outline = Outline::new();
        outline.add_item(chapter);
        outline.add_item(OutlineItem::new("Two", None, 0));
        let mut document = Document::new();
        document.outline = Some(outline);
        let flat: Vec<_> = flatten(&document)
            .into_iter()
            .map(|entry| (entry.level, entry.title, entry.page))
            .collect();
        assert_eq!(
            flat,
            [
                (1, "One".to_owned(), Some(1)),
                (2, "One A".to_owned(), Some(2)),
                (1, "Two".to_owned(), None),
            ]
        );
        assert!(flatten(&Document::new()).is_empty());
    }

    #[test]
    fn first_level_entries_cut_the_pages_and_a_leading_section_is_kept() {
        let entries = [entry(1, Some(3)), entry(2, Some(4)), entry(1, Some(6))];
        let plan = Plan::new(&entries, &[1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(plan.starts, [1, 3, 6]);
        assert_eq!(plan.sections, [1, 1, 2]);
    }

    #[test]
    fn an_entry_on_the_first_page_has_no_leading_section() {
        let plan = Plan::new(&[entry(1, Some(1)), entry(1, Some(2))], &[1, 2]);
        assert_eq!((plan.starts, plan.sections), (vec![1, 2], vec![0, 1]));
    }

    #[test]
    fn entries_that_lead_nowhere_or_backwards_share_the_section_before() {
        let entries = [
            entry(1, Some(2)),
            entry(1, None),
            entry(1, Some(1)),
            entry(2, None),
            entry(1, Some(2)),
        ];
        let plan = Plan::new(&entries, &[1, 2, 3]);
        assert_eq!(plan.starts, [1, 2]);
        assert_eq!(plan.sections, [1, 1, 1, 1, 1]);
    }

    #[test]
    fn a_page_the_document_lacks_leads_to_the_next_one_or_the_last() {
        let entries = [entry(1, Some(2)), entry(1, Some(50))];
        let plan = Plan::new(&entries, &[1, 3, 4]);
        assert_eq!(plan.starts, [1, 3, 4]);
        assert_eq!(plan.sections, [1, 2]);
    }

    #[test]
    fn entries_without_a_root_belong_to_the_only_section() {
        let plan = Plan::new(&[entry(2, Some(2)), entry(3, None)], &[1, 2]);
        assert_eq!((plan.starts, plan.sections), (vec![1], vec![0, 0]));
    }

    #[test]
    fn text_is_split_at_top_level_headings_only() {
        let text = "Intro.\n\n# One\n\nText.\n\n## Sub\n\n# Two\n\nMore.\n";
        assert_eq!(
            split_at_headings(text),
            [
                "Intro.\n\n",
                "# One\n\nText.\n\n## Sub\n\n",
                "# Two\n\nMore.\n"
            ]
        );
        assert_eq!(split_at_headings("# One\n\nText.\n"), ["# One\n\nText.\n"]);
        assert_eq!(
            split_at_headings("Text.\n\n## Two\n"),
            ["Text.\n\n## Two\n"]
        );
        assert_eq!(split_at_headings(""), [""]);
    }

    #[test]
    fn titles_are_compared_without_case_and_spacing() {
        assert_eq!(normalise("  The   Big\tOne "), "the big one");
    }

    #[test]
    fn edits_are_applied_in_order_and_an_overlapping_one_is_skipped() {
        let edits = vec![
            (4..7, "THE".to_owned()),
            (0..0, "> ".to_owned()),
            (5..6, "skipped".to_owned()),
        ];
        assert_eq!(apply("see the end", edits), "> see THE end");
        assert_eq!(apply("abc", vec![(5..6, "x".to_owned())]), "abc");
    }

    #[test]
    fn values_are_split_at_the_separators_and_trimmed() {
        assert_eq!(split(" a; b ;;c ", &[';']), ["a", "b", "c"]);
        assert_eq!(split("x, y; z", &[',', ';']), ["x", "y", "z"]);
        assert_eq!(split(" ; ", &[';']), Vec::<String>::new());
    }

    #[test]
    fn the_metadata_keeps_only_what_the_format_trusts() {
        let meta = metadata(&Metadata {
            title: Some(" A Title ".to_owned()),
            author: Some("Ann Lee and Bo Kim".to_owned()),
            keywords: Some("one, two; three".to_owned()),
            subject: Some("About it.".to_owned()),
            creator: Some("Writer".to_owned()),
            ..Metadata::default()
        });
        assert_eq!(meta.title.as_deref(), Some("A Title"));
        assert_eq!(meta.authors, ["Ann Lee and Bo Kim"]);
        assert_eq!(meta.genres, ["one", "two", "three"]);
        assert_eq!(meta.annotation.as_deref(), Some("About it."));
        assert!(meta.languages.is_empty() && meta.date.is_none() && meta.id.is_none());
        let empty = metadata(&Metadata::default());
        assert!(empty.title.is_none() && empty.authors.is_empty());
    }

    #[test]
    fn several_authors_are_split_at_semicolons() {
        let meta = Metadata {
            author: Some("Lee, Ann; Kim, Bo".to_owned()),
            ..Metadata::default()
        };
        assert_eq!(metadata(&meta).authors, ["Lee, Ann", "Kim, Bo"]);
    }

    #[test]
    fn a_page_is_painted_at_150_dpi_unless_it_has_too_many_pixels() {
        let a4 = PageSize {
            width: 595.0,
            height: 842.0,
        };
        assert_eq!(fitting_dpi(a4, 100_000_000), Some(150.0));
        let dpi = fitting_dpi(a4, 100_000);
        assert!(dpi.is_some_and(|dpi| {
            let (width, height) = a4.pixels(dpi);
            u64::from(width) * u64::from(height) <= 100_000 && dpi > 30.0
        }));
        assert_eq!(fitting_dpi(a4, 0), None);
    }

    #[test]
    fn only_encryption_is_protection() {
        assert!(matches!(
            convert_error(unpdf::Error::Encrypted),
            Error::Protected(_)
        ));
        assert!(matches!(
            convert_error(unpdf::Error::InvalidPassword),
            Error::Protected(_)
        ));
        assert!(matches!(
            convert_error(unpdf::Error::UnknownFormat),
            Error::Source { .. }
        ));
    }

    #[test]
    fn a_heading_with_code_in_it_is_found_by_its_text() {
        let wanted = [(0, "o1".to_owned(), "a b c")];
        let (text, marked) = finish("# a `b` c\n\n![alt](<x y.jpg>)\n", &wanted);
        assert_eq!(marked, [true]);
        assert_eq!(text, "[o1]: #o1\n\n# a `b` c\n\n![alt](x%20y.jpg)\n");
    }

    #[test]
    fn only_images_within_the_limits_are_pictures() {
        let mut page = Page::new(1, 612.0, 792.0);
        page.images
            .push(("a.jpg".to_owned(), Resource::jpeg(vec![1; 10])));
        page.images
            .push(("empty.jpg".to_owned(), Resource::jpeg(Vec::new())));
        page.images
            .push(("big.jpg".to_owned(), Resource::jpeg(vec![1; 5000])));
        page.images.push((
            "font.ttf".to_owned(),
            Resource::new(vec![1; 10], "font/ttf", unpdf::ResourceType::Font),
        ));
        page.images
            .push(("b.jpg".to_owned(), Resource::jpeg(vec![2; 10])));
        let mut document = Document::new();
        document.add_page(page);
        let mut limits = ConvertLimits::default();
        limits.book.max_resource_bytes = 1000;
        limits.book.max_file_bytes = 15;
        let names: Vec<_> = pictures(&document, &limits)
            .into_iter()
            .map(|picture| picture.name)
            .collect();
        assert_eq!(names, ["a.jpg"]);
    }
}
