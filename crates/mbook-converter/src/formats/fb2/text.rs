//! The text of a `FictionBook` document as Markdown: blocks, inline markup,
//! links, notes and pictures.
//!
//! The document is walked as a tree of elements named as the format names
//! them. Everything is written in the order of the document. An element the
//! format does not know is not lost: at the level of blocks it is a
//! container when it holds blocks and a paragraph otherwise, inside a line
//! its children are written as if it was not there.

use super::tree::{Element, Node};
use crate::markdown;
use crate::report::{Warning, WarningKind};
use std::collections::{HashMap, HashSet};

/// Starts the placeholder of a link into the book.
const OPEN: char = '\u{e000}';
/// Ends the placeholder of a link into the book.
const CLOSE: char = '\u{e001}';
/// The deepest heading Markdown has.
const MAX_LEVEL: usize = 6;
/// The text of a note that has none.
const EMPTY_NOTE: &str = "\u{2026}";
/// The marker of a footnote definition continues in this indentation.
const CONTINUATION: &str = "    ";
/// Elements that stand for a block: an unknown element that holds one is a
/// container, any other unknown element is a paragraph.
const BLOCKS: [&str; 12] = [
    "p",
    "subtitle",
    "poem",
    "cite",
    "table",
    "epigraph",
    "section",
    "title",
    "annotation",
    "empty-line",
    "stanza",
    "text-author",
];

/// The name of the section made of what the main body has in front of its
/// sections.
const FRONT: &str = "front";

/// Where a block is written.
#[derive(Clone, Copy)]
enum Place {
    /// In a section of this depth: a subtitle is a heading.
    Section(usize),
    /// In a block quote: a subtitle is a bold paragraph.
    Quote,
}

/// The text of the node and everything below it without any markup.
fn push_plain(nodes: &[Node], out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(text) => out.push_str(text),
            Node::Element(element) if element.name == "image" => {}
            Node::Element(element) => push_plain(&element.children, out),
        }
    }
}

/// The text of an element without markup, on one line.
pub(super) fn plain(element: &Element) -> String {
    let mut out = String::new();
    push_plain(&element.children, &mut out);
    collapse(&out).trim_matches(' ').to_owned()
}

