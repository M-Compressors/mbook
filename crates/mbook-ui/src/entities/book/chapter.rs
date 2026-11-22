//! A chapter prepared for the view, and what the Markdown of one says.

use std::collections::{HashMap, HashSet};

use markdown::ParseOptions;
use markdown::mdast::{Definition, Node};
use percent_encoding::percent_decode_str;

use crate::entities::figure::{FigureSources, ImageReference};

/// A chapter, read and analysed, with everything the view needs to show it
/// without touching the file again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedChapter {
    /// The index of the chapter in [`BookSnapshot::chapters`].
    ///
    /// [`BookSnapshot::chapters`]: super::snapshot::BookSnapshot::chapters
    pub(crate) index: usize,
    /// The Markdown of the chapter.
    pub(crate) text: String,
    /// The byte offset of every top-level block, ascending. The first is 0
    /// when the text is not empty.
    pub(crate) blocks: Vec<u32>,
    /// Every link destination of the text, as the Markdown parser reports it.
    pub(crate) links: HashMap<String, Target>,
    /// Every picture destination of the text that is a picture of the book.
    pub(crate) pictures: HashMap<String, PictureRef>,
    /// Every picture destination of the text that is a diagram or a formula
    /// of the book, with its source.
    pub(crate) figures: FigureSources,
}

/// A picture of the book, named by a chapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PictureRef {
    /// The resource number of the picture; private to the book module.
    pub(super) number: u32,
    /// The width in pixels, as displayed.
    pub(crate) width: u32,
    /// The height in pixels, as displayed.
    pub(crate) height: u32,
}

/// Where a link leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    /// A web address; opened only when it is followed.
    External(String),
    /// A place in a chapter of the book.
    Place {
        /// The index of the chapter in [`BookSnapshot::chapters`].
        ///
        /// [`BookSnapshot::chapters`]: super::snapshot::BookSnapshot::chapters
        chapter: usize,
        /// A byte offset into the text of the chapter.
        offset: u32,
    },
    /// A picture of the book.
    Picture(PictureRef),
    /// A named place that is not a heading: complete it with
    /// [`BookSource::follow`](super::source::BookSource::follow).
    Deferred,
    /// The link leads nowhere: the view shows the text of the link.
    Unresolved,
}

impl PreparedChapter {
    /// The index of the block that contains a byte offset: the last block
    /// that starts at or before it, or the first when the offset is before
    /// every block.
    #[cfg(test)]
    pub(crate) fn block_at(&self, offset: u32) -> usize {
        self.blocks
            .partition_point(|&start| start <= offset)
            .saturating_sub(1)
    }
}

/// What the Markdown of a chapter says that the view needs.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Analysis {
    /// The offset of every top-level block, ascending, starting at 0.
    pub(super) blocks: Vec<u32>,
    /// The destination of every link, once each.
    pub(super) links: Vec<String>,
    /// The destination of every picture, once each.
    pub(super) pictures: Vec<String>,
    /// The destination of every picture shown through a link reference
    /// definition, `![alt][name]`, by the identifier of the definition.
    pub(super) references: HashMap<String, ImageReference>,
}

/// Image destinations in the chapter, including reference-style images.
pub(crate) fn picture_destinations(text: &str) -> Vec<String> {
    analyse(text).pictures
}

/// What a text shows with the syntax of a picture: every destination, once,
/// and the destinations reached through a link reference definition, by the
/// identifier of the definition.
pub(crate) fn shown(text: &str) -> (Vec<String>, HashMap<String, ImageReference>) {
    let analysis = analyse(text);
    (analysis.pictures, analysis.references)
}

/// Resolves an image destination against a chapter path before looking it up.
pub(crate) fn image_path(chapter: &str, destination: &str) -> Option<String> {
    let has_scheme = destination.split_once(':').is_some_and(|(scheme, _)| {
        let mut bytes = scheme.bytes();
        bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
            && bytes.all(|byte| byte.is_ascii_alphanumeric() || b"+.-".contains(&byte))
    });
    if has_scheme || destination.contains(['#', '?']) {
        return None;
    }
    let path = percent_decode_str(destination).decode_utf8().ok()?;
    let mut segments = chapter.split('/').collect::<Vec<_>>();
    segments.pop();
    for name in path.split('/') {
        match name {
            "" => return None,
            "." => {}
            ".." => {
                segments.pop()?;
            }
            name => segments.push(name),
        }
    }
    Some(segments.join("/"))
}

