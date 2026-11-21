//! A picture that cannot be shown is replaced by its alternative text.
//!
//! The Markdown view draws nothing for such a picture. The widget gives it a
//! source in which `![alt](dest)` of a failed picture is written `*alt*` and
//! padded with spaces to the same length, so that every byte offset of the
//! chapter, and with it the place of the reader, is unchanged.

use std::collections::{HashMap, HashSet};

use markdown::ParseOptions;
use markdown::mdast::Node;

/// Characters of an alternative text that would be read as Markdown.
const MARKUP: &[char] = &[
    '*', '_', '`', '[', ']', '<', '>', '\\', '&', '\n', '\r', '!',
];

/// `text` with the pictures whose destination is in `failed` replaced by their
/// alternative text, or `None` when no picture is.
pub(super) fn with_alt_text(text: &str, failed: &HashSet<String>) -> Option<String> {
    if failed.is_empty() {
        return None;
    }
    let root = markdown::to_mdast(text, &ParseOptions::gfm()).ok()?;
    let mut definitions = HashMap::new();
    definitions_of(&root, &mut definitions);
    let mut spans = Vec::new();
    pictures_of(&root, &definitions, failed, &mut spans);
    if spans.is_empty() {
        return None;
    }
    spans.sort_by_key(|span| span.0);
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (start, end, alt) in spans {
        if start < at || end > text.len() {
            // Nested or overlapping: the outer one was already written.
            continue;
        }
        let (Some(before), Some(span)) = (text.get(at..start), text.get(start..end)) else {
            continue;
        };
        out.push_str(before);
        let written = replacement(&alt, span.len());
        out.push_str(&written);
        at = end;
    }
    out.push_str(text.get(at..)?);
    Some(out)
}

/// `*alt*`, padded with spaces to `len` bytes; spaces alone when the text
/// does not fit or is empty.
fn replacement(alt: &str, len: usize) -> String {
    let clean: String = alt.chars().filter(|c| !MARKUP.contains(c)).collect();
    let clean = clean.trim();
    let mut written = if clean.is_empty() {
        String::new()
    } else {
        format!("*{clean}*")
    };
    if written.len() > len {
        written.clear();
    }
    written.extend(std::iter::repeat_n(' ', len - written.len()));
    written
}

fn definitions_of<'a>(node: &'a Node, out: &mut HashMap<&'a str, &'a str>) {
    if let Node::Definition(definition) = node {
        out.entry(definition.identifier.as_str())
            .or_insert(definition.url.as_str());
    }
    for child in node.children().map_or_default(Vec::as_slice) {
        definitions_of(child, out);
    }
}

fn pictures_of(
    node: &Node,
    definitions: &HashMap<&str, &str>,
    failed: &HashSet<String>,
    out: &mut Vec<(usize, usize, String)>,
) {
    let picture = match node {
        Node::Image(image) => Some((image.url.as_str(), image.alt.as_str())),
        Node::ImageReference(reference) => definitions
            .get(reference.identifier.as_str())
            .map(|url| (*url, reference.alt.as_str())),
        _ => None,
    };
    if let Some((url, alt)) = picture
        && failed.contains(url)
        && let Some(position) = node.position()
    {
        out.push((position.start.offset, position.end.offset, alt.to_owned()));
        return;
    }
    for child in node.children().map_or_default(Vec::as_slice) {
        pictures_of(child, definitions, failed, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(destinations: &[&str]) -> HashSet<String> {
        destinations.iter().map(|d| (*d).to_owned()).collect()
    }

    #[test]
    fn a_failed_picture_is_written_as_its_alternative_text_in_the_same_length() {
        let text = "Before\n\n![A cat](img/cat.avif)\n\nAfter\n";
        let out = with_alt_text(text, &failed(&["img/cat.avif"])).unwrap();
        assert_eq!(out.len(), text.len());
        assert!(out.contains("*A cat*"));
        assert!(!out.contains("img/cat"));
        assert!(out.starts_with("Before\n\n"));
        assert!(out.ends_with("\n\nAfter\n"));
    }

    #[test]
    fn a_picture_that_did_not_fail_is_left_alone() {
        let text = "![A cat](img/cat.avif)\n";
        assert_eq!(with_alt_text(text, &failed(&["img/dog.avif"])), None);
    }

    #[test]
    fn nothing_changes_when_nothing_failed() {
        assert_eq!(with_alt_text("![a](b.avif)\n", &failed(&[])), None);
    }

    #[test]
    fn a_picture_without_alternative_text_leaves_blank_space() {
        let text = "x ![](a.avif) y\n";
        let out = with_alt_text(text, &failed(&["a.avif"])).unwrap();
        assert_eq!(out.len(), text.len());
        assert_eq!(out, format!("x {} y\n", " ".repeat(11)));
    }

    #[test]
    fn markup_in_an_alternative_text_is_dropped() {
        let text = "![a *b* [c]](p.avif)\n";
        let out = with_alt_text(text, &failed(&["p.avif"])).unwrap();
        assert!(out.starts_with("*a b c*"), "{out:?}");
        assert_eq!(out.len(), text.len());
    }

    #[test]
    fn a_reference_picture_is_found_through_its_definition() {
        let text = "![Map][m]\n\n[m]: maps/world.avif\n";
        let out = with_alt_text(text, &failed(&["maps/world.avif"])).unwrap();
        assert!(out.starts_with("*Map*"));
        assert_eq!(out.len(), text.len());
    }

    #[test]
    fn a_picture_inside_a_link_keeps_the_link() {
        let text = "[![Logo](l.avif)](https://example.org)\n";
        let out = with_alt_text(text, &failed(&["l.avif"])).unwrap();
        assert!(out.starts_with("[*Logo*"));
        assert!(out.ends_with("](https://example.org)\n"));
        assert_eq!(out.len(), text.len());
    }

    #[test]
    fn two_pictures_of_a_paragraph_are_both_replaced() {
        let text = "![a](1.avif) and ![b](2.avif)\n";
        let out = with_alt_text(text, &failed(&["1.avif", "2.avif"])).unwrap();
        assert!(out.contains("*a*") && out.contains("*b*") && out.contains(" and "));
        assert_eq!(out.len(), text.len());
    }

    #[test]
    fn multibyte_text_around_a_picture_is_kept_whole() {
        let text = "Привет ![Кот](к.avif) мир\n";
        let out = with_alt_text(text, &failed(&["к.avif"])).unwrap();
        assert_eq!(out.len(), text.len());
        assert!(out.starts_with("Привет *Кот*"));
        assert!(out.ends_with(" мир\n"));
    }

    #[test]
    fn an_alternative_text_that_does_not_fit_leaves_blank_space() {
        assert_eq!(replacement("long alternative text", 6), "      ");
    }
}
