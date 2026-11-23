//! The dialog of an entry of the contents: its title and where it leads.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, Focusable as _, FontWeight, ParentElement as _,
    SharedString, Styled as _, Window, div,
};

use super::EditorPage;
use super::place_picker::{ChapterChoice, OnPlace, Place, place_picker, resolve};
use crate::features::edit_book::command::{ContentsChange, EntryDraft};
use crate::features::edit_book::outline::ChapterOutline;
use crate::shared::ui::status_line;

/// The label of the field that holds the title of an entry.
pub(super) const TITLE_LABEL: &str = "Title of the entry";
/// The words over the controls that choose where an entry leads.
pub(super) const PLACE_LABEL: &str = "Where the entry leads";
/// What the dialog says when the title is empty.
pub(super) const NO_TITLE: &str = "Enter the title of the entry.";
/// What the dialog says when the entry leads to nothing the book has.
pub(super) const NO_PLACE: &str =
    "Choose a chapter of the book, and its start or one of its headings.";

/// What the dialog starts from.
pub(super) struct EntryRequest {
    /// The position of the entry to change; `None` adds an entry.
    pub(super) index: Option<usize>,
    /// The title the field holds when the dialog opens.
    pub(super) title: String,
    /// Where the entry leads when the dialog opens.
    pub(super) place: Place,
    /// The chapters of the book and their headings.
    pub(super) choices: Rc<[ChapterChoice]>,
    /// The title each place suggests.
    pub(super) titles: Rc<HashMap<Place, String>>,
}

/// The title each place suggests for an entry that leads to it: the words of
/// a heading, and for the start of a chapter the words of its first heading.
pub(super) fn suggested_titles(chapters: &[ChapterOutline]) -> HashMap<Place, String> {
    let mut titles = HashMap::new();
    for chapter in chapters {
        if let Some(heading) = &chapter.heading {
            titles.insert(
                Place {
                    chapter: Some(chapter.path.clone()),
                    anchor: None,
                },
                heading.clone(),
            );
        }
        let headings = chapter
            .places
            .iter()
            .filter(|place| place.level.is_some() && !place.title.is_empty());
        for place in headings {
            titles.insert(
                Place {
                    chapter: Some(chapter.path.clone()),
                    anchor: Some(place.anchor.clone()),
                },
                place.title.clone(),
            );
        }
    }
    titles
}

/// The entry the fields give, or what is still to be entered.
///
/// # Errors
///
/// The words that say what is missing: a title, or a place the book has.
pub(super) fn entry_draft(
    title: &str,
    place: &Place,
    choices: &[ChapterChoice],
) -> Result<EntryDraft, &'static str> {
    let title = title.trim().replace(['\n', '\r', '\t'], " ");
    if title.is_empty() {
        return Err(NO_TITLE);
    }
    let (chapter, anchor) = resolve(place, choices).ok_or(NO_PLACE)?;
    Ok(EntryDraft {
        title,
        chapter,
        anchor,
    })
}

/// What the dialog does when a place is chosen: it writes the title the
/// place suggests while the field is empty or still holds a suggestion. A
/// place without words of its own empties such a field, so that the title of
/// another place is not kept by mistake.
fn title_filler(
    title: Entity<InputState>,
    titles: Rc<HashMap<Place, String>>,
    suggested: Rc<RefCell<String>>,
) -> OnPlace {
    Rc::new(move |place: &Place, window: &mut Window, cx: &mut App| {
        let current = title.read(cx).value().to_string();
        if current.trim().is_empty() || current == *suggested.borrow() {
            let words = titles.get(place).cloned().unwrap_or_default();
            title.update(cx, |field, cx| {
                field.set_value(words.as_str(), window, cx);
            });
            suggested.replace(words);
        }
    })
}

