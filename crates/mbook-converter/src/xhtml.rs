//! HTML and XHTML as the Markdown of a section. Shared by the formats whose
//! text is HTML: EPUB and single HTML documents.
//!
//! `htmd` writes the Markdown, with its escaping of text. This module is the
//! layer around it: it adds what the Markdown of `htmd` would lose, and it
//! finishes the text with the rules of the mbook dialect.
//!
//! # How a front end uses it
//!
//! [`convert`] takes the text of one HTML or XHTML document, or of a
//! fragment, and two mappings that only the caller can make:
//!
//! - **links**: from the value of an `href` to the destination to write.
//!   The caller resolves relative paths and names sections, and writes the
//!   result as [`markdown::destination`] does. `None` writes the `href` as
//!   it is. A link into a footnote of the same document is not asked about;
//!   see below.
//! - **pictures**: from the value of a picture's `src` (or the `href` of an
//!   SVG `<image>`) to the `name` of a `SourcePicture` of the draft. `None`
//!   keeps the address; the assembler then drops the picture with
//!   `WarningKind::PictureDropped` and keeps its alternative text. An error
//!   stops the conversion and is returned by [`convert`].
//!
//! The result is a [`Document`]: the Markdown, the metadata of the document
//! and the losses. The Markdown never holds raw HTML. A document is cut into
//! sections with [`Document::parts`].
//!
//! Link targets are found in two passes: a front end whose links lead to
//! other parts of the book converts once with `|_| None` to learn which part
//! holds every `id` ([`Part::ids`]), and once more with the real mapping.
//! Cutting does not depend on the links.
//!
//! # The three steps
//!
//! 1. **`htmd`** converts the document in its pure mode. Text, paragraphs,
//!    headings, lists, block quotes, code, links, pictures and tables are
//!    its own. `script`, `style`, `head`, forms, frames and similar elements
//!    are skipped with their content.
//! 2. **A handler** for every element name adds what `htmd` drops: `~~` for
//!    `s`, `strike` and `del`; the warning for `sub`, `sup`, `u` and for a
//!    table with merged cells; a footnote for a note reference and its note;
//!    a picture with a title for a `figure` that holds pictures and a
//!    `figcaption`; the picture of an SVG `<image>`; a space for a `<br>` in
//!    a heading or a cell. It also writes a placeholder for every `id` and
//!    `<a name>`, and for every note reference. Placeholders are private-use
//!    characters around a number; they cannot occur in text.
//! 3. **The Markdown layer** reads the text with the parser of the dialect.
//!    Every placeholder of an `id` becomes a mark, `[name]: #name` and an
//!    empty line, in front of the top-level block that held it; a mark is
//!    never written inside a paragraph, a table or a list item. The text is
//!    cut at the top-level `h1` headings and the notes are written at the end
//!    of the part that refers to them. At last `crate::assemble::rewrite`
//!    applies the mappings to every link and picture.
//!
//! # What changes with `htmd`
//!
//! - A table without `th` or `thead` has no header row for GFM, so `htmd`
//!   writes the text of its cells as paragraphs.
//! - Two lists next to each other are one list in Markdown.
//! - An element with an `id` that sits in a `tr`, `thead` or `tbody` is not
//!   marked: `htmd` reads the cells without asking the handlers of rows.
//! - A document nested deeper than [`MAX_DEPTH`] elements is written as one
//!   paragraph of text, with `MarkupDropped` for the nesting.

use crate::assemble::rewrite::{self, Fate, Rules};
use crate::error::{Error, Result};
use crate::format::Format;
use crate::markdown;
use crate::report::WarningKind;
use ::markdown::mdast::Node as Mdast;
use htmd::HtmlToMarkdown;
use htmd::element_handler::{
    Context, ContextKind, Element, ElementHandler, HandlerResult, Handlers,
};
use htmd::node::Node;
use htmd::options::{BrStyle, BulletListMarker, HrStyle, Options};
use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Range;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// How deep elements are followed. A deeper document is written as text, so
/// that a hostile document cannot exhaust the stack.
const MAX_DEPTH: usize = 128;

/// Opens and closes the placeholder of an `id`; a number stands between.
const MARK: (char, char) = ('\u{E000}', '\u{E001}');

/// Opens and closes the placeholder of a reference to a note.
const NOTE: (char, char) = ('\u{E002}', '\u{E003}');

/// Elements that leave nothing behind, content included.
const SKIPPED: &[&str] = &[
    "applet", "area", "audio", "base", "button", "canvas", "col", "colgroup", "datalist", "embed",
    "form", "frame", "frameset", "head", "iframe", "input", "link", "map", "meta", "noframes",
    "noscript", "object", "optgroup", "option", "param", "rp", "script", "select", "source",
    "style", "template", "textarea", "title", "track", "video",
];

/// Every element name a handler is registered for: `htmd` has no handler for
/// all names, so the layer is added to each of them.
const ELEMENTS: &[&str] = &[
    "a",
    "abbr",
    "acronym",
    "address",
    "article",
    "aside",
    "b",
    "bdi",
    "bdo",
    "big",
    "blockquote",
    "body",
    "br",
    "caption",
    "center",
    "cite",
    "code",
    "data",
    "dd",
    "del",
    "details",
    "dfn",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "em",
    "fieldset",
    "figcaption",
    "figure",
    "font",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "html",
    "i",
    "image",
    "img",
    "ins",
    "kbd",
    "label",
    "legend",
    "li",
    "listing",
    "main",
    "mark",
    "marquee",
    "menu",
    "meter",
    "nav",
    "nobr",
    "ol",
    "output",
    "p",
    "picture",
    "plaintext",
    "pre",
    "progress",
    "q",
    "rb",
    "rt",
    "rtc",
    "ruby",
    "s",
    "samp",
    "search",
    "section",
    "slot",
    "small",
    "span",
    "strike",
    "strong",
    "sub",
    "summary",
    "sup",
    "svg",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "time",
    "tr",
    "tt",
    "u",
    "ul",
    "var",
    "wbr",
    "xmp",
];

/// What cannot stand in a link destination as it is. Unlike the set of
/// [`markdown::encoded_path`], it keeps `#`, `?` and `%`: it writes an
/// address of the source, not a path of the book.
const IN_ADDRESS: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'(')
    .add(b')')
    .add(b'<')
    .add(b'>')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'`');

/// A converted document.
#[derive(Debug)]
pub(crate) struct Document {
    /// The whole text as Markdown, without raw HTML. Marks stand in front of
    /// their blocks and the notes at the end.
    pub(crate) markdown: String,
    /// The text of `<title>`, when the document has one.
    #[cfg_attr(
        all(not(test), not(feature = "html")),
        expect(dead_code, reason = "only the html front end reads the metadata")
    )]
    pub(crate) title: Option<String>,
    /// The `lang` (or `xml:lang`) of `<html>`.
    #[cfg_attr(
        all(not(test), not(feature = "html")),
        expect(dead_code, reason = "only the html front end reads the metadata")
    )]
    pub(crate) lang: Option<String>,
    /// The content of every `<meta name="author">`, in order.
    #[cfg_attr(
        all(not(test), not(feature = "html")),
        expect(dead_code, reason = "only the html front end reads the metadata")
    )]
    pub(crate) authors: Vec<String>,
    /// The content of the first `<meta name="description">`.
    #[cfg_attr(
        all(not(test), not(feature = "html")),
        expect(dead_code, reason = "only the html front end reads the metadata")
    )]
    pub(crate) description: Option<String>,
    /// The content of the first `<meta name="keywords">`, as it is written.
    #[cfg_attr(
        all(not(test), not(feature = "html")),
        expect(dead_code, reason = "only the html front end reads the metadata")
    )]
    pub(crate) keywords: Option<String>,
    /// What could not be kept. Every kind is reported once.
    pub(crate) warnings: Vec<WarningKind>,
    /// The document cut at its top-level `h1` headings.
    parts: Vec<Part>,
}

/// One part of a document cut at its top-level headings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Part {
    /// The Markdown of the part, with the notes its blocks refer to.
    pub(crate) markdown: String,
    /// The `id` of every mark in the part, as written in the source.
    pub(crate) ids: Vec<String>,
}

impl Document {
    /// Cuts the document at its top-level `h1` headings.
    ///
    /// The text in front of the first heading is a part of its own, and a
    /// heading starts a part together with the marks in front of it. A note
    /// goes to the end of the part that refers to it. Parts without text are
    /// left out. `ids` of the parts tell where every link target is.
    pub(crate) fn parts(&self) -> Vec<Part> {
        self.parts.clone()
    }
}

/// What the handlers learn and write down while `htmd` walks the document.
#[derive(Default)]
struct State {
    /// The `id` behind every placeholder of a mark.
    ids: Vec<String>,
    /// The losses, once each.
    warnings: Vec<WarningKind>,
    /// The notes of the document that a note reference leads to.
    notes: Vec<Note>,
    /// The index in `notes` by the `id` of the note.
    note_of: HashMap<String, usize>,
    /// The `id` of the notes that have no content: their references go too.
    empty_notes: HashSet<String>,
}

/// A note that is taken out of the text.
struct Note {
    /// The label of the footnote.
    label: String,
    /// The text of the note, with placeholders, once it was walked.
    text: Option<String>,
}

impl State {
    fn warn(&mut self, kind: WarningKind) {
        if !self.warnings.contains(&kind) {
            self.warnings.push(kind);
        }
    }
}

/// The handler of every element: it delegates to `htmd` and adds a little.
struct Layer {
    state: Arc<Mutex<State>>,
}

impl Layer {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn warn(&self, what: &str) {
        self.state()
            .warn(WarningKind::MarkupDropped(what.to_owned()));
    }

