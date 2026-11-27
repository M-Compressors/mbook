//! The preview of the chapter being edited.
//!
//! It is the reader's own view of a chapter,
//! [`ChapterView`]: the Markdown
//! view of GPUI Kit set in the typeface, size, line height and page tone of
//! the settings, so that the preview and the reader are the same view with the
//! same style (mbook-ui docs, "Editing").
//!
//! Diagrams and formulas come from the editing session; Visual is given the
//! same ones. Pictures show their alternative text. Links do nothing.

use std::time::Duration;

use super::EditorPage;
use crate::entities::figure::FigureSources;
use gpui_kit::{AppContext as _, Context, Entity, Window};

use crate::widgets::chapter_view::ChapterView;

/// A view for the preview of a chapter: empty, and not following links.
pub(super) fn new_preview<T: 'static>(
    window: &mut Window,
    cx: &mut Context<'_, T>,
) -> Entity<ChapterView> {
    cx.new(|cx| {
        let mut view = ChapterView::new(window, cx);
        view.on_link(|_, _, _| {});
        view
    })
}

impl EditorPage {
    /// Shows the draft immediately and resolves its figures after a short
    /// typing pause. A newer draft drops the older request.
    pub(super) fn update_preview(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.preview_figures = None;
        let chapter = self
            .loaded
            .clone()
            .filter(|_| !self.session.figures().is_empty() && text.contains("!["));
        self.preview.update(cx, |view, cx| {
            if chapter.is_none() {
                view.set_figures(FigureSources::default(), cx);
            }
            view.set_text(text, cx);
        });
        if chapter.is_none() {
            self.set_visual_figures(FigureSources::default(), text, cx);
        }
        if let Some(chapter) = chapter {
            let handle = self.handle.clone();
            let draft = text.to_owned();
            let text = text.to_owned();
            self.preview_figures = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                let sources = handle
                    .chapter_figures(chapter, text)
                    .await
                    .unwrap_or_default();
                this.update(cx, |this, cx| {
                    this.set_visual_figures(sources.clone(), &draft, cx);
                    this.preview
                        .update(cx, |view, cx| view.set_figures(sources, cx));
                })
                .ok();
            }));
        }
    }
}
