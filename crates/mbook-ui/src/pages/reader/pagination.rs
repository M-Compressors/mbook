//! Whole-book numbering from the chapters measured for the current layout.

use std::collections::HashMap;

use gpui_kit::{Pixels, SharedString};
use rust_i18n::t;

use crate::entities::book::snapshot::BookSnapshot;

/// The dimensions and typography that determine a chapter's pagination.
#[derive(PartialEq)]
pub(super) struct Layout {
    /// Width of the text column in pixels.
    pub(super) column: f32,
    /// Height available to the chapter in pixels.
    pub(super) height: f32,
    /// Reading text size in pixels.
    pub(super) text_size: f32,
    /// Ratio of line height to text size.
    pub(super) line_height: f32,
    /// The size of a rem, which affects block spacing.
    pub(super) rem: Pixels,
    /// The reading typeface actually used.
    pub(super) font: SharedString,
    pub(super) paragraph_gap: f32,
    pub(super) heavier: bool,
}

impl Layout {
    fn estimated_bytes_per_page(&self) -> u128 {
        // An ordinary line averages just over half an em per character. Leave
        // part of the page free for paragraph gaps and headings. A measured
        // multi-page chapter supersedes this estimate.
        let columns = self.column / (self.text_size * 0.55);
        let lines = self.height / (self.text_size * self.line_height);
        let bytes = (columns * lines * 0.8).clamp(1.0, 1_000_000.0).round();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the rounded estimate is clamped to positive values"
        )]
        let bytes = bytes as u128;
        bytes.max(1)
    }
}

/// The physical counts already measured by the chapter view.
#[derive(Default)]
pub(super) struct BookPages {
    layout: Option<Layout>,
    counts: HashMap<usize, usize>,
}

impl BookPages {
    /// Discards measurements when a different layout could change the breaks.
    pub(super) fn set_layout(&mut self, layout: Layout) {
        if self.layout.as_ref() != Some(&layout) {
            self.counts.clear();
            self.layout = Some(layout);
        }
    }

    /// Saves a count after this chapter has been laid out.
    pub(super) fn record(&mut self, chapter: usize, count: usize) {
        self.counts.insert(chapter, count);
    }

    /// Formats the current page and total across the whole book.
    pub(super) fn label(
        &self,
        snapshot: &BookSnapshot,
        chapter: usize,
        (page, count): (usize, usize),
    ) -> String {
        let count_for = |index| {
            if index == chapter {
                Some(count)
            } else {
                self.counts.get(&index).copied()
            }
        };
        let (bytes, pages) = snapshot.chapters.iter().enumerate().fold(
            (0_u128, 0_u128),
            |(bytes, pages), (index, info)| match count_for(index) {
                Some(count) if count > 1 && info.decoded_len > 0 => {
                    (bytes + u128::from(info.decoded_len), pages + count as u128)
                }
                _ => (bytes, pages),
            },
        );
        let typical = if pages == 0 {
            self.layout
                .as_ref()
                .map_or(1800, Layout::estimated_bytes_per_page)
        } else {
            bytes.div_ceil(pages)
        };
        let (mut current, mut total, mut estimated) = (page as u128, 0_u128, false);
        for (index, info) in snapshot.chapters.iter().enumerate() {
            let pages = count_for(index).map_or_else(
                || {
                    estimated = true;
                    u128::from(info.decoded_len).div_ceil(typical).max(1)
                },
                |count| count as u128,
            );
            if index < chapter {
                current += pages;
            }
            total += pages;
        }
        if estimated {
            t!("reader.page_estimate", current = current, total = total).into_owned()
        } else {
            t!("reader.page_of", current = current, total = total).into_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::book::snapshot::ChapterInfo;
    use gpui_kit::px;

    fn book() -> BookSnapshot {
        BookSnapshot {
            title: String::new(),
            authors: Vec::new(),
            language: String::new(),
            chapters: [1000, 4000, 0]
                .into_iter()
                .enumerate()
                .map(|(index, decoded_len)| ChapterInfo {
                    path: format!("{index}.md"),
                    decoded_len,
                })
                .collect(),
            contents: Vec::new(),
            landmarks: Vec::new(),
            cover: None,
        }
    }

    fn layout(column: f32) -> Layout {
        Layout {
            column,
            height: 500.0,
            text_size: 20.0,
            line_height: 1.6,
            rem: px(16.0),
            font: "Literata".into(),
            paragraph_gap: 0.7,
            heavier: false,
        }
    }

    #[test]
    fn unknown_chapters_are_estimated_by_the_measured_density() {
        let pages = BookPages::default();
        assert_eq!(pages.label(&book(), 1, (2, 4)), "≈ 3 of 6");
    }

    #[test]
    fn a_short_first_chapter_does_not_extrapolate_its_own_density() {
        let mut pages = BookPages::default();
        pages.set_layout(layout(700.0));
        assert_eq!(pages.label(&book(), 0, (1, 1)), "≈ 1 of 8");
    }

    #[test]
    fn all_measured_chapters_give_an_exact_book_page_number() {
        let mut pages = BookPages::default();
        pages.record(0, 2);
        pages.record(1, 4);
        assert_eq!(pages.label(&book(), 2, (1, 1)), "7 of 7");
    }

    #[test]
    fn changing_layout_discards_the_old_chapter_counts() {
        let mut pages = BookPages::default();
        pages.set_layout(layout(700.0));
        pages.record(0, 2);
        pages.record(2, 1);
        pages.set_layout(layout(700.0));
        assert_eq!(pages.label(&book(), 1, (1, 4)), "3 of 7");
        pages.set_layout(layout(500.0));
        assert_eq!(pages.label(&book(), 1, (1, 4)), "≈ 2 of 6");
    }
}
