//! The page: its header, and its layout for each size class.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Placement, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
    px,
};

use super::EditorPage;
use super::keys::{LeaveEditor, LeaveEditorBackward, PAGE_CONTEXT};
use super::sections::control;
use super::session::{Phase, Section};
use crate::entities::catalogue::Library;
use crate::features::appearance::Appearance;
use crate::features::update_banner;
use crate::shared::actions::{Find, GoBack, Save};
use crate::shared::layout::SizeClass;
use crate::shared::ui::{back_button, empty_state, icon_button, page_header, status_line};

/// The width of the sidebar of an expanded window.
const SIDEBAR_WIDTH: f32 = 260.0;
/// The width of the sheet that slides the sections over a medium window.
const SHEET_WIDTH: f32 = 320.0;

impl EditorPage {
    /// The size class of the window.
    pub(super) fn class(window: &Window, cx: &Context<'_, Self>) -> SizeClass {
        Appearance::global(cx).read(cx).size_class(window)
    }

    /// The title of the book as the header shows it.
    fn header_title(&self, cx: &Context<'_, Self>) -> String {
        self.session.title().map_or_else(
            || {
                Library::global(cx)
                    .read(cx)
                    .book(self.key)
                    .map_or_else(|| "Book".to_owned(), |book| book.row.title.clone())
            },
            str::to_owned,
        )
    }

    /// Slides the sections over the content of a medium window.
    fn open_sections(window: &mut Window, cx: &mut Context<'_, Self>) {
        let page = cx.weak_entity();
        window.open_sheet_at(Placement::Left, cx, move |sheet, _, cx| {
            let sections = page
                .update(cx, |this, cx| this.render_sections(cx).into_any_element())
                .ok();
            sheet
                .title("Book and chapters")
                .size(px(SHEET_WIDTH))
                .children(sections)
        });
    }

    /// What the section in front shows.
    fn render_section(
        &mut self,
        class: SizeClass,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let section = self.session.section;
        let scrolled = |content: AnyElement| {
            div()
                .id("section-scroll")
                .size_full()
                .overflow_y_scroll()
                .child(content)
                .into_any_element()
        };
        let heading = div()
            .id("section-heading")
            .role(Role::Heading)
            .aria_level(2)
            .aria_label(section.label())
            .test_support()
            .px_4()
            .pt_3()
            .text_lg()
            .font_weight(FontWeight::SEMIBOLD)
            .child(section.label());
        match section {
            Section::Chapters => self
                .render_chapter_area(class, window, cx)
                .into_any_element(),
            Section::Details => scrolled(
                v_flex()
                    .child(heading)
                    .child(self.render_details(cx))
                    .into_any_element(),
            ),
            Section::Cover => scrolled(
                v_flex()
                    .child(heading)
                    .child(self.render_cover(cx))
                    .into_any_element(),
            ),
            Section::Contents => scrolled(
                v_flex()
                    .child(heading)
                    .child(self.render_contents(cx))
                    .into_any_element(),
            ),
            Section::Landmarks => scrolled(
                v_flex()
                    .child(heading)
                    .child(self.render_landmarks(cx))
                    .into_any_element(),
            ),
            Section::Pictures => scrolled(
                v_flex()
                    .child(heading)
                    .child(self.render_pictures(cx))
                    .into_any_element(),
            ),
            Section::Figures(kind) => scrolled(
                v_flex()
                    .child(heading)
                    .child(self.render_figures(kind, cx))
                    .into_any_element(),
            ),
            Section::Problems => scrolled(
                v_flex()
                    .child(heading)
                    .child(self.render_problems(cx))
                    .into_any_element(),
            ),
        }
    }

