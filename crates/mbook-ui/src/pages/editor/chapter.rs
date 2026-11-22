//! The chapter area: the Markdown editor, the preview, and what changes the
//! chapter being edited.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{ButtonVariants as _, Toggle, ToggleGroup};
use gpui_kit::component::input::Editor;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px, relative,
};

use super::EditorPage;
use super::encode::Target;
use super::keys::{ESCAPED_CONTEXT, SOURCE_CONTEXT};
use super::prompts::{Question, confirm};
use super::sections::control;
use super::session::{Control, ViewMode, chapter_label, chapter_title};
use crate::features::appearance::Appearance;
use crate::shared::layout::SizeClass;
use crate::shared::ui::{empty_state, icon_button, page_colors, status_line, target_size};

/// What the Markdown editor says about leaving it, as its description and as
/// the hint under it.
pub(super) const EDITOR_HINT: &str = "Tab indents. Press Escape, then Tab, to leave the editor.";

/// The question before a chapter is removed. It says what goes with the
/// chapter because it led into it: entries of a contents list the book
/// keeps, and landmarks.
fn removal_question(name: &str, entries: usize, landmarks: usize) -> Question {
    let mut gone = Vec::new();
    match entries {
        0 => {}
        1 => gone.push("Its entry of the contents".to_owned()),
        count => gone.push(format!("Its {count} entries of the contents")),
    }
    match (landmarks, gone.is_empty()) {
        (0, _) => {}
        (1, true) => gone.push("The landmark that leads into it".to_owned()),
        (1, false) => gone.push("the landmark that leads into it".to_owned()),
        (count, true) => gone.push(format!("The {count} landmarks that lead into it")),
        (count, false) => gone.push(format!("the {count} landmarks that lead into it")),
    }
    let verb = if entries + landmarks == 1 {
        "is"
    } else {
        "are"
    };
    let detail = if gone.is_empty() {
        "This cannot be undone.".to_owned()
    } else {
        format!(
            "{} {verb} removed with it. This cannot be undone.",
            gone.join(" and ")
        )
    };
    Question {
        title: format!("Remove {name}?"),
        detail,
        ok: "Remove",
    }
}

