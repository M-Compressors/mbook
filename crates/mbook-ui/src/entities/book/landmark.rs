//! The kinds of landmark of RFC 0001 in the words of the application: what
//! the editor offers to mark and what the reader lists.

use std::borrow::Cow;

use rust_i18n::t;

/// The kind of the place where the text of the book begins. A book has at
/// most one.
pub(crate) const START: &str = "start";

/// What the application calls the place where the text begins, in the
/// current language.
pub(crate) fn start_name() -> Cow<'static, str> {
    t!("landmark.start")
}

/// A kind the application names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Kind {
    /// What the format calls it.
    pub(crate) id: &'static str,
}

impl Kind {
    /// What a reader calls it, in the current language.
    pub(crate) fn name(self) -> Cow<'static, str> {
        kind_name(self.id).unwrap_or(Cow::Borrowed(self.id))
    }
}

/// What the kinds of a group have in common.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Common {
    /// Places a reader goes to while reading.
    LookedUp,
    /// Parts of the text itself.
    Text,
    /// Pages about the book rather than of its text.
    AboutBook,
}

/// Kinds that are shown together, under words that say what they have in
/// common.
#[derive(Debug, Clone, Copy)]
pub(crate) struct KindGroup {
    /// What the kinds have in common.
    common: Common,
    /// The kinds, the one readers look for most often first.
    pub(crate) kinds: &'static [Kind],
}

impl KindGroup {
    /// The words over the group, in the current language.
    pub(crate) fn heading(&self) -> Cow<'static, str> {
        match self.common {
            Common::LookedUp => t!("landmark.group.looked_up"),
            Common::Text => t!("landmark.group.text"),
            Common::AboutBook => t!("landmark.group.about_book"),
        }
    }
}

const fn kind(id: &'static str) -> Kind {
    Kind { id }
}

/// Every kind of RFC 0001 except `start`, grouped by what a reader looks for.
/// The places a reader goes to while reading come first.
pub(crate) const GROUPS: [KindGroup; 3] = [
    KindGroup {
        common: Common::LookedUp,
        kinds: &[
            kind("notes"),
            kind("glossary"),
            kind("index"),
            kind("bibliography"),
            kind("appendix"),
            kind("list-of-figures"),
            kind("list-of-tables"),
        ],
    },
    KindGroup {
        common: Common::Text,
        kinds: &[
            kind("foreword"),
            kind("preface"),
            kind("introduction"),
            kind("prologue"),
            kind("epilogue"),
            kind("conclusion"),
            kind("afterword"),
        ],
    },
    KindGroup {
        common: Common::AboutBook,
        kinds: &[
            kind("title-page"),
            kind("copyright"),
            kind("dedication"),
            kind("epigraph"),
            kind("acknowledgements"),
            kind("about-the-author"),
            kind("colophon"),
        ],
    },
];

/// What the application calls a kind, in the current language; `None` for a
/// kind it does not know, such as one for private use.
pub(crate) fn kind_name(id: &str) -> Option<Cow<'static, str>> {
    let name = match id {
        START => start_name(),
        "notes" => t!("landmark.notes"),
        "glossary" => t!("landmark.glossary"),
        "index" => t!("landmark.index"),
        "bibliography" => t!("landmark.bibliography"),
        "appendix" => t!("landmark.appendix"),
        "list-of-figures" => t!("landmark.list_of_figures"),
        "list-of-tables" => t!("landmark.list_of_tables"),
        "foreword" => t!("landmark.foreword"),
        "preface" => t!("landmark.preface"),
        "introduction" => t!("landmark.introduction"),
        "prologue" => t!("landmark.prologue"),
        "epilogue" => t!("landmark.epilogue"),
        "conclusion" => t!("landmark.conclusion"),
        "afterword" => t!("landmark.afterword"),
        "title-page" => t!("landmark.title_page"),
        "copyright" => t!("landmark.copyright"),
        "dedication" => t!("landmark.dedication"),
        "epigraph" => t!("landmark.epigraph"),
        "acknowledgements" => t!("landmark.acknowledgements"),
        "about-the-author" => t!("landmark.about_the_author"),
        "colophon" => t!("landmark.colophon"),
        _ => return None,
    };
    Some(name)
}

/// The name a landmark is listed by: its title, or the name of its kind, or,
/// for a kind the application does not know, the kind as the book writes it.
pub(crate) fn display_name(kind: &str, title: &str) -> String {
    let title = title.trim();
    if !title.is_empty() {
        return title.to_owned();
    }
    kind_name(kind).map_or_else(|| kind.to_owned(), Cow::into_owned)
}

#[cfg(test)]
mod tests {
    use super::{GROUPS, KindGroup, START, display_name, kind, kind_name, start_name};

    /// The kinds of the table in RFC 0001, "Landmarks".
    const RFC_KINDS: [&str; 22] = [
        "start",
        "title-page",
        "dedication",
        "epigraph",
        "foreword",
        "preface",
        "introduction",
        "prologue",
        "epilogue",
        "conclusion",
        "afterword",
        "acknowledgements",
        "appendix",
        "notes",
        "glossary",
        "bibliography",
        "index",
        "list-of-figures",
        "list-of-tables",
        "about-the-author",
        "copyright",
        "colophon",
    ];

    #[test]
    fn the_application_names_every_kind_of_the_format_once() {
        for id in RFC_KINDS {
            assert!(kind_name(id).is_some(), "{id}");
        }
        let offered: Vec<&str> = GROUPS
            .iter()
            .flat_map(|group| group.kinds)
            .map(|kind| kind.id)
            .collect();
        assert_eq!(offered.len(), RFC_KINDS.len() - 1);
        assert!(!offered.contains(&START));
        let mut unique = offered.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), offered.len());
    }

    #[test]
    fn a_kind_pairs_the_word_of_the_format_with_that_of_a_reader() {
        assert_eq!(kind("notes").id, "notes");
        assert_eq!(kind("notes").name(), "Notes");
    }

    #[test]
    fn every_group_has_a_heading() {
        let headings: Vec<_> = GROUPS.iter().map(KindGroup::heading).collect();
        assert_eq!(
            headings,
            [
                "Looked up while reading",
                "Parts of the text",
                "Pages about the book"
            ]
        );
    }

    #[test]
    fn a_kind_the_application_does_not_know_has_no_name() {
        assert_eq!(kind_name("x-recipes"), None);
        assert_eq!(kind_name(""), None);
        assert_eq!(kind("x-recipes").name(), "x-recipes");
    }

    #[test]
    fn a_landmark_is_named_by_its_title_then_its_kind() {
        assert_eq!(display_name("notes", "Endnotes"), "Endnotes");
        assert_eq!(display_name("notes", "  "), "Notes");
        assert_eq!(display_name(START, ""), start_name());
        assert_eq!(display_name("x-recipes", ""), "x-recipes");
    }
}
