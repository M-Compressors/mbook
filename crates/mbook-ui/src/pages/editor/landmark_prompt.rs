//! The dialog of a landmark: what the place is, the name a reader is shown
//! and where it is.

use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, Focusable as _, FontWeight, IntoElement, ParentElement as _,
    SharedString, Styled as _, Window, div, px,
};
use mbook::Landmark;
use rust_i18n::t;

use super::EditorPage;
use super::landmark_kinds::{GROUPS, START, kind_effect, kind_name, kind_words};
use super::place_picker::{ChapterChoice, Place, place_picker, resolve};
use crate::shared::ui::{status_line, target_size};

/// The words over the control that chooses the kind of a landmark.
pub(super) fn kind_label() -> Cow<'static, str> {
    t!("prompt.landmark.kind_label")
}

/// The label of the field that holds the title of a landmark.
pub(super) fn title_label() -> Cow<'static, str> {
    t!("prompt.landmark.title_label")
}

/// The words over the controls that choose where a landmark leads.
pub(super) fn place_label() -> Cow<'static, str> {
    t!("prompt.landmark.place_label")
}

/// What the dialog says when no kind is chosen.
pub(super) fn no_kind() -> Cow<'static, str> {
    t!("prompt.landmark.no_kind")
}

/// What the dialog says when the landmark leads to nothing the book has.
pub(super) fn no_place() -> Cow<'static, str> {
    t!("prompt.landmark.no_place")
}
/// The tallest the menu of kinds grows before it scrolls.
const MENU_HEIGHT: f32 = 360.0;

/// What the dialog starts from.
pub(super) struct LandmarkRequest {
    /// The landmark to change and its position; `None` adds one.
    pub(super) editing: Option<(usize, Landmark)>,
    /// The kind when the dialog opens. `start` is fixed: its dialog offers
    /// no other kind and no title.
    pub(super) kind: Option<String>,
    /// Where the landmark leads when the dialog opens.
    pub(super) place: Place,
    /// The chapters of the book and the places in them.
    pub(super) choices: Rc<[ChapterChoice]>,
}

/// The landmark the fields give, or what is still to be chosen.
///
/// The title is put on one line; a title that only repeats the name of the
/// kind is left empty, so that a reading application names it in its own
/// words.
///
/// # Errors
///
/// The words that say what is missing: a kind, or a place the book has.
pub(super) fn landmark_draft(
    kind: Option<&str>,
    title: &str,
    place: &Place,
    choices: &[ChapterChoice],
) -> Result<Landmark, Cow<'static, str>> {
    let kind = kind.filter(|kind| !kind.is_empty()).ok_or_else(no_kind)?;
    let (chapter, anchor) = resolve(place, choices).ok_or_else(no_place)?;
    let mut title = title.trim().replace(['\n', '\r', '\t'], " ");
    if kind_name(kind).as_deref() == Some(title.as_str()) {
        title.clear();
    }
    Ok(Landmark {
        kind: kind.to_owned(),
        title,
        chapter,
        anchor,
    })
}

