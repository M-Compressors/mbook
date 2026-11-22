//! Book-wide find, one chapter frame at a time.

use std::sync::Arc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Window};

use super::{Phase, ReaderPage, controls};
use crate::entities::book::error::BookError;
use crate::features::search_text::{self, RenderedMatch};
use crate::shared::ui::search_bar::TYPING_PAUSE;

/// A match and the chapter that contains it.
#[derive(Clone, Debug)]
pub(super) struct BookMatch {
    pub(super) chapter: usize,
    pub(super) found: RenderedMatch,
}

impl ReaderPage {
    pub(super) fn show_search(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if !matches!(&self.phase, Phase::Ready(reading) if reading.source.is_some()) {
            cx.propagate();
            return;
        }
        self.close_panel(window, cx);
        self.touch(cx);
        let was_open = self.search.open;
        self.search.show(window, cx);
        if !was_open {
            self.search_origin = Some(self.here(cx));
            self.search_book(false, false, cx);
        }
        cx.notify();
    }

    pub(super) fn close_search(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.search.hide();
        self.view.update(cx, |view, cx| view.set_search(None, cx));
        if let Some(origin) = self.search_origin.take()
            && origin != self.here(cx)
        {
            self.goto(origin, cx);
        }
        self.touch(cx);
        self.handles.search.focus(window, cx);
        cx.notify();
    }

    pub(super) fn on_search_input(
        &mut self,
        _: Entity<InputState>,
        event: &InputEvent,
        cx: &mut Context<'_, Self>,
    ) {
        match *event {
            InputEvent::Change => self.search_book(false, false, cx),
            InputEvent::PressEnter { shift, .. } => self.step_search(shift, cx),
            _ => {}
        }
    }

    pub(super) fn step_search(&mut self, backward: bool, cx: &mut Context<'_, Self>) {
        if self.search.task.is_none() && self.search.current.is_some() {
            self.search_book(true, backward, cx);
        }
    }

    fn search_book(&mut self, step: bool, backward: bool, cx: &mut Context<'_, Self>) {
        if !self.search.open {
            return;
        }
        let Phase::Ready(reading) = &self.phase else {
            return;
        };
        let Some(source) = reading.source.as_ref().map(Arc::downgrade) else {
            return;
        };
        let from = self.search.current.as_ref().filter(|_| step);
        let start = from.map_or(reading.chapter, |hit| hit.chapter);
        let cursor = from.map(|hit| hit.found.cursor);
        let count = reading.snapshot.chapters.len();
        self.search.reset();
        self.view.update(cx, |view, cx| view.set_search(None, cx));
        let query = self.search.input.read(cx).value();
        if query.trim().is_empty() {
            cx.notify();
            return;
        }
        self.search.task = Some(cx.spawn(async move |this, cx| {
            if !step {
                cx.background_executor().timer(TYPING_PAUSE).await;
            }
            let compiled = cx
                .background_spawn(async move { search_text::pattern(query.trim()) })
                .await;
            let Ok(pattern) = compiled else {
                this.update(cx, |this, cx| {
                    this.search_feedback(None, "This search is too long", cx);
                })
                .ok();
                return;
            };
            let mut failure: Option<BookError> = None;
            // After a full circuit, search the other side of the starting match.
            for step in 0..count + usize::from(cursor.is_some()) {
                let chapter = if backward {
                    (start + count - step) % count
                } else {
                    (start + step) % count
                };
                let boundary = cursor.filter(|_| step == 0);
                let pattern = pattern.clone();
                let source = source.clone();
                let result = cx
                    .background_spawn(async move {
                        let text = source
                            .upgrade()
                            .ok_or(BookError::Unusable)?
                            .read_chapter(chapter)?;
                        Ok(search_text::rendered(&text, &pattern, boundary, backward))
                    })
                    .await;
                match result {
                    Ok(Some(found)) => {
                        this.update(cx, |this, cx| {
                            this.show_match(BookMatch { chapter, found }, cx);
                        })
                        .ok();
                        return;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        failure = Some(error);
                    }
                }
            }
            let feedback = failure.map_or_else(
                || "No matches".to_owned(),
                |error| format!("Search incomplete. {}", super::describe(&error)),
            );
            this.update(cx, |this, cx| this.search_feedback(None, feedback, cx))
                .ok();
        }));
        cx.notify();
    }

    fn show_match(&mut self, hit: BookMatch, cx: &mut Context<'_, Self>) {
        let Phase::Ready(reading) = &self.phase else {
            return;
        };
        let offset = hit.found.offset;
        let title = controls::chapter_title(&reading.snapshot, hit.chapter, offset);
        let feedback = format!("{} · {}", title, hit.found.excerpt);
        let same_chapter = reading.chapter == hit.chapter && !reading.loading;
        let spot = super::Spot {
            chapter: hit.chapter,
            offset,
        };
        if same_chapter {
            self.view.update(cx, |view, cx| {
                view.set_search(Some(hit.found.source.clone()), cx);
            });
        }
        self.search_feedback(Some(hit), feedback, cx);
        self.goto(spot, cx);
    }

    fn search_feedback(
        &mut self,
        hit: Option<BookMatch>,
        feedback: impl Into<gpui_kit::SharedString>,
        cx: &mut Context<'_, Self>,
    ) {
        self.search.finish(hit, feedback);
        cx.notify();
    }
}