/// Asks for the title and the place of an entry, and sends the change. A
/// dialog that is dismissed changes nothing.
///
/// Choosing a place writes the title it suggests while the field is empty or
/// still holds a suggestion; words the author typed are kept.
pub(super) fn ask_entry(
    request: EntryRequest,
    window: &mut Window,
    cx: &mut Context<'_, EditorPage>,
) {
    let EntryRequest {
        index,
        title: initial,
        place,
        choices,
        titles,
    } = request;
    let page = cx.weak_entity();
    let title = cx.new(|cx| InputState::new(window, cx));
    title.update(cx, |field, cx| {
        field.set_value(initial.as_str(), window, cx);
    });
    let suggested = Rc::new(RefCell::new(
        titles.get(&place).cloned().unwrap_or_default(),
    ));
    let selected = cx.new(|_| place);
    let error = cx.new(|_| None::<SharedString>);
    let fill = title_filler(title.clone(), titles, suggested);
    let first = Cell::new(true);
    window.open_alert_dialog(cx, move |alert, window, cx| {
        if first.replace(false) {
            title.focus_handle(cx).focus(window, cx);
        }
        let danger = cx.theme().danger;
        let label = |words: &'static str| div().font_weight(FontWeight::MEDIUM).child(words);
        let form = v_flex()
            .gap_3()
            .child(
                v_flex()
                    .gap_1()
                    .child(label(TITLE_LABEL))
                    .child(Input::new(&title).aria_label(TITLE_LABEL)),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(label(PLACE_LABEL))
                    .child(place_picker(
                        "entry",
                        &selected,
                        &choices,
                        Some(fill.clone()),
                        cx,
                    )),
            )
            .children(
                error
                    .read(cx)
                    .clone()
                    .map(|message| status_line("entry-error", message).text_color(danger)),
            );
        let (page, title, selected) = (page.clone(), title.clone(), selected.clone());
        let (error, choices) = (error.clone(), choices.clone());
        alert
            .title(if index.is_some() {
                "Edit the entry"
            } else {
                "Add an entry to the contents"
            })
            .description(form)
            .confirm()
            .ok_text(if index.is_some() {
                "Apply"
            } else {
                "Add the entry"
            })
            .cancel_text("Cancel")
            .on_ok(move |_, window, cx| {
                let draft = entry_draft(&title.read(cx).value(), selected.read(cx), &choices);
                match draft {
                    Ok(entry) => {
                        let change = match index {
                            Some(index) => ContentsChange::Set { index, entry },
                            None => ContentsChange::Add(entry),
                        };
                        page.update(cx, |this, cx| this.edit_contents(change, window, cx))
                            .ok();
                        true
                    }
                    Err(words) => {
                        error.update(cx, |error, cx| {
                            *error = Some(words.into());
                            cx.notify();
                        });
                        window.refresh();
                        false
                    }
                }
            })
    });
}

#[cfg(test)]
mod tests {
    use super::{NO_PLACE, NO_TITLE, entry_draft, suggested_titles};
    use crate::features::edit_book::command::EntryDraft;
    use crate::features::edit_book::outline::{ChapterOutline, PlaceOutline};
    use crate::pages::editor::place_picker::{Place, heading_choices};

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
                    place("bare", "", Some(2)),
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

    fn at(chapter: &str, anchor: Option<&str>) -> Place {
        Place {
            chapter: Some(chapter.to_owned()),
            anchor: anchor.map(str::to_owned),
        }
    }

    #[test]
    fn a_heading_suggests_its_words_and_a_chapter_those_of_its_first_heading() {
        let titles = suggested_titles(&chapters());
        assert_eq!(
            titles.get(&at("a.md", None)).map(String::as_str),
            Some("One")
        );
        assert_eq!(
            titles.get(&at("a.md", Some("one"))).map(String::as_str),
            Some("One")
        );
    }

    #[test]
    fn a_place_without_words_suggests_no_title() {
        let titles = suggested_titles(&chapters());
        assert_eq!(titles.get(&at("a.md", Some("bare"))), None);
        assert_eq!(titles.get(&at("a.md", Some("fig-1"))), None);
        assert_eq!(titles.get(&at("b.md", None)), None);
        assert_eq!(titles.len(), 2);
    }

    #[test]
    fn the_fields_give_an_entry_with_its_title_on_one_line() {
        let choices = heading_choices(&chapters());
        assert_eq!(
            entry_draft(" The\nfirst ", &at("a.md", Some("one")), &choices),
            Ok(EntryDraft {
                title: "The first".to_owned(),
                chapter: "a.md".to_owned(),
                anchor: Some("one".to_owned()),
            })
        );
        assert_eq!(
            entry_draft("Two", &at("b.md", None), &choices).map(|entry| entry.anchor),
            Ok(None)
        );
    }

    #[test]
    fn an_empty_title_is_asked_for() {
        let choices = heading_choices(&chapters());
        assert_eq!(
            entry_draft("  ", &at("a.md", None), &choices),
            Err(NO_TITLE)
        );
    }

    #[test]
    fn a_place_the_book_does_not_have_is_asked_for_again() {
        let choices = heading_choices(&chapters());
        for place in [
            Place::default(),
            at("gone.md", None),
            at("a.md", Some("gone")),
            at("a.md", Some("fig-1")),
        ] {
            assert_eq!(entry_draft("Lost", &place, &choices), Err(NO_PLACE));
        }
    }
}
