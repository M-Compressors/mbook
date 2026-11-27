//! Reading and changing the text of a chapter the way the tools do: by
//! lines, by section, and by replacing what the agent quotes.

use std::collections::HashMap;
use std::ops::Range;

use markdown::mdast::Node;
use markdown::{ParseOptions, to_mdast};

use super::error::AgentError;
use super::types::{Edit, Lines};
use crate::entities::book::chapter::image_path;

/// The number of lines of a text: a last line without a line feed counts,
/// an empty text has none.
pub(crate) fn line_count(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.matches('\n').count() + usize::from(!text.ends_with('\n'))
    }
}

/// The byte range of lines `from` to `to` of a text, both from 1 and
/// included, brought into the text; and the lines it covers.
pub(crate) fn lines(text: &str, wanted: Lines) -> (Range<usize>, usize, usize) {
    let total = line_count(text);
    if total == 0 {
        return (0..0, 0, 0);
    }
    let from = wanted.from.clamp(1, total);
    let to = wanted.to.clamp(from, total);
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let start = starts[from - 1];
    let end = starts.get(to).copied().unwrap_or(text.len());
    (start..end, from, to)
}

/// The line of a byte offset, from 1.
pub(crate) fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

/// A heading of a chapter: where it starts and its level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Heading {
    pub(crate) offset: usize,
    pub(crate) level: u8,
    pub(crate) title: String,
}

/// The headings of a text, in order.
pub(crate) fn headings(text: &str) -> Vec<Heading> {
    let Ok(root) = to_mdast(text, &ParseOptions::gfm()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    visit(&root, &mut |node| {
        if let Node::Heading(heading) = node
            && let Some(position) = &heading.position
        {
            out.push(Heading {
                offset: position.start.offset,
                level: heading.depth,
                title: node.to_string(),
            });
        }
    });
    out
}

/// The section that starts at `offset`: a heading to the next heading of
/// the same level or higher, or another line to the next heading.
pub(crate) fn section(text: &str, offset: usize) -> Range<usize> {
    let all = headings(text);
    let level = all
        .iter()
        .find(|heading| heading.offset == offset)
        .map(|heading| heading.level);
    let end = all
        .iter()
        .filter(|heading| heading.offset > offset)
        .find(|heading| level.is_none_or(|level| heading.level <= level))
        .map_or(text.len(), |heading| heading.offset);
    offset.min(text.len())..end
}

/// Applies edits in order, all or none. Gives the new text and the ranges of
/// it that the edits wrote.
pub(crate) fn apply(text: &str, edits: &[Edit]) -> Result<(String, Vec<Range<usize>>), AgentError> {
    let mut out = text.to_owned();
    let mut written: Vec<Range<usize>> = Vec::new();
    for (index, edit) in edits.iter().enumerate() {
        if edit.old.is_empty() {
            return Err(AgentError::OldNotFound { index });
        }
        let found: Vec<usize> = out.match_indices(&edit.old).map(|(at, _)| at).collect();
        match found.len() {
            0 => return Err(AgentError::OldNotFound { index }),
            1 => {}
            count if !edit.replace_all => return Err(AgentError::OldAmbiguous { index, count }),
            _ => {}
        }
        for &at in found.iter().rev() {
            out.replace_range(at..at + edit.old.len(), &edit.new);
            mark(&mut written, at, edit.old.len(), edit.new.len());
        }
    }
    written.sort_by_key(|range| range.start);
    Ok((out, written))
}

/// Records that `old` bytes at `at` became `new` bytes: the ranges after it
/// move, and one that it touches is joined with it.
fn mark(written: &mut Vec<Range<usize>>, at: usize, old: usize, new: usize) {
    let end = at + old;
    let mut joined = at..at + new;
    written.retain_mut(|range| {
        if range.end < at {
            true
        } else if range.start > end {
            *range = range.start + new - old..range.end + new - old;
            true
        } else {
            joined.start = joined.start.min(range.start);
            joined.end = joined.end.max((range.end + new).saturating_sub(old));
            false
        }
    });
    written.push(joined);
}

/// The part of `new` that differs from `old`: everything between the bytes
/// they share at the start and those they share at the end.
pub(crate) fn changed(old: &str, new: &str) -> Range<usize> {
    let mut start = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !new.is_char_boundary(start) {
        start -= 1;
    }
    let most = old.len().min(new.len()) - start;
    let mut shared = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(most)
        .take_while(|(a, b)| a == b)
        .count();
    while !new.is_char_boundary(new.len() - shared) {
        shared -= 1;
    }
    start..new.len() - shared
}

/// A reference of a chapter to a picture, a diagram or a formula.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shown {
    /// The path of the resource in the book.
    pub(crate) path: String,
    pub(crate) line: usize,
    pub(crate) alt: String,
}

