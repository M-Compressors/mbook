//! What the editor says a landmark gives a reader, and where a landmark
//! stands among the others. The names of the kinds are those of
//! [`crate::entities::book::landmark`], which the reader lists too.

use std::borrow::Cow;

use mbook::Landmark;
use rust_i18n::t;

use crate::entities::book::landmark::display_name;
pub(super) use crate::entities::book::landmark::{GROUPS, START, kind_name, start_name};
use crate::features::edit_book::outline::ChapterOutline;

/// What a reader gets from a marked start.
pub(super) fn start_effect() -> Cow<'static, str> {
    t!("prompt.landmark.start_effect")
}

/// What a reader gets while the start is not marked.
pub(super) fn no_start_effect() -> Cow<'static, str> {
    t!("prompt.landmark.no_start_effect")
}

/// The name of a kind, or, for a kind the editor does not know, the kind as
/// the book writes it.
pub(super) fn kind_words(id: &str) -> String {
    display_name(id, "")
}

/// The name a landmark is listed by: its title, or the name of its kind, or,
/// for a kind the editor does not know, the kind as the book writes it.
pub(super) fn landmark_name(landmark: &Landmark) -> String {
    display_name(&landmark.kind, &landmark.title)
}

/// What a landmark of a kind does for a reader, in one sentence.
pub(super) fn kind_effect(id: Option<&str>) -> String {
    match id {
        Some(START) => start_effect().into_owned(),
        Some(id) => t!("prompt.landmark.kind_effect", name = kind_words(id)).into_owned(),
        None => t!("prompt.landmark.any_kind_effect").into_owned(),
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
        START, insertion_index, kind_effect, kind_words, landmark_name, start_effect, start_name,
    };
    use crate::features::edit_book::outline::{ChapterOutline, PlaceOutline};

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
    fn a_landmark_is_listed_by_its_title_then_its_kind() {
        assert_eq!(
            landmark_name(&Landmark::new("notes", "Endnotes", "a.md")),
            "Endnotes"
        );
        assert_eq!(landmark_name(&Landmark::new("notes", "", "a.md")), "Notes");
        assert_eq!(
            landmark_name(&Landmark::new(START, "", "a.md")),
            start_name()
        );
        assert_eq!(
            landmark_name(&Landmark::new("x-recipes", "", "a.md")),
            "x-recipes"
        );
    }

    #[test]
    fn the_effect_of_a_kind_names_it_as_a_reader_sees_it() {
        assert_eq!(kind_effect(Some(START)), start_effect());
        assert!(kind_effect(Some("glossary")).contains("“Glossary”"));
        assert!(kind_effect(Some("x-recipes")).contains("“x-recipes”"));
        assert!(kind_effect(None).contains("by its kind"));
    }

    #[test]
    fn a_kind_is_named_in_words_or_as_the_book_writes_it() {
        assert_eq!(kind_words("glossary"), "Glossary");
        assert_eq!(kind_words(START), start_name());
        assert_eq!(kind_words("x-recipes"), "x-recipes");
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
