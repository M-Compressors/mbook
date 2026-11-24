//! The Landmarks section: where the text of the book begins, and the other
//! places a reading application can take a reader to.

use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Role, SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window,
    div, px,
};
use mbook::Landmark;
use rust_i18n::t;

use super::EditorPage;
use super::commands::Origin;
use super::landmark_kinds::{
    START, insertion_index, kind_words, landmark_name, no_start_effect, start_effect, start_name,
};
use super::landmark_prompt::{LandmarkRequest, ask_landmark};
use super::place_picker::{ChapterChoice, Place, chapter_choices, chapter_words, place_words};
use super::prompts::confirm_remove;
use super::sections::control;
use super::session::Control;
use crate::features::edit_book::command::Command;
use crate::shared::ui::status_line;

/// Where a landmark leads, in words: its chapter, and its start or the place
/// in it.
fn target_words(landmark: &Landmark, choices: &[ChapterChoice]) -> String {
    let place = Place {
        chapter: Some(landmark.chapter.clone()),
        anchor: landmark.anchor.clone(),
    };
    format!(
        "{}, {}",
        chapter_words(&place, choices),
        place_words(&place, choices)
    )
}

impl EditorPage {
    /// The start of the text, then the other landmarks in their stored
    /// order, and the controls that add, change and remove them.
    pub(super) fn render_landmarks(&self, cx: &Context<'_, Self>) -> AnyElement {
        let pending = self.session.landmarks_pending;
        let has_chapters = !self.session.chapters().is_empty();
        let choices = chapter_choices(self.session.chapters());
        let landmarks = self.session.landmarks();
        let start = landmarks
            .iter()
            .enumerate()
            .find(|(_, landmark)| landmark.kind == START);
        let others: Vec<_> = landmarks
            .iter()
            .enumerate()
            .filter(|(_, landmark)| landmark.kind != START)
            .map(|(index, landmark)| {
                let target = target_words(landmark, &choices);
                Self::render_landmark(index, landmark, &target, pending, has_chapters, cx)
            })
            .collect();
        let muted = cx.theme().muted_foreground;
        let start_target = start.map(|(_, landmark)| target_words(landmark, &choices));
        v_flex()
            .id("landmarks")
            .role(Role::Group)
            .aria_label(t!("prompt.landmarks.label"))
            .gap_5()
            .p_4()
            .max_w(px(640.0))
            .w_full()
            .child(status_line("landmarks-hint", t!("prompt.landmarks.hint")).text_color(muted))
            .children(
                (!has_chapters).then(|| {
                    status_line("no-chapters-to-mark", t!("prompt.landmarks.no_chapters"))
                }),
            )
            .child(Self::render_start(
                start,
                start_target.as_deref(),
                pending,
                has_chapters,
                cx,
            ))
            .child(
                v_flex()
                    .id("landmarks-other")
                    .role(Role::Group)
                    .aria_label(t!("prompt.landmarks.other_places"))
                    .gap_2()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(t!("prompt.landmarks.other_places"))),
                    )
                    .children(others.is_empty().then(|| {
                        status_line("no-other-places", t!("prompt.landmarks.no_other_places"))
                            .text_color(muted)
                    }))
                    .child(v_flex().children(others))
                    .child(
                        control("add-landmark", t!("prompt.landmarks.add"))
                            .self_start()
                            .disabled(pending || !has_chapters)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.ask_new_landmark(None, window, cx);
                            })),
                    ),
            )
            .children(self.session.error_text(&Control::Landmarks).map(|message| {
                status_line("landmarks-error", message).text_color(cx.theme().danger)
            }))
            .into_any_element()
    }

    /// The block of the start: the one landmark a reader meets without
    /// asking for it, set apart by a band of the accent.
    fn render_start(
        start: Option<(usize, &Landmark)>,
        target: Option<&str>,
        pending: bool,
        has_chapters: bool,
        cx: &Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let start_words = start_name();
        let name = match target {
            Some(target) => format!("{start_words}, {target}"),
            None => t!("prompt.landmarks.start_not_marked", name = start_words).into_owned(),
        };
        let actions = match start {
            Some((index, landmark)) => {
                let (edited, removed) = (landmark.clone(), landmark.clone());
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        control("edit-start", t!("prompt.landmarks.change"))
                            .accessibility_label(t!("prompt.landmarks.change_named", name = name))
                            .disabled(pending || !has_chapters)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.ask_landmark_at(index, &edited, window, cx);
                            })),
                    )
                    .child(
                        control("remove-start", t!("prompt.remove.ok"))
                            .accessibility_label(t!("prompt.landmarks.remove_named", name = name))
                            .disabled(pending)
                            .on_click(cx.listener(move |_, _, window, cx| {
                                Self::confirm_remove_landmark(index, removed.clone(), window, cx);
                            })),
                    )
            }
            None => h_flex().child(
                control("mark-start", t!("prompt.landmarks.mark_start"))
                    .disabled(pending || !has_chapters)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.ask_new_landmark(Some(START), window, cx);
                    })),
            ),
        };
        h_flex()
            .id("landmark-start")
            .role(Role::Group)
            .aria_label(name)
            .test_support()
            .items_stretch()
            .overflow_hidden()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .child(div().flex_none().w(px(4.0)).bg(theme.primary))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .p_4()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(start_words)),
                    )
                    .children(target.map(|target| div().child(target.to_owned())))
                    .child(div().text_sm().text_color(theme.muted_foreground).child(
                        SharedString::from(if target.is_some() {
                            start_effect()
                        } else {
                            no_start_effect()
                        }),
                    ))
                    .child(div().pt_2().child(actions)),
            )
    }

    /// One landmark other than the start: the name a reader is shown, the
    /// kind when a title stands in for it, and where it leads.
    fn render_landmark(
        index: usize,
        landmark: &Landmark,
        target: &str,
        pending: bool,
        has_chapters: bool,
        cx: &Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let name = landmark_name(landmark);
        let kind = (!landmark.title.is_empty()).then(|| kind_words(&landmark.kind));
        let (edited, removed) = (landmark.clone(), landmark.clone());
        // Assistive technology hears the kind too when a title stands in
        // for it, as the eye sees it beside the title.
        let spoken = match &kind {
            Some(kind) => format!("{name}, {kind}, {target}"),
            None => format!("{name}, {target}"),
        };
        h_flex()
            .id(("landmark", index))
            .role(Role::Group)
            .aria_label(spoken.clone())
            .gap_2()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_baseline()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(name.clone()),
                            )
                            .children(kind.map(|kind| {
                                div()
                                    .flex_none()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child(kind)
                            })),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(target.to_owned()),
                    ),
            )
            .child(
                control(("edit-landmark", index), t!("prompt.landmarks.edit"))
                    .accessibility_label(t!("prompt.landmarks.edit_named", name = spoken))
                    .disabled(pending || !has_chapters)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.ask_landmark_at(index, &edited, window, cx);
                    })),
            )
            .child(
                control(("remove-landmark", index), t!("prompt.remove.ok"))
                    .accessibility_label(t!("prompt.landmarks.remove_named", name = spoken))
                    .disabled(pending)
                    .on_click(cx.listener(move |_, _, window, cx| {
                        Self::confirm_remove_landmark(index, removed.clone(), window, cx);
                    })),
            )
    }

    /// Asks for a new landmark of `kind`, or of a kind the author chooses.
    /// It starts at the chapter that is open, or the first one.
    pub(super) fn ask_new_landmark(
        &self,
        kind: Option<&str>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(first) = self.session.chapters().first() else {
            return;
        };
        let chapter = self
            .session
            .open
            .clone()
            .unwrap_or_else(|| first.path.clone());
        let request = LandmarkRequest {
            editing: None,
            kind: kind.map(str::to_owned),
            place: Place {
                chapter: Some(chapter),
                anchor: None,
            },
            choices: chapter_choices(self.session.chapters()),
        };
        ask_landmark(request, window, cx);
    }

    /// Asks for another kind, title or place of the landmark at `index`.
    pub(super) fn ask_landmark_at(
        &self,
        index: usize,
        landmark: &Landmark,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.session.chapters().is_empty() {
            return;
        }
        let request = LandmarkRequest {
            editing: Some((index, landmark.clone())),
            kind: Some(landmark.kind.clone()),
            place: Place {
                chapter: Some(landmark.chapter.clone()),
                anchor: landmark.anchor.clone(),
            },
            choices: chapter_choices(self.session.chapters()),
        };
        ask_landmark(request, window, cx);
    }

    /// Asks before the landmark at `index` is removed.
    fn confirm_remove_landmark(
        index: usize,
        landmark: Landmark,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let what = t!(
            "prompt.landmarks.remove_what",
            name = landmark_name(&landmark)
        );
        confirm_remove(&what, window, cx, move |this, window, cx| {
            let mut landmarks = this.session.landmarks().to_vec();
            if landmarks.get(index) != Some(&landmark) {
                return;
            }
            landmarks.remove(index);
            this.set_landmarks(landmarks, window, cx);
        });
    }

    /// Puts a landmark the dialog gave into the list: in place of the one it
    /// edits, which must still be where it was, or among the others in the
    /// order of the book.
    pub(super) fn put_landmark(
        &mut self,
        editing: Option<(usize, Landmark)>,
        landmark: Landmark,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let mut landmarks = self.session.landmarks().to_vec();
        if let Some((index, original)) = editing {
            match landmarks.get_mut(index) {
                Some(item) if *item == original => *item = landmark,
                _ => return,
            }
        } else {
            let at = insertion_index(&landmarks, self.session.chapters(), &landmark);
            landmarks.insert(at, landmark);
        }
        self.set_landmarks(landmarks, window, cx);
    }

    /// Queues a replacement while keeping the controls of this section
    /// disabled until the new outline arrives.
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