/// The pictures, diagrams and formulas a chapter shows, with the line of
/// each and its alternative text; references through a definition count.
pub(crate) fn shown(chapter: &str, text: &str) -> Vec<Shown> {
    let Ok(root) = to_mdast(text, &ParseOptions::gfm()) else {
        return Vec::new();
    };
    let mut definitions = HashMap::new();
    visit(&root, &mut |node| {
        if let Node::Definition(definition) = node {
            definitions
                .entry(definition.identifier.clone())
                .or_insert_with(|| definition.url.clone());
        }
    });
    let mut out = Vec::new();
    visit(&root, &mut |node| {
        let (url, alt, position) = match node {
            Node::Image(image) => (Some(&image.url), &image.alt, &image.position),
            Node::ImageReference(image) => (
                definitions.get(&image.identifier),
                &image.alt,
                &image.position,
            ),
            _ => return,
        };
        if let (Some(url), Some(position)) = (url, position)
            && let Some(path) = image_path(chapter, url)
        {
            out.push(Shown {
                path,
                line: position.start.line,
                alt: alt.clone(),
            });
        }
    });
    out
}

/// Visits every node, parents before children.
fn visit(node: &Node, f: &mut impl FnMut(&Node)) {
    f(node);
    if let Some(children) = node.children() {
        for child in children {
            visit(child, f);
        }
    }
}

/// At most this many characters of a line are shown around a match.
const SNIPPET: usize = 160;