    /// A reference to a note of the document, or the note itself.
    fn note(&self, handlers: &dyn Handlers, element: &Element<'_>) -> Option<HandlerResult> {
        let id = attr(element.node, "id");
        if element.tag == "a" && is_noteref(element.node) {
            let target = fragment(&attr(element.node, "href")?)?;
            let state = self.state();
            if state.empty_notes.contains(&target) {
                return Some(String::new().into());
            }
            let index = *state.note_of.get(&target)?;
            let label = state.notes.get(index)?.label.clone();
            return Some(format!("[^{label}]{}{index}{}", NOTE.0, NOTE.1).into());
        }
        let id = id?;
        if !is_note(element.node) {
            return None;
        }
        if self.state().empty_notes.contains(&id) {
            return Some(String::new().into());
        }
        let index = *self.state().note_of.get(&id)?;
        let text = handlers.walk_children_content(element.node, Context::BLOCK);
        if let Some(note) = self.state().notes.get_mut(index)
            && note.text.is_none()
        {
            note.text = Some(text);
        }
        Some(String::new().into())
    }

    /// A figure that holds nothing but pictures and a caption.
    fn figure(handlers: &dyn Handlers, element: &Element<'_>) -> Option<HandlerResult> {
        let mut pictures = Vec::new();
        let mut caption = None;
        let children = element.node.children();
        for child in &children {
            match child.tag_name() {
                Some("img" | "svg") => pictures.push(child),
                Some("figcaption") if caption.is_none() => caption = Some(child),
                None if child.text().is_none_or(|text| text.trim().is_empty()) => {}
                _ => return handlers.fallback(element),
            }
        }
        if pictures.is_empty() {
            return handlers.fallback(element);
        }
        let mut written: Vec<(String, bool)> = pictures
            .iter()
            .map(|picture| {
                let text = handlers
                    .handle(picture, Context::INLINE)
                    .map(|result| result.content)
                    .unwrap_or_default();
                (text, picture.attr("title").is_some())
            })
            .filter(|(text, _)| !text.is_empty())
            .collect();
        let title = caption
            .map(|caption| plain_text(&caption))
            .unwrap_or_default();
        if let Some((last, titled)) = written.last_mut()
            && !title.is_empty()
            && !*titled
            && let Some(open) = last.strip_suffix(')')
        {
            // The title of `markdown::target` follows the destination.
            let title = markdown::target("", Some(&title));
            *last = format!("{open}{}", title.get(1..).unwrap_or_default());
        }
        let texts: Vec<String> = written.into_iter().map(|(text, _)| text).collect();
        Some(format!("\n\n{}\n\n", texts.join("\n\n")).into())
    }

    /// `~~` around the children, with the white space outside.
    fn strike(handlers: &dyn Handlers, element: &Element<'_>) -> HandlerResult {
        let content = handlers.walk_children_content(
            element.node,
            Context {
                kind: ContextKind::Inline,
                ..element.context
            },
        );
        let core = content.trim();
        if core.is_empty() {
            return content.into();
        }
        let start = content.len() - content.trim_start().len();
        let (head, rest) = content.split_at(start);
        let (mut core, mut tail) = rest.split_at(core.len());
        // A hard break stays outside the marks: `\~` would be an escape.
        if let Some(inner) = core.strip_suffix('\\')
            && tail.starts_with('\n')
        {
            core = inner;
            tail = &rest[core.len()..];
        }
        format!("{head}~~{core}~~{tail}").into()
    }

    /// A table: `htmd` writes one that has a header row. Any other has its
    /// first row as the header, as GFM has no table without one.
    fn table(handlers: &dyn Handlers, element: &Element<'_>) -> Option<HandlerResult> {
        let (rows, headed) = table_rows(element.node);
        if headed {
            return handlers.fallback(element);
        }
        let mut captions = String::new();
        let children = element.node.children();
        for child in children.iter().filter(|c| c.tag_name() == Some("caption")) {
            if let Some(caption) = handlers.handle(&child, Context::BLOCK) {
                captions.push_str(caption.content.trim());
                captions.push_str("\n\n");
            }
        }
        let rows: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                let cells = row.children();
                cells
                    .iter()
                    .filter(|cell| matches!(cell.tag_name(), Some("td" | "th")))
                    .map(|cell| {
                        let content = handlers.handle(&cell, Context::BLOCK);
                        let content = content.map(|result| result.content).unwrap_or_default();
                        let lines: Vec<&str> = content.lines().map(str::trim).collect();
                        lines
                            .iter()
                            .filter(|line| !line.is_empty())
                            .copied()
                            .collect::<Vec<_>>()
                            .join(" ")
                            .replace('|', "\\|")
                    })
                    .collect()
            })
            .filter(|row: &Vec<String>| !row.is_empty())
            .collect();
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns == 0 {
            return Some(captions.trim_end().to_owned().into());
        }
        let widths: Vec<usize> = (0..columns)
            .map(|at| {
                rows.iter()
                    .filter_map(|row| row.get(at))
                    .map(|cell| cell.chars().count())
                    .max()
                    .unwrap_or(1)
                    .max(1)
            })
            .collect();
        let line = |row: &[String]| {
            let mut out = String::from("|");
            for (at, width) in widths.iter().enumerate() {
                let cell = row.get(at).map(String::as_str).unwrap_or_default();
                let pad = width.saturating_sub(cell.chars().count());
                out.push(' ');
                out.push_str(cell);
                out.push_str(&" ".repeat(pad));
                out.push_str(" |");
            }
            out.push('\n');
            out
        };
        let mut table = captions;
        for (at, row) in rows.iter().enumerate() {
            table.push_str(&line(row));
            if at == 0 {
                out_separator(&mut table, &widths);
            }
        }
        Some(format!("\n\n{table}\n").into())
    }

    /// A `<pre>` without a `<code>` is text with its line breaks and
    /// indentation; `htmd` leaves it as it is, which Markdown reads as a
    /// paragraph. It is a code block, like one with a `<code>`.
    fn pre(handlers: &dyn Handlers, element: &Element<'_>) -> HandlerResult {
        let has_code = element
            .node
            .children()
            .iter()
            .any(|child| child.tag_name() == Some("code"));
        if has_code && let Some(result) = handlers.fallback(element) {
            return result;
        }
        let content = handlers.walk_children_content(
            element.node,
            Context {
                literal: true,
                ..element.context
            },
        );
        let content = content.trim_matches('\n');
        if content.trim().is_empty() {
            return String::new().into();
        }
        let longest = content
            .split(|character| character != '`')
            .map(str::len)
            .max()
            .unwrap_or(0);
        let fence = "`".repeat((longest + 1).max(3));
        format!("\n\n{fence}\n{content}\n{fence}\n\n").into()
    }

    /// The pictures of an SVG: the `<image>` elements under it.
    fn svg(handlers: &dyn Handlers, element: &Element<'_>) -> HandlerResult {
        let mut images = Vec::new();
        walk(element.node, |node, _| {
            if node.tag_name() == Some("image") {
                images.push(node.clone());
            }
            true
        });
        let texts: Vec<String> = images
            .iter()
            .filter_map(|image| handlers.handle(image, Context::INLINE))
            .map(|result| result.content)
            .filter(|text| !text.is_empty())
            .collect();
        texts.join(" ").into()
    }

    /// The picture of an SVG `<image>`; `htmd` has no handler for it.
    fn image(element: &Element<'_>) -> HandlerResult {
        let href = attr(element.node, "href").unwrap_or_default();
        if href.trim().is_empty() {
            return String::new().into();
        }
        let mut destination = String::with_capacity(href.len() + 2);
        for character in href.trim().chars() {
            match character {
                '\\' | '<' | '>' => {
                    destination.push('\\');
                    destination.push(character);
                }
                '\n' | '\r' => destination.push_str("%0A"),
                _ => destination.push(character),
            }
        }
        format!("![](<{destination}>)").into()
    }

    /// Writes the placeholders of the identifiers of the element in front of
    /// what `htmd` wrote for it.
    fn mark(
        &self,
        element: &Element<'_>,
        result: HandlerResult,
        extra: &[String],
    ) -> HandlerResult {
        let mut names = Vec::new();
        names.extend(attr(element.node, "id"));
        names.extend(extra.iter().cloned());
        if element.tag == "a" {
            names.extend(attr(element.node, "name"));
        }
        names.retain(|name| !name.is_empty());
        names.dedup();
        if names.is_empty() {
            return result;
        }
        let mut tokens = String::new();
        let mut state = self.state();
        for name in names {
            state.ids.push(name);
            tokens.push(MARK.0);
            tokens.push_str(&(state.ids.len() - 1).to_string());
            tokens.push(MARK.1);
        }
        drop(state);
        let content = put_in_front(&result.content, &tokens);
        HandlerResult {
            content,
            markdown_translated: result.markdown_translated,
        }
    }
}

impl ElementHandler for Layer {
    fn handle(&self, handlers: &dyn Handlers, element: &Element<'_>) -> Option<HandlerResult> {
        if let Some(note) = self.note(handlers, element) {
            // A reference is marked for its own `id`; a note leaves nothing.
            return Some(if element.tag == "a" {
                self.mark(element, note, &[])
            } else {
                note
            });
        }
        let result = match element.tag {
            "s" | "strike" | "del" => Some(Self::strike(handlers, element)),
            "sub" | "sup" | "u" => {
                self.warn(element.tag);
                handlers.fallback(element)
            }
            "table" => {
                if has_merged_cells(element.node) {
                    self.warn("table with merged cells");
                }
                // `htmd` asks no handler for rows and sections: their ids are
                // marked here, in front of the table.
                let ids = table_ids(element.node);
                let result = if inside(element.node, &["td", "th"]) {
                    handlers.fallback(element)
                } else {
                    Self::table(handlers, element)
                };
                return result.map(|result| self.mark(element, result, &ids));
            }
            "br" if inside(
                element.node,
                &["h1", "h2", "h3", "h4", "h5", "h6", "td", "th"],
            ) =>
            {
                Some(" ".into())
            }
            "pre" => Some(Self::pre(handlers, element)),
            "figure" => Self::figure(handlers, element),
            "svg" => Some(Self::svg(handlers, element)),
            "image" => Some(Self::image(element)),
            _ => handlers.fallback(element),
        }?;
        Some(self.mark(element, result, &[]))
    }
}

