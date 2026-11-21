//! The metadata of an EPUB package.

use crate::draft::DraftMeta;
use crate::format::Format;
use crate::xhtml;
use ::markdown::mdast::Node;
use ::markdown::{ParseOptions, to_mdast};
use rbook::Epub;

/// The prefix of an identifier that is an ISBN, in any letter case.
const ISBN_URN: &str = "urn:isbn:";

/// The identifier scheme of an ISBN in ONIX code list 5: ISBN-10 and ISBN-13.
const ONIX_ISBNS: [&str; 2] = ["02", "15"];

/// Reads the metadata of the package. Nothing here can fail: what is not
/// there is left out.
pub(super) fn read(epub: &Epub) -> DraftMeta {
    let metadata = epub.metadata();
    let (series, series_index) = series(epub);
    DraftMeta {
        title: metadata
            .title()
            .map(|title| title.value().trim().to_owned()),
        languages: metadata
            .languages()
            .map(|language| language.value().trim().to_owned())
            .collect(),
        id: metadata
            .identifier()
            .map(|identifier| identifier.value().trim().to_owned()),
        authors: metadata
            .creators()
            .map(|creator| creator.value().trim().to_owned())
            .collect(),
        translators: metadata
            .contributors()
            .filter(|contributor| contributor.roles().any(|role| role.code() == "trl"))
            .map(|contributor| contributor.value().trim().to_owned())
            .collect(),
        series,
        series_index,
        publisher: metadata
            .publishers()
            .next()
            .map(|publisher| publisher.value().trim().to_owned()),
        date: metadata
            .published_entry()
            .map(|date| date.value().trim().to_owned()),
        isbns: metadata
            .identifiers()
            .filter_map(|identifier| isbn(identifier.value(), identifier.scheme()))
            .collect(),
        genres: metadata
            .tags()
            .map(|tag| tag.value().trim().to_owned())
            .collect(),
        annotation: metadata
            .description()
            .and_then(|description| plain_text(description.value())),
    }
}

/// The series and the position in it: a collection of EPUB 3, preferably one
/// of type `series`, otherwise the `calibre:series` of Calibre.
fn series(epub: &Epub) -> (Option<String>, Option<String>) {
    let metadata = epub.metadata();
    let collections: Vec<_> = metadata.by_property("belongs-to-collection").collect();
    let chosen = collections
        .iter()
        .find(|collection| {
            collection
                .refinements()
                .by_property("collection-type")
                .any(|kind| kind.value().trim() == "series")
        })
        .or_else(|| collections.first());
    if let Some(collection) = chosen {
        let index = collection
            .refinements()
            .by_property("group-position")
            .next()
            .map(|position| position.value().trim().to_owned());
        return (Some(collection.value().trim().to_owned()), index);
    }
    let first = |name: &'static str| {
        metadata
            .by_property(name)
            .next()
            .map(|entry| entry.value().trim().to_owned())
    };
    (first("calibre:series"), first("calibre:series_index"))
}

/// The ISBN an identifier holds: the rest of a `urn:isbn:` value, or the
/// whole value of an identifier whose scheme is an ISBN.
fn isbn(value: &str, scheme: Option<rbook::ebook::metadata::Scheme<'_>>) -> Option<String> {
    let value = value.trim();
    if let Some(prefix) = value.get(..ISBN_URN.len())
        && prefix.eq_ignore_ascii_case(ISBN_URN)
    {
        return Some(value.get(ISBN_URN.len()..)?.trim().to_owned());
    }
    let scheme = scheme?;
    let code = scheme.code().trim();
    let onix = scheme
        .source()
        .is_some_and(|source| source.to_ascii_lowercase().starts_with("onix"));
    (code.eq_ignore_ascii_case("isbn") || (onix && ONIX_ISBNS.contains(&code)))
        .then(|| value.to_owned())
}

