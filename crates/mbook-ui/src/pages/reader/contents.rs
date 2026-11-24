//! The contents of the book: a list of its entries, the current one marked,
//! and above it the landmarks of the book, the places a reader goes to
//! directly.

use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable as _, h_flex, v_flex};
use gpui_kit::{
    Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity,
    Window, div, px,
};
use rust_i18n::t;

use super::ReaderPage;
use crate::entities::book::landmark::display_name;
use crate::entities::book::snapshot::BookSnapshot;
use crate::shared::ui::target_size;

/// How far an entry is indented for each level below the first, in pixels.
const INDENT: f32 = 14.0;

/// The words over the landmarks.
fn go_to() -> SharedString {
    t!("reader.go_to").into()
}

/// The words over the entries, when landmarks are listed above them.
fn contents() -> SharedString {
    t!("reader.contents").into()
}

/// What the panel says when the book has no entries.
fn no_contents() -> SharedString {
    t!("reader.no_contents").into()
}

/// One landmark of the list: where the reader is taken, by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Mark {
    /// The id of its control, by its position in the list of the book.
    pub(super) id: String,
    /// The title the book gives it, or the name of its kind.
    pub(super) name: String,
}

/// The landmarks of `snapshot`, in the order the book lists them.
pub(super) fn marks(snapshot: &BookSnapshot) -> Vec<Mark> {
    snapshot
        .landmarks
        .iter()
        .enumerate()
        .map(|(index, landmark)| Mark {
            id: format!("landmark-{index}"),
            name: display_name(&landmark.kind, &landmark.title),
        })
        .collect()
}

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
            t!("reader.current_chapter", title = self.title).into_owned()
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
                t!("reader.chapter_number", number = entry.chapter + 1).into_owned()
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

impl ContentsPanel {
    /// The words over a list, when the panel shows two.
    fn heading(
        id: &'static str,
        words: impl Into<SharedString>,
        cx: &Context<'_, Self>,
    ) -> impl IntoElement {
        div()
            .id(id)
            .role(Role::Heading)
            .test_support()
            .px_3()
            .pt_2()
            .pb_1()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .text_color(cx.theme().muted_foreground)
            .child(words.into())
    }

    /// A control that fills the width of the panel, with `line` inside.
    fn line_button(
        id: String,
        name: String,
        selected: bool,
        line: impl IntoElement,
        on_click: impl Fn(&mut Window, &mut gpui_kit::App) + 'static,
    ) -> impl IntoElement {
        let row = SharedString::from(format!("{id}-row"));
        let button = Button::new(SharedString::from(id))
            .ghost()
            .accessibility_label(name)
            .min_h(px(target_size()))
            .w_full()
            .cursor_pointer()
            .selected(selected)
            .on_click(move |_, window, cx| on_click(window, cx))
            .child(line);
        div()
            .id(row)
            .role(Role::ListItem)
            .test_support()
            .child(button)
    }

    /// The landmarks of the book; `None` when it has none.
    fn landmarks(&self, snapshot: &BookSnapshot) -> Option<impl IntoElement + use<>> {
        let marks = marks(snapshot);
        if marks.is_empty() {
            return None;
        }
        let rows = marks.into_iter().enumerate().map(|(index, mark)| {
            let page = self.page.clone();
            let line = h_flex().w_full().child(mark.name.clone());
            Self::line_button(mark.id, mark.name, false, line, move |window, cx| {
                page.update(cx, |page, cx| page.open_landmark(index, window, cx))
                    .ok();
            })
        });
        Some(
            v_flex()
                .id("landmarks-list")
                .role(Role::List)
                .aria_label(go_to())
                .test_support()
                .gap_0p5()
                .children(rows),
        )
    }

    /// The entries of the contents, or the words that say there are none.
    fn entries(&self, rows: Vec<Row>, cx: &Context<'_, Self>) -> gpui_kit::AnyElement {
        if rows.is_empty() {
            return div()
                .id("contents-empty")
                .role(Role::Status)
                .aria_label(no_contents())
                .test_support()
                .p_3()
                .text_color(cx.theme().muted_foreground)
                .child(no_contents())
                .into_any_element();
        }
        v_flex()
            .id("contents-list")
            .role(Role::List)
            .aria_label(contents())
            .test_support()
            .gap_0p5()
            .children(rows.into_iter().enumerate().map(|(index, row)| {
                let page = self.page.clone();
                let label: SharedString = row.title.clone().into();
                // A button centres its content and ignores `justify_start`,
                // so the line is a child that fills the button: the mark,
                // then the title, indented by its level.
                let mut line = h_flex().w_full().gap_2().pl(px(indent(row.level)));
                if row.current {
                    line = line.child(Icon::new(IconName::Check));
                }
                let name = row.name();
                Self::line_button(
                    row.id,
                    name,
                    row.current,
                    line.child(label),
                    move |window, cx| {
                        page.update(cx, |page, cx| page.open_entry(index, window, cx))
                            .ok();
                    },
                )
            }))
            .into_any_element()
    }
}

impl Render for ContentsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl gpui_kit::IntoElement {
        let rows = self
            .snapshot
            .as_ref()
            .map_or_else(Vec::new, |snapshot| rows(snapshot, self.current));
        let landmarks = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| self.landmarks(snapshot));
        let entries = self.entries(rows, cx);
        // The headings tell the two lists apart; a book without landmarks
        // shows its contents alone, under the title of the panel.
        let list = match landmarks {
            Some(landmarks) => v_flex()
                .child(Self::heading("landmarks-heading", go_to(), cx))
                .child(landmarks)
                .child(div().mx_3().my_2().h(px(1.0)).bg(cx.theme().border))
                .child(Self::heading("contents-heading", contents(), cx))
                .child(entries),
            None => v_flex().child(entries),
        };
        v_flex()
            .id("contents-panel")
            .role(Role::Group)
            .aria_label(contents())
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
    use crate::entities::book::landmark::start_name;
    use crate::entities::book::snapshot::{ChapterInfo, ContentsEntry, LandmarkPlace};

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
            landmarks: Vec::new(),
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

    #[test]
    fn a_landmark_is_listed_by_its_title_or_the_name_of_its_kind() {
        let mut book = snapshot();
        let landmark = |kind: &str, title: &str| LandmarkPlace {
            kind: kind.to_owned(),
            title: title.to_owned(),
            chapter: 0,
            offset: 0,
        };
        book.landmarks = vec![
            landmark("start", ""),
            landmark("notes", "Endnotes"),
            landmark("x-recipes", ""),
        ];
        assert_eq!(
            marks(&book),
            [
                Mark {
                    id: "landmark-0".to_owned(),
                    name: start_name().into_owned()
                },
                Mark {
                    id: "landmark-1".to_owned(),
                    name: "Endnotes".to_owned()
                },
                Mark {
                    id: "landmark-2".to_owned(),
                    name: "x-recipes".to_owned()
                },
            ]
        );
    }

    #[test]
    fn a_book_without_landmarks_lists_none() {
        assert_eq!(marks(&snapshot()), Vec::new());
    }
}
