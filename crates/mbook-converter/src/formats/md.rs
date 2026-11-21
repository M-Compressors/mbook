//! One Markdown document.
//!
//! The text is Markdown already, so little is done to it:
//!
//! - The encoding is detected.
//! - The document is cut into sections at its top-level headings; text in
//!   front of the first one is a section of its own.
//! - A link to a heading, written the way GitHub understands it
//!   (`[see](#the-heading)`), keeps working across sections: every heading
//!   is marked with the name GitHub derives from it, and such links are
//!   pointed at the section that holds the heading.
//! - A metadata block at the start of the document (front matter) is
//!   removed, with a warning: the dialect of RFC 0001 would show it as text.
//!
//! A Markdown document has no metadata this front end reads and brings no
//! pictures with it: a stream has no folder to find them in, so the
//! assembler replaces every picture by its alternative text.

use crate::assemble::rewrite::{self, Fate, Rules};
use crate::draft::{Draft, Section};
use crate::encoding;
use crate::error::Result;
use crate::markdown as md;
use crate::options::ConvertOptions;
use crate::report::{Warning, WarningKind};
use github_slugger::Slugger;
use markdown::mdast::Node;
use markdown::{ParseOptions, to_mdast};
use percent_encoding::percent_decode_str;
use std::collections::HashMap;

/// One heading of the document.
struct Heading {
    /// Where its first line starts.
    start: usize,
    /// Whether a section starts here: a top-level heading between blocks.
    cuts: bool,
    /// The name GitHub gives it, when it stands between blocks and has one.
    name: Option<String>,
}

/// Reads a Markdown document into a draft.
#[expect(
    clippy::unnecessary_wraps,
    reason = "every front end has the same signature, and the others can fail"
)]
pub(crate) fn read(source: &[u8], _options: &ConvertOptions) -> Result<Draft> {
    let text = encoding::decode(source);
    let mut draft = Draft::default();
    let mut options = ParseOptions::gfm();
    options.constructs.frontmatter = true;
    // Parsing fails only for MDX syntax, which is not enabled.
    let tree = to_mdast(&text, &options).ok();
    let blocks = tree
        .as_ref()
        .and_then(Node::children)
        .map(Vec::as_slice)
        .unwrap_or_default();

    let body_start = match blocks.first() {
        Some(matter @ (Node::Yaml(_) | Node::Toml(_))) => {
            let kind = WarningKind::MarkupDropped("front matter".to_owned());
            draft.warnings.push(Warning::new(kind, ""));
            end_of(matter)
        }
        _ => 0,
    };
    let mut slugger = Slugger::default();
    let mut headings = Vec::new();
    for block in blocks {
        collect_headings(block, true, &mut slugger, &mut headings);
    }

    // A section runs from one cut to the next.
    let mut starts = vec![body_start];
    starts.extend(
        headings
            .iter()
            .filter(|heading| heading.cuts && heading.start > body_start)
            .map(|heading| heading.start),
    );
    let section_of = |offset: usize| starts.partition_point(|&start| start <= offset) - 1;
    let name_of = |section: usize| format!("s{}", section + 1);
    let home: HashMap<&str, usize> = headings
        .iter()
        .filter_map(|heading| Some((heading.name.as_deref()?, section_of(heading.start))))
        .collect();

    for (section, &start) in starts.iter().enumerate() {
        let end = starts.get(section + 1).copied().unwrap_or(text.len());
        let marked = with_marks(&text, start..end, &headings);
        let name = name_of(section);
        let link = |destination: &str| {
            let heading = destination
                .strip_prefix('#')
                .and_then(|fragment| percent_decode_str(fragment).decode_utf8().ok())
                .and_then(|fragment| home.get_key_value(&*fragment));
            match heading {
                Some((heading, &section)) => {
                    Fate::Replace(md::destination(&name_of(section), Some(heading)))
                }
                None => Fate::Keep,
            }
        };
        let markdown = match rewrite::parse(&marked) {
            Some(tree) => {
                let rules = Rules {
                    location: &name,
                    link: &link,
                    // Pictures and marks are for the assembler to judge.
                    picture: &mut |destination| {
                        let name = percent_decode_str(destination).decode_utf8().ok()?;
                        Some(name.into_owned())
                    },
                    mark: &|name| Some(name.to_owned()),
                    warnings: &mut draft.warnings,
                };
                rewrite::rewrite(&marked, &tree, &rewrite::scan(&tree), rules)
            }
            None => marked,
        };
        draft.sections.push(Section { name, markdown });
    }
    Ok(draft)
}

/// Where a node starts in the text.
fn start_of(node: &Node) -> usize {
    node.position().map_or(0, |position| position.start.offset)
}

/// Where a node ends in the text.
fn end_of(node: &Node) -> usize {
    node.position().map_or(0, |position| position.end.offset)
}

