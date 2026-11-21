//! The contents of the book: a list of its entries, the current one marked.

use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable as _, h_flex, v_flex};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity, Window, div, px,
};

use super::ReaderPage;
use crate::entities::book::snapshot::BookSnapshot;
use crate::shared::ui::target_size;

/// How far an entry is indented for each level below the first, in pixels.
const INDENT: f32 = 14.0;

/// One line of the contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Row {
    /// Stable while the entry means the same thing: the path of its chapter
    /// and its offset.
    pub(super) id: String,
    pub(super) title: String,
    pub(super) level: u8,
    pub(super) current: bool,
}

impl Row {
    /// What assistive technology hears: the title, and that it is the place
    /// being read.
    pub(super) fn name(&self) -> String {
        if self.current {
            format!("{}, current chapter", self.title)
        } else {
            self.title.clone()
        }
    }
}

/// The lines of the contents of `snapshot`, the entry `current` marked.
pub(super) fn rows(snapshot: &BookSnapshot, current: Option<usize>) -> Vec<Row> {
    snapshot
        .contents
        .iter()
        .enumerate()
        .map(|(index, entry)| Row {
            id: format!(
                "contents-{}-{}",
                snapshot
                    .chapters
                    .get(entry.chapter)
                    .map_or("", |info| info.path.as_str()),
                entry.offset
            ),
            title: if entry.title.trim().is_empty() {
                format!("Chapter {}", entry.chapter + 1)
            } else {
                entry.title.trim().to_owned()
            },
            level: entry.level,
            current: current == Some(index),
        })
        .collect()
}

/// The indent of a line, in pixels.
pub(super) fn indent(level: u8) -> f32 {
    f32::from(level.saturating_sub(1).min(6)) * INDENT
}

/// The list of entries, shown in a popover or in a sheet.
pub(super) struct ContentsPanel {
    page: WeakEntity<ReaderPage>,
    snapshot: Option<Arc<BookSnapshot>>,
    current: Option<usize>,
}

impl ContentsPanel {
    /// A panel with nothing to list yet.
    pub(super) fn new(page: WeakEntity<ReaderPage>) -> Self {
        Self {
            page,
            snapshot: None,
            current: None,
        }
    }

    /// Lists `snapshot` and marks the entry `current`.
    pub(super) fn show(
        &mut self,
        snapshot: Option<Arc<BookSnapshot>>,
        current: Option<usize>,
        cx: &mut Context<'_, Self>,
    ) {
        if self.current != current || self.snapshot.as_deref() != snapshot.as_deref() {
            self.snapshot = snapshot;
            self.current = current;
            cx.notify();
        }
    }
}

impl Render for ContentsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl gpui_kit::IntoElement {
        let rows = self
            .snapshot
            .as_ref()
            .map_or_else(Vec::new, |snapshot| rows(snapshot, self.current));
        let theme = cx.theme();
        let list = if rows.is_empty() {
            div()
                .id("contents-empty")
                .role(Role::Status)
                .aria_label("This book has no contents")
                .test_support()
                .p_3()
                .text_color(theme.muted_foreground)
                .child("This book has no contents")
                .into_any_element()
        } else {
            v_flex()
                .id("contents-list")
                .role(Role::List)
                .aria_label("Contents")
                .test_support()
                .gap_0p5()
                .children(rows.into_iter().enumerate().map(|(index, row)| {
                    let page = self.page.clone();
                    let level = row.level;
                    let label: SharedString = row.title.clone().into();
                    // A button centres its content and ignores `justify_start`,
                    // so the line is a child that fills the button: the mark,
                    // then the title, indented by its level.
                    let mut line = h_flex().w_full().gap_2().pl(px(indent(level)));
                    if row.current {
                        line = line.child(Icon::new(IconName::Check));
                    }
                    let button = Button::new(row.id.clone())
                        .ghost()
                        .accessibility_label(row.name())
                        .min_h(px(target_size()))
                        .w_full()
                        .cursor_pointer()
                        .selected(row.current)
                        .on_click(move |_, window, cx| {
                            page.update(cx, |page, cx| page.open_entry(index, window, cx))
                                .ok();
                        })
                        .child(line.child(label));
                    div()
                        .id(SharedString::from(format!("{}-row", row.id)))
                        .role(Role::ListItem)
                        .test_support()
                        .child(button)
                }))
                .into_any_element()
        };
        v_flex()
            .id("contents-panel")
            .role(Role::Group)
            .aria_label("Contents")
            .test_support()
            .w_full()
            .child(
                v_flex()
                    .id("contents-scroll")
                    .w_full()
                    .max_h(px(420.0))
                    .overflow_y_scrollbar()
                    .child(list),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::book::snapshot::{ChapterInfo, ContentsEntry};

    fn snapshot() -> BookSnapshot {
        let entry = |chapter: usize, offset: u32, level: u8, title: &str| ContentsEntry {
            level,
            title: title.to_owned(),
            chapter,
            offset,
        };
        BookSnapshot {
            title: "T".to_owned(),
            authors: Vec::new(),
            language: "en".to_owned(),
            chapters: vec![
                ChapterInfo {
                    path: "a.md".to_owned(),
                    decoded_len: 100,
                },
                ChapterInfo {
                    path: "b.md".to_owned(),
                    decoded_len: 100,
                },
            ],
            contents: vec![
                entry(0, 0, 1, "One"),
                entry(0, 40, 2, "One, part two"),
                entry(1, 0, 1, "  "),
            ],
            cover: None,
        }
    }

    #[test]
    fn every_entry_of_the_book_is_a_line() {
        let titles: Vec<_> = rows(&snapshot(), None)
            .into_iter()
            .map(|row| row.title)
            .collect();
        assert_eq!(titles, ["One", "One, part two", "Chapter 2"]);
    }

    #[test]
    fn the_current_entry_is_marked_and_named_as_such() {
        let rows = rows(&snapshot(), Some(1));
        assert_eq!(
            rows.iter().map(|row| row.current).collect::<Vec<_>>(),
            [false, true, false]
        );
        assert_eq!(rows[1].name(), "One, part two, current chapter");
        assert_eq!(rows[0].name(), "One");
    }

    #[test]
    fn a_line_is_identified_by_its_chapter_and_offset() {
        let rows = rows(&snapshot(), None);
        assert_eq!(rows[0].id, "contents-a.md-0");
        assert_eq!(rows[1].id, "contents-a.md-40");
        assert_eq!(rows[2].id, "contents-b.md-0");
    }

    #[test]
    fn a_book_without_entries_has_no_lines() {
        let mut book = snapshot();
        book.contents.clear();
        assert_eq!(rows(&book, None), Vec::new());
    }

    #[test]
    fn a_deeper_entry_is_indented_further() {
        assert!(indent(1).abs() < f32::EPSILON);
        assert!(indent(2) > indent(1));
        assert!(indent(3) > indent(2));
        assert!((indent(0)).abs() < f32::EPSILON);
        assert!((indent(200) - indent(7)).abs() < f32::EPSILON);
    }
}
