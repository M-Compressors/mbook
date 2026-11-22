//! Bringing the Markdown of a section into the dialect of RFC 0001 and into
//! the terms of the book: raw HTML is removed, and every link, picture and
//! mark gets the name it has after conversion.
//!
//! The text is changed at the positions the parser reports, so everything
//! that needs no change stays as the front end wrote it.

use crate::markdown as md;
use crate::report::{Warning, WarningKind};
use markdown::mdast::Node;
use markdown::{ParseOptions, to_mdast};
use percent_encoding::percent_decode_str;
use std::collections::HashSet;
use std::ops::Range;

/// Parses Markdown as the dialect of RFC 0001, the way `mbook` does.
pub(crate) fn parse(text: &str) -> Option<Node> {
    // Parsing fails only for MDX syntax, which the dialect does not enable.
    to_mdast(text, &ParseOptions::gfm()).ok()
}

/// What a first look at a chapter finds.
#[derive(Debug, Default)]
pub(crate) struct Scan {
    /// The names of the marks, in the order they appear.
    pub(crate) marks: Vec<String>,
    /// Where each of those marks ends in the text.
    pub(crate) mark_ends: Vec<usize>,
    /// The destinations of every link, as written.
    pub(crate) links: Vec<String>,
    /// The text of the first top-level heading.
    pub(crate) heading: Option<String>,
    /// The identifiers of the definitions that pictures refer to.
    image_definitions: HashSet<String>,
}

/// The name a link reference definition gives to its line, when it is a
/// mark: `[name]: #name`.
fn mark_name(url: &str, label: Option<&String>) -> Option<String> {
    let name = percent_decode_str(url.strip_prefix('#')?)
        .decode_utf8()
        .ok()?;
    (label.map(String::as_str) == Some(&*name)).then(|| name.into_owned())
}

/// Looks at a chapter without changing it.
pub(crate) fn scan(tree: &Node) -> Scan {
    fn visit(node: &Node, scan: &mut Scan, mark_identifiers: &mut Vec<(String, String)>) {
        match node {
            Node::Definition(definition) => {
                match mark_name(&definition.url, definition.label.as_ref()) {
                    Some(name) => {
                        mark_identifiers.push((definition.identifier.clone(), name.clone()));
                        scan.marks.push(name);
                        scan.mark_ends.push(end_of(node));
                    }
                    None => scan.links.push(definition.url.clone()),
                }
            }
            Node::Link(link) => scan.links.push(link.url.clone()),
            Node::ImageReference(image) => {
                scan.image_definitions.insert(image.identifier.clone());
            }
            Node::Heading(heading) if heading.depth == 1 && scan.heading.is_none() => {
                let mut title = String::new();
                plain_text(node, &mut title);
                scan.heading = Some(title).filter(|title| !title.trim().is_empty());
            }
            _ => {}
        }
        for child in node.children().map(Vec::as_slice).unwrap_or_default() {
            visit(child, scan, mark_identifiers);
        }
    }
    fn references(node: &Node, marks: &[(String, String)], links: &mut Vec<String>) {
        if let Node::LinkReference(reference) = node {
            // A reference through a mark is a link to it.
            let mark = marks.iter().find(|(id, _)| *id == reference.identifier);
            links.extend(mark.map(|(_, name)| md::destination("", Some(name))));
        }
        for child in node.children().map(Vec::as_slice).unwrap_or_default() {
            references(child, marks, links);
        }
    }
    let mut scan = Scan::default();
    let mut mark_identifiers = Vec::new();
    visit(tree, &mut scan, &mut mark_identifiers);
    references(tree, &mark_identifiers, &mut scan.links);
    scan
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

/// What becomes of a link.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Fate {
    /// It leads out of the book and stays as it is.
    Keep,
    /// It leads to this destination now.
    Replace(String),
    /// It leads nowhere and becomes its text.
    Drop,
}

/// What a chapter is rewritten with.
pub(crate) struct Rules<'a> {
    /// The source's name for the section, for warnings.
    pub(crate) location: &'a str,
    /// Decides what becomes of a link, from its destination as written.
    pub(crate) link: &'a dyn Fn(&str) -> Fate,
    /// The path of a picture in the book, from its destination as written.
    pub(crate) picture: &'a mut dyn FnMut(&str) -> Option<String>,
    /// The name a mark has in the book; `None` when no link leads to it.
    pub(crate) mark: &'a dyn Fn(&str) -> Option<String>,
    /// Where the losses are reported.
    pub(crate) warnings: &'a mut Vec<Warning>,
}