/// The line around a match at `column` (in characters), shortened to
/// `SNIPPET` characters with `…` where it was cut.
pub(crate) fn snippet(line: &str, column: usize) -> String {
    let count = line.chars().count();
    if count <= SNIPPET {
        return line.to_owned();
    }
    let start = column.saturating_sub(SNIPPET / 4).min(count - SNIPPET);
    let mut out: String = line.chars().skip(start).take(SNIPPET).collect();
    if start > 0 {
        out.insert(0, '…');
    }
    if start + SNIPPET < count {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(old: &str, new: &str) -> Edit {
        Edit {
            old: old.into(),
            new: new.into(),
            replace_all: false,
        }
    }

    #[test]
    fn lines_are_counted_with_or_without_a_last_line_feed() {
        assert_eq!(line_count(""), 0);
        assert_eq!(line_count("a"), 1);
        assert_eq!(line_count("a\n"), 1);
        assert_eq!(line_count("a\nb"), 2);
    }

    #[test]
    fn lines_are_taken_by_number_and_brought_into_the_text() {
        let text = "a\nb\nc\n";
        assert_eq!(lines(text, Lines { from: 2, to: 2 }), (2..4, 2, 2));
        assert_eq!(lines(text, Lines { from: 2, to: 99 }), (2..6, 2, 3));
        assert_eq!(lines(text, Lines { from: 0, to: 1 }), (0..2, 1, 1));
        assert_eq!(lines(text, Lines { from: 3, to: 1 }), (4..6, 3, 3));
        assert_eq!(lines("", Lines { from: 1, to: 2 }), (0..0, 0, 0));
        assert_eq!(lines("a\nb", Lines { from: 2, to: 2 }), (2..3, 2, 2));
    }

    #[test]
    fn a_section_ends_at_a_heading_of_its_level_or_higher() {
        let text = "# A\n\n## B\n\nb\n\n### C\n\n## D\n\n# E\n";
        let b = text.find("## B").unwrap();
        let d = text.find("## D").unwrap();
        assert_eq!(section(text, b), b..d);
        let a = section(text, 0);
        assert_eq!(&text[a], "# A\n\n## B\n\nb\n\n### C\n\n## D\n\n");
        let e = text.find("# E").unwrap();
        assert_eq!(section(text, e), e..text.len());
    }

    #[test]
    fn a_section_of_a_named_line_ends_at_the_next_heading() {
        let text = "# A\n\n[x]: #x\nText\n\n### C\n";
        let x = text.find("[x]").unwrap();
        assert_eq!(&text[section(text, x)], "[x]: #x\nText\n\n");
    }

    #[test]
    fn headings_have_their_level_and_text() {
        let found = headings("# One *two*\n\ntext\n\n## Three\n");
        assert_eq!(
            found,
            [
                Heading {
                    offset: 0,
                    level: 1,
                    title: "One two".into()
                },
                Heading {
                    offset: 19,
                    level: 2,
                    title: "Three".into()
                },
            ]
        );
    }

    #[test]
    fn edits_are_applied_in_order_and_their_text_is_marked() {
        let (text, written) =
            apply("one two three", &[edit("two", "2"), edit("one", "uno")]).unwrap();
        assert_eq!(text, "uno 2 three");
        assert_eq!(written, [0..3, 4..5]);
    }

    #[test]
    fn an_edit_inside_another_joins_its_mark() {
        let (text, written) = apply("abc", &[edit("b", "XYZ"), edit("Y", "-")]).unwrap();
        assert_eq!(text, "aX-Zc");
        let joined = 1..4;
        assert_eq!(written, vec![joined]);
    }

    #[test]
    fn every_occurrence_is_replaced_when_asked() {
        let mut all = edit("a", "bb");
        all.replace_all = true;
        let (text, written) = apply("a-a-a", &[all]).unwrap();
        assert_eq!(text, "bb-bb-bb");
        assert_eq!(written, [0..2, 3..5, 6..8]);
    }

    #[test]
    fn an_edit_that_is_not_found_once_refuses_all() {
        assert_eq!(
            apply("abc", &[edit("a", "x"), edit("q", "y")]),
            Err(AgentError::OldNotFound { index: 1 })
        );
        assert_eq!(
            apply("a a", &[edit("a", "x")]),
            Err(AgentError::OldAmbiguous { index: 0, count: 2 })
        );
        assert_eq!(
            apply("a", &[edit("", "x")]),
            Err(AgentError::OldNotFound { index: 0 })
        );
    }

    #[test]
    fn the_changed_part_is_between_what_both_share() {
        assert_eq!(changed("abc", "abc"), 3..3);
        assert_eq!(changed("abc", "aXc"), 1..2);
        assert_eq!(changed("abc", "abXc"), 2..3);
        assert_eq!(changed("", "new"), 0..3);
        assert_eq!(changed("aaa", "aa"), 2..2);
        assert_eq!(changed("я", "ю"), 0..2);
    }

    #[test]
    fn pictures_are_found_with_their_line_and_alternative_text() {
        let text = "# T\n\n![Pond](../img/pond.avif)\n\n![x][ref]\n\n[ref]: ../dia/0001.mmd\n";
        assert_eq!(
            shown("part/ch01.md", text),
            [
                Shown {
                    path: "img/pond.avif".into(),
                    line: 3,
                    alt: "Pond".into()
                },
                Shown {
                    path: "dia/0001.mmd".into(),
                    line: 5,
                    alt: "x".into()
                },
            ]
        );
        assert_eq!(shown("ch01.md", "![a](https://x.org/a.png)\n"), []);
    }

    #[test]
    fn a_long_line_is_shortened_around_the_match() {
        assert_eq!(snippet("short", 1), "short");
        let line = "x".repeat(300);
        let cut = snippet(&line, 150);
        assert!(cut.starts_with('…') && cut.ends_with('…'));
        assert_eq!(cut.chars().count(), SNIPPET + 2);
        assert!(!snippet(&line, 1).starts_with('…'));
    }
}