impl EditorPage {
    /// The chapter area of an expanded or a medium window, or the screen of a
    /// chapter of a compact one.
    pub(super) fn render_chapter_area(
        &mut self,
        class: SizeClass,
        window: &gpui_kit::Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let Some(path) = self.session.open.clone() else {
            return empty_state(
                "This book has no chapters yet",
                "Add the first chapter to start writing.",
            )
            .action(
                control("add-first-chapter", "New chapter…")
                    .primary()
                    .on_click(cx.listener(|_, _, window, cx| Self::add_chapter(window, cx))),
            )
            .into_any_element();
        };
        let mode = self.session.mode.effective(class);
        let theme = cx.theme().clone();
        let (paper, ink) = page_colors(Appearance::global(cx).read(cx).page_tone(window));
        let label = format!("Preview of {path}");
        self.preview
            .update(cx, |view, cx| view.set_label(label, cx));
        let shown = self.preview.clone();
        let body = match mode {
            ViewMode::Visual => self.render_visual(&path, window, cx),
            ViewMode::Source => self.render_source(&path, cx),
            ViewMode::Preview => div()
                .size_full()
                .px_4()
                .py_2()
                .bg(paper)
                .text_color(ink)
                .child(shown)
                .into_any_element(),
            ViewMode::Split => h_flex()
                .size_full()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .child(self.render_source(&path, cx)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .border_l_1()
                        .border_color(theme.border)
                        .px_4()
                        .py_2()
                        .bg(paper)
                        .text_color(ink)
                        .child(shown),
                )
                .into_any_element(),
        };
        v_flex()
            .id("chapter-area")
            .size_full()
            .min_h_0()
            .child(self.render_chapter_bar(&path, class, cx))
            .children(self.session.error_text(&Control::Chapters).map(|message| {
                status_line("chapters-error", message)
                    .px_3()
                    .py_1()
                    .text_sm()
                    .text_color(theme.danger)
            }))
            .children(
                self.session
                    .error_text(&Control::ChapterText)
                    .map(|message| {
                        status_line("chapter-text-error", message)
                            .px_3()
                            .py_1()
                            .text_sm()
                            .text_color(theme.danger)
                    }),
            )
            .children(
                self.session
                    .error_text(&Control::ChapterPicture)
                    .map(|message| {
                        status_line("chapter-picture-error", message)
                            .px_3()
                            .py_1()
                            .text_sm()
                            .text_color(theme.danger)
                    }),
            )
            .children(self.encoder.names(Target::Chapter).into_iter().map(|name| {
                status_line("chapter-picture-encoding", format!("Adding {name}…"))
                    .px_3()
                    .py_1()
                    .text_sm()
            }))
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    /// The number and title of the chapter with its file, the choice of view,
    /// and the menu of what changes the chapter.
    fn render_chapter_bar(
        &mut self,
        path: &str,
        class: SizeClass,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let modes = ViewMode::available(class);
        let mode = self.session.mode.effective(class);
        let index = self.session.chapter_index(path).unwrap_or(0);
        let last = self.session.chapters().len().saturating_sub(1);
        let edited = self.session.is_edited(path);
        let heading = self
            .session
            .chapter(path)
            .and_then(|chapter| chapter.heading.clone());
        let name = chapter_label(index + 1, path, heading.as_deref(), false, edited);
        let muted = cx.theme().muted_foreground;
        let words = class != SizeClass::Compact;
        h_flex()
            .id("chapter-bar")
            .w_full()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .id("chapter-title")
                    .role(Role::Heading)
                    .aria_level(2)
                    .aria_label(name)
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .items_baseline()
                    .gap_2()
                    .child(
                        div()
                            .flex_none()
                            .text_color(muted)
                            .child((index + 1).to_string()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(chapter_title(heading.as_deref()).to_owned()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_sm()
                            .text_color(muted)
                            .child(path.to_owned()),
                    )
                    .children(edited.then(|| {
                        div()
                            .flex_none()
                            .text_sm()
                            .text_color(muted)
                            .child("edited")
                    })),
            )
            .child(
                ToggleGroup::new("view-mode")
                    .segmented()
                    .single()
                    .on_click(cx.listener(move |this, checked: &Vec<bool>, window, cx| {
                        if let Some(next) = checked
                            .iter()
                            .position(|selected| *selected)
                            .and_then(|index| modes.get(index))
                        {
                            this.set_mode(*next, window, cx);
                        }
                    }))
                    .children(modes.iter().map(|candidate| {
                        let (id, icon) = match candidate {
                            ViewMode::Visual => ("mode-visual", IconName::Type),
                            ViewMode::Source => ("mode-source", IconName::Code),
                            ViewMode::Split => ("mode-split", IconName::Columns2),
                            ViewMode::Preview => ("mode-preview", IconName::Eye),
                        };
                        // A window with room names the views in words; the
                        // tooltip is where the control takes its name from.
                        let toggle = if words {
                            Toggle::new(id).label(candidate.label()).px_3()
                        } else {
                            Toggle::new(id).icon(icon)
                        };
                        toggle
                            .tooltip(candidate.label())
                            .checked(*candidate == mode)
                            .min_w(px(target_size()))
                            .min_h(px(target_size()))
                    })),
            )
            .child(Self::chapter_menu(path, index, last, cx))
            .into_any_element()
    }

    /// The menu of what changes the chapter itself: its place in the reading
    /// order, the name of its file, and removing it.
    fn chapter_menu(
        path: &str,
        index: usize,
        last: usize,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let page = cx.weak_entity();
        let path = path.to_owned();
        icon_button("chapter-menu", IconName::Ellipsis, "Chapter actions").dropdown_menu(
            move |menu, _, _| {
                let step = |label: &'static str, by: isize, disabled: bool| {
                    let (page, path) = (page.clone(), path.clone());
                    PopupMenuItem::new(label)
                        .disabled(disabled)
                        .on_click(move |_, window, cx| {
                            page.update(cx, |this, cx| this.move_chapter(&path, by, window, cx))
                                .ok();
                        })
                };
                let (rename_page, rename_path) = (page.clone(), path.clone());
                let (remove_page, remove_path) = (page.clone(), path.clone());
                menu.item(step("Move up", -1, index == 0))
                    .item(step("Move down", 1, index >= last))
                    .separator()
                    .item(
                        PopupMenuItem::new("Rename the file…").on_click(move |_, window, cx| {
                            let path = rename_path.clone();
                            rename_page
                                .update(cx, |_, cx| Self::change_path(path, window, cx))
                                .ok();
                        }),
                    )
                    .item(PopupMenuItem::new("Remove the chapter…").on_click(
                        move |_, window, cx| {
                            let path = remove_path.clone();
                            remove_page
                                .update(cx, |this, cx| {
                                    let question = removal_question(
                                        &this.chapter_name(&path),
                                        this.session.stored_entries_of(&path),
                                        this.session.landmarks_into(&path),
                                    );
                                    confirm(question, window, cx, move |this, window, cx| {
                                        this.remove_chapter(path.clone(), window, cx);
                                    });
                                })
                                .ok();
                        },
                    ))
            },
        )
    }

    /// A chapter in words, for a question about it: its title in quotes, or
    /// its file when it has no heading.
    pub(super) fn chapter_name(&self, path: &str) -> String {
        self.session
            .chapter(path)
            .and_then(|chapter| chapter.heading.as_deref())
            .map_or_else(|| path.to_owned(), |heading| format!("“{heading}”"))
    }

    /// The Markdown editor of the chapter and the hint about leaving it.
    fn render_source(&mut self, path: &str, cx: &mut Context<'_, Self>) -> AnyElement {
        let context = if self.escaped {
            ESCAPED_CONTEXT
        } else {
            SOURCE_CONTEXT
        };
        v_flex()
            .id("chapter-source")
            .role(Role::Group)
            .aria_label(format!("Markdown source of {path}"))
            .aria_description(EDITOR_HINT)
            .test_support()
            .key_context(context)
            .size_full()
            .min_w_0()
            .child(
                div().flex_1().min_h_0().child(
                    Editor::new(&self.text)
                        .aria_label(format!("Markdown of {path}"))
                        .h(relative(1.0)),
                ),
            )
            .child(
                status_line("editor-hint", EDITOR_HINT)
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::removal_question;

    #[test]
    fn the_question_about_a_chapter_names_it() {
        let question = removal_question("“One”", 0, 0);
        assert_eq!(question.title, "Remove “One”?");
        assert_eq!(question.detail, "This cannot be undone.");
        assert_eq!(question.ok, "Remove");
    }

    #[test]
    fn the_question_counts_the_entries_that_go_with_the_chapter() {
        assert_eq!(
            removal_question("“One”", 1, 0).detail,
            "Its entry of the contents is removed with it. This cannot be undone."
        );
        assert_eq!(
            removal_question("“One”", 12, 0).detail,
            "Its 12 entries of the contents are removed with it. This cannot be undone."
        );
    }

    #[test]
    fn the_question_counts_the_landmarks_that_go_with_the_chapter() {
        assert_eq!(
            removal_question("“One”", 0, 1).detail,
            "The landmark that leads into it is removed with it. This cannot be undone."
        );
        assert_eq!(
            removal_question("“One”", 0, 2).detail,
            "The 2 landmarks that lead into it are removed with it. This cannot be undone."
        );
    }

    #[test]
    fn the_question_names_entries_and_landmarks_together() {
        assert_eq!(
            removal_question("“One”", 1, 1).detail,
            "Its entry of the contents and the landmark that leads into it are removed with it. \
             This cannot be undone."
        );
        assert_eq!(
            removal_question("“One”", 3, 2).detail,
            "Its 3 entries of the contents and the 2 landmarks that lead into it are removed \
             with it. This cannot be undone."
        );
    }
}