/// White space runs of the source are one space, and the characters that
/// mark a link into the book are left out.
fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for character in text.chars().filter(|c| !matches!(*c, OPEN | CLOSE)) {
        if character.is_ascii_whitespace() {
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

/// Text of the source as Markdown. A space at either end is kept, as one
/// space, because the text next to it may be inline markup.
fn fragment(text: &str) -> String {
    let collapsed = collapse(text);
    let core = collapsed.trim_matches(' ');
    if core.is_empty() {
        return if collapsed.is_empty() {
            String::new()
        } else {
            " ".to_owned()
        };
    }
    let mut out = String::with_capacity(collapsed.len() + 2);
    if collapsed.starts_with(' ') {
        out.push(' ');
    }
    out.push_str(&markdown::escape(core));
    if collapsed.ends_with(' ') {
        out.push(' ');
    }
    out
}

/// Inline code: a fence longer than any run of backticks in the text. A
/// space at either end of the source stays outside the fence.
fn code(text: &str) -> String {
    let collapsed = collapse(text);
    let core = collapsed.trim_matches(' ');
    if core.is_empty() {
        return if collapsed.is_empty() {
            String::new()
        } else {
            " ".to_owned()
        };
    }
    let longest = core
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or_default();
    let fence = "`".repeat(longest + 1);
    let pad = if core.starts_with('`') || core.ends_with('`') {
        " "
    } else {
        ""
    };
    let lead = if collapsed.starts_with(' ') { " " } else { "" };
    let trail = if collapsed.ends_with(' ') { " " } else { "" };
    format!("{lead}{fence}{pad}{core}{pad}{fence}{trail}")
}

/// Moves the spaces at both ends of marked-up text out of the markup, which
/// Markdown does not accept inside it.
fn surround(inner: &str, open: &str, close: &str) -> String {
    let core = inner.trim_matches(' ');
    if core.is_empty() {
        return inner.to_owned();
    }
    let lead = &inner[..inner.len() - inner.trim_start_matches(' ').len()];
    let trail = &inner[inner.trim_end_matches(' ').len()..];
    format!("{lead}{open}{core}{close}{trail}")
}

/// Appends a piece of a line. Two spaces that meet are one.
fn push_piece(out: &mut String, piece: &str) {
    match piece.strip_prefix(' ') {
        Some(rest) if out.ends_with(' ') => out.push_str(rest),
        _ => out.push_str(piece),
    }
}

/// An address outside the book as the destination of a link.
fn external_destination(href: &str) -> String {
    let mut out = String::with_capacity(href.len());
    for character in href.chars() {
        match character {
            ' ' => out.push_str("%20"),
            '"' => out.push_str("%22"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            '[' => out.push_str("%5B"),
            '\\' => out.push_str("%5C"),
            ']' => out.push_str("%5D"),
            '`' => out.push_str("%60"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Prefixes every line of a block with the mark of a block quote.
fn quoted(block: &str) -> String {
    block
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                ">".to_owned()
            } else {
                format!("> {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The identifier a `l:href` leads to, when it is a reference inside the
/// document.
fn local_id(href: Option<&str>) -> Option<String> {
    href.and_then(|href| href.trim().strip_prefix('#'))
        .map(|id| id.chars().filter(|c| !matches!(*c, OPEN | CLOSE)).collect())
}

/// Writes the parts of a document as Markdown.
pub(super) struct Renderer<'a> {
    /// The sections of the bodies of notes, by identifier.
    notes: HashMap<String, &'a Element>,
    /// The names of the pictures of the draft.
    pictures: HashSet<String>,
    /// The name of the draft section that holds each marked element.
    ids: HashMap<String, String>,
    /// The identifiers that already have a mark.
    marked: HashSet<String>,
    /// What the front end could not keep.
    pub(super) warnings: Vec<Warning>,
    /// The draft section being written.
    section: String,
    /// The losses already reported in this section.
    warned: HashSet<&'static str>,
    /// The notes this section refers to, in order of appearance.
    used_notes: Vec<String>,
    /// The text of a note is being written.
    in_note: bool,
    /// How deep in block quotes the writer is.
    quoting: usize,
    /// Marks of elements inside a block quote, written in front of it.
    hoisted: Vec<String>,
}

impl<'a> Renderer<'a> {
    /// A writer for a document with these notes and pictures.
    pub(super) fn new(notes: HashMap<String, &'a Element>, pictures: HashSet<String>) -> Self {
        Self {
            notes,
            pictures,
            ids: HashMap::new(),
            marked: HashSet::new(),
            warnings: Vec::new(),
            section: String::new(),
            warned: HashSet::new(),
            used_notes: Vec::new(),
            in_note: false,
            quoting: 0,
            hoisted: Vec::new(),
        }
    }

    /// The sections of a body with their Markdown still unresolved.
    ///
    /// Each top-level `section` is `s1`, `s2`, and so on. What stands in
    /// front of the first one (a title, epigraphs, a picture) is `front`;
    /// what stands between or after sections is `t1`, `t2`, and so on.
    pub(super) fn render_body(&mut self, body: &Element) -> Vec<(String, String)> {
        let mut texts = Vec::new();
        let mut sections = 0_usize;
        let mut runs = 0_usize;
        let mut run: Option<(String, Vec<String>)> = None;
        for node in &body.children {
            match node {
                Node::Element(child) if child.name == "section" => {
                    self.flush(&mut run, &mut texts);
                    sections += 1;
                    let name = format!("s{sections}");
                    self.begin(&name);
                    let mut out = Vec::new();
                    self.section(child, 1, &mut out);
                    let markdown = self.finish(out);
                    texts.push((name, markdown));
                }
                Node::Text(text) if text.trim().is_empty() => {}
                loose => {
                    if run.is_none() {
                        let name = if sections == 0 && runs == 0 {
                            FRONT.to_owned()
                        } else {
                            format!("t{}", runs + 1)
                        };
                        runs += 1;
                        self.begin(&name);
                        run = Some((name, Vec::new()));
                    }
                    if let Some((_, out)) = &mut run {
                        self.node(loose, Place::Section(1), out);
                    }
                }
            }
        }
        self.flush(&mut run, &mut texts);
        texts
    }

    /// Ends the section of loose blocks that is being written.
    fn flush(
        &mut self,
        run: &mut Option<(String, Vec<String>)>,
        texts: &mut Vec<(String, String)>,
    ) {
        if let Some((name, out)) = run.take() {
            let markdown = self.finish(out);
            texts.push((name, markdown));
        }
    }

    /// Replaces the placeholders of links into the book by their targets.
    /// A link to an identifier that no section holds leads into its own
    /// section, where the assembler finds nothing and drops it.
    pub(super) fn resolve(&self, markdown: &str) -> String {
        let mut out = String::with_capacity(markdown.len());
        let mut rest = markdown;
        while let Some((before, after)) = rest.split_once(OPEN) {
            out.push_str(before);
            // A placeholder is always closed: the text of the source has no markers.
            let (id, tail) = after.split_once(CLOSE).unwrap_or((after, ""));
            let section = self.ids.get(id).map_or("", String::as_str);
            out.push_str(&markdown::target(
                &markdown::destination(section, Some(id)),
                None,
            ));
            rest = tail;
        }
        out.push_str(rest);
        out
    }

    /// Starts a section of the draft.
    fn begin(&mut self, name: &str) {
        name.clone_into(&mut self.section);
        self.warned.clear();
        self.used_notes.clear();
    }

    /// Ends a section: the notes it refers to are defined after its blocks.
    fn finish(&mut self, mut out: Vec<String>) -> String {
        let used = std::mem::take(&mut self.used_notes);
        for id in used {
            let text = self.note_text(&id);
            out.push(format!("[^{}]: {text}", markdown::anchor_name(&id)));
        }
        out.join("\n\n")
    }

    /// Reports a kind of markup once in a section.
    fn warn_once(&mut self, what: &'static str) {
        if self.warned.insert(what) {
            self.warnings.push(Warning::new(
                WarningKind::MarkupDropped(what.to_owned()),
                &self.section,
            ));
        }
    }

    /// Takes the identifier of an element for the section being written,
    /// unless it is empty, taken before, or inside the text of a note.
    fn claim(&mut self, id: Option<&str>) -> Option<String> {
        let id = id.map(str::trim).filter(|id| !id.is_empty())?;
        if self.in_note || !self.marked.insert(id.to_owned()) {
            return None;
        }
        self.ids.insert(id.to_owned(), self.section.clone());
        Some(id.to_owned())
    }

    /// Gives an identifier back, because what it marked wrote nothing.
    fn release(&mut self, id: &str) {
        self.marked.remove(id);
        self.ids.remove(id);
    }

    /// Writes the mark of an element in front of its block. Inside a block
    /// quote the mark is written in front of the quote, and in the text of
    /// a note there are no marks.
    fn mark(&mut self, id: Option<&str>, out: &mut Vec<String>) {
        let Some(id) = self.claim(id) else {
            return;
        };
        let mark = markdown::definition(&markdown::anchor_name(&id));
        if self.quoting > 0 {
            self.hoisted.push(mark);
        } else {
            out.push(mark);
        }
    }

    /// Starts a block quote.
    fn begin_quote(&mut self) {
        self.quoting += 1;
    }

    /// Ends a block quote whose blocks are `inner`.
    fn end_quote(&mut self, inner: &[String], out: &mut Vec<String>) {
        self.quoting = self.quoting.saturating_sub(1);
        if self.quoting == 0 {
            out.append(&mut self.hoisted);
        }
        if !inner.is_empty() {
            out.push(quoted(&inner.join("\n\n")));
        }
    }

    /// The definition text of a note, continuation lines indented.
    fn note_text(&mut self, id: &str) -> String {
        let Some(section) = self.notes.get(id).copied() else {
            return EMPTY_NOTE.to_owned();
        };
        let was = std::mem::replace(&mut self.in_note, true);
        let mut blocks = Vec::new();
        for node in &section.children {
            let skipped = matches!(
                node,
                Node::Element(child)
                    if matches!(
                        child.name.as_str(),
                        "section" | "title" | "epigraph" | "annotation"
                    )
            );
            if !skipped {
                self.node(node, Place::Section(1), &mut blocks);
            }
        }
        if blocks.is_empty()
            && let Some(title) = section.child("title")
        {
            let text = self.title_line(title);
            if !text.is_empty() {
                blocks.push(text);
            }
        }
        self.in_note = was;
        if blocks.is_empty() {
            return EMPTY_NOTE.to_owned();
        }
        blocks
            .join("\n\n")
            .split('\n')
            .enumerate()
            .map(|(index, line)| {
                if index > 0 && !line.is_empty() {
                    format!("{CONTINUATION}{line}")
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// A section at a depth, everything in it in the order of the document.
    /// A section that writes nothing is not marked.
    fn section(&mut self, section: &Element, depth: usize, out: &mut Vec<String>) {
        let claimed = self.claim(section.attr("id"));
        let start = out.len();
        for node in &section.children {
            self.node(node, Place::Section(depth), out);
        }
        match claimed {
            Some(id) if out.len() > start => {
                out.insert(start, markdown::definition(&markdown::anchor_name(&id)));
            }
            Some(id) => self.release(&id),
            None => {}
        }
    }

    /// A child of a block container: text, or an element.
    fn node(&mut self, node: &Node, place: Place, out: &mut Vec<String>) {
        match node {
            Node::Text(text) => {
                let text = fragment(text);
                let text = text.trim_matches(' ');
                if !text.is_empty() {
                    out.push(text.to_owned());
                }
            }
            Node::Element(element) => self.block(element, place, out),
        }
    }

    /// A block.
    fn block(&mut self, element: &Element, place: Place, out: &mut Vec<String>) {
        match (element.name.as_str(), place) {
            ("p" | "text-author", _) => self.paragraph(element, out),
            ("poem", _) => self.poem(element, out),
            ("cite" | "epigraph", _) | ("annotation", Place::Section(_)) => {
                self.quote(element, out);
            }
            ("table", _) => self.table(element, out),
            ("image", _) => self.figure(element, out),
            ("empty-line", _) => {}
            ("subtitle", Place::Quote) => self.bold(element, out),
            ("subtitle", Place::Section(depth)) => {
                let text = self.line(element);
                if !text.is_empty() {
                    self.mark(element.attr("id"), out);
                    let level = (depth + 1).clamp(1, MAX_LEVEL);
                    out.push(format!("{} {text}", "#".repeat(level)));
                }
            }
            ("title", Place::Section(depth)) => self.heading(depth, element, out),
            ("section", Place::Section(depth)) => self.section(element, depth + 1, out),
            _ => self.generic(element, place, out),
        }
    }

    /// An element the format does not have at this place: a container of
    /// blocks, or else a paragraph.
    fn generic(&mut self, element: &Element, place: Place, out: &mut Vec<String>) {
        if element
            .elements()
            .any(|child| BLOCKS.contains(&child.name.as_str()))
        {
            for node in &element.children {
                self.node(node, place, out);
            }
        } else {
            self.paragraph(element, out);
        }
    }

    /// A heading of a level, its lines joined with a space.
    fn heading(&mut self, level: usize, title: &Element, out: &mut Vec<String>) {
        let text = self.title_line(title);
        if !text.is_empty() {
            out.push(format!("{} {text}", "#".repeat(level.clamp(1, MAX_LEVEL))));
        }
    }

    /// The paragraphs of a title on one line.
    fn title_line(&mut self, title: &Element) -> String {
        let mut lines = Vec::new();
        for node in &title.children {
            let line = match node {
                Node::Element(element) if element.name == "empty-line" => String::new(),
                Node::Element(element) => self.line(element),
                Node::Text(text) => fragment(text).trim_matches(' ').to_owned(),
            };
            if !line.is_empty() {
                lines.push(line);
            }
        }
        lines.join(" ")
    }

    /// The inline content of an element, trimmed.
    fn line(&mut self, element: &Element) -> String {
        self.inline(&element.children).trim_matches(' ').to_owned()
    }

    /// A paragraph, marked when it has an identifier.
    fn paragraph(&mut self, paragraph: &Element, out: &mut Vec<String>) {
        let text = self.line(paragraph);
        if !text.is_empty() {
            self.mark(paragraph.attr("id"), out);
            out.push(text);
        }
    }

    /// A paragraph in bold.
    fn bold(&mut self, paragraph: &Element, out: &mut Vec<String>) {
        let text = self.line(paragraph);
        if !text.is_empty() {
            self.mark(paragraph.attr("id"), out);
            out.push(surround(&text, "**", "**"));
        }
    }

    /// An epigraph, a citation or an annotation inside the text: a block
    /// quote.
    fn quote(&mut self, element: &Element, out: &mut Vec<String>) {
        self.mark(element.attr("id"), out);
        self.begin_quote();
        let mut inner = Vec::new();
        for node in &element.children {
            self.node(node, Place::Quote, &mut inner);
        }
        self.end_quote(&inner, out);
    }

    /// A poem: its title in bold, one paragraph for each stanza with a hard
    /// line break after every verse but the last, then its authors.
    fn poem(&mut self, poem: &Element, out: &mut Vec<String>) {
        self.mark(poem.attr("id"), out);
        for child in poem.elements() {
            match child.name.as_str() {
                "title" => self.bold_title(child, out),
                "epigraph" => self.quote(child, out),
                "subtitle" => self.bold(child, out),
                "stanza" => self.stanza(child, out),
                "date" => {
                    let text = plain(child);
                    let date = if text.is_empty() {
                        child.attr("value").unwrap_or_default().to_owned()
                    } else {
                        text
                    };
                    let date = fragment(&date);
                    if !date.trim().is_empty() {
                        out.push(date.trim_matches(' ').to_owned());
                    }
                }
                "empty-line" => {}
                _ => self.paragraph(child, out),
            }
        }
    }

    /// The title of a poem or a stanza, in bold.
    fn bold_title(&mut self, title: &Element, out: &mut Vec<String>) {
        let text = self.title_line(title);
        if !text.is_empty() {
            out.push(surround(&text, "**", "**"));
        }
    }

    /// A stanza: one paragraph, a hard line break after every verse but the
    /// last.
    fn stanza(&mut self, stanza: &Element, out: &mut Vec<String>) {
        let mut verses = Vec::new();
        for child in stanza.elements() {
            match child.name.as_str() {
                "title" | "subtitle" => {
                    Self::flush_verses(&mut verses, out);
                    if child.name == "title" {
                        self.bold_title(child, out);
                    } else {
                        self.bold(child, out);
                    }
                }
                "empty-line" => {}
                _ => {
                    self.mark(child.attr("id"), out);
                    let text = self.line(child);
                    if !text.is_empty() {
                        verses.push(text);
                    }
                }
            }
        }
        Self::flush_verses(&mut verses, out);
    }

    /// Writes the verses gathered so far as a paragraph.
    fn flush_verses(verses: &mut Vec<String>, out: &mut Vec<String>) {
        if !verses.is_empty() {
            out.push(verses.join("\\\n"));
            verses.clear();
        }
    }

    /// A table: the first row is the header. Cells that span rows or
    /// columns are written as ordinary cells. A table without a cell
    /// writes nothing.
    fn table(&mut self, table: &Element, out: &mut Vec<String>) {
        let spans = |cell: &Element, name: &str| {
            cell.attr(name)
                .and_then(|span| span.trim().parse::<u32>().ok())
                .is_some_and(|span| span > 1)
        };
        let mut merged = false;
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut cell_ids = Vec::new();
        for row in table.named("tr") {
            let mut cells = Vec::new();
            for cell in row.elements() {
                merged |= spans(cell, "colspan") || spans(cell, "rowspan");
                cell_ids.push(cell.attr("id"));
                cells.push(self.line(cell));
            }
            rows.push(cells);
        }
        let columns = rows.iter().map(Vec::len).max().unwrap_or_default();
        if columns == 0 {
            return;
        }
        if merged {
            self.warn_once("table with merged cells");
        }
        self.mark(table.attr("id"), out);
        for id in cell_ids {
            self.mark(id, out);
        }
        let lines: Vec<String> = rows
            .iter()
            .map(|row| {
                let cells: Vec<&str> = (0..columns)
                    .map(|column| row.get(column).map_or("", String::as_str))
                    .collect();
                format!("| {} |", cells.join(" | "))
            })
            .collect();
        let separator = format!("| {} |", vec!["---"; columns].join(" | "));
        let mut table_lines = lines.into_iter();
        let mut block: Vec<String> = table_lines.by_ref().take(1).collect();
        block.push(separator);
        block.extend(table_lines);
        out.push(block.join("\n"));
    }

    /// A picture of a block: alone in its paragraph, with its title as the
    /// caption.
    fn figure(&mut self, image: &Element, out: &mut Vec<String>) {
        self.mark(image.attr("id"), out);
        let alt = image.attr("alt").unwrap_or_default();
        if let Some(id) = local_id(image.attr("href")).filter(|id| self.pictures.contains(id)) {
            out.push(markdown::image(
                alt,
                &markdown::encoded_path(&id),
                image.attr("title"),
            ));
        } else {
            self.picture_lost(image.attr("href"));
            let text = fragment(alt);
            if !text.trim().is_empty() {
                out.push(text.trim_matches(' ').to_owned());
            }
        }
    }

    /// Reports a picture the document refers to and does not hold.
    fn picture_lost(&mut self, href: Option<&str>) {
        let name = href.unwrap_or_default().trim().trim_start_matches('#');
        self.warnings.push(Warning::new(
            WarningKind::PictureDropped(name.to_owned()),
            &self.section,
        ));
    }

    /// Writes inline content: text and elements as the document has them.
    /// A space stands where the document has white space, and nowhere else.
    fn inline(&mut self, nodes: &[Node]) -> String {
        let mut out = String::new();
        for node in nodes {
            let piece = match node {
                Node::Text(text) => fragment(text),
                Node::Element(element) => self.inline_element(element),
            };
            push_piece(&mut out, &piece);
        }
        out
    }

    /// An element inside a line.
    fn inline_element(&mut self, element: &Element) -> String {
        match element.name.as_str() {
            "code" => {
                let mut plain = String::new();
                push_plain(&element.children, &mut plain);
                code(&plain)
            }
            "image" => self.inline_image(element),
            "a" => self.link(element),
            "strong" | "emphasis" | "strikethrough" | "sub" | "sup" | "style" => {
                let inner = self.inline(&element.children);
                self.mark_up(&element.name, inner)
            }
            // What is not known is not there, its text is.
            _ => self.inline(&element.children),
        }
    }

    /// The inline markup of an element around text that is already Markdown.
    fn mark_up(&mut self, name: &str, inner: String) -> String {
        match name {
            "strong" => surround(&inner, "**", "**"),
            "emphasis" => surround(&inner, "*", "*"),
            "strikethrough" => surround(&inner, "~~", "~~"),
            "sub" => {
                self.warn_once("sub");
                inner
            }
            "sup" => {
                self.warn_once("sup");
                inner
            }
            _ => {
                self.warn_once("style");
                inner
            }
        }
    }

    /// A picture inside a paragraph.
    fn inline_image(&mut self, image: &Element) -> String {
        let alt = image.attr("alt").unwrap_or_default();
        if let Some(id) = local_id(image.attr("href")).filter(|id| self.pictures.contains(id)) {
            markdown::image(alt, &markdown::encoded_path(&id), None)
        } else {
            self.picture_lost(image.attr("href"));
            fragment(alt)
        }
    }

    /// A link: a footnote, a link into the book, an address outside it, or
    /// its text alone.
    fn link(&mut self, link: &Element) -> String {
        let text = self.inline(&link.children);
        let href = link.attr("href").unwrap_or_default().trim();
        if let Some(id) = local_id(link.attr("href")) {
            let is_note = link.attr("type") == Some("note") || self.notes.contains_key(&id);
            if is_note {
                return self.note_reference(&id, href, &text);
            }
            let core = text.trim_matches(' ');
            if core.is_empty() {
                return String::new();
            }
            return surround(&text, "[", &format!("]{OPEN}{id}{CLOSE}"));
        }
        let lower = href.to_ascii_lowercase();
        let external = ["http://", "https://", "mailto:"]
            .iter()
            .any(|scheme| lower.starts_with(scheme));
        if external && !text.trim_matches(' ').is_empty() {
            let end = markdown::target(&external_destination(href), None);
            return surround(&text, "[", &format!("]{end}"));
        }
        if !external {
            self.warnings.push(Warning::new(
                WarningKind::LinkDropped(href.to_owned()),
                &self.section,
            ));
        }
        text
    }

    /// The reference to a note, or the text of the link when the document
    /// has no such note.
    fn note_reference(&mut self, id: &str, href: &str, text: &str) -> String {
        if !self.notes.contains_key(id) {
            self.warnings.push(Warning::new(
                WarningKind::LinkDropped(href.to_owned()),
                &self.section,
            ));
            return text.to_owned();
        }
        if self.in_note {
            return text.to_owned();
        }
        if !self.used_notes.iter().any(|used| used == id) {
            self.used_notes.push(id.to_owned());
        }
        format!("[^{}]", markdown::anchor_name(id))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        code, collapse, external_destination, fragment, plain, push_piece, quoted, surround,
    };
    use crate::formats::fb2::tree::{Node, parse};
    use mbook::Limits;

    #[test]
    fn white_space_runs_become_one_space() {
        assert_eq!(collapse("a \n\t b\r\n"), "a b ");
        assert_eq!(collapse("\u{e000}x\u{e001}"), "x");
        assert_eq!(collapse(""), "");
    }

    #[test]
    fn a_fragment_keeps_one_space_at_each_end_and_escapes_the_rest() {
        assert_eq!(fragment("  *a*  b\n"), " \\*a\\* b ");
        assert_eq!(fragment(" \n "), " ");
        assert_eq!(fragment(""), "");
    }

    #[test]
    fn code_is_fenced_longer_than_its_backticks() {
        assert_eq!(code("a"), "`a`");
        assert_eq!(code("a`b"), "``a`b``");
        assert_eq!(code("`a"), "`` `a ``");
        assert_eq!(code(""), "");
    }

    #[test]
    fn spaces_stay_outside_the_fence_of_code() {
        assert_eq!(code(" a  b\n"), " `a b` ");
        assert_eq!(code("  "), " ");
    }

    #[test]
    fn spaces_move_out_of_markup() {
        assert_eq!(surround(" a ", "**", "**"), " **a** ");
        assert_eq!(surround("  ", "*", "*"), "  ");
    }

    #[test]
    fn two_spaces_that_meet_are_one() {
        let mut out = "a ".to_owned();
        push_piece(&mut out, " b");
        push_piece(&mut out, "c");
        push_piece(&mut out, " d");
        assert_eq!(out, "a bc d");
    }

    #[test]
    fn an_address_loses_what_would_end_a_destination() {
        assert_eq!(
            external_destination("http://a.b/c d(e)\u{7}"),
            "http://a.b/c%20d%28e%29"
        );
        assert_eq!(
            external_destination("<a>[b]\\\"`"),
            "%3Ca%3E%5Bb%5D%5C%22%60"
        );
    }

    #[test]
    fn a_quote_prefixes_every_line() {
        assert_eq!(quoted("a\n\nb"), "> a\n>\n> b");
    }

    #[test]
    fn the_plain_text_of_an_element_has_no_markup_and_no_pictures()
    -> Result<(), crate::error::Error> {
        let document = parse(
            b"<p> a <strong>b\n</strong><image href=\"#x\"/><a>c</a>  d </p>",
            &Limits::default(),
        )?;
        assert_eq!(plain(&document.root), "a b c d");
        assert!(matches!(document.root.children[0], Node::Text(_)));
        Ok(())
    }
}
