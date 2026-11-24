//! Choosing a place of the book in a dialog: a chapter by its number and
//! title, and optionally a heading or a named line inside it.

use std::borrow::Cow;
use std::rc::Rc;

use gpui_kit::component::Disableable as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::h_flex;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::{App, Entity, IntoElement, ParentElement as _, SharedString, Styled as _, px};
use rust_i18n::t;

use super::session::chapter_choice;
use crate::features::edit_book::outline::{ChapterOutline, PlaceOutline};
use crate::shared::ui::target_size;

/// What the second menu calls the first line of a chapter.
pub(super) fn start_of_chapter() -> Cow<'static, str> {
    t!("prompt.place.chapter_start")
}

/// The tallest a menu of chapters or places grows before it scrolls.
const MENU_HEIGHT: f32 = 320.0;

/// A place of the book: a chapter, and a named place inside it or its start.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub(super) struct Place {
    /// The path of the chapter; `None` until one is chosen.
    pub(super) chapter: Option<String>,
    /// The name of the place inside the chapter; `None` is its start.
    pub(super) anchor: Option<String>,
}

/// A chapter the first menu offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ChapterChoice {
    /// The path of the chapter.
    pub(super) path: String,
    /// Its number and title.
    pub(super) label: String,
    /// The places inside it: the name and the words of the menu.
    pub(super) places: Vec<(String, String)>,
}

/// The chapters of a book as the menus offer them, with every place a link
/// or a landmark can name.
pub(super) fn chapter_choices(chapters: &[ChapterOutline]) -> Rc<[ChapterChoice]> {
    choices(chapters, |_| true)
}

/// The chapters of a book with their headings only: the places an entry of
/// the contents can lead to (RFC 0001, "Resource 3: `toc`").
pub(super) fn heading_choices(chapters: &[ChapterOutline]) -> Rc<[ChapterChoice]> {
    choices(chapters, |place| place.level.is_some())
}

/// The chapters of a book with the places that `offered` keeps.
fn choices(
    chapters: &[ChapterOutline],
    offered: impl Fn(&PlaceOutline) -> bool,
) -> Rc<[ChapterChoice]> {
    chapters
        .iter()
        .enumerate()
        .map(|(index, chapter)| ChapterChoice {
            path: chapter.path.clone(),
            label: chapter_choice(index + 1, chapter.heading.as_deref()),
            places: chapter
                .places
                .iter()
                .filter(|place| offered(place))
                .map(|place| (place.anchor.clone(), place_label(place)))
                .collect(),
        })
        .collect()
}

/// A place inside a chapter in words: a heading by its title, set in by its
/// level, and a named line by its name.
pub(super) fn place_label(place: &PlaceOutline) -> String {
    match place.level {
        Some(level) if !place.title.is_empty() => {
            let indent = "\u{2003}".repeat(usize::from(level.saturating_sub(1)));
            format!("{indent}{}", place.title)
        }
        _ => format!("#{}", place.anchor),
    }
}

/// The words of the control that chooses the chapter.
pub(super) fn chapter_words(place: &Place, choices: &[ChapterChoice]) -> String {
    match &place.chapter {
        None => t!("prompt.place.choose_chapter").into_owned(),
        Some(path) => choices
            .iter()
            .find(|choice| &choice.path == path)
            .map_or_else(
                || t!("prompt.place.missing_chapter", path = path).into_owned(),
                |choice| choice.label.clone(),
            ),
    }
}

/// The words of the control that chooses the place inside the chapter.
pub(super) fn place_words(place: &Place, choices: &[ChapterChoice]) -> String {
    let Some(anchor) = &place.anchor else {
        return start_of_chapter().into_owned();
    };
    choices
        .iter()
        .find(|choice| Some(&choice.path) == place.chapter.as_ref())
        .and_then(|choice| choice.places.iter().find(|(name, _)| name == anchor))
        .map_or_else(
            || t!("prompt.place.missing_place", anchor = anchor).into_owned(),
            |(_, label)| label.trim_start().to_owned(),
        )
}

/// The chapter path and anchor of a place among `choices`; `None` when the
/// book has no such chapter, or the chapter has no such place.
pub(super) fn resolve(
    place: &Place,
    choices: &[ChapterChoice],
) -> Option<(String, Option<String>)> {
    let chapter = choices
        .iter()
        .find(|choice| Some(&choice.path) == place.chapter.as_ref())?;
    let known = place
        .anchor
        .as_ref()
        .is_none_or(|anchor| chapter.places.iter().any(|(name, _)| name == anchor));
    known.then(|| (chapter.path.clone(), place.anchor.clone()))
}

/// What a dialog does when another place is chosen.
pub(super) type OnPlace = Rc<dyn Fn(&Place, &mut gpui_kit::Window, &mut App)>;