/// Puts placeholders at the start of the text of what `htmd` wrote, after
/// the markers of its first line, so that the structure stays as it is.
fn put_in_front(content: &str, tokens: &str) -> String {
    let lead = content.len() - content.trim_start_matches('\n').len();
    let (head, body) = content.split_at(lead);
    let first_length = body.find('\n').unwrap_or(body.len());
    let (first, rest) = body.split_at(first_length);
    let at = if lead == 1 {
        // An item of a list: the bullet is written by the item.
        Some(if first.starts_with("- ") { 2 } else { 0 })
    } else if lead == 0 {
        Some(0)
    } else if first.starts_with("```") || first.starts_with("~~~") {
        // Inside the code, where the fence stays whole.
        return format!(
            "{head}{first}\n{tokens}{}",
            rest.get(1..).unwrap_or_default()
        );
    } else if first.starts_with("| ") {
        Some(2)
    } else if first.chars().all(|c| matches!(c, '-' | '*' | '_' | ' ')) {
        None
    } else {
        Some(marker_length(first))
    };
    match at {
        Some(at) => format!(
            "{head}{}{tokens}{}{rest}",
            first.get(..at).unwrap_or_default(),
            first.get(at..).unwrap_or_default()
        ),
        // A paragraph of its own: the block stays as it is.
        None => format!("{head}{tokens}\n\n{body}"),
    }
}

/// The length of the markers of block quotes, headings and list items that
/// start a line.
fn marker_length(line: &str) -> usize {
    let mut length = 0;
    loop {
        let rest = line.get(length..).unwrap_or_default();
        let hashes = rest.bytes().take_while(|&byte| byte == b'#').count();
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if rest.starts_with("> ") || rest.starts_with("- ") {
            length += 2;
        } else if (1..=6).contains(&hashes)
            && rest.get(hashes..).is_some_and(|r| r.starts_with(' '))
        {
            length += hashes + 1;
        } else if digits > 0 && rest.get(digits..).is_some_and(|r| r.starts_with(". ")) {
            length += digits + 2;
        } else {
            return length;
        }
    }
}

/// Converts a document or a fragment of HTML or XHTML.
///
/// `links` maps the value of an `href` to the destination to write, or
/// `None` to write it as it is. `pictures` maps the value of a `src` to the
/// `name` of a picture of the draft, or `None` to keep the address; an error
/// stops the conversion. Both are called once for every occurrence, in the
/// order of the text.
///
/// # Errors
///
/// `pictures` fails, or the HTML parser reports an error, which it does not
/// for any text; `format` names the format of the source in that case.
pub(crate) fn convert(
    format: Format,
    html: &str,
    links: impl FnMut(&str) -> Option<String>,
    pictures: impl FnMut(&str) -> Result<Option<String>>,
) -> Result<Document> {
    let source = prepare(html);
    let reader = HtmlToMarkdown::new();
    let tree = reader
        .html_to_tree(&source)
        .map_err(|error| Error::source(format, error))?;
    let scan = scan(&tree.root());
    let mut document = scan.document();
    if scan.too_deep {
        document
            .warnings
            .push(WarningKind::MarkupDropped("deeply nested elements".into()));
        let text = markdown::escape(collapse(&all_text(&tree.root(), " ")).trim());
        if !text.is_empty() {
            document.markdown = format!("{text}\n");
            document.parts = vec![Part {
                markdown: document.markdown.clone(),
                ids: Vec::new(),
            }];
        }
        return Ok(document);
    }

    let state = Arc::new(Mutex::new(scan.state()));
    let converter = HtmlToMarkdown::builder()
        .options(Options {
            bullet_list_marker: BulletListMarker::Dash,
            ul_bullet_spacing: 1,
            ol_number_spacing: 1,
            br_style: BrStyle::TwoSpaces,
            hr_style: HrStyle::Dashes,
            ..Options::default()
        })
        .add_handler(
            ELEMENTS.to_vec(),
            Layer {
                state: Arc::clone(&state),
            },
        )
        .skip_tags(SKIPPED.to_vec())
        .build();
    let written = converter
        .convert(&tree)
        .map_err(|error| Error::source(format, error))?;
    let state = std::mem::take(&mut *state.lock().unwrap_or_else(PoisonError::into_inner));

    let mut finish = Finish {
        links: RefCell::new(links),
        pictures,
        failure: None,
        unmapped: Vec::new(),
        warnings: state.warnings.clone(),
    };
    let (markdown, parts) = layer(&written, &state, &mut finish);
    if let Some(error) = finish.failure {
        return Err(error);
    }
    document.markdown = markdown;
    document.parts = parts;
    document.warnings = finish.warnings;
    Ok(document)
}

/// What a look at the whole tree finds before `htmd` writes anything.
#[derive(Default)]
struct Scan {
    title: Option<String>,
    lang: Option<String>,
    authors: Vec<String>,
    description: Option<String>,
    keywords: Option<String>,
    /// Whether elements are nested deeper than [`MAX_DEPTH`].
    too_deep: bool,
    /// The ids that a reference to a note of the document leads to.
    targets: HashSet<String>,
    /// Every note of the document: its `id` and whether it has content.
    candidates: Vec<(String, bool)>,
}

impl Scan {
    /// The document with the metadata and nothing else.
    fn document(&self) -> Document {
        Document {
            markdown: String::new(),
            title: self.title.clone(),
            lang: self.lang.clone(),
            authors: self.authors.clone(),
            description: self.description.clone(),
            keywords: self.keywords.clone(),
            warnings: Vec::new(),
            parts: Vec::new(),
        }
    }

    /// The notes that are taken out of the text: those a reference of the
    /// document leads to, with a label each.
    fn state(&self) -> State {
        let mut state = State::default();
        let mut labels = HashSet::new();
        for (id, filled) in &self.candidates {
            if !self.targets.contains(id) || state.note_of.contains_key(id) {
                continue;
            }
            if !filled {
                state.empty_notes.insert(id.clone());
                continue;
            }
            let base = markdown::anchor_name(id);
            let mut label = base.clone();
            let mut number = 1;
            while !labels.insert(label.clone()) {
                number += 1;
                label = format!("{base}-{number}");
            }
            state.note_of.insert(id.clone(), state.notes.len());
            state.notes.push(Note { label, text: None });
        }
        state
    }
}

/// Looks at every element for the metadata, the notes and the depth.
fn scan(root: &Node<'_>) -> Scan {
    let mut found = Scan::default();
    walk(root, |node, depth| {
        found.too_deep |= depth > MAX_DEPTH;
        let Some(tag) = node.tag_name() else {
            return true;
        };
        match tag {
            "html" => {
                let lang = attr(node, "lang").or_else(|| attr(node, "xml:lang"));
                found.lang = lang
                    .map(|lang| lang.trim().to_owned())
                    .filter(|l| !l.is_empty());
            }
            "title" if found.title.is_none() && has_parent(node, "head") => {
                found.title = Some(collapse(&all_text(node, "")).trim().to_owned())
                    .filter(|title| !title.is_empty());
            }
            "meta" => scan_meta(node, &mut found),
            "a" if is_noteref(node) => {
                found
                    .targets
                    .extend(attr(node, "href").and_then(|href| fragment(&href)));
            }
            _ => {}
        }
        if is_note(node)
            && let Some(id) = attr(node, "id")
        {
            let filled = node
                .find(|inner| {
                    inner.text().is_some_and(|text| !text.trim().is_empty())
                        || matches!(inner.tag_name(), Some("img" | "image"))
                })
                .is_some();
            found.candidates.push((id, filled));
        }
        true
    });
    found
}

/// Reads a `<meta>` element into the metadata.
fn scan_meta(node: &Node<'_>, found: &mut Scan) {
    let name = attr(node, "name").map(|name| name.trim().to_ascii_lowercase());
    let content = attr(node, "content").map(|content| collapse(&content).trim().to_owned());
    let Some(content) = content.filter(|content| !content.is_empty()) else {
        return;
    };
    match name.as_deref() {
        Some("author") => found.authors.push(content),
        Some("description") => {
            found.description.get_or_insert(content);
        }
        Some("keywords") => {
            found.keywords.get_or_insert(content);
        }
        _ => {}
    }
}

/// A placeholder found in the text.
struct Token {
    range: Range<usize>,
    /// Whether it stands for a note and not for a mark.
    note: bool,
    /// The index in the list of ids or of notes.
    index: usize,
}

/// Finds the placeholders of a text, in order.
fn find_tokens(text: &str) -> Vec<Token> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = text.get(at..).and_then(|rest| rest.find([MARK.0, NOTE.0])) {
        let start = at + offset;
        let note = text
            .get(start..)
            .is_some_and(|rest| rest.starts_with(NOTE.0));
        let (open, close) = if note { NOTE } else { MARK };
        let from = start + open.len_utf8();
        at = from;
        let Some(length) = text.get(from..).and_then(|rest| rest.find(close)) else {
            break;
        };
        let index = text.get(from..from + length).and_then(|n| n.parse().ok());
        if let Some(index) = index {
            let end = from + length + close.len_utf8();
            found.push(Token {
                range: start..end,
                note,
                index,
            });
            at = end;
        }
    }
    found
}

/// The text without its placeholders.
fn without(text: &str, tokens: &[Token]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for token in tokens {
        out.push_str(text.get(at..token.range.start).unwrap_or_default());
        at = token.range.end;
    }
    out.push_str(text.get(at..).unwrap_or_default());
    out
}

/// One top-level block of the text.
struct Block {
    /// The text, with the marks in front of it.
    text: String,
    /// Whether it is an `h1`, where a part starts.
    h1: bool,
    /// The `id` of the marks in front of it.
    ids: Vec<String>,
    /// The notes it refers to.
    notes: Vec<usize>,
}

/// The marks for some `id` of the list: one line each, and an empty line.
/// A name is marked once in the whole document.
fn marks(indexes: &[usize], ids: &[String], seen: &mut HashSet<String>) -> (String, Vec<String>) {
    let mut text = String::new();
    let mut marked = Vec::new();
    for id in indexes.iter().filter_map(|&index| ids.get(index)) {
        let name = markdown::anchor_name(id);
        if !seen.contains(&name) {
            text.push_str(&markdown::definition(&name));
            text.push_str("\n\n");
            seen.insert(name);
            marked.push(id.clone());
        }
    }
    (text, marked)
}

