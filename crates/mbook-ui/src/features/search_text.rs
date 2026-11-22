//! Literal matching and mapping text shown by Markdown to a place in its source.

use std::ops::Range;

use markdown::{ParseOptions, mdast::Node};
use regex::{Regex, RegexBuilder};

/// A match's block, then its byte offset in the block's searchable text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cursor(pub(crate) usize, pub(crate) usize);

/// A match in rendered Markdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RenderedMatch {
    /// The place from which a further search continues.
    pub(crate) cursor: Cursor,
    /// The source offset to reveal.
    pub(crate) offset: u32,
    /// A short stretch of the text that contains the match.
    pub(crate) excerpt: String,
}

/// Compiles a case-insensitive literal query.
///
/// # Errors
///
/// Returns an error if the query exceeds the regex engine's size limit.
pub(crate) fn pattern(query: &str) -> Result<Regex, regex::Error> {
    RegexBuilder::new(&regex::escape(query))
        .case_insensitive(true)
        .build()
}

/// Finds a match inside one block, on the requested side of the cursor.
pub(crate) fn in_block(
    pattern: &Regex,
    text: &str,
    block: usize,
    from: Option<Cursor>,
    backward: bool,
) -> Option<Range<usize>> {
    if let Some(Cursor(other, _)) = from
        && if backward {
            block > other
        } else {
            block < other
        }
    {
        return None;
    }
    let mut matches = pattern.find_iter(text).filter(|found| {
        from.is_none_or(|Cursor(other, at)| {
            block != other
                || if backward {
                    found.start() < at
                } else {
                    found.start() > at
                }
        })
    });
    if backward {
        matches.last()
    } else {
        matches.next()
    }
    .map(|found| found.range())
}

/// Finds a match in the visible text of Markdown and records its source place.
pub(crate) fn rendered(
    source: &str,
    pattern: &Regex,
    from: Option<Cursor>,
    backward: bool,
) -> Option<RenderedMatch> {
    let root = markdown::to_mdast(source, &ParseOptions::gfm()).ok()?;
    find_node(&root, pattern, from, backward)
}

fn find_node(
    node: &Node,
    pattern: &Regex,
    from: Option<Cursor>,
    backward: bool,
) -> Option<RenderedMatch> {
    if matches!(
        node,
        Node::Paragraph(_) | Node::Heading(_) | Node::TableCell(_) | Node::Code(_)
    ) {
        let start = node.position()?.start.offset;
        let mut text = String::new();
        let mut spans = Vec::new();
        inline_text(node, &mut text, &mut spans);
        let found = in_block(pattern, &text, start, from, backward)?;
        let offset = spans
            .iter()
            .rev()
            .find(|(at, _)| *at <= found.start)
            .map_or(start, |(at, source)| {
                (source.start + found.start - at).min(source.end.saturating_sub(1))
            });
        return Some(RenderedMatch {
            cursor: Cursor(start, found.start),
            offset: u32::try_from(offset).unwrap_or(u32::MAX),
            excerpt: excerpt(&text, found),
        });
    }
    let mut children = node.children()?.iter();
    let find = |child| find_node(child, pattern, from, backward);
    if backward {
        children.rev().find_map(find)
    } else {
        children.find_map(find)
    }
}

fn inline_text(node: &Node, text: &mut String, spans: &mut Vec<(usize, Range<usize>)>) {
    if let Some(children) = node.children() {
        for child in children {
            inline_text(child, text, spans);
        }
        return;
    }
    let content = match node {
        Node::Text(node) => &node.value,
        Node::Code(node) => &node.value,
        Node::InlineCode(node) => &node.value,
        Node::Image(node) => &node.alt,
        Node::ImageReference(node) => &node.alt,
        Node::Break(_) => " ",
        _ => return,
    };
    if let Some(position) = node.position() {
        spans.push((text.len(), position.start.offset..position.end.offset));
    }
    text.extend(content.chars().map(|ch| if ch == '\n' { ' ' } else { ch }));
}

/// Returns context around a match without splitting UTF-8.
pub(crate) fn excerpt(text: &str, found: Range<usize>) -> String {
    let start = text[..found.start]
        .char_indices()
        .rev()
        .nth(35)
        .map_or(0, |(at, ch)| at + ch.len_utf8());
    let end = text[found.start..]
        .char_indices()
        .nth(85)
        .map_or(text.len(), |(at, _)| found.start + at);
    let body = text[start..end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let leading = if start == 0 { "" } else { "…" };
    let trailing = if end == text.len() { "" } else { "…" };
    format!("{leading}{body}{trailing}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_are_literal_and_ignore_unicode_case() {
        let pattern = pattern("ПРИВЕТ?").unwrap();
        assert_eq!(
            in_block(&pattern, "a привет? b", 0, None, false),
            Some(2..15)
        );
        assert_eq!(in_block(&pattern, "привет!", 0, None, false), None);
    }

    #[test]
    fn matching_crosses_inline_markup_and_soft_line_breaks() {
        let source = "# Title\n\nText with **bold**\n[words](other.md).";
        let found = rendered(source, &pattern("bold words").unwrap(), None, false).unwrap();
        assert_eq!(found.excerpt, "Text with bold words.");
        assert_eq!(Some(found.offset as usize), source.find("bold"));
    }

    #[test]
    fn link_destinations_are_hidden_from_reading_search() {
        assert!(
            rendered(
                "[Read](hidden.md)",
                &pattern("hidden").unwrap(),
                None,
                false
            )
            .is_none()
        );
    }

    #[test]
    fn previous_matches_are_given_in_reverse_reading_order() {
        let query = pattern("one").unwrap();
        let source = "one one\n\nlast one";
        let last = rendered(source, &query, None, true).unwrap();
        let before = rendered(source, &query, Some(last.cursor), true).unwrap();
        let first = rendered(source, &query, Some(before.cursor), true).unwrap();
        assert_eq!([last.offset, before.offset, first.offset], [14, 4, 0]);
        assert!(rendered(source, &query, Some(first.cursor), true).is_none());
    }

    #[test]
    fn excerpts_clip_long_unicode_context() {
        let text = format!("{}hit{}", "я".repeat(100), "ж".repeat(100));
        let shown = excerpt(&text, 200..203);
        assert!(shown.starts_with('…') && shown.ends_with('…') && shown.contains("hit"));
    }
}
