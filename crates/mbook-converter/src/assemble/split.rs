//! Cutting a section that is too long for one chapter.

use markdown::mdast::Node;
use markdown::{ParseOptions, to_mdast};
use std::ops::Range;

/// The size RFC 0001 asks a chapter to stay under.
pub(crate) const CHAPTER_LIMIT: usize = 256 * 1024;

/// Something that must be defined in the chapter that refers to it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Footnote,
    Link,
}

/// Cuts a section into chapters that stay under `limit` where they can.
///
/// A section is cut at its top-level headings first, and then between the
/// blocks nearest to the limit; one block is never cut. A footnote or a link
/// reference that ends up in another chapter than its definition gets a
/// copy of the definition, because both must be in one chapter.
pub(crate) fn split(text: String, limit: usize) -> Vec<String> {
    if text.len() <= limit {
        return vec![text];
    }
    let Ok(root) = to_mdast(&text, &ParseOptions::gfm()) else {
        return vec![text];
    };
    let blocks = root.children().map(Vec::as_slice).unwrap_or_default();
    let starts: Vec<usize> = blocks.iter().map(start_of).collect();
    // A block runs to the start of the next one, so that nothing between
    // two blocks is lost.
    let block_range = |index: usize| {
        let start = if index == 0 { 0 } else { starts[index] };
        start..starts.get(index + 1).copied().unwrap_or(text.len())
    };
    let is_top_heading =
        |index: usize| matches!(&blocks[index], Node::Heading(heading) if heading.depth == 1);
    // A definition shows nothing, so it never starts a chapter of its own.
    let is_definition = |index: usize| {
        matches!(
            &blocks[index],
            Node::Definition(_) | Node::FootnoteDefinition(_)
        )
    };

    let mut pieces: Vec<Range<usize>> = Vec::new();
    for index in 0..blocks.len() {
        let block = block_range(index);
        let cut = match pieces.last() {
            None => true,
            Some(piece) => {
                !is_definition(index) && (is_top_heading(index) || block.end - piece.start > limit)
            }
        };
        match pieces.last_mut() {
            Some(piece) if !cut => piece.end = block.end,
            _ => pieces.push(block),
        }
    }
    if pieces.len() < 2 {
        return vec![text];
    }

    let definitions: Vec<(Kind, &str, Range<usize>)> = blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| match block {
            Node::FootnoteDefinition(note) => {
                Some((Kind::Footnote, note.identifier.as_str(), index))
            }
            Node::Definition(link) => Some((Kind::Link, link.identifier.as_str(), index)),
            _ => None,
        })
        .map(|(kind, identifier, index)| (kind, identifier, block_range(index)))
        .collect();
    let mut references = Vec::new();
    collect_references(&root, &mut references);

    pieces
        .iter()
        .map(|piece| {
            let mut chapter = text[piece.clone()].trim_end().to_owned();
            chapter.push('\n');
            let used = |kind: Kind, identifier: &str| {
                references.iter().any(|(at, used_kind, used)| {
                    piece.contains(at) && *used_kind == kind && *used == identifier
                })
            };
            for (kind, identifier, range) in &definitions {
                if !piece.contains(&range.start) && used(*kind, identifier) {
                    chapter.push('\n');
                    chapter.push_str(text[range.clone()].trim_end());
                    chapter.push('\n');
                }
            }
            chapter
        })
        .collect()
}

/// Where a node starts in its text.
fn start_of(node: &Node) -> usize {
    node.position().map_or(0, |position| position.start.offset)
}

/// Finds every use of a footnote or of a link reference definition.
fn collect_references<'a>(node: &'a Node, found: &mut Vec<(usize, Kind, &'a str)>) {
    let reference = match node {
        Node::FootnoteReference(footnote) => Some((Kind::Footnote, footnote.identifier.as_str())),
        Node::LinkReference(link) => Some((Kind::Link, link.identifier.as_str())),
        Node::ImageReference(image) => Some((Kind::Link, image.identifier.as_str())),
        _ => None,
    };
    if let Some((kind, identifier)) = reference {
        found.push((start_of(node), kind, identifier));
    }
    for child in node.children().map(Vec::as_slice).unwrap_or_default() {
        collect_references(child, found);
    }
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn a_short_section_is_one_chapter() {
        assert_eq!(
            split("# One\n\nText.\n".to_owned(), 100),
            ["# One\n\nText.\n"]
        );
    }

    #[test]
    fn a_long_section_is_cut_at_its_top_level_headings() {
        let text = "# One\n\nFirst.\n\n## Inside\n\nMore.\n\n# Two\n\nSecond.\n";
        assert_eq!(
            split(text.to_owned(), 20),
            [
                "# One\n\nFirst.\n",
                "## Inside\n\nMore.\n",
                "# Two\n\nSecond.\n"
            ]
        );
    }

    #[test]
    fn a_section_without_headings_is_cut_between_blocks() {
        let text = "Alpha one.\n\nBeta two.\n\nGamma three.\n\nDelta four.\n";
        assert_eq!(
            split(text.to_owned(), 30),
            ["Alpha one.\n\nBeta two.\n", "Gamma three.\n\nDelta four.\n"]
        );
    }

    #[test]
    fn one_block_longer_than_the_limit_is_not_cut() {
        let text = format!("{}\n\nShort.\n", "word ".repeat(20).trim_end());
        let chapters = split(text, 30);
        assert_eq!(chapters.len(), 2);
        assert!(chapters[0].len() > 30);
    }

    #[test]
    fn a_section_of_one_block_stays_whole() {
        let text = "word ".repeat(20);
        assert_eq!(split(text.clone(), 30), [text]);
    }

    #[test]
    fn a_footnote_follows_its_reference_into_another_chapter() {
        let text = "# One\n\nA note[^n].\n\n# Two\n\nAnother[^n] and [a link][l].\n\n[^n]: The note.\n\n[l]: https://example.org\n";
        let chapters = split(text.to_owned(), 90);
        assert_eq!(
            chapters,
            [
                "# One\n\nA note[^n].\n\n[^n]: The note.\n",
                "# Two\n\nAnother[^n] and [a link][l].\n\n[^n]: The note.\n\n[l]: https://example.org\n"
            ]
        );
    }

    #[test]
    fn a_picture_shown_through_a_definition_takes_the_definition_along() {
        let text = "# One\n\n![A cat][cat]\n\n# Two\n\nNo picture here.\n\n[cat]: a.png\n";
        let chapters = split(text.to_owned(), 30);
        assert_eq!(
            chapters,
            [
                "# One\n\n![A cat][cat]\n\n[cat]: a.png\n",
                "# Two\n\nNo picture here.\n\n[cat]: a.png\n"
            ]
        );
    }

    #[test]
    fn definitions_never_start_a_chapter() {
        let text = "Alpha one, a long line.\n\nBeta[^n] two.\n\n[^n]: A note that is long.\n\n[l]: https://example.org\n";
        let chapters = split(text.to_owned(), 30);
        assert_eq!(
            chapters,
            [
                "Alpha one, a long line.\n",
                "Beta[^n] two.\n\n[^n]: A note that is long.\n\n[l]: https://example.org\n"
            ]
        );
    }

    #[test]
    fn a_definition_nobody_uses_is_not_copied() {
        let text = "# One\n\nPlain.\n\n# Two\n\nPlain too.\n\n[^n]: Unused.\n";
        let chapters = split(text.to_owned(), 20);
        assert_eq!(chapters[0], "# One\n\nPlain.\n");
    }
}
