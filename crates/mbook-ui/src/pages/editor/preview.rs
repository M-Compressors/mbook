//! The preview of the chapter being edited.
//!
//! It is the reader's own view of a chapter,
//! [`ChapterView`]: the Markdown
//! view of GPUI Kit set in the typeface, size, line height and page tone of
//! the settings, so that the preview and the reader are the same view with the
//! same style (RFC 0004, "Editing").
//!
//! Pictures of the book are not drawn in this version: the view is given no
//! source for them, so a picture shows its alternative text. Links do
//! nothing.

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
