//! The landmarks of an EPUB: the `landmarks` navigation of EPUB 3, otherwise
//! the `<guide>` of the EPUB 2 package.

use super::{Targets, destination};
use crate::draft::DraftLandmark;
use rbook::Epub;
use rbook::ebook::toc::TocEntryKind;
use std::collections::HashSet;

/// The kind of RFC 0001 that a semantic type of EPUB stands for.
///
/// `types` is the `epub:type` of an entry of the landmarks navigation, which
/// may hold several values separated by white space, or the `type` of a
/// `<reference>` of the guide; the first value that maps wins and the case
/// does not matter. `None` for the cover, which is a picture of an mbook, for
/// a table of contents set in the text, which the contents of the book
/// replace, and for every type without a kind.
pub(super) fn kind_of(types: &str) -> Option<&'static str> {
    types.split_whitespace().find_map(|value| {
        Some(match value.to_ascii_lowercase().as_str() {
            "bodymatter" | "text" | "start" => "start",
            "titlepage" | "title-page" => "title-page",
            "dedication" => "dedication",
            "epigraph" => "epigraph",
            "foreword" => "foreword",
            "preface" => "preface",
            "introduction" => "introduction",
            "prologue" => "prologue",
            "epilogue" => "epilogue",
            "conclusion" => "conclusion",
            "acknowledgments" | "acknowledgements" => "acknowledgements",
            "afterword" => "afterword",
            "appendix" => "appendix",
            "endnotes" | "rearnotes" | "footnotes" | "notes" => "notes",
            "glossary" => "glossary",
            "bibliography" => "bibliography",
            "index" => "index",
            "loi" => "list-of-figures",
            "lot" => "list-of-tables",
            "copyright-page" => "copyright",
            "colophon" => "colophon",
            _ => return None,
        })
    })
}

/// The landmarks of the book, in document order.
///
/// An entry without an address or a type that has a kind is not a landmark.
/// The address is resolved like that of an entry of the contents; an entry
/// that leads to a left out spine item is dropped silently.
pub(super) fn read(
    epub: &Epub,
    targets: &Targets,
    skipped: &HashSet<String>,
) -> Vec<DraftLandmark> {
    // `rbook` prefers the navigation of EPUB 3 and falls back to the guide.
    let Some(root) = epub.toc().by_kind(TocEntryKind::Landmarks) else {
        return Vec::new();
    };
    root.flatten()
        .filter_map(|entry| {
            let kind = kind_of(entry.kind_raw()?)?;
            let target = destination(targets, skipped, entry.href()?.as_str())?;
            Some(DraftLandmark {
                kind: kind.to_owned(),
                title: entry.label().trim().to_owned(),
                target,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::kind_of;

    #[test]
    fn every_type_of_the_vocabularies_maps_to_its_kind() {
        let table = [
            ("bodymatter", "start"),
            ("text", "start"),
            ("start", "start"),
            ("introduction", "introduction"),
            ("prologue", "prologue"),
            ("epilogue", "epilogue"),
            ("conclusion", "conclusion"),
            ("titlepage", "title-page"),
            ("title-page", "title-page"),
            ("dedication", "dedication"),
            ("epigraph", "epigraph"),
            ("foreword", "foreword"),
            ("preface", "preface"),
            ("acknowledgments", "acknowledgements"),
            ("acknowledgements", "acknowledgements"),
            ("afterword", "afterword"),
            ("appendix", "appendix"),
            ("endnotes", "notes"),
            ("rearnotes", "notes"),
            ("footnotes", "notes"),
            ("notes", "notes"),
            ("glossary", "glossary"),
            ("bibliography", "bibliography"),
            ("index", "index"),
            ("loi", "list-of-figures"),
            ("lot", "list-of-tables"),
            ("copyright-page", "copyright"),
            ("colophon", "colophon"),
        ];
        for (types, kind) in table {
            assert_eq!(kind_of(types), Some(kind), "{types}");
        }
    }

    #[test]
    fn the_cover_and_unknown_types_have_no_kind() {
        assert_eq!(kind_of("cover"), None);
        assert_eq!(kind_of("toc"), None);
        assert_eq!(kind_of("chapter"), None);
        assert_eq!(kind_of("other.unknown"), None);
        assert_eq!(kind_of(""), None);
        assert_eq!(kind_of("  "), None);
    }

    #[test]
    fn the_first_value_that_maps_wins_and_case_does_not_matter() {
        assert_eq!(kind_of("frontmatter Glossary index"), Some("glossary"));
        assert_eq!(kind_of("cover  \t INDEX"), Some("index"));
        assert_eq!(kind_of("BodyMatter"), Some("start"));
    }
}
