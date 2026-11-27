//! The library: the books on a shelf.
//!
//! One page for the three size classes (mbook-ui docs, "Library" and "Layout"):
//!
//! - **Expanded**: a docked sidebar (search, sections, imports) beside the
//!   shelf, with the card "Continue reading" above it.
//! - **Medium**: the same, with the sidebar behind a control in the header,
//!   in a sheet that slides over the shelf.
//! - **Compact**: no sidebar. A bar at the bottom holds Reading, Library and
//!   Search; the imports are a row above it.
//!
//! The shelf is a grid of covers, virtualised by rows. Which books it shows is
//! decided on the pool of `compute` ([`crate::features::search_library`]),
//! whether a book has a thumbnail on the background executor; the page draws
//! what it was told
//! and never touches a file.

mod cover;
mod create;
mod export;
#[cfg(not(target_os = "android"))]
pub(crate) use export::save as export_to;
mod imports;
mod menu;
mod share;
mod shelf;
mod sidebar;
mod view;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString,
    Subscription, Task, Window,
};
use rust_i18n::t;

use self::shelf::Shelf;
use self::sidebar::LibrarySidebar;
use crate::entities::catalogue::{Library, ShelfBook, Store};
use crate::features::import_book::{ImportEvent, ImportQueue};
use crate::features::search_library::{Counts, Section, Sort, counts, select};
use crate::shared::compute;
use crate::shared::layout::SizeClass;
use crate::shared::model::BookKey;
use crate::shared::ui::{announce, follow_language};

/// What the library asks the root to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibraryEvent {
    /// Open a book in the reader.
    Open(BookKey),
    /// Edit a book.
    Edit(BookKey),
    /// Start writing a newly created book.
    Write(BookKey),
    /// Go to the settings.
    Settings,
}

/// What a compact window shows under its bar at the bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    /// The books being read.
    Reading,
    /// Every book.
    Library,
    /// Every book, with the search field above them.
    Search,
}

