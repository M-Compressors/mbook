//! The kinds of landmark of RFC 0001 in the words of the editor, and where a
//! landmark stands among the others.

use mbook::Landmark;

use crate::features::edit_book::outline::ChapterOutline;

/// The kind of the place where the text of the book begins. A book has at
/// most one.
pub(super) const START: &str = "start";
/// What the editor calls the place where the text begins.
pub(super) const START_NAME: &str = "Where the text begins";
/// What a reader gets from a marked start.
pub(super) const START_EFFECT: &str =
    "A new book opens here instead of at its cover or title page.";
/// What a reader gets while the start is not marked.
pub(super) const NO_START_EFFECT: &str =
    "Not marked yet. A new book opens at its first page, which is often the title page.";

/// A kind the editor offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Kind {
    /// What the format calls it.
    pub(super) id: &'static str,
    /// What a reader calls it.
    pub(super) name: &'static str,
}

/// Kinds that the menu shows together, under words that say what they have
/// in common.
#[derive(Debug, Clone, Copy)]
pub(super) struct KindGroup {
    /// The words over the group.
    pub(super) heading: &'static str,
    /// The kinds, the one readers look for most often first.
    pub(super) kinds: &'static [Kind],
}

const fn kind(id: &'static str, name: &'static str) -> Kind {
    Kind { id, name }
}