/// Parses a chapter as the dialect of RFC 0001, `CommonMark` with the GFM
/// extensions, as `mbook` does.
///
/// Destinations are those the parser reports, after escapes and entities. An
/// anchor definition (`[name]: #name`) is not a link; a link that uses one by
/// its name is.
pub(super) fn analyse(text: &str) -> Analysis {
    let mut analysis = Analysis::default();
    // Parsing fails only for MDX syntax, which the dialect does not enable.
    if let Ok(root) = markdown::to_mdast(text, &ParseOptions::gfm()) {
        let mut definitions = HashMap::new();
        collect_definitions(&root, &mut definitions);
        analysis.blocks = block_offsets(&root, text);
        let mut seen = (HashSet::new(), HashSet::new());
        visit(&root, &definitions, &mut analysis, &mut seen);
    }
    if analysis.blocks.first() != Some(&0) && !text.is_empty() {
        analysis.blocks.insert(0, 0);
    }
    analysis
}

/// The first byte of the line of every child of the root.
fn block_offsets(root: &Node, text: &str) -> Vec<u32> {
    let mut blocks: Vec<u32> = root
        .children()
        .map_or_default(Vec::as_slice)
        .iter()
        .filter_map(Node::position)
        .map(|position| to_offset(line_start(text, position.start.offset)))
        .collect();
    blocks.sort_unstable();
    blocks.dedup();
    blocks
}

/// Maps the identifier of every link reference definition to its destination.
fn collect_definitions<'a>(node: &'a Node, definitions: &mut HashMap<&'a str, &'a Definition>) {
    if let Node::Definition(definition) = node {
        // The first definition of an identifier wins, as in `CommonMark`.
        definitions
            .entry(definition.identifier.as_str())
            .or_insert(definition);
    }
    for child in node.children().map_or_default(Vec::as_slice) {
        collect_definitions(child, definitions);
    }
}