/// Finds every heading, in the order GitHub numbers repeated names in.
/// `between_blocks` tells whether the node stands at the top of the
/// document, where a mark can be put in front of it.
fn collect_headings(
    node: &Node,
    between_blocks: bool,
    slugger: &mut Slugger,
    headings: &mut Vec<Heading>,
) {
    if let Node::Heading(heading) = node {
        let mut title = String::new();
        plain_text(node, &mut title);
        let name = slugger.slug(&title);
        let named = !github_slugger::slug(&title).is_empty();
        headings.push(Heading {
            start: start_of(node),
            cuts: between_blocks && heading.depth == 1,
            name: (between_blocks && named).then_some(name),
        });
        return;
    }
    for child in node.children().map(Vec::as_slice).unwrap_or_default() {
        collect_headings(child, false, slugger, headings);
    }
}

/// Appends the text of a node without its markup.
fn plain_text(node: &Node, out: &mut String) {
    match node {
        Node::Text(text) => out.push_str(&text.value),
        Node::InlineCode(code) => out.push_str(&code.value),
        Node::Image(image) => out.push_str(&image.alt),
        Node::Break(_) => out.push(' '),
        other => {
            for child in other.children().map(Vec::as_slice).unwrap_or_default() {
                plain_text(child, out);
            }
        }
    }
}

/// A piece of the document with a mark in front of every named heading.
fn with_marks(text: &str, range: std::ops::Range<usize>, headings: &[Heading]) -> String {
    let mut out = String::with_capacity(range.len());
    let mut at = range.start;
    for heading in headings {
        let Some(name) = &heading.name else { continue };
        if !range.contains(&heading.start) {
            continue;
        }
        out.push_str(&text[at..heading.start]);
        // A definition cannot interrupt a paragraph.
        if !out.is_empty() && !out.ends_with("\n\n") {
            out.push('\n');
        }
        out.push_str(&md::definition(&md::anchor_name(name)));
        out.push_str("\n\n");
        at = heading.start;
    }
    out.push_str(&text[at..range.end]);
    out
}

#[cfg(test)]
mod tests {
    use super::read;
    use crate::options::ConvertOptions;
    use crate::report::WarningKind;

    /// The sections a document is read into.
    fn sections(text: &str) -> Vec<String> {
        let draft = read(text.as_bytes(), &ConvertOptions::default()).unwrap();
        draft
            .sections
            .into_iter()
            .map(|section| section.markdown)
            .collect()
    }

    #[test]
    fn a_document_is_cut_at_its_top_level_headings() {
        assert_eq!(
            sections("Preface.\n\n# One\n\nFirst.\n\n## Inside\n\n# Two\n\nSecond.\n"),
            [
                "Preface.\n\n",
                "[one]: #one\n\n# One\n\nFirst.\n\n[inside]: #inside\n\n## Inside\n\n",
                "[two]: #two\n\n# Two\n\nSecond.\n"
            ]
        );
    }

    #[test]
    fn a_document_without_headings_is_one_section() {
        assert_eq!(sections("Just text.\n"), ["Just text.\n"]);
    }

    #[test]
    fn a_heading_right_after_a_paragraph_keeps_its_mark_apart() {
        assert_eq!(
            sections("# One\ntext\n## Two\n"),
            ["[one]: #one\n\n# One\ntext\n\n[two]: #two\n\n## Two\n"]
        );
    }

    #[test]
    fn repeated_headings_are_numbered_the_way_github_does() {
        let all = sections("# Notes\n\n# Notes\n").concat();
        assert!(all.contains("[notes]: #notes") && all.contains("[notes-1]: #notes-1"));
    }

    #[test]
    fn code_pictures_and_line_breaks_count_in_the_name_of_a_heading() {
        let all = sections("The `mbook` ![logo](l.png) format\\\nexplained\n===\n").concat();
        assert!(all.starts_with("[the-mbook-logo-format-explained]: #"), "{all}");
    }

    #[test]
    fn a_heading_inside_a_quote_or_without_letters_gets_no_mark() {
        assert_eq!(
            sections("> # Quoted\n\n## ***\n"),
            ["> # Quoted\n\n## ***\n"]
        );
    }

    #[test]
    fn a_link_to_a_heading_names_the_section_that_holds_it() {
        let read = sections("# One\n\nSee [two](#two) and [nothing](#gone).\n\n# Two\n");
        assert_eq!(
            read[0],
            "[one]: #one\n\n# One\n\nSee [two](s2#two) and [nothing](#gone).\n\n"
        );
    }

    #[test]
    fn front_matter_is_removed_with_a_warning() {
        let draft = read(
            b"---\ntitle: The Example\n---\n\n# One\n",
            &ConvertOptions::default(),
        )
        .unwrap();
        assert_eq!(draft.sections.len(), 2);
        assert_eq!(draft.sections[1].markdown, "[one]: #one\n\n# One\n");
        assert_eq!(
            draft.warnings[0].kind,
            WarningKind::MarkupDropped("front matter".into())
        );
    }

    #[test]
    fn an_empty_document_has_one_empty_section() {
        assert_eq!(sections(""), [""]);
    }
}