/// The control that chooses the kind, with the kinds grouped by what a
/// reader looks for. Choosing one writes its name as the placeholder of the
/// title, which is what a reader is shown while the title is empty.
fn kind_picker(
    kind: &Entity<Option<String>>,
    title: &Entity<InputState>,
    cx: &gpui_kit::App,
) -> impl IntoElement + use<> {
    let current = kind.read(cx).clone();
    let label = current.as_deref().map_or_else(
        || t!("prompt.landmark.choose_kind").into_owned(),
        kind_words,
    );
    let (kind, title) = (kind.clone(), title.clone());
    Button::new("landmark-kind")
        .label(label.clone())
        .accessibility_label(format!("{}, {label}", kind_label()))
        .min_h(px(target_size()))
        .self_start()
        .dropdown_caret(true)
        .dropdown_menu(move |menu, _, _| {
            let mut menu = menu.scrollable(true).max_h(px(MENU_HEIGHT));
            for (number, group) in GROUPS.iter().enumerate() {
                if number > 0 {
                    menu = menu.separator();
                }
                menu = menu.label(group.heading());
                for item in group.kinds {
                    let (kind, title) = (kind.clone(), title.clone());
                    let id = item.id;
                    menu = menu.item(
                        PopupMenuItem::new(item.name())
                            .checked(current.as_deref() == Some(id))
                            .on_click(move |_, window, cx| {
                                kind.update(cx, |kind, cx| {
                                    *kind = Some(id.to_owned());
                                    cx.notify();
                                });
                                title.update(cx, |field, cx| {
                                    field.set_placeholder(kind_words(id), window, cx);
                                });
                                window.refresh();
                            }),
                    );
                }
            }
            menu
        })
}

/// The state of the controls of an open dialog.
#[derive(Clone)]
struct Fields {
    /// The chosen kind; `None` until one is chosen.
    kind: Entity<Option<String>>,
    /// The title a reader is shown.
    title: Entity<InputState>,
    /// Where the landmark leads.
    place: Entity<Place>,
    /// What is still to be chosen, after a refused confirmation.
    error: Entity<Option<SharedString>>,
}

/// The controls of the dialog. The start has neither a kind to choose nor a
/// title: its dialog says what it gives a reader and asks where it is.
fn landmark_form(
    fields: &Fields,
    is_start: bool,
    choices: &Rc<[ChapterChoice]>,
    cx: &gpui_kit::App,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let label = |words: Cow<'static, str>| {
        div()
            .font_weight(FontWeight::MEDIUM)
            .child(SharedString::from(words))
    };
    let effect = kind_effect(fields.kind.read(cx).as_deref());
    let form = if is_start {
        v_flex()
            .gap_4()
            .child(status_line("landmark-effect", effect).text_color(theme.muted_foreground))
    } else {
        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_1()
                    .child(label(kind_label()))
                    .child(kind_picker(&fields.kind, &fields.title, cx))
                    .child(
                        status_line("landmark-effect", effect)
                            .text_sm()
                            .text_color(theme.muted_foreground),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(label(title_label()))
                    .child(Input::new(&fields.title).aria_label(title_label())),
            )
    };
    form.child(
        v_flex()
            .gap_1()
            .child(label(place_label()))
            .child(place_picker("landmark", &fields.place, choices, None, cx)),
    )
    .children(
        fields
            .error
            .read(cx)
            .clone()
            .map(|message| status_line("landmark-error", message).text_color(theme.danger)),
    )
}