    /// The layout of a book that is open.
    fn render_ready(
        &mut self,
        class: SizeClass,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        match class {
            SizeClass::Expanded => {
                let sidebar = self.render_sections(cx);
                let content = self.render_section(class, window, cx);
                h_flex()
                    .size_full()
                    .min_h_0()
                    .child(
                        div()
                            .id("editor-sidebar")
                            .w(px(SIDEBAR_WIDTH))
                            .h_full()
                            .flex_none()
                            .overflow_y_scroll()
                            .border_r_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().sidebar)
                            .child(sidebar),
                    )
                    .child(div().flex_1().min_w_0().h_full().child(content))
                    .into_any_element()
            }
            SizeClass::Medium => {
                let content = self.render_section(class, window, cx);
                v_flex()
                    .size_full()
                    .min_h_0()
                    .child(
                        h_flex()
                            .px_3()
                            .py_1()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(control("open-sections", "Book and chapters").on_click(
                                cx.listener(|_, _, window, cx| Self::open_sections(window, cx)),
                            )),
                    )
                    .child(div().flex_1().min_h_0().child(content))
                    .into_any_element()
            }
            // The list of sections and chapters opens from the menu button
            // of the header.
            SizeClass::Compact => {
                let content = self.render_section(class, window, cx);
                div().size_full().child(content).into_any_element()
            }
        }
    }
}

impl Render for EditorPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let class = Self::class(window, cx);
        let title = self.header_title(cx);
        let ready = self.session.phase == Phase::Ready;
        let status = self.session.status(self.saving.active());
        let back =
            back_button("back").on_click(cx.listener(|this, _, window, cx| this.back(window, cx)));
        let leading = if class == SizeClass::Compact && ready {
            // A phone has no room for the row of a medium window: the list of
            // sections and chapters is behind the menu button beside back.
            h_flex()
                .gap_1()
                .child(back)
                .child(
                    icon_button("open-sections", IconName::Menu, "Book and chapters")
                        .on_click(cx.listener(|_, _, window, cx| Self::open_sections(window, cx))),
                )
                .into_any_element()
        } else {
            back.into_any_element()
        };
        let mut header = page_header(title).leading(leading);
        if let Some(status) = status {
            header = header.trailing(status_line("editor-status", status).text_sm());
        }
        if ready {
            header = header.trailing(
                control("save", "Save")
                    .primary()
                    .disabled(self.saving.running)
                    .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
            );
        }
        let body = match self.session.phase.clone() {
            Phase::Loading => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(status_line("editor-opening", "Opening the book"))
                .into_any_element(),
            Phase::Failed(reason) => {
                empty_state("This book could not be opened", reason).into_any_element()
            }
            Phase::Ready => self.render_ready(class, window, cx),
        };
        let save_error = self.session.error_text(&super::session::Control::Save);
        v_flex()
            .id("editor-page")
            .role(Role::Main)
            .aria_label("Editor")
            .test_support()
            .key_context(PAGE_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &GoBack, window, cx| this.on_go_back(window, cx)))
            .on_action(cx.listener(|this, _: &Save, window, cx| this.save(window, cx)))
            .on_action(cx.listener(|this, _: &Find, window, cx| this.show_search(window, cx)))
            .on_action(cx.listener(|this, _: &LeaveEditor, window, cx| {
                this.leave_editor(false, window, cx);
            }))
            .on_action(cx.listener(|this, _: &LeaveEditorBackward, window, cx| {
                this.leave_editor(true, window, cx);
            }))
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(header)
            .children(update_banner(cx))
            .children(save_error.map(|message| {
                status_line("save-error", message)
                    .px_3()
                    .py_1()
                    .text_sm()
                    .text_color(cx.theme().danger)
            }))
            .child(div().flex_1().min_h_0().child(body))
    }
}

impl EditorPage {
    /// Escape, or the key that goes back: in the Markdown editor it only
    /// arms Tab to leave it; anywhere else it leaves the page.
    fn on_go_back(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.search.open {
            self.close_search(window, cx);
        } else if !self.escaped && self.text_focus(cx).contains_focused(window, cx) {
            self.escaped = true;
            cx.notify();
        } else {
            self.back(window, cx);
        }
    }

    /// Tab, after Escape, leaves the Markdown editor.
    fn leave_editor(&mut self, backward: bool, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.escaped = false;
        if backward {
            window.focus_prev(cx);
        } else {
            window.focus_next(cx);
        }
        cx.notify();
    }
}
