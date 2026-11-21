//! The library as state of the interface: the store every page reaches, and
//! the entity that holds the books of the shelf.
//!
//! Nothing here blocks the main thread. Every read of the catalogue and every
//! write to it, and every removal of a file, runs on the background executor
//! of GPUI (RFC 0004, "Threads"). The entity keeps its tasks, so that dropping
//! it cancels what has not started.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::sync::Arc;

use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui_kit::{App, AppContext as _, Context, Entity, Global, Task};

use super::{BookRow, Catalogue, CatalogueError, ReadingState};
use crate::shared::model::BookKey;
use crate::shared::roots::Roots;

/// What every page needs to reach the files.
///
/// Installed as a GPUI global once the catalogue is open; before that no page
/// exists, so [`Store::global`] is only called by pages and models.
#[derive(Debug)]
pub(crate) struct Store {
    /// The directories of the library.
    pub(crate) roots: Roots,
    /// The open catalogue. Its calls are synchronous: make them on the
    /// background executor.
    pub(crate) catalogue: Arc<Catalogue>,
}

impl Global for Store {}

impl Store {
    /// The installed store.
    ///
    /// # Panics
    ///
    /// When no store was installed: the application installs it before it
    /// builds any page.
    #[expect(
        clippy::panic,
        reason = "a page built before the store is a programming error, as for any GPUI global"
    )]
    pub(crate) fn global(cx: &App) -> &Self {
        cx.try_global::<Self>()
            .unwrap_or_else(|| panic!("the store is installed before any page is built"))
    }

    /// The installed store, or `None` while the catalogue is still opening.
    pub(crate) fn try_global(cx: &App) -> Option<&Self> {
        cx.try_global::<Self>()
    }

    /// Makes this store the one every page reaches.
    pub(crate) fn install(self, cx: &mut App) {
        cx.set_global(self);
    }
}

/// One book as the shelf shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShelfBook {
    /// The copy of the book's metadata in the catalogue.
    pub(crate) row: BookRow,
    /// What was read of the book.
    pub(crate) state: ReadingState,
}