/// Rewrites a chapter by the rules of "Assembling the book" in the docs.
pub(crate) fn rewrite(text: &str, tree: &Node, scan: &Scan, rules: Rules<'_>) -> String {
    let mut rewriter = Rewriter {
        text,
        scan,
        rules,
        edits: Vec::new(),
        dropped_definitions: HashSet::new(),
        kept_marks: HashSet::new(),
        reported: HashSet::new(),
    };
    rewriter.definitions(tree);
    rewriter.visit(tree, Context::Block);
    rewriter.finish()
}

/// Where a node stands.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    /// Between blocks.
    Block,
    /// Inside a paragraph, where a line may be broken.
    Inline,
    /// Inside a heading or a cell of a table, which are one line.
    Line,
}

struct Rewriter<'text, 'rules> {
    text: &'text str,
    scan: &'text Scan,
    rules: Rules<'rules>,
    /// What is replaced, and with what.
    edits: Vec<(Range<usize>, String)>,
    /// The identifiers of the definitions that were removed.
    dropped_definitions: HashSet<String>,
    /// The marks of this chapter that are kept: the first of a name.
    kept_marks: HashSet<String>,
    /// The kinds of markup already reported for this chapter.
    reported: HashSet<String>,
}

impl Rewriter<'_, '_> {
    fn warn(&mut self, kind: WarningKind) {
        self.rules
            .warnings
            .push(Warning::new(kind, self.rules.location));
    }

    fn replace(&mut self, range: Range<usize>, with: impl Into<String>) {
        self.edits.push((range, with.into()));
    }

    /// Removes a block. The empty line after it goes too, when the block
    /// has its lines to itself.
    fn remove_block(&mut self, range: Range<usize>) {
        let before = &self.text[..range.start];
        let alone = before.is_empty() || before.ends_with('\n');
        let after = &self.text[range.end..];
        let line_ends = if alone {
            after
                .bytes()
                .take(2)
                .take_while(|&byte| byte == b'\n')
                .count()
        } else {
            0
        };
        self.replace(range.start..range.end + line_ends, "");
    }

    /// Rewrites every link reference definition, and remembers the ones
    /// that are removed, before anything that refers to one is visited.
    fn definitions(&mut self, node: &Node) {
        if let Node::Definition(definition) = node {
            let range = range_of(node);
            let kept = match mark_name(&definition.url, definition.label.as_ref()) {
                // A name marks one line: the first that carries it.
                Some(name) => (self.rules.mark)(&name)
                    .filter(|_| self.kept_marks.insert(name))
                    .map(|name| md::definition(&name)),
                None => self.destination_of(definition).map(|destination| {
                    let raw = &self.text[range.clone()];
                    let head = raw.find("]:").map_or(raw, |end| &raw[..end + 2]);
                    let tail = md::target(&destination, definition.title.as_deref());
                    // The target is written without the brackets of a link.
                    format!("{head} {}", &tail[1..tail.len() - 1])
                }),
            };
            if let Some(line) = kept {
                self.replace(range, line);
            } else {
                self.dropped_definitions
                    .insert(definition.identifier.clone());
                self.remove_block(range);
            }
        }
        for child in node.children().map(Vec::as_slice).unwrap_or_default() {
            self.definitions(child);
        }
    }

    /// The new destination of a definition that is not a mark; `None` when
    /// it leads nowhere.
    fn destination_of(&mut self, definition: &markdown::mdast::Definition) -> Option<String> {
        let url = definition.url.as_str();
        if self.scan.image_definitions.contains(&definition.identifier) {
            let path = (self.rules.picture)(url);
            if path.is_none() {
                self.warn(WarningKind::PictureDropped(url.to_owned()));
            }
            return path.map(|path| md::encoded_path(&path));
        }
        match (self.rules.link)(url) {
            Fate::Keep => Some(url.to_owned()),
            Fate::Replace(destination) => Some(destination),
            Fate::Drop => {
                self.warn(WarningKind::LinkDropped(url.to_owned()));
                None
            }
        }
    }

