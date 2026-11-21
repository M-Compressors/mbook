//! The imports that are running or have finished.
//!
//! Files are imported one at a time (RFC 0004, "Threads": a conversion holds
//! its source and its book in memory, and the AVIF encoder uses every core).
//! Each import runs inside a task on the background executor and tells the
//! entity its steps over a channel, so the main thread neither waits nor polls.
//! The entity keeps its task: dropping the queue cancels what has not started,
//! and a conversion that has started runs to its end, as the converter has no
//! cancellation.

use std::collections::VecDeque;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use futures::StreamExt as _;
use futures::channel::mpsc::unbounded;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, AsyncApp, Context, Entity, EventEmitter, Global, Task,
    WeakEntity, Window,
};

use super::{ImportError, Imported, Step, import};
use crate::entities::catalogue::{Catalogue, Library, Store};
use crate::shared::model::BookKey;
use crate::shared::roots::Roots;
use crate::shared::ui::announce;

/// Where one import is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ImportStatus {
    /// Waiting for the imports before it.
    Queued,
    /// Finding out whether the file is an mbook file.
    Probing,
    /// Copying an mbook file.
    Copying,
    /// Converting a file in another format.
    Converting,
    /// Moving the book into the library and recording it.
    Indexing,
    /// The book is in the library.
    Done {
        /// The new book.
        key: BookKey,
        /// What the conversion could not keep, one line each.
        warnings: Vec<String>,
    },
    /// The library had the book already; nothing was added.
    Already {
        /// The book that was there.
        key: BookKey,
    },
    /// The file was not added.
    Failed {
        /// What went wrong, as a sentence for a person.
        message: String,
    },
}

impl ImportStatus {
    /// Whether the import is over, whatever came of it.
    pub(crate) fn is_finished(&self) -> bool {
        matches!(
            self,
            Self::Done { .. } | Self::Already { .. } | Self::Failed { .. }
        )
    }

    /// Whether the import is working now: started and not over.
    pub(crate) fn is_running(&self) -> bool {
        matches!(
            self,
            Self::Probing | Self::Copying | Self::Converting | Self::Indexing
        )
    }

    /// The status in words, as the sidebar says it.
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Queued => "Waiting".to_owned(),
            Self::Probing => "Looking at the file".to_owned(),
            Self::Copying => "Copying".to_owned(),
            Self::Converting => "Converting".to_owned(),
            Self::Indexing => "Adding to the library".to_owned(),
            Self::Done { warnings, .. } => match warnings.len() {
                0 => "Added".to_owned(),
                1 => "Added, with 1 warning".to_owned(),
                count => format!("Added, with {count} warnings"),
            },
            Self::Already { .. } => "Already in the library".to_owned(),
            Self::Failed { message } => message.clone(),
        }
    }

    /// The book the import led to, when it did.
    #[cfg(test)]
    pub(crate) fn book(&self) -> Option<BookKey> {
        match self {
            Self::Done { key, .. } | Self::Already { key } => Some(*key),
            _ => None,
        }
    }

    fn of_step(step: Step) -> Self {
        match step {
            Step::Probing => Self::Probing,
            Step::Copying => Self::Copying,
            Step::Converting => Self::Converting,
            Step::Indexing => Self::Indexing,
        }
    }
}

/// One file on its way into the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportEntry {
    id: u64,
    path: PathBuf,
    name: String,
    title: Option<String>,
    status: ImportStatus,
    /// Whether the book is opened when this import ends well: the file was
    /// the only one that was added.
    open_when_done: bool,
}

impl ImportEntry {
    /// The number that tells this entry from every other the queue has had.
    /// It stays the same while the entry is in the queue.
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    /// The name of the file, which is what the entry is called until the book
    /// has a title.
    #[cfg(test)]
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Where the import is.
    pub(crate) fn status(&self) -> &ImportStatus {
        &self.status
    }

    /// What the entry is called: the title of the book when there is one, else
    /// the name of the file.
    pub(crate) fn display_name(&self) -> &str {
        self.title.as_deref().unwrap_or(&self.name)
    }
}

/// What the queue asks of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportEvent {
    /// The only file that was added is in the library: open its book.
    Opened(BookKey),
}

/// The files that are being added to the library, and those that were.
pub(crate) struct ImportQueue {
    /// The window the results are announced in.
    window: AnyWindowHandle,
    entries: Vec<ImportEntry>,
    next_id: u64,
    waiting: VecDeque<u64>,
    /// Whether the worker is between its first and its last import.
    running: bool,
    /// The task that works through `waiting`. Held so that dropping the queue
    /// cancels what has not started.
    worker: Option<Task<()>>,
}

