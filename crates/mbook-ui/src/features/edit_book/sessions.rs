//! The editing sessions of the application: at most one per book, shared by
//! the editor and agents (mbook-ui docs, "Agents (MCP)", "How the server
//! works").
//!
//! A session lives while an editor shows its book, or while an agent used it
//! in the last ten minutes, or while it holds changes an agent made and did
//! not save.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui_kit::{App, Global, Task};

use super::actor::{self, EditHandle};
use super::command::Command;
use crate::entities::catalogue::Store;
use crate::shared::model::BookKey;

/// How long a session without an editor stays after the last call of an
/// agent.
pub(crate) const IDLE: Duration = Duration::from_mins(10);

/// One session and who uses it.
struct Entry {
    handle: EditHandle,
    /// The task that owns the book; dropping it ends the session.
    _actor: Task<()>,
    /// The editors that show the book.
    editors: usize,
    /// The last call of an agent; `None` when no agent used the session.
    agent: Option<Instant>,
    /// Ends the session when it is idle.
    idle: Option<Task<()>>,
    /// An agent's last call left changes that are not saved.
    #[cfg(not(target_os = "android"))]
    unsaved: bool,
}

/// Every editing session, by book.
#[derive(Default)]
pub(crate) struct Sessions {
    open: HashMap<BookKey, Entry>,
}

impl Global for Sessions {}

impl Sessions {
    /// The session of a book, opened when there is none.
    fn entry(key: BookKey, cx: &mut App) -> &mut Entry {
        let path = Store::global(cx).roots.book(key);
        let executor = cx.background_executor().clone();
        cx.default_global::<Self>()
            .open
            .entry(key)
            .or_insert_with(|| {
                let (handle, serving) = actor::session(path);
                Entry {
                    handle,
                    _actor: executor.spawn(serving),
                    editors: 0,
                    agent: None,
                    idle: None,
                    #[cfg(not(target_os = "android"))]
                    unsaved: false,
                }
            })
    }

    /// The session an editor of `key` uses. Each call must be matched by
    /// [`leave`](Self::leave) when the editor goes.
    pub(crate) fn edit(key: BookKey, cx: &mut App) -> EditHandle {
        let entry = Self::entry(key, cx);
        entry.editors += 1;
        entry.handle.clone()
    }

    /// An editor of `key` went. `discard` says that it left unsaved changes
    /// the person chose not to keep: a session an agent still uses reads the
    /// book again from its file. A session no one uses ends.
    pub(crate) fn leave(key: BookKey, discard: bool, cx: &mut App) {
        let Some(sessions) = cx.try_global::<Self>() else {
            return;
        };
        let Some(entry) = sessions.open.get(&key) else {
            return;
        };
        let handle = entry.handle.clone();
        // The editor may have left text it held unsent.
        handle.hold(None);
        let used = entry.agent.is_some();
        let sessions = cx.global_mut::<Self>();
        let Some(entry) = sessions.open.get_mut(&key) else {
            return;
        };
        entry.editors = entry.editors.saturating_sub(1);
        if entry.editors > 0 {
            return;
        }
        if !used {
            sessions.open.remove(&key);
            return;
        }
        if discard {
            cx.background_executor()
                .spawn(async move { drop(handle.discard().await) })
                .detach();
        }
        Self::watch_idle(key, cx);
    }

    /// The session an agent uses for `key`, opened when there is none. The
    /// session stays for [`IDLE`] after the last call.
    #[cfg(not(target_os = "android"))]
    pub(crate) fn agent(key: BookKey, cx: &mut App) -> EditHandle {
        // The clock of the executor, which its timers follow.
        let now = cx.background_executor().now();
        let entry = Self::entry(key, cx);
        entry.agent = Some(now);
        let handle = entry.handle.clone();
        Self::watch_idle(key, cx);
        handle
    }

    /// Records whether the last call of an agent left the book unsaved.
    #[cfg(not(target_os = "android"))]
    pub(crate) fn note(key: BookKey, unsaved: bool, cx: &mut App) {
        if let Some(entry) = cx.default_global::<Self>().open.get_mut(&key) {
            entry.unsaved = unsaved;
        }
    }

    /// The books that only agents edit and that have changes not saved.
    #[cfg(not(target_os = "android"))]
    pub(crate) fn unsaved_of_agents(cx: &App) -> Vec<(BookKey, EditHandle)> {
        cx.try_global::<Self>().map_or_else(Vec::new, |sessions| {
            sessions
                .open
                .iter()
                .filter(|(_, entry)| entry.editors == 0 && entry.unsaved)
                .map(|(key, entry)| (*key, entry.handle.clone()))
                .collect()
        })
    }

    /// Whether an editor shows the book.
    #[cfg(not(target_os = "android"))]
    pub(crate) fn in_editor(key: BookKey, cx: &App) -> bool {
        cx.try_global::<Self>()
            .and_then(|sessions| sessions.open.get(&key))
            .is_some_and(|entry| entry.editors > 0)
    }

    /// Ends the session of a book: its file is about to go.
    #[cfg(not(target_os = "android"))]
    pub(crate) fn forget(key: BookKey, cx: &mut App) {
        cx.default_global::<Self>().open.remove(&key);
    }