/// Collects the destinations of links and pictures, once each.
fn visit(
    node: &Node,
    definitions: &HashMap<&str, &Definition>,
    analysis: &mut Analysis,
    seen: &mut (HashSet<String>, HashSet<String>),
) {
    let (link, picture) = match node {
        Node::Link(link) => (Some(link.url.as_str()), None),
        Node::Image(image) => (None, Some(image.url.as_str())),
        Node::LinkReference(reference) => (
            definitions
                .get(reference.identifier.as_str())
                .map(|definition| definition.url.as_str()),
            None,
        ),
        Node::ImageReference(reference) => {
            let definition = definitions.get(reference.identifier.as_str());
            if let Some(definition) = definition {
                analysis
                    .references
                    .entry(reference.identifier.clone())
                    .or_insert_with(|| ImageReference {
                        destination: definition.url.clone(),
                        title: definition.title.clone(),
                    });
            }
            (None, definition.map(|definition| definition.url.as_str()))
        }
        _ => (None, None),
    };
    if let Some(url) = link.filter(|url| seen.0.insert((*url).to_owned())) {
        analysis.links.push(url.to_owned());
    }
    if let Some(url) = picture.filter(|url| seen.1.insert((*url).to_owned())) {
        analysis.pictures.push(url.to_owned());
    }
    for child in node.children().map_or_default(Vec::as_slice) {
        visit(child, definitions, analysis, seen);
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
    use super::*;

    fn chapter(blocks: &[u32]) -> PreparedChapter {
        PreparedChapter {
            index: 0,
            text: String::new(),
            blocks: blocks.to_vec(),
            links: HashMap::new(),
            pictures: HashMap::new(),
            figures: FigureSources::default(),
        }
    }

    fn offset_of(text: &str, needle: &str) -> u32 {
        u32::try_from(text.find(needle).unwrap()).unwrap()
    }

    #[test]
    fn every_top_level_block_starts_at_its_line() {
        let text = "# Title\n\nA paragraph\nof two lines.\n\n- one\n- two\n\n```\ncode\n```\n\n| a | b |\n| - | - |\n| 1 | 2 |\n";
        let expected: Vec<u32> = ["# Title", "A paragraph", "- one", "```\ncode", "| a | b |"]
            .iter()
            .map(|needle| offset_of(text, needle))
            .collect();
        assert_eq!(analyse(text).blocks, expected);
    }

    #[test]
    fn the_items_of_a_list_are_not_blocks_of_their_own() {
        let text = "- one\n- two\n- three\n";
        assert_eq!(analyse(text).blocks, vec![0]);
    }

    #[test]
    fn an_indented_block_starts_at_its_line() {
        let text = "Intro\n\n   - indented\n";
        assert_eq!(analyse(text).blocks, vec![0, offset_of(text, "   - ")]);
    }

    #[test]
    fn the_first_block_is_at_zero_when_the_text_starts_with_blank_lines() {
        let text = "\n\nAfter blanks\n";
        assert_eq!(analyse(text).blocks, vec![0, offset_of(text, "After")]);
    }

    #[test]
    fn a_text_of_blank_lines_is_one_block() {
        assert_eq!(analyse("\n\n").blocks, vec![0]);
    }

    #[test]
    fn an_empty_text_has_no_block() {
        assert_eq!(analyse(""), Analysis::default());
    }

    #[test]
    fn a_link_destination_is_reported_after_escapes_and_entities() {
        let analysis = analyse("[a](ch\\_2.md#x&amp;y)\n");
        assert_eq!(analysis.links, vec!["ch_2.md#x&y".to_owned()]);
    }

    #[test]
    fn a_destination_used_twice_is_reported_once() {
        let analysis = analyse("[a](b.md) and [c](b.md)\n");
        assert_eq!(analysis.links, vec!["b.md".to_owned()]);
    }

    #[test]
    fn a_reference_link_is_reported_with_the_destination_of_its_definition() {
        let analysis = analyse("[text][ref]\n\n[ref]: other.md#x\n");
        assert_eq!(analysis.links, vec!["other.md#x".to_owned()]);
    }

    #[test]
    fn a_definition_that_nothing_uses_is_not_a_link() {
        assert_eq!(analyse("[ref]: other.md\n").links, Vec::<String>::new());
    }

    #[test]
    fn an_anchor_definition_is_not_a_link() {
        let analysis = analyse("[fig-1]: #fig-1\nA named line.\n");
        assert_eq!(analysis.links, Vec::<String>::new());
        assert_eq!(analysis.pictures, Vec::<String>::new());
    }

    #[test]
    fn an_autolink_is_a_link() {
        let analysis = analyse("<https://example.org>\n");
        assert_eq!(analysis.links, vec!["https://example.org".to_owned()]);
    }

    #[test]
    fn a_picture_is_not_a_link() {
        let analysis = analyse("![alt](img/a.avif)\n");
        assert_eq!(analysis.links, Vec::<String>::new());
        assert_eq!(analysis.pictures, vec!["img/a.avif".to_owned()]);
    }

    #[test]
    fn a_reference_picture_is_reported_with_the_destination_of_its_definition() {
        let analysis = analyse("![alt][pic]\n\n[pic]: img/b.avif\n");
        assert_eq!(analysis.pictures, vec!["img/b.avif".to_owned()]);
    }

    #[test]
    fn a_reference_picture_is_remembered_by_the_identifier_of_its_definition() {
        let (pictures, references) =
            shown("![alt][Pic] and ![x](a.avif)\n\n[pic]: math/e.tex \"A formula\"\n");
        assert_eq!(pictures, ["math/e.tex", "a.avif"]);
        assert_eq!(
            references,
            HashMap::from([(
                "pic".to_owned(),
                ImageReference {
                    destination: "math/e.tex".to_owned(),
                    title: Some("A formula".to_owned()),
                }
            )])
        );
    }

    #[test]
    fn a_reference_without_a_definition_shows_nothing() {
        let (pictures, references) = shown("![alt][nothing]\n");
        assert!(pictures.is_empty() && references.is_empty());
    }

    #[test]
    fn a_link_inside_a_list_inside_a_quote_is_found() {
        let analysis = analyse("> - [deep](d.md)\n");
        assert_eq!(analysis.links, vec!["d.md".to_owned()]);
    }

    #[test]
    fn the_block_of_an_offset_is_the_last_one_that_starts_at_or_before_it() {
        let chapter = chapter(&[0, 10, 30]);
        assert_eq!(chapter.block_at(0), 0);
        assert_eq!(chapter.block_at(9), 0);
        assert_eq!(chapter.block_at(10), 1);
        assert_eq!(chapter.block_at(29), 1);
        assert_eq!(chapter.block_at(30), 2);
        assert_eq!(chapter.block_at(u32::MAX), 2);
    }

    #[test]
    fn an_offset_before_the_first_block_is_in_the_first() {
        assert_eq!(chapter(&[5, 10]).block_at(2), 0);
    }

    #[test]
    fn a_chapter_without_blocks_has_block_zero() {
        assert_eq!(chapter(&[]).block_at(10), 0);
    }
}