/// The page of the library.
pub(crate) struct LibraryPage {
    focus_handle: FocusHandle,
    library: Entity<Library>,
    queue: Entity<ImportQueue>,
    search: Entity<InputState>,
    sidebar: Entity<LibrarySidebar>,
    /// What the shelf shows, and how it is laid out.
    shelf: Shelf,
    query: SharedString,
    section: Section,
    sort: Sort,
    tab: Tab,
    /// The width of a cover on the shelf, in pixels, for the size class last drawn.
    cover_width: f32,
    /// The books as they were when the filter was last started. Shared with
    /// the background tasks, so that a keystroke copies nothing.
    books: Arc<Vec<ShelfBook>>,
    /// Where each book is in `books`.
    index: HashMap<BookKey, usize>,
    counts: Counts,
    /// Tells a result of the filter that was overtaken by a newer request.
    generation: u64,
    filtering: Option<Task<()>>,
    /// The books whose thumbnail is a file, as the background found out.
    thumbnails: HashSet<BookKey>,
    thumbnails_generation: u64,
    probing: Option<Task<()>>,
    /// Writing and indexing a newly created book.
    creating: Option<Task<()>>,
    /// Whether the failure the library holds was announced, so that it is said
    /// once and not at every change of the shelf.
    error_told: bool,
    /// The files this page was handed, for the tests of the root.
    #[cfg(test)]
    received: Vec<PathBuf>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<LibraryEvent> for LibraryPage {}

impl LibraryPage {
    /// A library page over the library of the application.
    ///
    /// The first page also installs the queue of imports, which outlives the
    /// page.
    pub(crate) fn new(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let library = Library::global(cx);
        let queue = ImportQueue::install(window, cx);
        let search = cx.new(|cx| InputState::new(window, cx).clean_on_escape());
        let search_language = follow_language(&search, || t!("library.search"), window, cx);
        let page = cx.weak_entity();
        let sidebar = cx.new(|cx| LibrarySidebar::new(page, search.clone(), queue.clone(), cx));
        let subscriptions = vec![
            cx.observe_in(&library, window, |this, _, window, cx| {
                this.library_changed(cx);
                this.announce_error(window, cx);
            }),
            cx.subscribe(&queue, |_, _, event: &ImportEvent, cx| match *event {
                ImportEvent::Opened(key) => cx.emit(LibraryEvent::Open(key)),
            }),
            cx.subscribe(&search, |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = input.read(cx).value();
                    this.set_query(query, cx);
                }
            }),
            search_language,
        ];
        let mut page = Self {
            focus_handle: cx.focus_handle(),
            library,
            queue,
            search,
            sidebar,
            shelf: Shelf::new(),
            query: SharedString::default(),
            section: Section::All,
            sort: Sort::Recent,
            tab: Tab::Library,
            cover_width: SizeClass::Expanded.cover_width(),
            books: Arc::new(Vec::new()),
            index: HashMap::new(),
            counts: Counts::default(),
            generation: 0,
            filtering: None,
            thumbnails: HashSet::new(),
            thumbnails_generation: 0,
            probing: None,
            creating: None,
            error_told: false,
            #[cfg(test)]
            received: Vec::new(),
            _subscriptions: subscriptions,
        };
        page.library_changed(cx);
        page
    }

    /// Adds the files that were chosen in the file dialog or dropped on the
    /// window to the library: one import for each, one at a time.
    ///
    /// When there is one file and its import ends in a book, the page asks the
    /// root to open it; several files are only added.
    pub(crate) fn import(
        &mut self,
        paths: Vec<PathBuf>,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        #[cfg(test)]
        self.received.extend(paths.iter().cloned());
        self.queue.update(cx, |queue, cx| queue.add(paths, cx));
        cx.notify();
    }

    /// The files this page was handed.
    #[cfg(test)]
    pub(crate) fn received(&self) -> &[PathBuf] {
        &self.received
    }

    /// The section the shelf shows.
    pub(crate) fn section(&self) -> Section {
        self.section
    }

    /// The words the shelf is filtered by.
    #[cfg(test)]
    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    /// How many books each section holds.
    pub(crate) fn counts(&self) -> Counts {
        self.counts
    }

    /// The books the shelf shows, in order.
    #[cfg(test)]
    pub(crate) fn visible(&self) -> &[BookKey] {
        self.shelf.visible()
    }

    /// Shows another section of the library.
    pub(crate) fn set_section(&mut self, section: Section, cx: &mut Context<'_, Self>) {
        if self.section == section {
            return;
        }
        self.section = section;
        self.tab = match section {
            Section::Reading => Tab::Reading,
            Section::All | Section::Finished => Tab::Library,
        };
        self.refilter(cx);
    }

    /// Puts the shelf in another order.
    pub(crate) fn set_sort(&mut self, sort: Sort, cx: &mut Context<'_, Self>) {
        if self.sort == sort {
            return;
        }
        self.sort = sort;
        self.refilter(cx);
    }

    /// Filters the shelf by words.
    pub(crate) fn set_query(&mut self, query: SharedString, cx: &mut Context<'_, Self>) {
        if self.query == query {
            return;
        }
        self.query = query;
        self.refilter(cx);
    }

    /// Moves between the tabs of a compact window. Leaving the search drops
    /// the words, which are no longer in sight.
    pub(crate) fn set_tab(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.tab == tab {
            return;
        }
        let leaving_search = self.tab == Tab::Search;
        self.tab = tab;
        self.section = match tab {
            Tab::Reading => Section::Reading,
            Tab::Library | Tab::Search => Section::All,
        };
        if leaving_search && !self.query.is_empty() {
            self.search
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.query = SharedString::default();
        }
        self.refilter(cx);
        if tab == Tab::Search {
            let input = self.search.clone();
            window.defer(cx, move |window, cx| {
                input.update(cx, |input, cx| input.focus(window, cx));
            });
        }
    }

    /// The tab a compact window shows.
    #[cfg(test)]
    pub(crate) fn tab(&self) -> Tab {
        self.tab
    }

    /// Moves the focus to the search field. In a compact window that is the
    /// search tab; in a medium one the sidebar opens first.
    fn find(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let class = crate::features::appearance::Appearance::global(cx)
            .read(cx)
            .size_class(window);
        match class {
            crate::shared::layout::SizeClass::Compact => self.set_tab(Tab::Search, window, cx),
            crate::shared::layout::SizeClass::Medium => self.show_sections(window, cx),
            crate::shared::layout::SizeClass::Expanded => {
                self.search.update(cx, |input, cx| input.focus(window, cx));
            }
        }
    }

    /// Says once that the library could not be read or changed. The words stay
    /// on the page with a way to try again; this is for those who do not look.
    fn announce_error(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let words = self.library.read(cx).error().map(error_words);
        match words {
            Some(words) if !self.error_told => {
                self.error_told = true;
                announce(words, window, cx);
            }
            Some(_) => {}
            None => self.error_told = false,
        }
    }

    /// Reads the books again: the library changed.
    fn library_changed(&mut self, cx: &mut Context<'_, Self>) {
        let library = self.library.read(cx);
        // An empty library equals the empty list the page starts with, and
        // would never be filtered: its heading would say "Loading" for good.
        let unfiltered =
            !self.shelf.is_ready() && !library.is_loading() && self.filtering.is_none();
        if library.books() != self.books.as_slice() || unfiltered {
            self.books = Arc::new(library.books().to_vec());
            self.index = self
                .books
                .iter()
                .enumerate()
                .map(|(at, book)| (book.row.key, at))
                .collect();
            self.counts = counts(&self.books);
            self.refilter(cx);
            self.probe_thumbnails(cx);
        }
        cx.notify();
    }

    /// Chooses the books to show, on the pool of `compute`. A newer request
    /// makes the answer to an older one useless.
    fn refilter(&mut self, cx: &mut Context<'_, Self>) {
        self.generation += 1;
        let generation = self.generation;
        let books = Arc::clone(&self.books);
        let (query, section, sort) = (self.query.clone(), self.section, self.sort);
        self.filtering = Some(cx.spawn(async move |this, cx| {
            let keys = cx
                .background_spawn(compute::spawn(move || {
                    select(&books, &query, section, sort)
                }))
                .await;
            this.update(cx, |this, cx| this.filtered(generation, keys, cx))
                .ok();
        }));
        cx.notify();
    }

    fn filtered(&mut self, generation: u64, keys: Vec<BookKey>, cx: &mut Context<'_, Self>) {
        if generation != self.generation {
            return;
        }
        self.shelf.show(keys);
        cx.notify();
    }

    /// Finds, on the background executor, which books have a thumbnail on
    /// disk. Until it knows, a book is drawn from its title.
    fn probe_thumbnails(&mut self, cx: &mut Context<'_, Self>) {
        let roots = Store::global(cx).roots.clone();
        let keys: Vec<BookKey> = self
            .books
            .iter()
            .filter(|book| book.row.cover.is_some())
            .map(|book| book.row.key)
            .collect();
        self.thumbnails_generation += 1;
        let generation = self.thumbnails_generation;
        self.probing = Some(cx.spawn(async move |this, cx| {
            let found: HashSet<BookKey> = cx
                .background_spawn(async move {
                    keys.into_iter()
                        .filter(|key| roots.cover(*key).is_file())
                        .collect()
                })
                .await;
            this.update(cx, |this, cx| {
                if generation == this.thumbnails_generation {
                    this.thumbnails = found;
                    cx.notify();
                }
            })
            .ok();
        }));
    }
}

/// What went wrong with the catalogue, in words for a person.
pub(super) fn error_words(error: &crate::entities::catalogue::LibraryError) -> String {
    t!(
        "library.error",
        reason = crate::features::import_book::error_chain(error)
    )
    .into_owned()
}

impl Focusable for LibraryPage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests;