impl EventEmitter<ImportEvent> for ImportQueue {}

/// The handle of the one [`ImportQueue`] of the application.
struct QueueHandle(Entity<ImportQueue>);

impl Global for QueueHandle {}

/// One import, ready for the background.
struct Job {
    id: u64,
    path: PathBuf,
    roots: Roots,
    catalogue: Arc<Catalogue>,
}

/// What a finished import hands back to the entity.
type Outcome = Result<(Imported, Option<String>), ImportError>;

impl ImportQueue {
    /// An empty queue that announces its results in `window`.
    pub(crate) fn new(window: AnyWindowHandle) -> Self {
        Self {
            window,
            entries: Vec::new(),
            next_id: 0,
            waiting: VecDeque::new(),
            running: false,
            worker: None,
        }
    }

    /// Makes the queue of the application, unless there is one already, and
    /// returns it. `window` is where results are announced.
    pub(crate) fn install(window: &Window, cx: &mut App) -> Entity<Self> {
        if let Some(handle) = cx.try_global::<QueueHandle>() {
            return handle.0.clone();
        }
        let handle = window.window_handle();
        let queue = cx.new(|_| Self::new(handle));
        cx.set_global(QueueHandle(queue.clone()));
        queue
    }

    /// The queue of the application.
    ///
    /// # Panics
    ///
    /// When [`Self::install`] was not called: the library page installs it
    /// when it is created.
    #[cfg(test)]
    pub(crate) fn global(cx: &App) -> Entity<Self> {
        cx.try_global::<QueueHandle>().map_or_else(
            || panic!("the import queue is installed by the library page"),
            |handle| handle.0.clone(),
        )
    }