/// Asks for the kind, the title and the place of a landmark and hands the
/// landmark to `EditorPage::put_landmark`. A dialog that is dismissed
/// changes nothing.
pub(super) fn ask_landmark(
    request: LandmarkRequest,
    window: &mut Window,
    cx: &mut Context<'_, EditorPage>,
) {
    let LandmarkRequest {
        editing,
        kind,
        place,
        choices,
    } = request;
    let is_start = kind.as_deref() == Some(START);
    let page = cx.weak_entity();
    let placeholder = kind.as_deref().map_or_else(String::new, kind_words);
    let title = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
    // The start keeps the title it came with: its dialog has no field for
    // one, so the field holds it unseen.
    if let Some((_, landmark)) = &editing {
        title.update(cx, |field, cx| {
            field.set_value(landmark.title.as_str(), window, cx);
        });
    }
    let fields = Fields {
        kind: cx.new(|_| kind),
        title,
        place: cx.new(|_| place),
        error: cx.new(|_| None),
    };
    let first = Cell::new(!is_start);
    window.open_alert_dialog(cx, move |alert, window, cx| {
        if first.replace(false) {
            fields.title.focus_handle(cx).focus(window, cx);
        }
        let form = landmark_form(&fields, is_start, &choices, cx);
        let editing_now = editing.is_some();
        let (page, fields, choices, editing) = (
            page.clone(),
            fields.clone(),
            choices.clone(),
            editing.clone(),
        );
        alert
            .title(match (is_start, editing_now) {
                (true, false) => t!("prompt.landmark.mark_start_title"),
                (true, true) => t!("prompt.landmark.change_start_title"),
                (false, false) => t!("prompt.landmark.add_title"),
                (false, true) => t!("prompt.landmark.edit_title"),
            })
            .description(form)
            .confirm()
            .ok_text(match (is_start, editing_now) {
                (true, false) => t!("prompt.landmark.mark"),
                (false, false) => t!("prompt.add"),
                (_, true) => t!("prompt.apply"),
            })
            .cancel_text(t!("prompt.cancel"))
            .on_ok(move |_, window, cx| {
                let draft = landmark_draft(
                    fields.kind.read(cx).as_deref(),
                    &fields.title.read(cx).value(),
                    fields.place.read(cx),
                    &choices,
                );
                match draft {
                    Ok(landmark) => {
                        let editing = editing.clone();
                        page.update(cx, |this, cx| {
                            this.put_landmark(editing, landmark, window, cx);
                        })
                        .ok();
                        true
                    }
                    Err(words) => {
                        fields.error.update(cx, |error, cx| {
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
    use mbook::Landmark;

    use super::{landmark_draft, no_kind, no_place};
    use crate::features::edit_book::outline::{ChapterOutline, PlaceOutline};
    use crate::pages::editor::place_picker::{Place, chapter_choices};

    fn chapters() -> Vec<ChapterOutline> {
        vec![ChapterOutline {
            path: "a.md".to_owned(),
            len: 1,
            heading: Some("One".to_owned()),
            places: vec![PlaceOutline {
                anchor: "words".to_owned(),
                title: "Words".to_owned(),
                level: Some(2),
            }],
        }]
    }

    fn at(chapter: &str, anchor: Option<&str>) -> Place {
        Place {
            chapter: Some(chapter.to_owned()),
            anchor: anchor.map(str::to_owned),
        }
    }

    #[test]
    fn the_fields_give_a_landmark_with_its_title_on_one_line() {
        let choices = chapter_choices(&chapters());
        assert_eq!(
            landmark_draft(
                Some("glossary"),
                " Terms\nof the orchard ",
                &at("a.md", Some("words")),
                &choices
            ),
            Ok(Landmark::new(
                "glossary",
                "Terms of the orchard",
                "a.md#words"
            ))
        );
    }

    #[test]
    fn a_title_that_repeats_the_kind_is_left_to_the_reading_application() {
        let choices = chapter_choices(&chapters());
        assert_eq!(
            landmark_draft(Some("notes"), " Notes ", &at("a.md", None), &choices),
            Ok(Landmark::new("notes", "", "a.md"))
        );
    }

    #[test]
    fn a_kind_the_editor_does_not_know_is_kept() {
        let choices = chapter_choices(&chapters());
        assert_eq!(
            landmark_draft(Some("x-recipes"), "", &at("a.md", None), &choices),
            Ok(Landmark::new("x-recipes", "", "a.md"))
        );
    }

    #[test]
    fn a_kind_is_asked_for() {
        let choices = chapter_choices(&chapters());
        for kind in [None, Some("")] {
            assert_eq!(
                landmark_draft(kind, "Words", &at("a.md", None), &choices),
                Err(no_kind())
            );
        }
    }

    #[test]
    fn a_place_the_book_does_not_have_is_asked_for_again() {
        let choices = chapter_choices(&chapters());
        for place in [
            Place::default(),
            at("gone.md", None),
            at("a.md", Some("gone")),
        ] {
            assert_eq!(
                landmark_draft(Some("notes"), "", &place, &choices),
                Err(no_place())
            );
        }
    }
}