/// Why the library could not be read or changed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum LibraryError {
    /// The catalogue could not be read or written.
    #[error("the catalogue could not be read or written")]
    Catalogue(#[from] CatalogueError),
    /// The file of a removed book could not be deleted.
    #[error("the file of a book could not be deleted")]
    File(#[from] io::Error),
}

/// A change waiting for the catalogue, in the order it was made.
#[derive(Debug, Clone)]
enum Write {
    /// Store the state of a book. `serial` tells a later change of the same
    /// book from this one.
    State {
        key: BookKey,
        state: ReadingState,
        serial: u64,
    },
    /// Delete a book: its row, its state, its file and its thumbnail.
    Remove(BookKey),
}

/// The books of the library, in memory. Views observe it.
///
/// The books are those of the catalogue, in the order they were added. A view
/// asks for a change by calling a method; the change is visible at once and
/// reaches the catalogue in the background, in the order it was asked for.
pub(crate) struct Library {
    books: Vec<ShelfBook>,
    loading: bool,
    error: Option<LibraryError>,
    /// Tells a load that was overtaken by a newer one from the newer one.
    generation: u64,
    serial: u64,
    /// States asked for and not yet stored. A load that was read before the
    /// write must not take them back.
    unsaved: HashMap<BookKey, (u64, ReadingState)>,
    /// Books asked to be removed and not yet gone, for the same reason.
    removing: HashSet<BookKey>,
    writes: UnboundedSender<Write>,
    /// The load in flight. Held so that dropping the library cancels it.
    load: Option<Task<()>>,
    /// Applies the writes in order, for as long as the library lives.
    _applying: Task<()>,
}

/// The handle of the one [`Library`] of the application.
struct LibraryHandle(Entity<Library>);

impl Global for LibraryHandle {}

impl Library {
    /// Starts loading the books on the background executor. The entity is
    /// [loading](Self::is_loading) until they arrive.
    pub(crate) fn new(cx: &mut Context<'_, Self>) -> Self {
        let (writes, queue) = unbounded();
        let applying = cx.spawn(async move |this, cx| write_all(queue, this, cx).await);
        let mut library = Self {
            books: Vec::new(),
            loading: false,
            error: None,
            generation: 0,
            serial: 0,
            unsaved: HashMap::new(),
            removing: HashSet::new(),
            writes,
            load: None,
            _applying: applying,
        };
        library.reload(cx);
        library
    }

    /// The library of the application, as an entity to observe.
    ///
    /// # Panics
    ///
    /// When [`Self::install`] was not called: the application installs it
    /// before it builds any page.
    #[expect(
        clippy::panic,
        reason = "a page built before the library is a programming error, as for any GPUI global"
    )]
    pub(crate) fn global(cx: &App) -> Entity<Self> {
        cx.try_global::<LibraryHandle>().map_or_else(
            || panic!("the library is installed before any page is built"),
            |handle| handle.0.clone(),
        )
    }

    /// Makes `library` the one every page reaches through [`Self::global`].
    pub(crate) fn install(library: Entity<Self>, cx: &mut App) {
        cx.set_global(LibraryHandle(library));
    }

    /// Every book, in the order they were added.
    pub(crate) fn books(&self) -> &[ShelfBook] {
        &self.books
    }

    /// One book; `None` when the library has no such book.
    pub(crate) fn book(&self, key: BookKey) -> Option<&ShelfBook> {
        self.books.iter().find(|book| book.row.key == key)
    }

    /// Whether the books are being read: the shelf has not been shown yet, or
    /// is being read again.
    pub(crate) fn is_loading(&self) -> bool {
        self.loading
    }

    /// What went wrong the last time the catalogue was read or changed. It is
    /// cleared by the next successful load.
    #[cfg(test)]
    pub(crate) fn error(&self) -> Option<&LibraryError> {
        self.error.as_ref()
    }

    /// Reads the rows and the states again, on the background. The books
    /// already shown stay until the new ones arrive.
    pub(crate) fn reload(&mut self, cx: &mut Context<'_, Self>) {
        let catalogue = Store::global(cx).catalogue.clone();
        self.generation += 1;
        let generation = self.generation;
        self.loading = true;
        self.load = Some(cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_spawn(async move { read_books(&catalogue) })
                .await;
            // The entity may be gone by now: then there is nobody to tell.
            this.update(cx, |this, cx| this.loaded(generation, loaded, cx))
                .ok();
        }));
        cx.notify();
    }

    /// Removes a book: it leaves the shelf at once, and its row, state, file
    /// and thumbnail are deleted on the background.
    pub(crate) fn remove(&mut self, key: BookKey, cx: &mut Context<'_, Self>) {
        self.books.retain(|book| book.row.key != key);
        self.unsaved.remove(&key);
        self.removing.insert(key);
        self.send(Write::Remove(key));
        cx.notify();
    }

    /// Records what was read of a book: in memory at once, in the catalogue on
    /// the background. A key the library does not have is ignored.
    pub(crate) fn set_state(
        &mut self,
        key: BookKey,
        state: ReadingState,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(book) = self.books.iter_mut().find(|book| book.row.key == key) else {
            return;
        };
        if book.state == state {
            return;
        }
        book.state.clone_from(&state);
        self.serial += 1;
        let serial = self.serial;
        self.unsaved.insert(key, (serial, state.clone()));
        self.send(Write::State { key, state, serial });
        cx.notify();
    }

    fn send(&self, write: Write) {
        // The writer ends with the entity, and so does this sender; a failed
        // send can only mean there is nothing left to write for.
        self.writes.unbounded_send(write).ok();
    }

    fn loaded(
        &mut self,
        generation: u64,
        loaded: Result<Vec<ShelfBook>, CatalogueError>,
        cx: &mut Context<'_, Self>,
    ) {
        if generation != self.generation {
            // A newer load is on its way; this one is stale.
            return;
        }
        self.loading = false;
        match loaded {
            Ok(mut books) => {
                books.retain(|book| !self.removing.contains(&book.row.key));
                for book in &mut books {
                    if let Some((_, state)) = self.unsaved.get(&book.row.key) {
                        book.state.clone_from(state);
                    }
                }
                self.books = books;
                self.error = None;
            }
            Err(error) => self.error = Some(error.into()),
        }
        cx.notify();
    }

    /// A write reached the catalogue, or failed.
    fn written(
        &mut self,
        write: &Write,
        result: Result<(), LibraryError>,
        cx: &mut Context<'_, Self>,
    ) {
        match write {
            Write::State { key, serial, .. } => {
                if self
                    .unsaved
                    .get(key)
                    .is_some_and(|(pending, _)| pending == serial)
                {
                    self.unsaved.remove(key);
                }
            }
            Write::Remove(key) => {
                self.removing.remove(key);
            }
        }
        if let Err(error) = result {
            self.error = Some(error);
            // The memory no longer says what the catalogue holds.
            self.reload(cx);
        }
        cx.notify();
    }
}

/// Applies the writes one after another, in the order they were asked for, so
/// that two changes of one book cannot reach the catalogue the wrong way
/// round.
async fn write_all(
    mut queue: futures::channel::mpsc::UnboundedReceiver<Write>,
    this: gpui_kit::WeakEntity<Library>,
    cx: &mut gpui_kit::AsyncApp,
) {
    while let Some(write) = queue.next().await {
        let Ok(store) = this.read_with(cx, |_, cx| {
            Store::try_global(cx).map(|store| (store.roots.clone(), store.catalogue.clone()))
        }) else {
            return;
        };
        let Some((roots, catalogue)) = store else {
            continue;
        };
        let result = cx
            .background_spawn({
                let write = write.clone();
                async move { perform(&write, &roots, &catalogue) }
            })
            .await;
        if this
            .update(cx, |this, cx| this.written(&write, result, cx))
            .is_err()
        {
            return;
        }
    }
}

/// Reads every row and its state.
fn read_books(catalogue: &Catalogue) -> Result<Vec<ShelfBook>, CatalogueError> {
    catalogue
        .books()?
        .into_iter()
        .map(|row| {
            let state = catalogue.state(row.key)?;
            Ok(ShelfBook { row, state })
        })
        .collect()
}

/// Makes one change in the catalogue and on disk.
fn perform(write: &Write, roots: &Roots, catalogue: &Catalogue) -> Result<(), LibraryError> {
    match write {
        Write::State { key, state, .. } => Ok(catalogue.put_state(*key, state)?),
        Write::Remove(key) => {
            // The row goes first: a book that is gone from the shelf must not
            // come back because its file could not be deleted.
            catalogue.remove_book(*key)?;
            remove_file(&roots.book(*key))?;
            remove_file(&roots.cover(*key))?;
            Ok(())
        }
    }
}

/// Deletes a file; a file that is not there is already deleted.
fn remove_file(path: &std::path::Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests;
