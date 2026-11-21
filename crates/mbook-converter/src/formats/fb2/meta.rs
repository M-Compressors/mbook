//! The description of a `FictionBook` document as the metadata of a draft.
//!
//! Values are taken as the document writes them, trimmed; the core checks
//! them.

use super::text::plain;
use super::tree::Element;
use crate::draft::DraftMeta;

/// Reads the metadata. Whatever the document lacks stays empty.
pub(super) fn read(root: &Element) -> DraftMeta {
    let description = root.child("description");
    let info = description.and_then(|description| description.child("title-info"));
    let publish = description.and_then(|description| description.child("publish-info"));
    let document = description.and_then(|description| description.child("document-info"));
    let info_children =
        |name: &'static str| info.into_iter().flat_map(move |info| info.named(name));

    let sequence = info_children("sequence")
        .chain(
            publish
                .into_iter()
                .flat_map(|publish| publish.named("sequence")),
        )
        .find(|sequence| non_empty(sequence.attr("name")).is_some());
    let date = publish
        .and_then(|publish| text_of(publish, "year"))
        .or_else(|| {
            info.and_then(|info| info.child("date")).and_then(|date| {
                non_empty(date.attr("value"))
                    .map(str::to_owned)
                    .or_else(|| non_empty(Some(&plain(date))).map(str::to_owned))
            })
        });

    DraftMeta {
        title: info.and_then(|info| text_of(info, "book-title")),
        languages: info
            .and_then(|info| text_of(info, "lang"))
            .into_iter()
            .collect(),
        id: document.and_then(|document| text_of(document, "id")),
        authors: info_children("author").filter_map(person).collect(),
        translators: info_children("translator").filter_map(person).collect(),
        series: sequence.and_then(|sequence| non_empty(sequence.attr("name")).map(str::to_owned)),
        series_index: sequence
            .and_then(|sequence| non_empty(sequence.attr("number")).map(str::to_owned)),
        publisher: publish.and_then(|publish| text_of(publish, "publisher")),
        date,
        isbns: publish
            .and_then(|publish| text_of(publish, "isbn"))
            .into_iter()
            .collect(),
        genres: info_children("genre")
            .filter_map(|genre| non_empty(Some(&plain(genre))).map(str::to_owned))
            .collect(),
        annotation: info
            .and_then(|info| info.child("annotation"))
            .and_then(annotation),
    }
}

/// The text, when it holds more than white space.
fn non_empty(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|text| !text.is_empty())
}

/// The text of the first child of a name, when it holds more than white
/// space.
fn text_of(parent: &Element, name: &str) -> Option<String> {
    parent
        .child(name)
        .map(plain)
        .filter(|text| !text.is_empty())
}

/// A person as "First Middle Last", or the nickname when there are no names.
fn person(author: &Element) -> Option<String> {
    let names: Vec<String> = ["first-name", "middle-name", "last-name"]
        .iter()
        .filter_map(|name| text_of(author, name))
        .collect();
    if names.is_empty() {
        text_of(author, "nickname")
    } else {
        Some(names.join(" "))
    }
}

/// An annotation as plain text, paragraphs separated by an empty line.
fn annotation(annotation: &Element) -> Option<String> {
    let mut blocks = Vec::new();
    for element in annotation.elements() {
        match element.name.as_str() {
            "poem" => {
                for stanza in element.named("stanza") {
                    let lines: Vec<String> = stanza
                        .elements()
                        .map(plain)
                        .filter(|line| !line.is_empty())
                        .collect();
                    blocks.push(lines.join("\n"));
                }
            }
            "cite" => blocks.extend(element.named("p").map(plain)),
            "table" | "empty-line" => {}
            _ => blocks.push(plain(element)),
        }
    }
    blocks.retain(|block| !block.is_empty());
    (!blocks.is_empty()).then(|| blocks.join("\n\n"))
}
