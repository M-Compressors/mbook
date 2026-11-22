//! Find in the chapter's current draft, in the view the author has open.

use std::ops::Range;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, SharedString, Window};
use markdown_editor::{BlockId, BlockPosition, BlockSelection};
use regex::Regex;
use unicode_segmentation::UnicodeSegmentation as _;

use super::EditorPage;
use super::session::{Level, Section, ViewMode};
use crate::features::search_text::{self, Cursor, RenderedMatch};
use crate::shared::layout::SizeClass;
use crate::shared::ui::search_bar::TYPING_PAUSE;

enum Draft {
    Source(SharedString),
    Visual(Vec<(BlockId, String)>),
    Preview(SharedString),
}

enum MatchPlace {
    Source(Range<usize>),
    Visual(BlockSelection),
    Preview(u32),
}

/// A match in the active chapter view.
pub(super) struct ChapterMatch {
    cursor: Cursor,
    place: MatchPlace,
    excerpt: String,
}

impl EditorPage {
    pub(super) fn show_search(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.session.section != Section::Chapters
            || self.loaded.is_none()
            || (Self::class(window, cx) == SizeClass::Compact
                && self.session.level == Level::Sections)
        {
            cx.propagate();
            return;
        }
        let was_open = self.search.open;
        self.search.show(window, cx);
        if !was_open {
            self.refresh_search(true, cx);
        }
        cx.notify();
    }

    pub(super) fn close_search(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let focus = if self.search.current.is_some() && self.session.mode != ViewMode::Preview {
            self.text_focus(cx)
        } else {
            self.search_button.clone()
        };
        self.search.hide();
        focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn on_search_input(
        &mut self,
        _: Entity<InputState>,
        event: &InputEvent,
        cx: &mut Context<'_, Self>,
    ) {
        match *event {
            InputEvent::Change => self.refresh_search(true, cx),
            InputEvent::PressEnter { shift, .. } => self.step_search(shift, cx),
            _ => {}
        }
    }

    pub(super) fn refresh_search(&mut self, reveal: bool, cx: &mut Context<'_, Self>) {
        if self.search.open {
            self.search_chapter(None, reveal, cx);
        }
    }

    pub(super) fn step_search(&mut self, backward: bool, cx: &mut Context<'_, Self>) {
        if self.search.task.is_none() && self.search.current.is_some() {
            self.search_chapter(Some(backward), true, cx);
        }
    }

    fn search_chapter(&mut self, step: Option<bool>, reveal: bool, cx: &mut Context<'_, Self>) {
        let backward = step.unwrap_or(false);
        let from = self
            .search
            .current
            .as_ref()
            .map(|hit| hit.cursor)
            .filter(|_| step.is_some());
        self.search.reset();
        let query = self.search.input.read(cx).value();
        if query.trim().is_empty() {
            cx.notify();
            return;
        }
        self.search.task = Some(cx.spawn(async move |this, cx| {
            if step.is_none() {
                cx.background_executor().timer(TYPING_PAUSE).await;
            }
            let Ok(Some(draft)) = this.update(cx, Self::search_draft) else {
                return;
            };
            let result = cx
                .background_spawn(async move {
                    let pattern = search_text::pattern(query.trim())?;
                    Ok::<_, regex::Error>(
                        draft
                            .find(&pattern, from, backward)
                            .or_else(|| from.and_then(|_| draft.find(&pattern, None, backward))),
                    )
                })
                .await;
            this.update(cx, |this, cx| this.search_finished(result, reveal, cx))
                .ok();
        }));
        cx.notify();
    }

    fn search_draft(&mut self, cx: &mut Context<'_, Self>) -> Option<Draft> {
        let visual = self.visual.input.read(cx);
        Some(match self.session.mode {
            ViewMode::Source | ViewMode::Split => Draft::Source(self.text.read(cx).value()),
            ViewMode::Preview => Draft::Preview(self.text.read(cx).value()),
            ViewMode::Visual if visual.is_readonly() => {
                self.search
                    .finish(None, "Open Source to search this chapter");
                cx.notify();
                return None;
            }
            ViewMode::Visual => Draft::Visual(
                visual
                    .model()
                    .document()
                    .leaves()
                    .filter_map(|block| Some((block.id(), block.text()?.to_owned())))
                    .collect(),
            ),
        })
    }

    fn search_finished(
        &mut self,
        result: Result<Option<ChapterMatch>, regex::Error>,
        reveal: bool,
        cx: &mut Context<'_, Self>,
    ) {
        match result {
            Ok(Some(hit)) => {
                if reveal {
                    self.reveal_search_match(&hit.place, cx);
                }
                let feedback = hit.excerpt.clone();
                self.search.finish(Some(hit), feedback);
            }
            Ok(None) => self.search.finish(None, "No matches"),
            Err(_) => self.search.finish(None, "This search is too long"),
        }
        cx.notify();
    }

    fn reveal_search_match(&self, place: &MatchPlace, cx: &mut Context<'_, Self>) {
        match place {
            MatchPlace::Source(range) => self.text.update(cx, |state, cx| {
                state.set_selected_range(range.clone(), cx);
            }),
            MatchPlace::Visual(selection) => self.visual.input.update(cx, |state, cx| {
                let _ = state.set_selection(selection.clone(), cx);
            }),
            MatchPlace::Preview(offset) => {
                self.preview.update(cx, |view, cx| view.reveal(*offset, cx));
            }
        }
    }
}

impl Draft {
    fn find(&self, pattern: &Regex, from: Option<Cursor>, backward: bool) -> Option<ChapterMatch> {
        match self {
            Self::Source(text) => {
                let range = search_text::in_block(pattern, text, 0, from, backward)?;
                Some(ChapterMatch {
                    cursor: Cursor(0, range.start),
                    excerpt: search_text::excerpt(text, range.clone()),
                    place: MatchPlace::Source(range),
                })
            }
            Self::Preview(text) => {
                let RenderedMatch {
                    cursor,
                    offset,
                    excerpt,
                    ..
                } = search_text::rendered(text, pattern, from, backward)?;
                Some(ChapterMatch {
                    cursor,
                    excerpt,
                    place: MatchPlace::Preview(offset),
                })
            }
            Self::Visual(leaves) => {
                let mut blocks = leaves.iter().enumerate();
                let find = |(index, (id, text)): (usize, &(BlockId, String))| {
                    let range = search_text::in_block(pattern, text, index, from, backward)?;
                    let head = text
                        .grapheme_indices(true)
                        .map(|(at, _)| at)
                        .find(|&at| at >= range.end)
                        .unwrap_or(text.len());
                    let anchor = text[..head]
                        .grapheme_indices(true)
                        .map(|(at, _)| at)
                        .take_while(|&at| at <= range.start)
                        .last()
                        .unwrap_or(0);
                    Some(ChapterMatch {
                        cursor: Cursor(index, range.start),
                        excerpt: search_text::excerpt(text, range),
                        place: MatchPlace::Visual(BlockSelection::Text {
                            anchor: BlockPosition::new(*id, anchor),
                            head: BlockPosition::new(*id, head),
                        }),
                    })
                };
                if backward {
                    blocks.rev().find_map(find)
                } else {
                    blocks.find_map(find)
                }
            }
        }
    }
}