    /// Ends the session of `key` once it has been idle for [`IDLE`] without
    /// an editor and without unsaved changes.
    fn watch_idle(key: BookKey, cx: &mut App) {
        let task = cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(IDLE).await;
                let state = cx.update(|cx| {
                    let now = cx.background_executor().now();
                    let entry = cx.try_global::<Self>()?.open.get(&key)?;
                    let since = entry
                        .agent
                        .map_or(IDLE, |at| now.saturating_duration_since(at));
                    Some((entry.editors, since, entry.handle.clone()))
                });
                let Some((editors, since, handle)) = state else {
                    return;
                };
                if editors > 0 {
                    return;
                }
                if since < IDLE {
                    continue;
                }
                // Load answers with the book as it is; a book with unsaved
                // changes waits for a save, a discard or the end.
                if handle
                    .send(Command::Load)
                    .await
                    .is_ok_and(|outline| outline.unsaved)
                {
                    continue;
                }
                cx.update(|cx| {
                    let sessions = cx.default_global::<Self>();
                    if sessions
                        .open
                        .get(&key)
                        .is_some_and(|entry| entry.editors == 0)
                    {
                        sessions.open.remove(&key);
                    }
                });
                return;
            }
        });
        if let Some(entry) = cx.default_global::<Self>().open.get_mut(&key) {
            entry.idle = Some(task);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui_kit::TestAppContext;

    use super::{IDLE, Sessions};
    use crate::features::edit_book::command::{Command, MetaChange};
    use crate::features::edit_book::error::EditError;
    use crate::shared::model::BookKey;
    use crate::testing::Fixture;

    /// A library with one real book in it.
    fn book(cx: &mut TestAppContext) -> (Fixture, BookKey) {
        let fixture = Fixture::new();
        fixture.install(cx);
        let key = BookKey::generate();
        let mut book = mbook::Book::new("T", "en").unwrap();
        book.add_chapter("ch01.md", "# One\n").unwrap();
        book.save(fixture.roots.book(key)).unwrap();
        (fixture, key)
    }

    /// Whether the session behind a handle has ended.
    fn ended(handle: &super::EditHandle, cx: &mut TestAppContext) -> bool {
        let reply = handle.chapter_text("ch01.md");
        cx.run_until_parked();
        matches!(
            cx.foreground_executor().block_test(reply),
            Err(EditError::Ended)
        )
    }

    #[gpui_kit::test]
    fn an_editor_and_an_agent_share_one_session(cx: &mut TestAppContext) {
        let (_fixture, key) = book(cx);
        let editor = cx.update(|cx| Sessions::edit(key, cx));
        let agent = cx.update(|cx| Sessions::agent(key, cx));
        let title = MetaChange::Title("Changed".into());
        let outline = cx
            .foreground_executor()
            .block_test(agent.send(Command::SetMeta(title)))
            .unwrap();
        assert_eq!(outline.meta.title, "Changed");
        let seen = cx
            .foreground_executor()
            .block_test(editor.send(Command::Load))
            .unwrap();
        assert_eq!(seen.meta.title, "Changed");
        assert!(cx.update(|cx| Sessions::in_editor(key, cx)));
    }

    #[gpui_kit::test]
    fn a_session_of_an_agent_ends_when_it_has_been_idle(cx: &mut TestAppContext) {
        let (_fixture, key) = book(cx);
        let agent = cx.update(|cx| Sessions::agent(key, cx));
        cx.executor().advance_clock(IDLE / 2);
        cx.run_until_parked();
        assert!(!ended(&agent, cx));
        cx.executor().advance_clock(IDLE + Duration::from_secs(1));
        cx.run_until_parked();
        assert!(ended(&agent, cx));
    }

    #[gpui_kit::test]
    fn an_idle_session_with_unsaved_changes_stays(cx: &mut TestAppContext) {
        let (_fixture, key) = book(cx);
        let agent = cx.update(|cx| Sessions::agent(key, cx));
        let title = MetaChange::Title("Changed".into());
        cx.foreground_executor()
            .block_test(agent.send(Command::SetMeta(title)))
            .unwrap();
        cx.update(|cx| Sessions::note(key, true, cx));
        assert_eq!(cx.update(|cx| Sessions::unsaved_of_agents(cx)).len(), 1);
        cx.executor().advance_clock(IDLE * 2);
        cx.run_until_parked();
        assert!(!ended(&agent, cx));
        cx.update(|cx| Sessions::forget(key, cx));
        assert!(ended(&agent, cx));
        assert!(cx.update(|cx| Sessions::unsaved_of_agents(cx)).is_empty());
    }

    #[gpui_kit::test]
    fn an_editor_that_leaves_unsaved_changes_of_a_shared_session_discards_them(
        cx: &mut TestAppContext,
    ) {
        let (_fixture, key) = book(cx);
        let editor = cx.update(|cx| Sessions::edit(key, cx));
        let agent = cx.update(|cx| Sessions::agent(key, cx));
        let title = MetaChange::Title("Changed".into());
        cx.foreground_executor()
            .block_test(editor.send(Command::SetMeta(title)))
            .unwrap();
        cx.update(|cx| Sessions::leave(key, true, cx));
        cx.run_until_parked();
        let outline = cx
            .foreground_executor()
            .block_test(agent.send(Command::Load))
            .unwrap();
        assert_eq!(outline.meta.title, "T");
        assert!(!outline.unsaved);
        assert!(!cx.update(|cx| Sessions::in_editor(key, cx)));
    }

    #[gpui_kit::test]
    fn leaving_a_book_without_a_session_changes_nothing(cx: &mut TestAppContext) {
        let (_fixture, key) = book(cx);
        cx.update(|cx| Sessions::leave(key, false, cx));
        cx.update(|cx| Sessions::note(key, true, cx));
        assert!(cx.update(|cx| Sessions::unsaved_of_agents(cx)).is_empty());
    }
}
