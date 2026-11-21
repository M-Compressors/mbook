//! What the write side learns from the Markdown of a chapter: its headings,
//! the destinations it refers to, and whether it contains raw HTML.

use crate::types::Anchor;
use github_slugger::Slugger;
use markdown::ParseOptions;
use markdown::mdast::Node;
use std::collections::HashMap;

/// One heading of a chapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Heading {
    /// The first byte of the line the heading starts on.
    pub(crate) offset: u32,
    /// The level in the Markdown source, 1 to 6.
    pub(crate) depth: u8,
    /// The text of the heading without markup.
    pub(crate) title: String,
}

/// What a destination is used for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceKind {
    Link,
    Picture,
}

/// One link or picture of a chapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Reference {
    /// Where the link or picture starts.
    pub(crate) offset: u32,
    pub(crate) kind: ReferenceKind,
    /// The destination, after escapes and entities were processed.
    pub(crate) destination: String,
}

/// Everything the checks need to know about one chapter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Analysis {
    /// Headings in the order they appear.
    pub(crate) headings: Vec<Heading>,
    /// Links and pictures in the order they appear.
    pub(crate) references: Vec<Reference>,
    /// Where each piece of raw HTML starts.
    pub(crate) html: Vec<u32>,
}

/// Parses a chapter as the dialect of RFC 0001: `CommonMark` with the GFM
/// extensions.
pub(crate) fn analyse(text: &str) -> Analysis {
    // Parsing fails only for MDX syntax, which the dialect does not enable.
    let Ok(root) = markdown::to_mdast(text, &ParseOptions::gfm()) else {
        return Analysis::default();
    };
    let mut definitions = HashMap::new();
    collect_definitions(&root, &mut definitions);
    let mut analysis = Analysis::default();
    visit(&root, text, &definitions, &mut analysis);
    analysis
}

/// One anchor for every heading that has a name, sorted by offset.
///
/// Names follow the rule of RFC 0001, which is the one GitHub uses: the text
/// of the heading in lower case, without punctuation, with spaces replaced by
/// `-`, and with `-1`, `-2`, … appended to names that repeat in the chapter.
/// The rule is defined by GitHub's implementation, so it is taken from a port
/// of that implementation. A heading whose name would be empty gets no
/// anchor, because the format does not allow an empty name.
pub(crate) fn anchors(headings: &[Heading]) -> Vec<Anchor> {
    let mut slugger = Slugger::default();
    headings
        .iter()
        .filter(|heading| !github_slugger::slug(&heading.title).is_empty())
        .map(|heading| Anchor {
            offset: heading.offset,
            name: slugger.slug(&heading.title),
        })
        .collect()
}

/// Maps the identifier of every link reference definition to its destination.
fn collect_definitions<'a>(node: &'a Node, definitions: &mut HashMap<&'a str, &'a str>) {
    if let Node::Definition(definition) = node {
        // The first definition of an identifier wins, as in `CommonMark`.
        definitions
            .entry(definition.identifier.as_str())
            .or_insert(definition.url.as_str());
    }
    for child in node.children().map(Vec::as_slice).unwrap_or_default() {
        collect_definitions(child, definitions);
    }
}

fn visit(node: &Node, text: &str, definitions: &HashMap<&str, &str>, analysis: &mut Analysis) {
    let start = node.position().map_or(0, |position| position.start.offset);
    let mut refer = |kind, destination: &str| {
        analysis.references.push(Reference {
            offset: to_offset(start),
            kind,
            destination: destination.to_owned(),
        });
    };
    match node {
        Node::Heading(heading) => {
            let mut title = String::new();
            heading
                .children
                .iter()
                .for_each(|child| plain_text(child, &mut title));
            analysis.headings.push(Heading {
                offset: to_offset(line_start(text, start)),
                depth: heading.depth,
                title,
            });
        }
        Node::Html(_) => analysis.html.push(to_offset(start)),
        Node::Link(link) => refer(ReferenceKind::Link, &link.url),
        Node::Image(image) => refer(ReferenceKind::Picture, &image.url),
        Node::LinkReference(reference) => {
            if let Some(url) = definitions.get(reference.identifier.as_str()) {
                refer(ReferenceKind::Link, url);
            }
        }
        Node::ImageReference(reference) => {
            if let Some(url) = definitions.get(reference.identifier.as_str()) {
                refer(ReferenceKind::Picture, url);
            }
        }
        _ => {}
    }
    for child in node.children().map(Vec::as_slice).unwrap_or_default() {
        visit(child, text, definitions, analysis);
    }
}

/// Appends the text of a node without its markup.
fn plain_text(node: &Node, out: &mut String) {
    match node {
        Node::Text(text) => out.push_str(&text.value),
        Node::InlineCode(code) => out.push_str(&code.value),
        Node::Image(image) => out.push_str(&image.alt),
        Node::ImageReference(image) => out.push_str(&image.alt),
        Node::Break(_) => out.push(' '),
        other => {
            for child in other.children().map(Vec::as_slice).unwrap_or_default() {
                plain_text(child, out);
            }
        }
    }
}

/// The first byte of the line that holds `offset`.
fn line_start(text: &str, offset: usize) -> usize {
    text.as_bytes()
        .get(..offset)
        .and_then(|before| before.iter().rposition(|&byte| byte == b'\n'))
        .map_or(0, |newline| newline + 1)
}