/// Reads what `htmd` wrote as top-level blocks. A mark goes in front of the
/// block that held its placeholder; one in a block with no text of its own
/// goes in front of the next block, or of the last one.
fn blocks(written: &str, tree: &Mdast, ids: &[String], seen: &mut HashSet<String>) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut pending: Vec<usize> = Vec::new();
    for child in tree.children().map(Vec::as_slice).unwrap_or_default() {
        let Some(position) = child.position() else {
            continue;
        };
        let raw = written
            .get(position.start.offset..position.end.offset)
            .unwrap_or_default();
        let found = find_tokens(raw);
        let text = without(raw, &found);
        let held = found
            .iter()
            .filter(|token| !token.note)
            .map(|token| token.index);
        if text.trim().is_empty() {
            pending.extend(held);
            continue;
        }
        pending.extend(held);
        let (mut front, marked) = marks(&std::mem::take(&mut pending), ids, seen);
        front.push_str(&text);
        out.push(Block {
            text: front,
            h1: matches!(child, Mdast::Heading(heading) if heading.depth == 1),
            ids: marked,
            notes: found
                .iter()
                .filter(|token| token.note)
                .map(|token| token.index)
                .collect(),
        });
    }
    if let Some(last) = out.last_mut() {
        let (mut front, mut marked) = marks(&pending, ids, seen);
        front.push_str(&last.text);
        last.text = front;
        marked.append(&mut last.ids);
        last.ids = marked;
    }
    out
}

/// The text of a note as the definition of a footnote: lines after the first
/// are indented by four spaces.
fn definition(label: &str, text: &str) -> String {
    let mut out = format!("[^{label}]: ");
    for (index, line) in text.trim().lines().enumerate() {
        if index > 0 {
            out.push('\n');
            if !line.trim().is_empty() {
                out.push_str("    ");
            }
        }
        out.push_str(line);
    }
    out
}

/// Joins blocks into text that ends with one line feed.
fn compose<'a>(blocks: impl IntoIterator<Item = &'a str>) -> String {
    let mut out = String::new();
    for block in blocks.into_iter().map(str::trim).filter(|b| !b.is_empty()) {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(block);
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Turns what `htmd` wrote into the text of the whole document and its
/// parts.
fn layer<L, P>(written: &str, state: &State, finish: &mut Finish<L, P>) -> (String, Vec<Part>)
where
    L: FnMut(&str) -> Option<String>,
    P: FnMut(&str) -> Result<Option<String>>,
{
    let Some(tree) = rewrite::parse(written) else {
        return (String::new(), Vec::new());
    };
    let mut seen = HashSet::new();
    let blocks = blocks(written, &tree, &state.ids, &mut seen);
    let mut groups: Vec<Vec<&Block>> = Vec::new();
    for block in &blocks {
        match groups.last_mut() {
            Some(group) if !block.h1 => group.push(block),
            _ => groups.push(vec![block]),
        }
    }
    let mut placed = HashSet::new();
    let mut bodies = Vec::new();
    let mut all_notes = Vec::new();
    let mut parts = Vec::new();
    for group in groups {
        let body = finish.run(&compose(group.iter().map(|block| block.text.as_str())));
        let mut ids: Vec<String> = group.iter().flat_map(|block| block.ids.clone()).collect();
        let mut queue: VecDeque<usize> =
            group.iter().flat_map(|block| block.notes.clone()).collect();
        let mut notes = Vec::new();
        while let Some(index) = queue.pop_front() {
            let Some(note) = state.notes.get(index).filter(|_| placed.insert(index)) else {
                continue;
            };
            let Some(text) = note.text.as_deref() else {
                continue;
            };
            let found = find_tokens(text);
            queue.extend(
                found
                    .iter()
                    .filter(|token| token.note)
                    .map(|token| token.index),
            );
            let held: Vec<usize> = found
                .iter()
                .filter(|token| !token.note)
                .map(|token| token.index)
                .collect();
            let (front, marked) = marks(&held, &state.ids, &mut seen);
            ids.extend(marked);
            let text = format!("{front}{}", definition(&note.label, &without(text, &found)));
            notes.push(finish.run(&text));
        }
        parts.push(Part {
            markdown: compose(
                std::iter::once(body.as_str()).chain(notes.iter().map(String::as_str)),
            ),
            ids,
        });
        bodies.push(body);
        all_notes.extend(notes);
    }
    let whole = compose(bodies.iter().chain(all_notes.iter()).map(String::as_str));
    (whole, parts)
}

/// Applies the mappings of the caller to the links and pictures of a text.
struct Finish<L, P> {
    links: RefCell<L>,
    pictures: P,
    /// The error of the first picture that failed.
    failure: Option<Error>,
    /// The addresses of pictures that the caller does not know.
    unmapped: Vec<String>,
    /// The losses.
    warnings: Vec<WarningKind>,
}

impl<L, P> Finish<L, P>
where
    L: FnMut(&str) -> Option<String>,
    P: FnMut(&str) -> Result<Option<String>>,
{
    /// The text with its links and pictures mapped.
    fn run(&mut self, text: &str) -> String {
        let Some(tree) = rewrite::parse(text) else {
            return text.to_owned();
        };
        let scan = rewrite::scan(&tree);
        let first = self.unmapped.len();
        let mut warnings = Vec::new();
        let link = |destination: &str| match (self.links.borrow_mut())(destination) {
            Some(destination) => Fate::Replace(destination),
            None => Fate::Keep,
        };
        let mut picture = |source: &str| match (self.pictures)(source) {
            Ok(Some(name)) => Some(name),
            Ok(None) => {
                // The address stays as it is: a stand-in is put back below.
                self.unmapped.push(source.to_owned());
                Some(format!("\u{1}pic{}\u{1}", self.unmapped.len() - 1))
            }
            Err(error) => {
                self.failure.get_or_insert(error);
                None
            }
        };
        let mark = |name: &str| Some(name.to_owned());
        let mut out = rewrite::rewrite(
            text,
            &tree,
            &scan,
            Rules {
                location: "",
                link: &link,
                picture: &mut picture,
                mark: &mark,
                warnings: &mut warnings,
            },
        );
        for (index, address) in self.unmapped.iter().enumerate().skip(first) {
            out = out.replace(
                &format!("%01pic{index}%01"),
                &utf8_percent_encode(address, IN_ADDRESS).to_string(),
            );
        }
        for warning in warnings {
            if matches!(warning.kind, WarningKind::MarkupDropped(_))
                && !self.warnings.contains(&warning.kind)
            {
                self.warnings.push(warning.kind);
            }
        }
        out
    }
}

/// The delimiter row of a table.
fn out_separator(table: &mut String, widths: &[usize]) {
    table.push('|');
    for width in widths {
        table.push(' ');
        table.push_str(&"-".repeat(*width));
        table.push_str(" |");
    }
    table.push('\n');
}

/// The rows of a table, without those of tables inside it, and whether it
/// has a header: a `th` or a `thead`.
fn table_rows<'a>(table: &Node<'a>) -> (Vec<Node<'a>>, bool) {
    let mut rows = Vec::new();
    let mut headed = false;
    walk(table, |node, _| match node.tag_name() {
        Some("table") if node != table => false,
        Some("thead") => {
            headed = true;
            true
        }
        Some("tr") => {
            let cells = node.children();
            headed |= cells.iter().any(|cell| cell.tag_name() == Some("th"));
            rows.push(node.clone());
            false
        }
        _ => true,
    });
    (rows, headed)
}

/// The ids of the rows and sections of a table.
fn table_ids(table: &Node<'_>) -> Vec<String> {
    let mut ids = Vec::new();
    walk(table, |node, _| {
        if node != table && matches!(node.tag_name(), Some("tr" | "thead" | "tbody" | "tfoot")) {
            ids.extend(attr(node, "id").filter(|id| !id.is_empty()));
        }
        true
    });
    ids
}

/// Whether a table has a cell that spans rows or columns.
fn has_merged_cells(table: &Node<'_>) -> bool {
    let spans = |node: &Node<'_>, name: &str| {
        attr(node, name).and_then(|value| value.trim().parse::<u32>().ok()) > Some(1)
    };
    table
        .find(|node| {
            matches!(node.tag_name(), Some("td" | "th"))
                && (spans(node, "colspan") || spans(node, "rowspan"))
        })
        .is_some()
}

/// The decoded `id` an `href` that leads into its own document names.
fn fragment(href: &str) -> Option<String> {
    let id = href.trim().strip_prefix('#')?;
    Some(percent_decode_str(id).decode_utf8_lossy().into_owned())
}

/// Whether the element is a reference to a note.
fn is_noteref(node: &Node<'_>) -> bool {
    has_token(node, "epub:type", &["noteref"]) || has_token(node, "role", &["doc-noteref"])
}

/// Whether the element is a note.
fn is_note(node: &Node<'_>) -> bool {
    has_token(node, "epub:type", &["footnote", "endnote", "rearnote"])
        || has_token(node, "role", &["doc-footnote", "doc-endnote"])
}

/// Whether an attribute with white space separated values has one of `any`.
fn has_token(node: &Node<'_>, name: &str, any: &[&str]) -> bool {
    node.attr(name)
        .is_some_and(|value| value.split_whitespace().any(|token| any.contains(&token)))
}

/// Whether the parent of the element is `tag`.
fn has_parent(node: &Node<'_>, tag: &str) -> bool {
    node.parent()
        .is_some_and(|parent| parent.tag_name() == Some(tag))
}

/// Whether an element of `tags` holds the node.
fn inside(node: &Node<'_>, tags: &[&str]) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.tag_name().is_some_and(|tag| tags.contains(&tag)) {
            return true;
        }
        current = parent.parent();
    }
    false
}

/// The value of an attribute.
fn attr(node: &Node<'_>, name: &str) -> Option<String> {
    node.attr(name).map(|value| value.to_string())
}