/// The two controls: the chapter, and the place inside it.
///
/// `id` tells the controls of one dialog from those of another. `changed` is
/// called after the choice is stored in `selected`.
pub(super) fn place_picker(
    id: &'static str,
    selected: &Entity<Place>,
    choices: &Rc<[ChapterChoice]>,
    changed: Option<OnPlace>,
    cx: &App,
) -> impl IntoElement + use<> {
    let place = selected.read(cx).clone();
    let chapter_label = chapter_words(&place, choices);
    let inner_label = place_words(&place, choices);
    let places: Rc<[(String, String)]> = choices
        .iter()
        .find(|choice| Some(&choice.path) == place.chapter.as_ref())
        .map_or_default(|choice| choice.places.clone())
        .into();
    let choose = {
        let selected = selected.clone();
        move |next: Place, window: &mut gpui_kit::Window, cx: &mut App| {
            selected.update(cx, |place, cx| {
                *place = next.clone();
                cx.notify();
            });
            if let Some(changed) = &changed {
                changed(&next, window, cx);
            }
            window.refresh();
        }
    };
    let choose = Rc::new(choose);
    let chapter_menu = {
        let (choices, choose) = (choices.clone(), choose.clone());
        let current = place.chapter.clone();
        Button::new(SharedString::from(format!("{id}-chapter")))
            .label(chapter_label.clone())
            .accessibility_label(t!("prompt.place.chapter_spoken", label = chapter_label))
            .min_h(px(target_size()))
            .dropdown_caret(true)
            .dropdown_menu(move |menu, _, _| {
                choices.iter().fold(
                    menu.scrollable(true).max_h(px(MENU_HEIGHT)),
                    |menu, choice| {
                        let choose = choose.clone();
                        let path = choice.path.clone();
                        menu.item(
                            PopupMenuItem::new(choice.label.clone())
                                .checked(current.as_ref() == Some(&choice.path))
                                .on_click(move |_, window, cx| {
                                    choose(
                                        Place {
                                            chapter: Some(path.clone()),
                                            anchor: None,
                                        },
                                        window,
                                        cx,
                                    );
                                }),
                        )
                    },
                )
            })
    };
    let place_menu = {
        let chapter = place.chapter.clone();
        let current = place.anchor.clone();
        let has_chapter = chapter.is_some();
        Button::new(SharedString::from(format!("{id}-place")))
            .label(inner_label.clone())
            .accessibility_label(t!("prompt.place.place_spoken", label = inner_label))
            .min_h(px(target_size()))
            .dropdown_caret(true)
            .disabled(!has_chapter)
            .dropdown_menu(move |menu, _, _| {
                let item = |label: String, anchor: Option<String>| {
                    let (choose, chapter) = (choose.clone(), chapter.clone());
                    PopupMenuItem::new(label)
                        .checked(current == anchor)
                        .on_click(move |_, window, cx| {
                            choose(
                                Place {
                                    chapter: chapter.clone(),
                                    anchor: anchor.clone(),
                                },
                                window,
                                cx,
                            );
                        })
                };
                places.iter().fold(
                    menu.scrollable(true)
                        .max_h(px(MENU_HEIGHT))
                        .item(item(start_of_chapter().into_owned(), None)),
                    |menu, (anchor, label)| menu.item(item(label.clone(), Some(anchor.clone()))),
                )
            })
    };
    h_flex()
        .gap_2()
        .flex_wrap()
        .child(chapter_menu)
        .child(place_menu)
}

#[cfg(test)]
mod tests {
    use super::{
        Place, chapter_choices, chapter_words, heading_choices, place_label, place_words,
        start_of_chapter,
    };
    use crate::features::edit_book::outline::{ChapterOutline, PlaceOutline};

    fn place(anchor: &str, title: &str, level: Option<u8>) -> PlaceOutline {
        PlaceOutline {
            anchor: anchor.to_owned(),
            title: title.to_owned(),
            level,
        }
    }

    fn chapters() -> Vec<ChapterOutline> {
        vec![
            ChapterOutline {
                path: "a.md".to_owned(),
                len: 1,
                heading: Some("One".to_owned()),
                places: vec![
                    place("one", "One", Some(1)),
                    place("deep", "Deep", Some(3)),
                    place("fig-1", "", None),
                ],
            },
            ChapterOutline {
                path: "b.md".to_owned(),
                len: 1,
                heading: None,
                places: Vec::new(),
            },
        ]
    }

    #[test]
    fn a_heading_is_offered_by_its_title_and_a_named_line_by_its_name() {
        assert_eq!(place_label(&place("one", "One", Some(1))), "One");
        assert_eq!(
            place_label(&place("deep", "Deep", Some(3))),
            "\u{2003}\u{2003}Deep"
        );
        assert_eq!(place_label(&place("fig-1", "", None)), "#fig-1");
        assert_eq!(place_label(&place("x", "", Some(2))), "#x");
    }

    #[test]
    fn chapters_are_offered_by_number_and_title_with_their_places() {
        let choices = chapter_choices(&chapters());
        assert_eq!(choices[0].label, "1. One");
        assert_eq!(choices[1].label, "2. Untitled chapter");
        assert_eq!(choices[0].places.len(), 3);
        assert_eq!(
            choices[0].places[2],
            ("fig-1".to_owned(), "#fig-1".to_owned())
        );
    }

    #[test]
    fn the_contents_are_offered_headings_and_no_named_lines() {
        let choices = heading_choices(&chapters());
        let names: Vec<&str> = choices[0]
            .places
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(names, ["one", "deep"]);
        assert_eq!(choices[1].label, "2. Untitled chapter");
    }

    #[test]
    fn the_controls_say_what_is_chosen_and_what_is_missing() {
        let choices = chapter_choices(&chapters());
        let nothing = Place::default();
        assert_eq!(chapter_words(&nothing, &choices), "Choose a chapter");
        assert_eq!(place_words(&nothing, &choices), start_of_chapter());
        let deep = Place {
            chapter: Some("a.md".to_owned()),
            anchor: Some("deep".to_owned()),
        };
        assert_eq!(chapter_words(&deep, &choices), "1. One");
        assert_eq!(place_words(&deep, &choices), "Deep");
        let gone = Place {
            chapter: Some("gone.md".to_owned()),
            anchor: Some("lost".to_owned()),
        };
        assert_eq!(chapter_words(&gone, &choices), "gone.md (missing)");
        assert_eq!(place_words(&gone, &choices), "#lost (missing)");
    }
}
