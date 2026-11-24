//! The list of sections and the list of chapters.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Role, SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window,
    div, px,
};
use rust_i18n::t;

use super::EditorPage;
use super::session::{Section, chapter_label, chapter_title};
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
    list_row(id, label.clone(), div().w_full().text_left().child(label))
}

/// A line of a list that is a button and shows `content`, which fills it.
/// `name` is what assistive technology calls it.
pub(super) fn list_row(
    id: impl Into<gpui_kit::ElementId>,
    name: impl Into<SharedString>,
    content: impl IntoElement,
) -> Button {
    Button::new(id)
        .accessibility_label(name)
        .min_h(px(target_size()))
        .cursor_pointer()
        .w_full()
        .child(content)
}

/// The words over a group of the list. They are not a control: the group
/// under them carries the same name for assistive technology.
fn group_heading(label: impl Into<SharedString>, cx: &Context<'_, EditorPage>) -> impl IntoElement {
    div()
        .px_3()
        .pt_3()
        .pb_1()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(label.into())
}

impl EditorPage {
    /// What the sidebar of an expanded window, the sheet of a medium one and
    /// the first screen of a compact one show: the sections about the whole
    /// book, then the chapters by number and title.
    pub(super) fn render_sections(&mut self, cx: &mut Context<'_, Self>) -> AnyElement {
        let problems = self.session.problems().len();
        let mut book = v_flex()
            .id("book-sections")
            .role(Role::Group)
            .aria_label(t!("editor.book"))
            .gap_1();
        for section in Section::BOOK {
            let current = self.session.section == section;
            let words = section.label();
            let (label, mut name) = if section == Section::Problems && problems > 0 {
                (
                    format!("{words} ({problems})"),
                    format!("{words}, {problems}"),
                )
            } else {
                (words.clone().into_owned(), words.into_owned())
            };
            if current {
                name.push_str(&t!("editor.current_section"));
            }
            book = book.child(
                list_item(section.id(), label)
                    .ghost()
                    .selected(current)
                    .accessibility_label(name)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_section(section, window, cx);
                    })),
            );
        }
        v_flex()
            .id("sections")
            .role(Role::Navigation)
            .aria_label(t!("editor.book_and_chapters"))
            .test_support()
            .p_2()
            .child(group_heading(t!("editor.book"), cx))
            .child(book)
            .child(
                // The control that adds a chapter stands over the list, so
                // that a long book does not push it out of sight.
                h_flex()
                    .items_end()
                    .justify_between()
                    .child(group_heading(t!("editor.section.chapters"), cx))
                    .child(
                        Button::new("add-chapter")
                            .ghost()
                            .small()
                            .icon(IconName::Plus)
                            .label(t!("editor.chapter.new"))
                            .cursor_pointer()
                            .on_click(
                                cx.listener(|_, _, window, cx| Self::add_chapter(window, cx)),
                            ),
                    ),
            )
            .child(self.render_chapter_list(cx))
            .into_any_element()
    }

    /// The chapters in reading order, each by its number and the title its
    /// first heading gives it.
    fn render_chapter_list(&mut self, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let mut list = v_flex()
            .id("chapters-list")
            .role(Role::Group)
            .aria_label(t!("editor.section.chapters"))
            .gap_1();
        let current = self.session.open.clone();
        let chapters = self.session.chapters().to_vec();
        for (index, chapter) in chapters.iter().enumerate() {
            let number = index + 1;
            let is_current = current.as_deref() == Some(chapter.path.as_str())
                && self.session.section == Section::Chapters;
            let edited = self.session.is_edited(&chapter.path);
            let heading = chapter.heading.as_deref();
            let name = chapter_label(number, &chapter.path, heading, is_current, edited);
            let title = div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_left()
                .child(chapter_title(heading).into_owned());
            let row = h_flex()
                .w_full()
                .gap_2()
                .child(
                    div()
                        .flex_none()
                        .w(px(22.0))
                        .text_right()
                        .text_color(theme.muted_foreground)
                        .child(number.to_string()),
                )
                .child(if heading.is_some() {
                    title
                } else {
                    title.text_color(theme.muted_foreground)
                })
                .children(edited.then(|| {
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(t!("editor.edited"))
                }));
            let path = chapter.path.clone();
            list = list.child(
                list_row(format!("chapter-{}", chapter.path), name, row)
                    .ghost()
                    .selected(is_current)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_chapter(&path, window, cx);
                    })),
            );
        }
        if chapters.is_empty() {
            list = list.child(
                status_line("no-chapters", t!("editor.chapter.none"))
                    .px_3()
                    .py_1()
                    .text_sm()
                    .text_color(theme.muted_foreground),
            );
        }
        list.into_any_element()
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
        if section != Section::Chapters {
            self.search.hide();
        }
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
