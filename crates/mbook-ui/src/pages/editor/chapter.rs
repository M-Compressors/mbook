//! The chapter area: the Markdown editor, the preview, and what changes the
//! chapter being edited.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{ButtonVariants as _, Toggle, ToggleGroup};
use gpui_kit::component::input::Editor;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px, relative,
};

use super::EditorPage;
use super::keys::{ESCAPED_CONTEXT, SOURCE_CONTEXT};
use super::prompts::confirm_remove;
use super::sections::control;
use super::session::{Control, ViewMode};
use crate::features::appearance::Appearance;
use crate::shared::layout::SizeClass;
use crate::shared::ui::{empty_state, icon_button, page_colors, status_line, target_size};

/// What the Markdown editor says about leaving it, as its description and as
/// the hint under it.
pub(super) const EDITOR_HINT: &str = "Tab indents. Press Escape, then Tab, to leave the editor.";

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
                "This book has no chapters",
                "Add a chapter to start writing.",
            )
            .action(
                control("add-first-chapter", "Add a chapter…")
                    .primary()
                    .on_click(cx.listener(|this, _, window, cx| this.add_chapter(window, cx))),
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
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    /// The name of the chapter, the choice of view and what changes the
    /// chapter.
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
        let title = if edited {
            format!("{path} - edited")
        } else {
            path.to_owned()
        };
        let (up_path, down_path, rename_path, remove_path) = (
            path.to_owned(),
            path.to_owned(),
            path.to_owned(),
            path.to_owned(),
        );
        let modes_for_click = modes;
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
                div()
                    .id("chapter-title")
                    .role(Role::Heading)
                    .aria_level(2)
                    .aria_label(title.clone())
                    .test_support()
                    .font_weight(FontWeight::SEMIBOLD)
                    .min_w_0()
                    .truncate()
                    .child(title),
            )
            .child(
                ToggleGroup::new("view-mode")
                    .segmented()
                    .single()
                    .on_click(cx.listener(move |this, checked: &Vec<bool>, window, cx| {
                        if let Some(next) = checked
                            .iter()
                            .position(|selected| *selected)
                            .and_then(|index| modes_for_click.get(index))
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
                        Toggle::new(id)
                            .icon(icon)
                            .tooltip(candidate.label())
                            .checked(*candidate == mode)
                            .min_w(px(target_size()))
                            .min_h(px(target_size()))
                    })),
            )
            .child(
                icon_button("chapter-up", IconName::ArrowUp, "Move up")
                    .disabled(index == 0)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.move_chapter(&up_path, -1, window, cx);
                    })),
            )
            .child(
                icon_button("chapter-down", IconName::ArrowDown, "Move down")
                    .disabled(index >= last)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.move_chapter(&down_path, 1, window, cx);
                    })),
            )
            .child(
                icon_button("chapter-path", IconName::FilePenLine, "Change the path…").on_click(
                    cx.listener(move |_, _, window, cx| {
                        Self::change_path(rename_path.clone(), window, cx);
                    }),
                ),
            )
            .child(
                icon_button("chapter-remove", IconName::Trash, "Remove").on_click(cx.listener(
                    move |_, _, window, cx| {
                        let path = remove_path.clone();
                        confirm_remove(&path.clone(), window, cx, move |this, window, cx| {
                            this.remove_chapter(path.clone(), window, cx);
                        });
                    },
                )),
            )
            .into_any_element()
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

    /// The chapters on the screen of a compact window: the list, and the
    /// control that adds one.
    pub(super) fn render_chapter_screen(&mut self, cx: &mut Context<'_, Self>) -> AnyElement {
        v_flex()
            .id("chapter-screen")
            .gap_2()
            .p_2()
            .child(self.render_chapter_list(cx))
            .children(self.session.error_text(&Control::Chapters).map(|message| {
                status_line("chapters-error", message)
                    .px_3()
                    .text_sm()
                    .text_color(cx.theme().danger)
            }))
            .child(div().h(px(1.0)))
            .into_any_element()
    }
}
