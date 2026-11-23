//! The kinds of landmark of RFC 0001 in the words of the application: what
//! the editor offers to mark and what the reader lists.

/// The kind of the place where the text of the book begins. A book has at
/// most one.
pub(crate) const START: &str = "start";
/// What the application calls the place where the text begins.
pub(crate) const START_NAME: &str = "Where the text begins";

/// A kind the application names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Kind {
    /// What the format calls it.
    pub(crate) id: &'static str,
    /// What a reader calls it.
    pub(crate) name: &'static str,
}

/// Kinds that are shown together, under words that say what they have in
/// common.
#[derive(Debug, Clone, Copy)]
pub(crate) struct KindGroup {
    /// The words over the group.
    pub(crate) heading: &'static str,
    /// The kinds, the one readers look for most often first.
    pub(crate) kinds: &'static [Kind],
}

const fn kind(id: &'static str, name: &'static str) -> Kind {
    Kind { id, name }
}

/// Every kind of RFC 0001 except `start`, grouped by what a reader looks for.
/// The places a reader goes to while reading come first.
pub(crate) const GROUPS: [KindGroup; 3] = [
    KindGroup {
        heading: "Looked up while reading",
        kinds: &[
            kind("notes", "Notes"),
            kind("glossary", "Glossary"),
            kind("index", "Index"),
            kind("bibliography", "Bibliography"),
            kind("appendix", "Appendix"),
            kind("list-of-figures", "List of figures"),
            kind("list-of-tables", "List of tables"),
        ],
    },
    KindGroup {
        heading: "Parts of the text",
        kinds: &[
            kind("foreword", "Foreword"),
            kind("preface", "Preface"),
            kind("introduction", "Introduction"),
            kind("prologue", "Prologue"),
            kind("epilogue", "Epilogue"),
            kind("conclusion", "Conclusion"),
            kind("afterword", "Afterword"),
        ],
    },
    KindGroup {
        heading: "Pages about the book",
        kinds: &[
            kind("title-page", "Title page"),
            kind("copyright", "Copyright page"),
            kind("dedication", "Dedication"),
            kind("epigraph", "Epigraph"),
            kind("acknowledgements", "Acknowledgements"),
            kind("about-the-author", "About the author"),
            kind("colophon", "Colophon"),
        ],
    },
];

/// What the application calls a kind; `None` for a kind it does not know,
/// such as one for private use.
pub(crate) fn kind_name(id: &str) -> Option<&'static str> {
    if id == START {
        return Some(START_NAME);
    }
    GROUPS
        .iter()
        .flat_map(|group| group.kinds)
        .find(|kind| kind.id == id)
        .map(|kind| kind.name)
}

/// The name a landmark is listed by: its title, or the name of its kind, or,
/// for a kind the application does not know, the kind as the book writes it.
pub(crate) fn display_name(kind: &str, title: &str) -> String {
    let title = title.trim();
    if !title.is_empty() {
        return title.to_owned();
    }
    kind_name(kind).map_or_else(|| kind.to_owned(), str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{GROUPS, Kind, START, START_NAME, display_name, kind, kind_name};

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
        assert_eq!(
            kind("notes", "Notes"),
            Kind {
                id: "notes",
                name: "Notes"
            }
        );
    }

    #[test]
    fn a_kind_the_application_does_not_know_has_no_name() {
        assert_eq!(kind_name("x-recipes"), None);
        assert_eq!(kind_name(""), None);
    }

    #[test]
    fn a_landmark_is_named_by_its_title_then_its_kind() {
        assert_eq!(display_name("notes", "Endnotes"), "Endnotes");
        assert_eq!(display_name("notes", "  "), "Notes");
        assert_eq!(display_name(START, ""), START_NAME);
        assert_eq!(display_name("x-recipes", ""), "x-recipes");
    }
}