    fn visit(&mut self, node: &Node, context: Context) {
        let range = range_of(node);
        let inside = match node {
            Node::Paragraph(_) | Node::Emphasis(_) | Node::Strong(_) | Node::Delete(_)
                if context != Context::Line =>
            {
                Context::Inline
            }
            Node::Heading(_) | Node::TableCell(_) => Context::Line,
            Node::Root(_)
            | Node::Blockquote(_)
            | Node::List(_)
            | Node::ListItem(_)
            | Node::FootnoteDefinition(_)
            | Node::Table(_)
            | Node::TableRow(_) => Context::Block,
            _ => context,
        };
        match node {
            Node::Html(html) => self.html(&html.value, range, context),
            Node::Image(image) => {
                let picture = (self.rules.picture)(&image.url);
                self.picture(
                    range,
                    &image.alt,
                    &image.url,
                    picture,
                    image.title.as_deref(),
                );
            }
            Node::ImageReference(image) if self.dropped_definitions.contains(&image.identifier) => {
                self.replace(range, md::escape(&image.alt).replace('\n', " "));
            }
            Node::Link(link) => {
                let inner_end = link.children.last().map_or(range.start + 1, end_of);
                let fate = (self.rules.link)(&link.url);
                if !self.link(&range, inner_end, &link.url, fate, link.title.as_deref()) {
                    return;
                }
            }
            Node::LinkReference(link) if self.dropped_definitions.contains(&link.identifier) => {
                let inner_end = link.children.last().map_or(range.start + 1, end_of);
                self.replace(range.start..range.start + 1, "");
                self.replace(inner_end..range.end, "");
            }
            _ => {}
        }
        for child in node.children().map(Vec::as_slice).unwrap_or_default() {
            self.visit(child, inside);
        }
    }

    /// Raw HTML: a line break becomes one of Markdown, anything else goes.
    fn html(&mut self, value: &str, range: Range<usize>, context: Context) {
        let name = tag_name(value);
        if name == "br" && context != Context::Block {
            let after = &self.text[range.end..];
            let ends_paragraph = after.is_empty() || after == "\n" || after.starts_with("\n\n");
            let with = match context {
                Context::Line => " ",
                _ if ends_paragraph => "",
                _ if after.starts_with('\n') => "\\",
                _ => "\\\n",
            };
            self.replace(range, with);
            return;
        }
        if context == Context::Block {
            self.remove_block(range);
        } else {
            self.replace(range, "");
        }
        if self.reported.insert(name.clone()) {
            self.warn(WarningKind::MarkupDropped(name));
        }
    }

    /// A picture gets its path in the book, or becomes its alternative text.
    fn picture(
        &mut self,
        range: Range<usize>,
        alt: &str,
        url: &str,
        path: Option<String>,
        title: Option<&str>,
    ) {
        if let Some(path) = path {
            self.replace(range, md::image(alt, &md::encoded_path(&path), title));
        } else {
            self.replace(range, md::escape(alt).replace('\n', " "));
            self.warn(WarningKind::PictureDropped(url.to_owned()));
        }
    }

    /// A link gets its new destination, or becomes its text. `false` when
    /// the whole link was replaced and its text needs no visit.
    fn link(
        &mut self,
        range: &Range<usize>,
        inner_end: usize,
        url: &str,
        fate: Fate,
        title: Option<&str>,
    ) -> bool {
        let tail = self.text.get(inner_end..range.end).unwrap_or_default();
        // Anything else is an address written out, which is its own text.
        let bracketed = self.text[range.clone()].starts_with('[') && tail.starts_with("](");
        match fate {
            Fate::Replace(destination) if bracketed => {
                let target = md::target(&destination, title);
                self.replace(inner_end..range.end, format!("]{target}"));
            }
            Fate::Keep | Fate::Replace(_) => {}
            Fate::Drop => {
                self.warn(WarningKind::LinkDropped(url.to_owned()));
                if bracketed {
                    self.replace(range.start..range.start + 1, "");
                    self.replace(inner_end..range.end, "");
                } else {
                    self.replace(range.clone(), md::escape(url));
                    return false;
                }
            }
        }
        true
    }

    /// Applies the edits, from the start of the text to its end.
    fn finish(mut self) -> String {
        self.edits.sort_by_key(|(range, _)| range.start);
        let mut out = String::with_capacity(self.text.len());
        let mut at = 0;
        for (range, with) in &self.edits {
            // An edit inside one already applied has nothing left to change.
            if range.start < at {
                continue;
            }
            out.push_str(&self.text[at..range.start]);
            out.push_str(with);
            at = range.end;
        }
        out.push_str(&self.text[at..]);
        out
    }
}

/// The part of the text a node covers.
fn range_of(node: &Node) -> Range<usize> {
    node.position()
        .map_or(0..0, |position| position.start.offset..position.end.offset)
}

fn end_of(node: &Node) -> usize {
    range_of(node).end
}

/// The name of the element a piece of raw HTML opens or closes.
fn tag_name(html: &str) -> String {
    let inside = html.trim_start().trim_start_matches('<');
    if inside.starts_with("!--") {
        return "comment".to_owned();
    }
    let name: String = inside
        .trim_start_matches('/')
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect();
    if name.is_empty() {
        "html".to_owned()
    } else {
        name.to_ascii_lowercase()
    }
}