/// A description as plain text, paragraphs separated by an empty line. A
/// description that holds markup is read as HTML.
fn plain_text(description: &str) -> Option<String> {
    let description = description.trim();
    if description.is_empty() {
        return None;
    }
    let paragraphs: Vec<String> = if description.contains('<') {
        html_paragraphs(description)
    } else {
        let mut paragraphs = vec![String::new()];
        for line in description.lines().map(str::trim) {
            match (line.is_empty(), paragraphs.last_mut()) {
                (true, Some(last)) if !last.is_empty() => paragraphs.push(String::new()),
                (false, Some(last)) => {
                    if !last.is_empty() {
                        last.push('\n');
                    }
                    last.push_str(line);
                }
                _ => {}
            }
        }
        paragraphs
    };
    let text = paragraphs
        .into_iter()
        .filter(|paragraph| !paragraph.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (!text.is_empty()).then_some(text)
}

/// The blocks of an HTML fragment as plain text.
fn html_paragraphs(html: &str) -> Vec<String> {
    let Ok(document) = xhtml::convert(Format::Epub, html, |_| None, |_| Ok(None)) else {
        return Vec::new();
    };
    // Parsing fails only for MDX syntax, which the dialect does not enable.
    let Ok(root) = to_mdast(&document.markdown, &ParseOptions::gfm()) else {
        return Vec::new();
    };
    let mut blocks = Vec::new();
    collect_blocks(&root, &mut blocks);
    blocks
}

/// Adds the text of every paragraph, heading and code block below a node.
fn collect_blocks(node: &Node, blocks: &mut Vec<String>) {
    match node {
        Node::Paragraph(_) | Node::Heading(_) | Node::TableCell(_) => {
            let mut text = String::new();
            inline_text(node, &mut text);
            blocks.push(text.trim().to_owned());
        }
        Node::Code(code) => blocks.push(code.value.trim().to_owned()),
        other => {
            for child in other.children().map(Vec::as_slice).unwrap_or_default() {
                collect_blocks(child, blocks);
            }
        }
    }
}

/// The text of the inline content of a block.
fn inline_text(node: &Node, out: &mut String) {
    match node {
        Node::Text(text) => out.push_str(&text.value),
        Node::InlineCode(code) => out.push_str(&code.value),
        Node::Image(image) => out.push_str(&image.alt),
        Node::Break(_) => out.push(' '),
        Node::Html(_) | Node::FootnoteReference(_) => {}
        other => {
            for child in other.children().map(Vec::as_slice).unwrap_or_default() {
                inline_text(child, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{isbn, plain_text};

    #[test]
    fn plain_text_keeps_its_paragraphs() {
        assert_eq!(
            plain_text("  One.\r\nStill one.\n\n\n Two. \n").as_deref(),
            Some("One.\nStill one.\n\nTwo.")
        );
    }

    #[test]
    fn code_and_pictures_in_a_description_are_text() {
        assert_eq!(
            plain_text(
                "<p>Use <code>x</code> <img src=\"a.png\" alt=\"a pic\"/>.</p><pre>a block</pre>"
            )
            .as_deref(),
            Some("Use x a pic.\n\na block")
        );
    }

    #[test]
    fn an_empty_description_is_nothing() {
        assert_eq!(plain_text(" \n "), None);
        assert_eq!(plain_text("<p> </p>"), None);
    }

    #[test]
    fn html_in_a_description_becomes_paragraphs_of_text() {
        assert_eq!(
            plain_text("<p>One <b>bold</b> &amp; <a href=\"x\">linked</a>.</p><p>Two<br/>lines.</p><ul><li>Item</li></ul>")
                .as_deref(),
            Some("One bold & linked.\n\nTwo lines.\n\nItem")
        );
    }

    #[test]
    fn an_isbn_is_a_urn_or_an_identifier_of_that_scheme() {
        assert_eq!(
            isbn("URN:ISBN: 978-3-16-148410-0 ", None).as_deref(),
            Some("978-3-16-148410-0")
        );
        assert_eq!(isbn("urn:uuid:1234", None), None);
        assert_eq!(isbn("9783161484100", None), None);
        assert_eq!(isbn("urn:isb", None), None);
    }
}