/// A chapter is smaller than 4 GiB, so its offsets fit.
fn to_offset(offset: usize) -> u32 {
    u32::try_from(offset).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::{Reference, ReferenceKind, analyse, anchors};

    fn headings(text: &str) -> Vec<(u32, u8, String)> {
        analyse(text)
            .headings
            .into_iter()
            .map(|heading| (heading.offset, heading.depth, heading.title))
            .collect()
    }

    fn names(text: &str) -> Vec<(u32, String)> {
        anchors(&analyse(text).headings)
            .into_iter()
            .map(|anchor| (anchor.offset, anchor.name))
            .collect()
    }

    fn references(text: &str) -> Vec<(ReferenceKind, String)> {
        analyse(text)
            .references
            .into_iter()
            .map(
                |Reference {
                     kind, destination, ..
                 }| (kind, destination),
            )
            .collect()
    }

    #[test]
    fn headings_are_found_with_their_line_start_and_depth() {
        let text = "# One\n\ntext\n\n## Two\n";
        assert_eq!(
            headings(text),
            [(0, 1, "One".to_owned()), (13, 2, "Two".to_owned())]
        );
    }

    #[test]
    fn a_heading_title_drops_its_markup() {
        let text = "# The *seventh* `proof` of [it](ch02.md)\n";
        assert_eq!(headings(text)[0].2, "The seventh proof of it");
    }

    #[test]
    fn a_setext_heading_starts_at_its_text_line() {
        let text = "intro\n\nTitle\n=====\n";
        assert_eq!(headings(text), [(7, 1, "Title".to_owned())]);
    }

    #[test]
    fn a_heading_inside_a_quote_is_placed_at_its_line_start() {
        let text = "text\n\n> ## Quoted\n";
        assert_eq!(headings(text), [(6, 2, "Quoted".to_owned())]);
    }

    #[test]
    fn a_hash_inside_a_code_block_is_not_a_heading() {
        assert_eq!(headings("```\n# not a heading\n```\n"), []);
    }

    #[test]
    fn the_name_of_the_format_rfc_example_is_reproduced() {
        assert_eq!(
            names("# Chapter 3. The Seventh Proof\n"),
            [(0, "chapter-3-the-seventh-proof".to_owned())]
        );
    }

    #[test]
    fn names_keep_letters_of_any_script_and_drop_punctuation() {
        assert_eq!(
            names("# Глава 1: «Приезд»!\n"),
            [(0, "глава-1-приезд".to_owned())]
        );
    }

    #[test]
    fn names_keep_hyphens_and_underscores() {
        assert_eq!(names("# a-b_c\n"), [(0, "a-b_c".to_owned())]);
    }

    #[test]
    fn repeated_names_are_numbered() {
        let text = "# Notes\n\n# Notes\n\n# Notes\n";
        let found: Vec<_> = names(text).into_iter().map(|(_, name)| name).collect();
        assert_eq!(found, ["notes", "notes-1", "notes-2"]);
    }

    #[test]
    fn a_numbered_name_does_not_collide_with_a_real_heading() {
        let text = "# Notes\n\n# Notes 1\n\n# Notes\n";
        let found: Vec<_> = names(text).into_iter().map(|(_, name)| name).collect();
        assert_eq!(found, ["notes", "notes-1", "notes-2"]);
    }

    #[test]
    fn a_heading_of_punctuation_gets_no_anchor() {
        assert_eq!(names("# ***\n\n# Real\n"), [(7, "real".to_owned())]);
    }

    #[test]
    fn links_and_pictures_are_found_with_their_destinations() {
        let text = "See [one](ch01.md#top) and ![fig](img/a.avif \"Caption\").\n";
        assert_eq!(
            references(text),
            [
                (ReferenceKind::Link, "ch01.md#top".to_owned()),
                (ReferenceKind::Picture, "img/a.avif".to_owned())
            ]
        );
    }

    #[test]
    fn a_reference_carries_the_offset_where_it_starts() {
        let analysis = analyse("ab [x](y.md)\n");
        assert_eq!(analysis.references[0].offset, 3);
    }

    #[test]
    fn reference_style_links_and_pictures_use_their_definition() {
        let text = "[one][a] and ![fig][b]\n\n[a]: ch01.md\n[b]: img/a.avif\n";
        assert_eq!(
            references(text),
            [
                (ReferenceKind::Link, "ch01.md".to_owned()),
                (ReferenceKind::Picture, "img/a.avif".to_owned())
            ]
        );
    }

    #[test]
    fn a_bare_url_is_an_external_link() {
        assert_eq!(
            references("Visit https://example.org now.\n"),
            [(ReferenceKind::Link, "https://example.org".to_owned())]
        );
    }

    #[test]
    fn escapes_in_a_destination_are_processed() {
        assert_eq!(
            references("[x](a\\_b.md)\n"),
            [(ReferenceKind::Link, "a_b.md".to_owned())]
        );
    }

    #[test]
    fn a_link_inside_code_is_not_a_reference() {
        assert_eq!(references("`[x](y.md)`\n"), []);
    }

    #[test]
    fn html_blocks_and_inline_html_are_reported() {
        let analysis = analyse("<div>block</div>\n\ntext <br> more\n");
        assert_eq!(analysis.html, [0, 23]);
    }

    #[test]
    fn an_autolink_in_angle_brackets_is_not_html() {
        let analysis = analyse("<https://example.org>\n");
        assert_eq!((analysis.html.len(), analysis.references.len()), (0, 1));
    }

    #[test]
    fn footnotes_are_neither_links_nor_html() {
        let analysis = analyse("Text[^n1].\n\n[^n1]: The note.\n");
        assert_eq!((analysis.html.len(), analysis.references.len()), (0, 0));
    }

    #[test]
    fn plain_text_has_nothing_to_report() {
        let analysis = analyse("Just a paragraph.\n");
        assert_eq!(analysis, super::Analysis::default());
    }
}
