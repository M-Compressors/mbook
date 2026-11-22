//! Editing the landmarks that lead to the book's notes.

use std::cell::Cell;

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Focusable as _, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Role, StatefulInteractiveElement as _, Styled as _, Window,
    div,
};
use mbook::Landmark;

use super::EditorPage;
use super::commands::Origin;
use super::place_picker::{Place, chapter_choices, chapter_words, place_picker, place_words};
use super::prompts::confirm_remove;
use super::sections::control;
use super::session::Control;
use crate::features::edit_book::command::Command;
use crate::shared::ui::status_line;

const NOTES: &str = "notes";
/// What the section says a landmark is, in the words of a reader.
pub(super) const LANDMARKS_HINT: &str = "A landmark tells a reading application where a part of the book is, so that it can lead a reader there. Here you mark where the notes of the book are collected.";

impl EditorPage {
    /// The notes landmarks and the controls to add, edit and remove them.
    pub(super) fn render_landmarks(&self, cx: &Context<'_, Self>) -> AnyElement {
        let pending = self.session.landmarks_pending;
        let has_chapters = !self.session.chapters().is_empty();
        let notes = self
            .session
            .landmarks()
            .iter()
            .enumerate()
            .filter(|(_, landmark)| landmark.kind == NOTES);
        let empty_message = if has_chapters {
            "No place is marked as the notes yet."
        } else {
            "Add a chapter before adding a notes landmark."
        };
        let choices = chapter_choices(self.session.chapters());
        v_flex()
            .id("landmarks")
            .role(Role::Group)
            .aria_label("Landmarks")
            .gap_3()
            .p_4()
            .max_w(gpui_kit::px(640.0))
            .w_full()
            .child(
                status_line("landmarks-hint", LANDMARKS_HINT)
                    .text_color(cx.theme().muted_foreground),
            )
            .children(
                notes
                    .clone()
                    .next()
                    .is_none()
                    .then(|| status_line("no-notes", empty_message)),
            )
            .children(notes.map(|(index, landmark)| {
                let place = Place {
                    chapter: Some(landmark.chapter.clone()),
                    anchor: landmark.anchor.clone(),
                };
                let target = format!(
                    "{}, {}",
                    chapter_words(&place, &choices),
                    place_words(&place, &choices)
                );
                Self::render_note(index, landmark, &target, pending, has_chapters, cx)
            }))
            .child(
                control("add-note", "Add a notes landmark…")
                    .self_start()
                    .disabled(pending || !has_chapters)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.ask_note(None, window, cx);
                    })),
            )
            .children(self.session.error_text(&Control::Landmarks).map(|message| {
                status_line("landmarks-error", message).text_color(cx.theme().danger)
            }))
            .into_any_element()
    }

    fn render_note(
        index: usize,
        landmark: &Landmark,
        target: &str,
        pending: bool,
        has_chapters: bool,
        cx: &Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let title = if landmark.title.is_empty() {
            "Notes".to_owned()
        } else {
            landmark.title.clone()
        };
        h_flex()
            .id(("note", index))
            .role(Role::Group)
            .aria_label(format!("{title}, {target}"))
            .gap_2()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(title.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(target.to_owned()),
                    ),
            )
            .child(
                control(("edit-note", index), "Edit…")
                    .accessibility_label(format!("Edit {title}, {target}"))
                    .disabled(pending || !has_chapters)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.ask_note(Some(index), window, cx);
                    })),
            )
            .child(
                control(("remove-note", index), "Remove")
                    .accessibility_label(format!("Remove {title}, {target}"))
                    .disabled(pending)
                    .on_click(cx.listener(move |_, _, window, cx| {
                        confirm_remove(
                            "this notes landmark",
                            window,
                            cx,
                            move |this, window, cx| {
                                let mut landmarks = this.session.landmarks().to_vec();
                                if landmarks.get(index).is_none_or(|item| item.kind != NOTES) {
                                    return;
                                }
                                landmarks.remove(index);
                                this.set_landmarks(landmarks, window, cx);
                            },
                        );
                    })),
            )
    }

    fn ask_note(&self, index: Option<usize>, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(first_chapter) = self.session.chapters().first() else {
            return;
        };
        let existing = index.and_then(|index| self.session.landmarks().get(index));
        let place = existing.map_or_else(
            || Place {
                chapter: Some(
                    self.session
                        .open
                        .clone()
                        .unwrap_or_else(|| first_chapter.path.clone()),
                ),
                anchor: None,
            },
            |note| Place {
                chapter: Some(note.chapter.clone()),
                anchor: note.anchor.clone(),
            },
        );
        let selected = cx.new(|_| place);
        let choices = chapter_choices(self.session.chapters());
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Notes"));
        if let Some(note) = existing {
            title.update(cx, |field, cx| {
                field.set_value(note.title.as_str(), window, cx);
            });
        }
        let page = cx.weak_entity();
        let first = Cell::new(true);
        let editing = index.is_some();
        window.open_alert_dialog(cx, move |alert, window, cx| {
            if first.replace(false) {
                title.focus_handle(cx).focus(window, cx);
            }
            let form = v_flex()
                .gap_3()
                .child(
                    v_flex()
                        .gap_1()
                        .child("Title (optional)")
                        .child(Input::new(&title).aria_label("Title (optional)")),
                )
                .child(
                    v_flex()
                        .gap_1()
                        .child("Where the notes are")
                        .child(place_picker("note", &selected, &choices, None, cx)),
                );
            let (page, title, selected) = (page.clone(), title.clone(), selected.clone());
            alert
                .title(if editing {
                    "Edit notes landmark"
                } else {
                    "Add a notes landmark"
                })
                .description(form)
                .confirm()
                .ok_text(if editing { "Apply" } else { "Add" })
                .cancel_text("Cancel")
                .on_ok(move |_, window, cx| {
                    let place = selected.read(cx).clone();
                    let note = Landmark {
                        kind: NOTES.to_owned(),
                        title: title.read(cx).value().trim().to_owned(),
                        // The dialog opens with a chapter chosen; a book
                        // refuses a landmark without one.
                        chapter: place.chapter.unwrap_or_default(),
                        anchor: place.anchor,
                    };
                    page.update(cx, |this, cx| {
                        let mut landmarks = this.session.landmarks().to_vec();
                        if let Some(index) = index {
                            if let Some(item) =
                                landmarks.get_mut(index).filter(|item| item.kind == NOTES)
                            {
                                *item = note;
                            } else {
                                return;
                            }
                        } else {
                            landmarks.push(note);
                        }
                        this.set_landmarks(landmarks, window, cx);
                    })
                    .ok();
                    true
                })
        });
    }

    /// Queues a replacement while keeping the notes controls on this outline
    /// disabled until the new one arrives.
    fn set_landmarks(
        &mut self,
        landmarks: Vec<Landmark>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.session.landmarks_pending {
            return;
        }
        self.session.landmarks_pending = true;
        self.send(
            Command::SetLandmarks(landmarks),
            Origin::Landmarks,
            window,
            cx,
        );
        cx.notify();
    }
}