/// Visits a node and everything under it, in document order, without
/// recursion, with its depth under the node. `visit` returns `false` to skip
/// what is under a node.
fn walk<'a>(root: &Node<'a>, mut visit: impl FnMut(&Node<'a>, usize) -> bool) {
    let mut depth = 0;
    let mut current = root.clone();
    loop {
        let mut next = if visit(&current, depth) {
            current.first_child()
        } else {
            None
        };
        if next.is_some() {
            depth += 1;
        }
        // Down to the first child, or on to the next sibling of the nearest
        // ancestor that has one.
        current = loop {
            if let Some(node) = next {
                break node;
            }
            if current == *root {
                return;
            }
            next = current.next_sibling();
            if next.is_none() {
                let Some(parent) = current.parent() else {
                    return;
                };
                current = parent;
                depth = depth.saturating_sub(1);
            }
        };
    }
}

/// The text under a node, without the elements that are skipped.
fn all_text(root: &Node<'_>, gap: &str) -> String {
    let mut out = String::new();
    walk(root, |node, _| {
        if let Some(text) = node.text() {
            out.push_str(&text);
            out.push_str(gap);
        }
        node == root || !node.tag_name().is_some_and(|tag| SKIPPED.contains(&tag))
    });
    out
}

/// The text under a node with white space collapsed, as one line.
fn plain_text(root: &Node<'_>) -> String {
    collapse(&all_text(root, "")).trim().to_owned()
}

/// White space of HTML, runs of it as one space.
fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for character in text.chars() {
        if matches!(character, ' ' | '\t' | '\n' | '\r' | '\u{c}') {
            space = true;
        } else {
            if space {
                out.push(' ');
                space = false;
            }
            out.push(character);
        }
    }
    if space {
        out.push(' ');
    }
    out
}

/// The document as `htmd` reads it: the placeholder characters are not in
/// the source, and XHTML's empty elements are closed.
fn prepare(html: &str) -> Cow<'_, str> {
    let html = if html.contains([MARK.0, MARK.1, NOTE.0, NOTE.1]) {
        Cow::Owned(html.replace([MARK.0, MARK.1, NOTE.0, NOTE.1], "\u{fffd}"))
    } else {
        Cow::Borrowed(html)
    };
    match expand_empty_elements(&html) {
        Cow::Borrowed(_) => html,
        Cow::Owned(expanded) => Cow::Owned(expanded),
    }
}

