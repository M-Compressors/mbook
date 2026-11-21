//! The list of sections and the list of chapters.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, v_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
    px,
};

use super::EditorPage;
use super::session::{Section, chapter_label};
use crate::shared::ui::{status_line, target_size};

/// A labelled button of the editor: at least as tall as a control.
pub(super) fn control(
    id: impl Into<gpui_kit::ElementId>,
    label: impl Into<SharedString>,
) -> Button {
    Button::new(id)
        .label(label)
        .min_h(px(target_size()))
        .cursor_pointer()
}

/// A line of a list that is a button: its words start at the left edge. A
/// button centres what it holds and ignores `justify_start`, so the words are
/// a child that fills it.
pub(super) fn list_item(
    id: impl Into<gpui_kit::ElementId>,
    label: impl Into<SharedString>,
) -> Button {
    let label = label.into();
    Button::new(id)
        .accessibility_label(label.clone())
        .min_h(px(target_size()))
        .cursor_pointer()
        .w_full()
        .child(div().w_full().text_left().child(label))
}

impl EditorPage {
    /// The sections, and the chapters under the chapters, as the sidebar of
    /// an expanded window and the sheet of a medium one show them. A compact
    /// window lists only the sections, and the chapters on a screen of their
    /// own.
    pub(super) fn render_sections(
        &mut self,
        nest_chapters: bool,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let problems = self.session.problems().len();
        let mut list = v_flex()
            .id("sections")
            .role(Role::Navigation)
            .aria_label("Sections")
            .test_support()
            .gap_1()
            .p_2();
        for section in Section::ALL {
            let current = self.session.section == section;
            let (label, mut name) = if section == Section::Problems && problems > 0 {
                (
                    format!("{} ({problems})", section.label()),
                    format!("{}, {problems}", section.label()),
                )
            } else {
                (section.label().to_owned(), section.label().to_owned())
            };
            if current {
                name.push_str(", current section");
            }
            list = list.child(
                list_item(section.id(), label)
                    .ghost()
                    .selected(current)
                    .accessibility_label(name)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_section(section, window, cx);
                    })),
            );
            if section == Section::Chapters && nest_chapters {
                list = list.child(self.render_chapter_list(cx));
            }
        }
        list.into_any_element()
    }

    /// The chapters in reading order, and the control that adds one.
    pub(super) fn render_chapter_list(&mut self, cx: &mut Context<'_, Self>) -> AnyElement {
        let mut list = v_flex()
            .id("chapters-list")
            .role(Role::Group)
            .aria_label("Chapters")
            .gap_1()
            .pl_3();
        let current = self.session.open.clone();
        let chapters = self.session.chapters().to_vec();
        for chapter in &chapters {
            let is_current = current.as_deref() == Some(chapter.path.as_str())
                && self.session.section == Section::Chapters;
            let edited = self.session.is_edited(&chapter.path);
            let visible = if edited {
                format!("{} - edited", chapter.path)
            } else {
                chapter.path.clone()
            };
            let name = chapter_label(
                &chapter.path,
                chapter.heading.as_deref(),
                is_current,
                edited,
            );
            let path = chapter.path.clone();
            list = list.child(
                list_item(format!("chapter-{}", chapter.path), visible)
                    .ghost()
                    .selected(is_current)
                    .accessibility_label(name)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_chapter(&path, window, cx);
                    })),
            );
        }
        if chapters.is_empty() {
            list = list.child(
                status_line("no-chapters", "This book has no chapters.")
                    .text_sm()
                    .text_color(cx.theme().muted_foreground),
            );
        }
        list.child(
            list_item("add-chapter", "Add a chapter…")
                .ghost()
                .on_click(cx.listener(|this, _, window, cx| this.add_chapter(window, cx))),
        )
        .into_any_element()
    }

    /// Opens a section; a window that slid the sections over the content
    /// slides them away.
    pub(super) fn choose_section(
        &mut self,
        section: Section,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        use gpui_kit::component::WindowExt as _;
        self.flush_text(window, cx);
        self.session.show_section(section);
        if section == Section::Chapters && self.session.open.is_none() {
            if let Some(first) = self
                .session
                .chapters()
                .first()
                .map(|chapter| chapter.path.clone())
            {
                self.session.open = Some(first);
            }
            self.load_open_text(window, cx);
        }
        window.close_sheet(cx);
        cx.notify();
    }

    /// Opens a chapter in the editor from the list.
    pub(super) fn choose_chapter(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        use gpui_kit::component::WindowExt as _;
        self.open_chapter(path, window, cx);
        window.close_sheet(cx);
    }
}
