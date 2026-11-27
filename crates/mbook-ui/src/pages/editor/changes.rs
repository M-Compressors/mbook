//! What the editor does when an agent changes the book it shows (mbook-ui
//! docs, "Agents (MCP)", "An agent and a person in one chapter").
//!
//! The session tells which chapters an agent changed. The editor then asks
//! for the outline again, through the same queue as its own commands so that
//! the answer is the newest, and loads the text of the open chapter again
//! when it holds none that it has not sent.

use futures::StreamExt as _;
use gpui_kit::{Context, Task, Window};

use super::EditorPage;
use super::commands::Origin;
use crate::features::edit_book::actor::EditHandle;
use crate::features::edit_book::command::{Change, Command};

/// Listens to the changes agents make, for as long as the page lives.
pub(super) fn listen(
    handle: &EditHandle,
    window: &mut Window,
    cx: &mut Context<'_, EditorPage>,
) -> Task<()> {
    let mut changes = handle.subscribe();
    cx.spawn_in(window, async move |this, cx| {
        while let Some(change) = changes.next().await {
            if this
                .update_in(cx, |this, window, cx| {
                    this.agent_changed(&change, window, cx);
                })
                .is_err()
            {
                return;
            }
        }
    })
}

/// The byte offset of the start of line `line` (from 1) of a text, or its
/// end.
fn line_start(text: &str, line: usize) -> usize {
    text.match_indices('\n')
        .nth(line.saturating_sub(2))
        .filter(|_| line > 1)
        .map_or(if line > 1 { text.len() } else { 0 }, |(at, _)| at + 1)
}

impl EditorPage {
    /// An agent changed the book.
    pub(super) fn agent_changed(
        &mut self,
        change: &Change,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.send(Command::Load, Origin::Agent, window, cx);
        let Some(open) = self.loaded.clone() else {
            return;
        };
        let Some((_, seen)) = change.chapters.iter().find(|(path, _)| *path == open) else {
            return;
        };
        if self.session.holds_text() || self.session.draft(&open).is_some() {
            // The text the person typed stays; sending it is refused once, as
            // a conflict, and typing on replaces the agent's text.
            return;
        }
        self.seen = *seen;
        // The caret stays on its line.
        let (cursor, text) = {
            let state = self.text.read(cx);
            (state.cursor(), state.value())
        };
        let line = text[..cursor.min(text.len())].matches('\n').count() + 1;
        self.caret_line = Some(line);
        self.loaded = None;
        self.load_open_text(window, cx);
    }

    /// Learns the last change an agent made to a chapter, after the book
    /// refused text the editor sent over it.
    pub(super) fn see_agent(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let reply = self.handle.editor_text(path);
        let path = path.to_owned();
        self.agent_seen = Some(cx.spawn_in(window, async move |this, cx| {
            if let Ok((_, seen)) = reply.await {
                drop(this.update(cx, |this, _| {
                    if this.loaded.as_deref() == Some(path.as_str()) {
                        this.seen = seen;
                    }
                }));
            }
        }));
    }

    /// Where the caret goes in a text that was loaded again: the start of
    /// the line it was on.
    pub(super) fn caret_after_reload(&mut self, text: &str) -> Option<usize> {
        self.caret_line.take().map(|line| line_start(text, line))
    }
}

#[cfg(test)]
mod tests {
    use super::line_start;

    #[test]
    fn a_line_is_found_by_its_number_or_the_end_is_given() {
        let text = "a\nbb\nc";
        assert_eq!(line_start(text, 1), 0);
        assert_eq!(line_start(text, 2), 2);
        assert_eq!(line_start(text, 3), 5);
        assert_eq!(line_start(text, 9), text.len());
        assert_eq!(line_start(text, 0), 0);
    }
}