/// XHTML writes `<a id="x"/>` and `<div/>`, which HTML reads as the start of
/// an element that is never closed. Gives every empty element of XHTML its
/// end tag.
fn expand_empty_elements(html: &str) -> Cow<'_, str> {
    const VOID: &[&str] = &[
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
        "source", "track", "wbr",
    ];
    if !html.contains("/>") {
        return Cow::Borrowed(html);
    }
    let lower = html.to_ascii_lowercase();
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len() + 16);
    let (mut at, mut copied) = (0, 0);
    while let Some(offset) = bytes
        .get(at..)
        .and_then(|rest| rest.iter().position(|&b| b == b'<'))
    {
        let start = at + offset;
        at = start + 1;
        if lower
            .get(start..)
            .is_some_and(|rest| rest.starts_with("<!--"))
        {
            at = lower
                .get(start + 4..)
                .and_then(|rest| rest.find("-->"))
                .map_or(bytes.len(), |end| start + 4 + end + 3);
            continue;
        }
        if !bytes.get(at).is_some_and(u8::is_ascii_alphabetic) {
            continue;
        }
        let name_end = at
            + bytes
                .get(at..)
                .unwrap_or_default()
                .iter()
                .take_while(|&&b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'-' | b'_'))
                .count();
        let name = lower.get(at..name_end).unwrap_or_default();
        // The end of the tag: the first `>` outside quotes.
        let mut quote = None;
        let mut end = name_end;
        while let Some(&byte) = bytes.get(end) {
            match (quote, byte) {
                (Some(open), _) if open == byte => quote = None,
                (None, b'"' | b'\'') => quote = Some(byte),
                (None, b'>') => break,
                _ => {}
            }
            end += 1;
        }
        if end >= bytes.len() {
            break;
        }
        at = end + 1;
        let empty = end > name_end && bytes.get(end - 1) == Some(&b'/');
        if empty && !VOID.contains(&name) {
            out.push_str(html.get(copied..end - 1).unwrap_or_default());
            out.push_str("></");
            out.push_str(html.get(start + 1..name_end).unwrap_or_default());
            out.push('>');
            copied = end + 1;
        } else if !empty && matches!(name, "script" | "style") {
            let close = format!("</{name}");
            at = lower
                .get(at..)
                .and_then(|rest| rest.find(&close))
                .map_or(bytes.len(), |found| at + found);
        }
    }
    out.push_str(html.get(copied..).unwrap_or_default());
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::{Document, Part, collapse, convert, expand_empty_elements};
    use crate::error::{Error, Result};
    use crate::format::Format;
    use crate::report::WarningKind;
    use ::markdown::mdast::Node;
    use ::markdown::{ParseOptions, to_mdast};
    use std::borrow::Cow;
    use std::collections::BTreeSet;

    fn run(
        html: &str,
        links: impl FnMut(&str) -> Option<String>,
        pictures: impl FnMut(&str) -> Result<Option<String>>,
    ) -> Document {
        convert(Format::Html, html, links, pictures).unwrap()
    }

    /// A document converted with the addresses left as they are.
    fn doc(html: &str) -> Document {
        run(html, |_| None, |_| Ok(None))
    }

    /// The Markdown of a fragment.
    fn md(html: &str) -> String {
        doc(html).markdown
    }

    fn parts(html: &str) -> Vec<Part> {
        doc(html).parts()
    }

    fn texts(parts: &[Part]) -> Vec<&str> {
        parts.iter().map(|part| part.markdown.as_str()).collect()
    }

    fn markup(what: &str) -> WarningKind {
        WarningKind::MarkupDropped(what.to_owned())
    }

    /// The kinds of the nodes of some Markdown.
    fn kinds(markdown: &str) -> BTreeSet<&'static str> {
        fn visit(node: &Node, kinds: &mut BTreeSet<&'static str>) {
            let kind = match node {
                Node::Html(_) => "html",
                Node::Emphasis(_) => "emphasis",
                Node::Strong(_) => "strong",
                Node::Delete(_) => "delete",
                Node::Heading(_) => "heading",
                Node::List(_) => "list",
                Node::Blockquote(_) => "quote",
                Node::Code(_) | Node::InlineCode(_) => "code",
                Node::Link(_) => "link",
                Node::Image(_) => "image",
                Node::Table(_) => "table",
                Node::ThematicBreak(_) => "rule",
                Node::Break(_) => "break",
                Node::Definition(_) => "definition",
                Node::FootnoteDefinition(_) => "footnote definition",
                Node::FootnoteReference(_) => "footnote reference",
                _ => "",
            };
            if !kind.is_empty() {
                kinds.insert(kind);
            }
            for child in node.children().map(Vec::as_slice).unwrap_or_default() {
                visit(child, kinds);
            }
        }
        let mut out = BTreeSet::new();
        visit(&to_mdast(markdown, &ParseOptions::gfm()).unwrap(), &mut out);
        out
    }

    /// The text a reader sees: the text nodes of some Markdown.
    fn text_of(markdown: &str) -> String {
        fn visit(node: &Node, out: &mut String) {
            match node {
                Node::Text(text) => out.push_str(&text.value),
                Node::Paragraph(_) if !out.is_empty() => out.push('\n'),
                _ => {}
            }
            for child in node.children().map(Vec::as_slice).unwrap_or_default() {
                visit(child, out);
            }
        }
        let mut out = String::new();
        visit(&to_mdast(markdown, &ParseOptions::gfm()).unwrap(), &mut out);
        out
    }

    // No raw HTML

    #[test]
    fn a_hostile_document_leaves_no_raw_html() {
        let markdown = md("<!-- c --><script>alert(1)</script><style>p{}</style>\
             <iframe src=\"x\">frame</iframe><unknown-tag><blink>b</blink></unknown-tag>\
             <p>a &lt;b&gt;c&lt;/b&gt; <b onclick=\"x\">d</b> <font color=red>e</font></p>\
             <object>o</object><form><input value=v><button>b</button></form>");
        assert!(!kinds(&markdown).contains("html"), "{markdown}");
        assert!(!markdown.contains("alert"));
        assert!(!markdown.contains("p{}"));
        assert_eq!(text_of(&markdown), "b\na <b>c</b> d e");
    }

    #[test]
    fn scripts_styles_and_the_head_leave_nothing() {
        assert_eq!(
            md(
                "<html><head><title>T</title><meta charset=\"utf-8\"><style>x{}</style></head>\
                <body><p>A</p><template><b>t</b></template><noscript>n</noscript></body></html>"
            ),
            "A\n"
        );
    }

    // Text

    #[test]
    fn literal_markdown_characters_are_read_back_as_text() {
        let lines = [
            "~5 km and ~10 km",
            "2 * 3 and snake_case",
            "# not a heading",
            "- not a list",
            "1. not a list",
            "> not a quote",
            "[x] and [^1] and ![alt](x)",
            "AT&T &copy;",
            "a back\\slash",
        ];
        for line in lines {
            let html = format!("<p>x</p><p>{}</p>", line.replace('&', "&amp;"));
            let markdown = md(&html);
            assert_eq!(text_of(&markdown), format!("x\n{line}"), "{markdown}");
            assert_eq!(kinds(&markdown), BTreeSet::new(), "{markdown}");
        }
    }

    #[test]
    fn white_space_is_collapsed() {
        assert_eq!(md("<p>a \n\t b   <b> c </b>d</p>"), "a b **c** d\n");
        assert_eq!(collapse(" a \n b "), " a b ");
    }

    #[test]
    fn text_outside_any_block_is_a_paragraph() {
        assert_eq!(md("plain <b>text</b>"), "plain **text**\n");
    }

    #[test]
    fn a_line_break_is_a_hard_break_and_a_space_in_a_heading_or_a_cell() {
        assert!(kinds(&md("<p>a<br>b</p>")).contains("break"));
        assert_eq!(md("<h2>a<br>b</h2>"), "## a b\n");
        assert_eq!(
            md("<table><tr><th>h</th></tr><tr><td>a<br>b</td></tr></table>"),
            "| h   |\n| --- |\n| a b |\n"
        );
    }

    // Styles

    #[test]
    fn bold_italic_strike_and_code_are_written_with_their_marks() {
        let markdown =
            md("<p><b>b</b> <i>i</i> <s>s</s> <strike>t</strike> <del>d</del> <code>c</code></p>");
        assert_eq!(markdown, "**b** *i* ~~s~~ ~~t~~ ~~d~~ `c`\n");
        assert!(kinds(&markdown).contains("delete"));
    }

    #[test]
    fn white_space_around_struck_text_stays_outside_the_marks() {
        assert_eq!(md("<p>a<s> b </s>c</p>"), "a ~~b~~ c\n");
        assert_eq!(md("<p>a<s> </s>c</p>"), "a c\n");
        assert_eq!(md("<p>a<s></s>c</p>"), "ac\n");
    }

    #[test]
    fn a_line_break_at_the_end_of_struck_text_stays_outside_the_marks() {
        let markdown = md("<p><s>a<br></s>b</p>");
        assert!(kinds(&markdown).contains("delete"), "{markdown}");
        assert!(markdown.contains("~~a~~"), "{markdown}");
    }

    #[test]
    fn subscripts_superscripts_and_underlines_keep_their_text_and_are_reported_once() {
        let document =
            doc("<p>H<sub>2</sub>O x<sup>2</sup> <u>u</u> y<sup>3</sup> H<sub>2</sub></p>");
        assert_eq!(document.markdown, "H2O x2 u y3 H2\n");
        assert_eq!(
            document.warnings,
            [markup("sub"), markup("sup"), markup("u")]
        );
    }

    #[test]
    fn elements_that_only_style_keep_their_text() {
        assert_eq!(
            md(
                "<p><span class=\"x\">a</span><font>b</font><small>c</small><q>d</q><ruby>e<rt>f</rt></ruby></p>"
            ),
            "abcdef\n"
        );
    }

    // Blocks

    #[test]
    fn headings_lists_quotes_rules_and_code_are_markdown() {
        let markdown = md(
            "<h1>T</h1><ul><li>a<ul><li>b</li></ul></li></ul><ol><li>x</li><li>y</li></ol>\
             <blockquote><p>q</p></blockquote><hr>\
             <pre><code class=\"language-rust\">fn a() {}\n</code></pre>",
        );
        assert_eq!(
            markdown,
            "# T\n\n- a\n  - b\n\n1. x\n2. y\n\n> q\n\n- - -\n\n```rust\nfn a() {}\n```\n"
        );
        let found = kinds(&markdown);
        for kind in ["heading", "list", "quote", "rule", "code"] {
            assert!(found.contains(kind), "{kind}");
        }
    }

    #[test]
    fn unknown_block_elements_keep_their_text_as_paragraphs() {
        assert_eq!(md("<div>a<section>b</section>c</div>"), "a\n\nb\n\nc\n");
    }

    // Tables

    #[test]
    fn a_table_with_a_header_is_a_gfm_table() {
        let markdown =
            md("<table><tr><th>Name</th><th>Size</th></tr><tr><td>a|b</td><td>1</td></tr></table>");
        assert_eq!(
            markdown,
            "| Name | Size |\n| ---- | ---- |\n| a\\|b | 1    |\n"
        );
        assert!(kinds(&markdown).contains("table"));
    }

    #[test]
    fn merged_cells_are_reported_once() {
        let document = doc(
            "<table><tr><th colspan=\"2\">A</th></tr><tr><td rowspan=\"2\">1</td><td>2</td></tr></table>",
        );
        assert!(
            document.markdown.starts_with("| A"),
            "{}",
            document.markdown
        );
        assert_eq!(document.warnings, [markup("table with merged cells")]);
    }

    #[test]
    fn a_span_of_one_is_not_merging() {
        let document = doc("<table><tr><th colspan=\"1\">A</th></tr></table>");
        assert_eq!(document.warnings, []);
    }

    #[test]
    fn a_table_without_a_header_has_its_first_row_as_the_header() {
        let markdown =
            md("<table><tr><td>a</td><td>b</td></tr><tr><td>1</td><td>2</td></tr></table>");
        assert_eq!(markdown, "| a | b |\n| - | - |\n| 1 | 2 |\n");
        assert!(kinds(&markdown).contains("table"));
    }

    #[test]
    fn a_short_row_of_a_headerless_table_is_padded() {
        assert_eq!(
            md("<table><tbody><tr><td>a</td><td>b</td></tr><tr><td>1</td></tr></tbody></table>"),
            "| a | b |\n| - | - |\n| 1 |   |\n"
        );
    }

    #[test]
    fn a_pipe_in_a_cell_of_a_headerless_table_does_not_end_it() {
        assert_eq!(
            md("<table><tr><td>a|b</td></tr><tr><td>c</td></tr></table>"),
            "| a\\|b |\n| ---- |\n| c    |\n"
        );
    }

    #[test]
    fn a_cell_of_a_headerless_table_keeps_its_links_and_styles() {
        let document = run(
            "<table><tr><td><b>x</b> <a href=\"#t\">t</a></td><td><p>p</p><p>q</p></td></tr></table>",
            |_| Some("to".to_owned()),
            |_| Ok(None),
        );
        assert_eq!(
            document.markdown,
            "| **x** [t](to) | p q |\n| ------------- | --- |\n"
        );
    }

    #[test]
    fn merged_cells_of_a_headerless_table_are_reported_and_it_stays_a_table() {
        let document =
            doc("<table><tr><td colspan=\"2\">A</td></tr><tr><td>1</td><td>2</td></tr></table>");
        assert_eq!(document.markdown, "| A |   |\n| - | - |\n| 1 | 2 |\n");
        assert_eq!(document.warnings, [markup("table with merged cells")]);
    }

    #[test]
    fn a_headerless_table_of_one_row_is_a_header_alone() {
        assert_eq!(
            md("<table><tr><td>a</td><td>b</td></tr></table>"),
            "| a | b |\n| - | - |\n"
        );
    }

    #[test]
    fn a_table_without_cells_writes_nothing() {
        assert_eq!(md("<table></table><p>A</p>"), "A\n");
        assert_eq!(md("<table><tr></tr></table><p>A</p>"), "A\n");
    }

    #[test]
    fn a_headerless_table_keeps_its_caption_and_a_nested_table_is_flattened() {
        assert_eq!(
            md("<table><caption>Cap</caption><tr><td>a</td></tr></table>"),
            "Cap\n\n| a |\n| - |\n"
        );
        assert_eq!(
            md("<table><tr><td><table><tr><td>in</td></tr></table></td><td>b</td></tr></table>"),
            "| in | b |\n| -- | - |\n"
        );
    }

    #[test]
    fn an_id_on_a_row_or_a_section_is_marked_in_front_of_the_table() {
        let document = run(
            "<p>A</p><table id=\"t\"><tbody id=\"b\"><tr id=\"r\"><td><a href=\"#r\">x</a></td></tr></tbody></table>",
            |href| Some(format!("dest{href}")),
            |_| Ok(None),
        );
        assert_eq!(
            document.markdown,
            "A\n\n[t]: #t\n\n[b]: #b\n\n[r]: #r\n\n| [x](dest#r) |\n| ------- |\n"
        );
        let headed = md("<table><thead id=\"h\"><tr id=\"r\"><th>H</th></tr></thead></table>");
        assert!(headed.starts_with("[h]: #h\n\n[r]: #r\n\n| H"), "{headed}");
        assert!(kinds(&headed).contains("table"));
    }

    // Marks

    #[test]
    fn an_id_is_marked_in_front_of_its_paragraph_and_heading() {
        assert_eq!(md("<p id=\"a\">A</p>"), "[a]: #a\n\nA\n");
        assert_eq!(md("<h2 id=\"h\">T</h2><p>B</p>"), "[h]: #h\n\n## T\n\nB\n");
    }

    #[test]
    fn an_id_on_an_element_wrapping_a_heading_marks_the_heading() {
        assert_eq!(
            md("<div id=\"w\"><section><h1>T</h1><p>B</p></section></div>"),
            "[w]: #w\n\n# T\n\nB\n"
        );
    }

    #[test]
    fn an_empty_anchor_before_a_heading_marks_the_heading() {
        assert_eq!(md("<a id=\"x\"></a><h1>T</h1>"), "[x]: #x\n\n# T\n");
        assert_eq!(md("<a id=\"x\"/><h1>T</h1>"), "[x]: #x\n\n# T\n");
        assert_eq!(
            md("<p>A</p><div id=\"y\"/><h1>T</h1>"),
            "A\n\n[y]: #y\n\n# T\n"
        );
    }

    #[test]
    fn an_id_inside_a_paragraph_is_marked_in_front_of_the_paragraph() {
        assert_eq!(
            md("<p>A <b id=\"b\">B</b> <a id=\"c\">C</a> <span id=\"d\">D</span></p>"),
            "[b]: #b\n\n[c]: #c\n\n[d]: #d\n\nA **B** C D\n"
        );
    }

    #[test]
    fn a_name_of_an_anchor_is_a_target() {
        assert_eq!(md("<p>A<a name=\"n\"></a></p>"), "[n]: #n\n\nA\n");
    }

    #[test]
    fn an_id_in_a_list_item_is_marked_in_front_of_the_list() {
        let markdown =
            md("<p>A</p><ul><li id=\"i\">one</li><li>two</li></ul><ol><li id=\"j\">x</li></ol>");
        assert_eq!(
            markdown,
            "A\n\n[i]: #i\n\n- one\n- two\n\n[j]: #j\n\n1. x\n"
        );
    }

    #[test]
    fn an_id_in_a_table_cell_is_marked_in_front_of_the_table() {
        assert_eq!(
            md("<table><tr><th id=\"h\">H</th></tr><tr><td id=\"c\">C</td></tr></table>"),
            "[h]: #h\n\n[c]: #c\n\n| H |\n| ---- |\n| C |\n"
        );
    }

    #[test]
    fn an_id_in_a_block_quote_or_code_is_marked_in_front_of_the_block() {
        assert_eq!(
            md("<blockquote id=\"q\"><p id=\"p\">Q</p></blockquote><pre id=\"c\">x</pre>"),
            "[q]: #q\n\n[p]: #p\n\n> Q\n\n[c]: #c\n\n```\nx\n```\n"
        );
    }

    #[test]
    fn a_mark_is_never_inside_a_block() {
        let markdown = md("<p id=\"a\">A <i id=\"b\">B</i></p><ul><li><p id=\"c\">C</p></li></ul>");
        assert_eq!(markdown, "[a]: #a\n\n[b]: #b\n\nA *B*\n\n[c]: #c\n\n- C\n");
        assert!(!markdown.contains('\u{e000}'));
    }

    #[test]
    fn a_pre_without_code_is_a_code_block_with_its_lines() {
        assert_eq!(md("<pre>a\n  b ``` c</pre>"), "````\na\n  b ``` c\n````\n");
        assert_eq!(md("<pre><span>x</span>\ny</pre>"), "```\nx\ny\n```\n");
        assert_eq!(md("<pre> \n </pre><p>A</p>"), "A\n");
    }

    #[test]
    fn an_id_on_a_list_a_rule_a_table_or_code_is_marked_in_front_of_it() {
        assert_eq!(
            md(
                "<ol id=\"o\"><li>x</li></ol><hr id=\"r\"><table id=\"t\"><tr><th>a</th></tr></table>\
                <pre id=\"c\"><code>x</code></pre><ul id=\"u\"><li>y</li></ul>"
            ),
            "[o]: #o\n\n1. x\n\n[r]: #r\n\n- - -\n\n[t]: #t\n\n| a |\n| - |\n\n[c]: #c\n\n```\nx\n```\n\n[u]: #u\n\n- y\n"
        );
    }

    #[test]
    fn a_name_is_marked_once() {
        assert_eq!(
            md("<p id=\"a\">A</p><p id=\"a\">B</p><p id=\"a b\">C</p><p id=\"a-b\">D</p>"),
            "[a]: #a\n\nA\n\nB\n\n[a-b]: #a-b\n\nC\n\nD\n"
        );
    }

    #[test]
    fn marks_at_the_end_go_in_front_of_the_last_block() {
        assert_eq!(
            md("<p>A</p><p>B</p><a id=\"end\"></a>"),
            "A\n\n[end]: #end\n\nB\n"
        );
        assert_eq!(md("<a id=\"alone\"></a>"), "");
    }

    #[test]
    fn an_id_of_a_dropped_element_is_not_a_target() {
        assert_eq!(md("<script id=\"s\">x</script><p>A</p>"), "A\n");
    }

    #[test]
    fn placeholder_characters_of_the_source_are_not_placeholders() {
        let markdown = md("<p id=\"a\">A\u{e000}7\u{e001} B</p>");
        assert_eq!(markdown, "[a]: #a\n\nA\u{fffd}7\u{fffd} B\n");
    }

    // Links

    #[test]
    fn a_link_gets_the_destination_the_caller_gives() {
        let mut asked = Vec::new();
        let document = run(
            "<p><a href=\"ch2.html#x\">two</a> <a href=\"#y\" title=\"T\">here</a></p>",
            |href| {
                asked.push(href.to_owned());
                Some(format!("new/{}", href.len()))
            },
            |_| Ok(None),
        );
        assert_eq!(asked, ["ch2.html#x", "#y"]);
        assert_eq!(document.markdown, "[two](new/10) [here](new/2 \"T\")\n");
    }

    #[test]
    fn a_link_the_caller_does_not_map_is_written_as_it_is() {
        assert_eq!(
            md("<p><a href=\"http://e.org/a b\">e</a> <a href=\"mailto:m@e.org\">m</a></p>"),
            "[e](<http://e.org/a b>) [m](mailto:m@e.org)\n"
        );
    }

    #[test]
    fn an_anchor_without_href_is_its_text() {
        assert_eq!(md("<p><a>text</a></p>"), "text\n");
    }

    // Pictures

    #[test]
    fn a_picture_gets_the_name_the_caller_gives() {
        let mut asked = Vec::new();
        let document = run(
            "<p><img src=\"a b.png\" alt=\"A\" title=\"T\"> <img src=\"c.png\"></p>",
            |_| None,
            |src| {
                asked.push(src.to_owned());
                Ok(Some(format!("img/{}.avif", asked.len())))
            },
        );
        assert_eq!(asked, ["a b.png", "c.png"]);
        assert_eq!(
            document.markdown,
            "![A](img/1.avif \"T\") ![](img/2.avif)\n"
        );
    }

    #[test]
    fn a_picture_the_caller_does_not_know_keeps_its_address() {
        assert_eq!(
            md("<p><img src=\"x%20y.png?v=1#a\" alt=\"A\"></p>"),
            "![A](x%20y.png?v=1#a)\n"
        );
    }

    #[test]
    fn an_error_of_the_caller_stops_the_conversion() {
        let result = convert(
            Format::Epub,
            "<p><img src=\"a.png\"><img src=\"b.png\"></p>",
            |_| None,
            |src| {
                if src == "b.png" {
                    Err(Error::Limit("too many pictures"))
                } else {
                    Ok(None)
                }
            },
        );
        assert!(matches!(result, Err(Error::Limit("too many pictures"))));
    }

    #[test]
    fn an_svg_image_is_a_picture_and_the_rest_of_the_svg_leaves_nothing() {
        let document = run(
            "<p>a<svg><title>T</title><text>t</text><image href=\"p q.png\"/></svg>b</p>",
            |_| None,
            |src| Ok(Some(format!("img/{src}"))),
        );
        assert_eq!(document.markdown, "a![](img/p%20q.png)b\n");
        assert_eq!(md("<p>a<svg><text>t</text></svg>b</p>"), "ab\n");
    }

    #[test]
    fn a_picture_inside_a_link_keeps_working() {
        let document = run(
            "<p><a href=\"big.png\"><img src=\"s.png\" alt=\"S\"></a></p>",
            |_| Some("L".to_owned()),
            |_| Ok(Some("P".to_owned())),
        );
        assert_eq!(document.markdown, "[![S](P)](L)\n");
    }

    #[test]
    fn an_svg_image_address_is_written_so_that_it_reads_back() {
        let mut asked = Vec::new();
        let document = run(
            "<svg><image href=\" \"/><image href=\"a&lt;b\\c&gt;.png\"/><image href=\"x\ny\"/></svg>",
            |_| None,
            |src| {
                asked.push(src.to_owned());
                Ok(None)
            },
        );
        assert_eq!(asked, ["a<b\\c>.png", "x%0Ay"]);
        assert!(!kinds(&document.markdown).contains("html"));
    }

    // Figures

    #[test]
    fn a_figure_with_a_caption_is_a_picture_with_a_title() {
        let document = run(
            "<figure><img src=\"a.png\" alt=\"A\"><figcaption>Figure <b>1</b>: \"A\" box.</figcaption></figure>",
            |_| None,
            |_| Ok(Some("img/1.avif".to_owned())),
        );
        assert_eq!(
            document.markdown,
            "![A](img/1.avif \"Figure 1: \\\"A\\\" box.\")\n"
        );
    }

    #[test]
    fn a_figure_with_several_pictures_captions_the_last() {
        let document = run(
            "<figure><img src=\"a.png\" alt=\"A\"><img src=\"b.png\" alt=\"B\"><figcaption>Two</figcaption></figure>",
            |_| None,
            |src| Ok(Some(format!("img/{src}"))),
        );
        assert_eq!(
            document.markdown,
            "![A](img/a.png)\n\n![B](img/b.png \"Two\")\n"
        );
    }

    #[test]
    fn a_figure_keeps_the_title_of_its_picture() {
        assert_eq!(
            md(
                "<figure> <img src=\"a.png\" alt=\"A\" title=\"Own\"> <figcaption>Cap</figcaption></figure>"
            ),
            "![A](a.png \"Own\")\n"
        );
    }

    #[test]
    fn a_figure_with_other_content_is_a_container() {
        assert_eq!(
            md("<figure><p>Text</p><figcaption>Cap</figcaption></figure>"),
            "Text\n\nCap\n"
        );
        assert_eq!(md("<figure><figcaption>Cap</figcaption></figure>"), "Cap\n");
    }

    #[test]
    fn a_figure_with_an_svg_picture_is_a_picture_with_a_title() {
        let document = run(
            "<figure><svg><image href=\"a.svg\"/></svg><figcaption>Cap</figcaption></figure>",
            |_| None,
            |_| Ok(Some("img/1.avif".to_owned())),
        );
        assert_eq!(document.markdown, "![](img/1.avif \"Cap\")\n");
    }

    // Footnotes

    #[test]
    fn a_note_reference_and_its_note_become_a_footnote() {
        let document = doc("<p>A<a epub:type=\"noteref\" href=\"#n1\">1</a></p>\
             <aside epub:type=\"footnote\" id=\"n1\"><p>Note <b>text</b>.</p></aside><p>B</p>");
        assert_eq!(document.markdown, "A[^n1]\n\nB\n\n[^n1]: Note **text**.\n");
        let found = kinds(&document.markdown);
        assert!(found.contains("footnote definition") && found.contains("footnote reference"));
    }

    #[test]
    fn the_roles_of_dpub_mark_notes_too() {
        assert_eq!(
            md(
                "<p>A<a role=\"doc-noteref\" href=\"#n\">1</a></p><li role=\"doc-endnote\" id=\"n\">N</li>"
            ),
            "A[^n]\n\n[^n]: N\n"
        );
    }

    #[test]
    fn a_note_with_several_paragraphs_continues_with_four_spaces() {
        assert_eq!(
            md("<p>A<a epub:type=\"noteref\" href=\"#n\">1</a></p>\
                <div epub:type=\"footnote\" id=\"n\"><p>One</p><p>Two</p></div>"),
            "A[^n]\n\n[^n]: One\n\n    Two\n"
        );
    }

    #[test]
    fn a_note_cited_twice_is_written_once() {
        assert_eq!(
            md(
                "<p>A<a role=\"doc-noteref\" href=\"#n\">1</a>B<a role=\"doc-noteref\" href=\"#n\">1</a></p>\
                <aside role=\"doc-footnote\" id=\"n\">N</aside>"
            ),
            "A[^n]B[^n]\n\n[^n]: N\n"
        );
    }

    #[test]
    fn a_note_reference_to_another_document_stays_a_link() {
        let document = run(
            "<p>A<a epub:type=\"noteref\" href=\"notes.xhtml#n\">1</a></p>",
            |_| Some("notes.md#n".to_owned()),
            |_| Ok(None),
        );
        assert_eq!(document.markdown, "A[1](notes.md#n)\n");
    }

    #[test]
    fn a_note_reference_to_something_that_is_no_note_stays_a_link() {
        let document = run(
            "<p>A<a epub:type=\"noteref\" href=\"#x\">1</a></p><p id=\"x\">P</p>",
            |href| Some(format!("self{href}")),
            |_| Ok(None),
        );
        assert_eq!(document.markdown, "A[1](self#x)\n\n[x]: #x\n\nP\n");
    }

    #[test]
    fn a_note_nobody_refers_to_stays_where_it_stands() {
        assert_eq!(
            md("<p>A</p><aside epub:type=\"footnote\" id=\"n\">N</aside><p>B</p>"),
            "A\n\n[n]: #n\n\nN\n\nB\n"
        );
    }

    #[test]
    fn a_note_with_no_text_leaves_no_reference() {
        assert_eq!(
            md(
                "<p>A<a epub:type=\"noteref\" href=\"#n\">1</a></p><aside epub:type=\"footnote\" id=\"n\"> </aside>"
            ),
            "A\n"
        );
    }

    #[test]
    fn a_note_may_refer_to_another_note() {
        assert_eq!(
            md("<p>A<a role=\"doc-noteref\" href=\"#a\">1</a></p>\
                <aside role=\"doc-footnote\" id=\"a\">See<a role=\"doc-noteref\" href=\"#b\">2</a>.</aside>\
                <aside role=\"doc-footnote\" id=\"b\">Two</aside>"),
            "A[^a]\n\n[^a]: See[^b].\n\n[^b]: Two\n"
        );
    }

    #[test]
    fn an_id_inside_a_note_is_marked_in_front_of_the_note() {
        assert_eq!(
            md("<p>A<a role=\"doc-noteref\" href=\"#n\">1</a></p>\
                <aside role=\"doc-footnote\" id=\"n\"><span id=\"in\">N</span></aside>"),
            "A[^n]\n\n[in]: #in\n\n[^n]: N\n"
        );
    }

    #[test]
    fn notes_with_ids_of_one_label_get_different_labels() {
        let markdown = md(
            "<p>A<a role=\"doc-noteref\" href=\"#a b\">1</a><a role=\"doc-noteref\" href=\"#a-b\">2</a></p>\
             <aside role=\"doc-footnote\" id=\"a b\">One</aside><aside role=\"doc-footnote\" id=\"a-b\">Two</aside>",
        );
        assert_eq!(
            markdown,
            "A[^a-b][^a-b-2]\n\n[^a-b]: One\n\n[^a-b-2]: Two\n"
        );
    }

    // Parts

    #[test]
    fn a_document_without_headings_is_one_part() {
        assert_eq!(texts(&parts("<p>A</p><p>B</p>")), ["A\n\nB\n"]);
    }

    #[test]
    fn a_document_is_cut_at_its_h1_headings() {
        assert_eq!(
            texts(&parts(
                "<p>0</p><h1>One</h1><p>A</p><h2>Sub</h2><h1>Two</h1><p>B</p>"
            )),
            ["0\n", "# One\n\nA\n\n## Sub\n", "# Two\n\nB\n"]
        );
    }

    #[test]
    fn a_document_that_starts_with_a_heading_has_no_empty_part_in_front() {
        assert_eq!(texts(&parts("<h1>One</h1><p>A</p>")), ["# One\n\nA\n"]);
    }

    #[test]
    fn a_heading_in_a_container_is_a_cut_but_in_a_quote_or_a_list_it_is_not() {
        assert_eq!(parts("<div><h1>A</h1></div><div><h1>B</h1></div>").len(), 2);
        assert_eq!(
            parts("<p>x</p><blockquote><h1>A</h1></blockquote><ul><li><h1>B</h1></li></ul>").len(),
            1
        );
    }

    #[test]
    fn every_id_is_listed_with_the_part_that_holds_its_mark() {
        let found = parts(
            "<p id=\"p0\">0</p><a id=\"a\"/><h1 id=\"h1\">One</h1><p id=\"p1\">A</p>\
             <h1>Two</h1><p>B<i id=\"deep\">x</i></p><p id=\"p1\">dup</p>",
        );
        let ids: Vec<&[String]> = found.iter().map(|part| part.ids.as_slice()).collect();
        assert_eq!(
            ids,
            [
                &["p0".to_owned()][..],
                &["a".into(), "h1".into(), "p1".into()],
                &["deep".into()]
            ]
        );
    }

    #[test]
    fn a_note_is_written_at_the_end_of_the_part_that_refers_to_it() {
        let found = parts(
            "<h1>One</h1><p>A</p><h1>Two</h1><p>B<a epub:type=\"noteref\" href=\"#n1\">1</a></p>\
             <aside epub:type=\"footnote\" id=\"n1\">N1</aside>",
        );
        assert_eq!(
            texts(&found),
            ["# One\n\nA\n", "# Two\n\nB[^n1]\n\n[^n1]: N1\n"]
        );
    }

    #[test]
    fn the_whole_text_has_the_notes_at_its_end() {
        let document = doc(
            "<h1>One</h1><p>A<a epub:type=\"noteref\" href=\"#n1\">1</a></p><h1>Two</h1><p>B</p>\
             <aside epub:type=\"footnote\" id=\"n1\">N1</aside>",
        );
        assert_eq!(
            document.markdown,
            "# One\n\nA[^n1]\n\n# Two\n\nB\n\n[^n1]: N1\n"
        );
    }

    #[test]
    fn a_document_with_no_text_has_no_parts() {
        assert_eq!(parts(""), []);
        assert_eq!(parts("<div> </div><script>x</script>"), []);
    }

    #[test]
    fn cutting_does_not_depend_on_the_links() {
        let source = "<h1>A</h1><p><a href=\"#a\">x</a></p><h1 id=\"a\">B</h1>";
        let plain = doc(source).parts();
        let mapped = run(source, |_| Some("elsewhere".to_owned()), |_| Ok(None)).parts();
        assert_eq!(plain.len(), mapped.len());
        assert_eq!(plain[1].ids, mapped[1].ids);
    }

    // Metadata

    #[test]
    fn the_metadata_of_the_head_is_read() {
        let document = doc("<html lang=\" de \"><head><title> A \n Title </title>\
             <meta name=\"Author\" content=\"One\"><meta name=\"author\" content=\" Two \">\
             <meta name=\"description\" content=\"D\"><meta name=\"description\" content=\"E\">\
             <meta name=\"keywords\" content=\"a, b\"><meta name=\"other\" content=\"x\">\
             <meta name=\"author\" content=\" \"></head><body><p>T</p></body></html>");
        assert_eq!(document.title.as_deref(), Some("A Title"));
        assert_eq!(document.lang.as_deref(), Some("de"));
        assert_eq!(document.authors, ["One", "Two"]);
        assert_eq!(document.description.as_deref(), Some("D"));
        assert_eq!(document.keywords.as_deref(), Some("a, b"));
        assert_eq!(document.markdown, "T\n");
    }

    #[test]
    fn the_language_may_be_an_xml_attribute() {
        assert_eq!(
            doc("<html xml:lang=\"fr\"><body/></html>").lang.as_deref(),
            Some("fr")
        );
    }

    #[test]
    fn a_title_outside_the_head_is_not_the_title_of_the_document() {
        let document = doc("<body><svg><title>T</title></svg><p>A</p></body>");
        assert_eq!(document.title, None);
        assert_eq!(document.markdown, "A\n");
    }

    #[test]
    fn a_fragment_without_html_is_converted() {
        let document = doc("<p>A</p><p>B</p>");
        assert_eq!(document.markdown, "A\n\nB\n");
        assert_eq!((document.title, document.lang), (None, None));
        assert_eq!(doc("").markdown, "");
    }

    // XHTML

    #[test]
    fn an_empty_element_of_xhtml_is_closed() {
        assert_eq!(md("<div><a id=\"x\"/><p>A</p></div>"), "[x]: #x\n\nA\n");
        assert_eq!(md("<p>A<br/>B</p>").lines().count(), 2);
    }

    #[test]
    fn void_elements_comments_and_scripts_are_left_as_they_are() {
        let html =
            "<p>a<br/>b<img src=\"x\"/></p><!-- <x/> --><script>if (a<b) x = '<y/>';</script>";
        let expanded = expand_empty_elements(html);
        assert_eq!(expanded, html);
        assert!(matches!(expanded, Cow::Owned(_)));
        assert!(matches!(
            expand_empty_elements("<p>a</p>"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn an_unfinished_tag_is_left_alone() {
        assert_eq!(
            expand_empty_elements("<p>a</p><div id=\"x\"/"),
            "<p>a</p><div id=\"x\"/"
        );
        assert_eq!(
            expand_empty_elements("<div id=\"a>b\"/>"),
            "<div id=\"a>b\"></div>"
        );
    }

    // Limits

    #[test]
    fn elements_nested_too_deep_are_kept_as_text() {
        let html = format!(
            "{}x &amp; y{}<p>after</p>",
            "<div>".repeat(1_000),
            "</div>".repeat(1_000)
        );
        let document = doc(&html);
        assert_eq!(document.markdown, "x \\& y after\n");
        assert_eq!(document.warnings, [markup("deeply nested elements")]);
        assert_eq!(document.parts().len(), 1);
    }

    #[test]
    fn nesting_up_to_the_limit_is_converted() {
        let html = format!("{}x{}", "<div>".repeat(100), "</div>".repeat(100));
        assert_eq!(md(&html), "x\n");
    }
}