    /// The queue of the application, when the library page has made it.
    #[cfg(any(feature = "self-update", feature = "visual", test))]
    pub(crate) fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<QueueHandle>()
            .map(|handle| handle.0.clone())
    }

    /// The imports, in the order they were added.
    pub(crate) fn entries(&self) -> &[ImportEntry] {
        &self.entries
    }

    /// Whether nothing is waiting or working. The update installer asks before
    /// it restarts the application.
    #[cfg(any(feature = "self-update", test))]
    pub(crate) fn is_idle(&self) -> bool {
        self.entries.iter().all(|entry| entry.status.is_finished())
    }

    /// Puts entries in the queue as if their imports had come to the given
    /// statuses, so that a picture of the list can show every state without a
    /// conversion that takes a while. Nothing is imported.
    #[cfg(feature = "visual")]
    pub(crate) fn seed(&mut self, entries: Vec<(String, Option<String>, ImportStatus)>) {
        for (name, title, status) in entries {
            let id = self.next_id;
            self.next_id += 1;
            self.entries.push(ImportEntry {
                id,
                path: PathBuf::from(&name),
                name,
                title,
                status,
                open_when_done: false,
            });
        }
    }

    /// How many entries are over.
    pub(crate) fn finished_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.status.is_finished())
            .count()
    }

    /// Adds one entry for each file and starts importing when nothing else is
    /// being imported.
    ///
    /// When there is exactly one file and its import ends in a book, the queue
    /// emits [`ImportEvent::Opened`]; several files are only added.
    pub(crate) fn add(&mut self, paths: Vec<PathBuf>, cx: &mut Context<'_, Self>) {
        if paths.is_empty() {
            return;
        }
        let single = paths.len() == 1;
        for path in paths {
            let id = self.next_id;
            self.next_id += 1;
            let name = path
                .file_name()
                .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy())
                .into_owned();
            self.entries.push(ImportEntry {
                id,
                path,
                name,
                title: None,
                status: ImportStatus::Queued,
                open_when_done: single,
            });
            self.waiting.push_back(id);
        }
        self.start(cx);
        cx.notify();
    }

    /// Removes the finished entry at `index`. An entry that is not over, or no
    /// entry, is left alone.
    pub(crate) fn dismiss(&mut self, index: usize, cx: &mut Context<'_, Self>) {
        if self
            .entries
            .get(index)
            .is_some_and(|entry| entry.status.is_finished())
        {
            self.entries.remove(index);
            cx.notify();
        }
    }

    /// Removes every entry that is over.
    pub(crate) fn clear_finished(&mut self, cx: &mut Context<'_, Self>) {
        let before = self.entries.len();
        self.entries.retain(|entry| !entry.status.is_finished());
        if self.entries.len() != before {
            cx.notify();
        }
    }

    /// Starts the worker, unless it is working.
    fn start(&mut self, cx: &mut Context<'_, Self>) {
        if self.running {
            return;
        }
        self.running = true;
        self.worker = Some(cx.spawn(async move |this, cx| {
            loop {
                let Ok(Some(job)) = this.update(cx, Self::next_job) else {
                    break;
                };
                if !Self::run(job, &this, cx).await {
                    break;
                }
            }
        }));
    }

    /// The next import to run, or `None` (and the worker rests) when there is
    /// none.
    fn next_job(&mut self, cx: &mut Context<'_, Self>) -> Option<Job> {
        let store = Store::global(cx);
        let (roots, catalogue) = (store.roots.clone(), store.catalogue.clone());
        while let Some(id) = self.waiting.pop_front() {
            let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
                continue;
            };
            entry.status = ImportStatus::Probing;
            let path = entry.path.clone();
            cx.notify();
            return Some(Job {
                id,
                path,
                roots,
                catalogue,
            });
        }
        self.running = false;
        None
    }

    /// Runs one import on the background and follows its steps. `false` when
    /// the queue is gone.
    async fn run(job: Job, this: &WeakEntity<Self>, cx: &mut AsyncApp) -> bool {
        let Job {
            id,
            path,
            roots,
            catalogue,
        } = job;
        let (steps, mut told) = unbounded();
        let work = cx.background_spawn(async move {
            let outcome = import(&roots, &catalogue, &path, now_ms(), &|step| {
                // The queue may be gone; the import goes on regardless.
                steps.unbounded_send(step).ok();
            });
            outcome.map(|imported| {
                let title = catalogue
                    .book(imported.key)
                    .ok()
                    .flatten()
                    .map(|row| row.title);
                (imported, title)
            })
        });
        // The sender is dropped with the closure, which ends this loop.
        while let Some(step) = told.next().await {
            if this
                .update(cx, |this, cx| this.set_step(id, step, cx))
                .is_err()
            {
                return false;
            }
        }
        let outcome = work.await;
        this.update(cx, |this, cx| this.finish(id, outcome, cx))
            .is_ok()
    }

    fn set_step(&mut self, id: u64, step: Step, cx: &mut Context<'_, Self>) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.status = ImportStatus::of_step(step);
            cx.notify();
        }
    }

    fn finish(&mut self, id: u64, outcome: Outcome, cx: &mut Context<'_, Self>) {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return;
        };
        let open_when_done = entry.open_when_done;
        let (message, opened) = match outcome {
            Ok((imported, title)) => {
                let shown = title.clone().unwrap_or_else(|| entry.name.clone());
                entry.title = title;
                let key = imported.key;
                let message = if imported.added {
                    entry.status = ImportStatus::Done {
                        key,
                        warnings: imported.warnings,
                    };
                    Library::global(cx).update(cx, Library::reload);
                    format!("Added {shown}")
                } else {
                    entry.status = ImportStatus::Already { key };
                    format!("{shown} is already in the library")
                };
                (message, open_when_done.then_some(key))
            }
            Err(error) => {
                let chain = error_chain(&error);
                let message = format!("{} could not be added: {chain}", entry.name);
                entry.status = ImportStatus::Failed {
                    message: sentence(&chain),
                };
                (message, None)
            }
        };
        cx.notify();
        self.window
            .update(cx, |_, window, cx| announce(message, window, cx))
            .ok();
        if let Some(key) = opened {
            cx.emit(ImportEvent::Opened(key));
        }
    }
}

/// The milliseconds of Unix time now; zero if the clock is before 1970.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// The message of an error and of each error under it, joined by colons.
pub(crate) fn error_chain(error: &dyn Error) -> String {
    let mut chain = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        // A message that already says its cause is not said twice.
        if !chain.ends_with(&text) {
            chain.push_str(": ");
            chain.push_str(&text);
        }
        source = cause.source();
    }
    chain
}

/// `text` as a sentence: it starts with a capital and ends with a full stop.
fn sentence(text: &str) -> String {
    let mut letters = text.chars();
    let mut sentence: String = letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    });
    if !sentence.ends_with(['.', '!', '?']) {
        sentence.push('.');
    }
    sentence
}

#[cfg(test)]
mod tests;