/// Every kind of RFC 0001 except `start`, grouped by what a reader looks for.
/// The places a reader goes to while reading come first.
pub(super) const GROUPS: [KindGroup; 3] = [
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

/// What the editor calls a kind; `None` for a kind it does not know, such as
/// one for private use.
pub(super) fn kind_name(id: &str) -> Option<&'static str> {
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
/// for a kind the editor does not know, the kind as the book writes it.
pub(super) fn landmark_name(landmark: &Landmark) -> String {
    if !landmark.title.is_empty() {
        return landmark.title.clone();
    }
    kind_name(&landmark.kind).map_or_else(|| landmark.kind.clone(), str::to_owned)
}

/// What a landmark of a kind does for a reader, in one sentence.
pub(super) fn kind_effect(id: Option<&str>) -> String {
    match id {
        Some(START) => START_EFFECT.to_owned(),
        Some(id) => {
            let name = kind_name(id).unwrap_or(id);
            format!(
                "A reading application can list this place as “{name}” and take a reader straight to it."
            )
        }
        None => {
            "A reading application lists the place by its kind and takes a reader straight to it."
                .to_owned()
        }
    }
}

/// Where a place stands in the book: the number of its chapter in reading
/// order, then 0 for the start of the chapter or one more than the number of
/// the named place inside it. `None` for a place the book does not have.
fn reading_position(
    chapters: &[ChapterOutline],
    chapter: &str,
    anchor: Option<&str>,
) -> Option<(usize, usize)> {
    let (number, outline) = chapters
        .iter()
        .enumerate()
        .find(|(_, outline)| outline.path == chapter)?;
    let inner = match anchor {
        None => 0,
        Some(anchor) => {
            outline
                .places
                .iter()
                .position(|place| place.anchor == anchor)?
                + 1
        }
    };
    Some((number, inner))
}

/// Where a new landmark goes in the list: after the last one that leads to
/// the same place or an earlier one, so that a list in the order of the book
/// stays so. Landmarks that lead nowhere are passed over.
pub(super) fn insertion_index(
    landmarks: &[Landmark],
    chapters: &[ChapterOutline],
    new: &Landmark,
) -> usize {
    let Some(at) = reading_position(chapters, &new.chapter, new.anchor.as_deref()) else {
        return landmarks.len();
    };
    landmarks
        .iter()
        .rposition(|landmark| {
            reading_position(chapters, &landmark.chapter, landmark.anchor.as_deref())
                .is_some_and(|other| other <= at)
        })
        .map_or(0, |index| index + 1)
}

#[cfg(test)]
mod tests {
    use mbook::Landmark;

    use super::{
        GROUPS, Kind, START, START_EFFECT, START_NAME, insertion_index, kind, kind_effect,
        kind_name, landmark_name,
    };
    use crate::features::edit_book::outline::{ChapterOutline, PlaceOutline};

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

    fn chapters() -> Vec<ChapterOutline> {
        let chapter = |path: &str, anchors: &[&str]| ChapterOutline {
            path: path.to_owned(),
            len: 1,
            heading: None,
            places: anchors
                .iter()
                .map(|anchor| PlaceOutline {
                    anchor: (*anchor).to_owned(),
                    title: String::new(),
                    level: Some(2),
                })
                .collect(),
        };
        vec![
            chapter("a.md", &["one", "two"]),
            chapter("b.md", &[]),
            chapter("c.md", &["three"]),
        ]
    }

    #[test]
    fn the_editor_names_every_kind_of_the_format_once() {
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
    fn a_kind_the_editor_does_not_know_has_no_name() {
        assert_eq!(kind_name("x-recipes"), None);
        assert_eq!(kind_name(""), None);
    }

    #[test]
    fn a_landmark_is_listed_by_its_title_then_its_kind() {
        assert_eq!(
            landmark_name(&Landmark::new("notes", "Endnotes", "a.md")),
            "Endnotes"
        );
        assert_eq!(landmark_name(&Landmark::new("notes", "", "a.md")), "Notes");
        assert_eq!(landmark_name(&Landmark::new(START, "", "a.md")), START_NAME);
        assert_eq!(
            landmark_name(&Landmark::new("x-recipes", "", "a.md")),
            "x-recipes"
        );
    }

    #[test]
    fn the_effect_of_a_kind_names_it_as_a_reader_sees_it() {
        assert_eq!(kind_effect(Some(START)), START_EFFECT);
        assert!(kind_effect(Some("glossary")).contains("“Glossary”"));
        assert!(kind_effect(Some("x-recipes")).contains("“x-recipes”"));
        assert!(kind_effect(None).contains("by its kind"));
    }

    #[test]
    fn a_new_landmark_goes_after_those_at_or_before_its_place() {
        let chapters = chapters();
        let list = [
            Landmark::new(START, "", "a.md"),
            Landmark::new("glossary", "", "b.md"),
            Landmark::new("index", "", "c.md#three"),
        ];
        let at = |href: &str| insertion_index(&list, &chapters, &Landmark::new("notes", "", href));
        assert_eq!(at("a.md"), 1);
        assert_eq!(at("a.md#two"), 1);
        assert_eq!(at("b.md"), 2);
        assert_eq!(at("c.md"), 2);
        assert_eq!(at("c.md#three"), 3);
    }

    #[test]
    fn a_new_landmark_before_every_other_goes_first() {
        let chapters = chapters();
        let list = [Landmark::new("index", "", "c.md")];
        let new = Landmark::new("foreword", "", "a.md#one");
        assert_eq!(insertion_index(&list, &chapters, &new), 0);
        assert_eq!(insertion_index(&[], &chapters, &new), 0);
    }

    #[test]
    fn landmarks_that_lead_nowhere_are_passed_over() {
        let chapters = chapters();
        let list = [
            Landmark::new("glossary", "", "a.md"),
            Landmark::new("index", "", "gone.md"),
            Landmark::new("appendix", "", "a.md#gone"),
        ];
        let new = Landmark::new("notes", "", "c.md");
        assert_eq!(insertion_index(&list, &chapters, &new), 1);
    }

    #[test]
    fn a_new_landmark_that_leads_nowhere_goes_last() {
        let chapters = chapters();
        let list = [Landmark::new("glossary", "", "a.md")];
        let new = Landmark::new("notes", "", "gone.md");
        assert_eq!(insertion_index(&list, &chapters, &new), 1);
    }
}
