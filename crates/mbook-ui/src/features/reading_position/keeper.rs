//! Writing the reading state, not more than once a second.

use std::time::Duration;

use gpui_kit::{App, AppContext as _, Context, Entity, Subscription, Task};

use crate::entities::catalogue::{Library, ReadingState};
use crate::shared::model::BookKey;

/// How long a state waits before it is written.
pub(crate) const WRITE_EVERY: Duration = Duration::from_secs(1);

/// Writes the state of one book to the library, which keeps it in memory at
/// once and stores it on the background.
///
/// [`Self::note`] remembers the newest state and writes it when the second is
/// up; [`Self::flush`] writes it now. Dropping the keeper writes what is left.
pub(crate) struct PositionKeeper {
    key: BookKey,
    pending: Option<ReadingState>,
    timer: Option<Task<()>>,
    writes: usize,
    _release: Subscription,
}

impl PositionKeeper {
    /// A keeper for the book `key`.
    pub(crate) fn new(key: BookKey, cx: &mut Context<'_, Self>) -> Self {
        let release = cx.on_release(Self::write);
        Self {
            key,
            pending: None,
            timer: None,
            writes: 0,
            _release: release,
        }
    }

    /// An entity that keeps the place of `key`.
    pub(crate) fn entity(key: BookKey, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(key, cx))
    }

    /// Remembers `state`; it is written within a second.
    pub(crate) fn note(&mut self, state: ReadingState, cx: &mut Context<'_, Self>) {
        self.pending = Some(state);
        if self.timer.is_some() {
            return;
        }
        self.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(WRITE_EVERY).await;
            this.update(cx, |this, cx| {
                this.timer = None;
                this.write(cx);
            })
            .ok();
        }));
    }

    /// Writes what is remembered, now.
    pub(crate) fn flush(&mut self, cx: &mut Context<'_, Self>) {
        self.timer = None;
        self.write(cx);
    }

    /// Whether a state waits to be written.
    #[cfg(test)]
    pub(crate) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// How many states were handed to the library.
    #[cfg(test)]
    pub(crate) fn writes(&self) -> usize {
        self.writes
    }

    fn write(&mut self, cx: &mut App) {
        let Some(state) = self.pending.take() else {
            return;
        };
        self.writes += 1;
        let key = self.key;
        Library::global(cx).update(cx, |library, cx| library.set_state(key, state, cx));
    }
}
