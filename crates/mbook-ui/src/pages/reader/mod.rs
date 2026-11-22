//! The reader: one book, a chapter at a time.
//!
//! The page opens the file of the book on the background and keeps the
//! [`BookSnapshot`] and the [`BookSource`]; it asks the source for one chapter
//! at a time and for the pictures that are drawn, and never loads a book as a
//! whole. Every call of the source runs on the background executor; a result
//! that comes back for a chapter the reader has left is dropped.
//!
//! * the text is the [`ChapterView`], the place is kept by
//!   [`PositionKeeper`];
//! * the controls hide while the book is read and come back with the pointer,
//!   a key or a tap in the middle ([`controls`]);
//! * the contents and the appearance are panels: a popover on an expanded
//!   window, a sheet on a smaller one.

mod chrome;
mod contents;
mod controls;
mod keys;
mod pagination;
mod pictures;
mod scrubber;
mod search;
#[cfg(test)]
mod support;
#[cfg(test)]
mod tests;
mod typography;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt as _;
use futures::channel::mpsc::unbounded;
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString,
    Subscription, Task, Window,
};

use self::contents::ContentsPanel;
use self::controls::{HIDE_AFTER, Plan};
use self::pagination::BookPages;
use self::pictures::{CONCURRENCY, Job, Pictures};
use self::scrubber::Scrubber;
use self::search::BookMatch;
use self::typography::TypographyPanel;
use crate::entities::book::chapter::{PreparedChapter, Target};
use crate::entities::book::error::BookError;
use crate::entities::book::snapshot::BookSnapshot;
use crate::entities::book::source::{self, BookSource};
use crate::entities::catalogue::{Library, Store};
use crate::features::appearance::Appearance;
use crate::features::reading_position::{PositionKeeper, now_ms, reading_state};
use crate::shared::layout::SizeClass;
use crate::shared::model::{BookKey, Place};
use crate::shared::ui::search_bar::SearchBar;
use crate::widgets::chapter_view::{ChapterView, ChapterViewEvent, Direction};

/// How many places the "back" of the reader remembers.
const HISTORY: usize = 64;

/// What the reader asks the root to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReaderEvent {
    /// Go back to the previous screen.
    Close,
    /// Edit this book.
    Edit(BookKey),
}

/// A place in the book: a chapter and a byte offset of its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Spot {
    chapter: usize,
    offset: u32,
}

/// Where a chapter is opened.
enum Arrival {
    Offset(u32),
    Resume(u32, Arc<PreparedChapter>),
    Start,
    End,
}

/// The panel that is open over the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Contents,
    Appearance,
}

/// What the page shows.
enum Phase {
    /// The book is being opened.
    Opening,
    /// The book could not be opened; the reason, as text.
    Failed(String),
    /// The book has no chapter.
    Empty,
    /// The book is open.
    Ready(Reading),
}

/// An open book.
struct Reading {
    snapshot: Arc<BookSnapshot>,
    /// `None` while the file is closed for the editor.
    source: Option<Arc<BookSource>>,
    chapter: usize,
    prepared: Option<Arc<PreparedChapter>>,
    loading: bool,
    /// Why the chapter could not be read.
    error: Option<String>,
}

/// What keeps the controls on screen.
#[derive(Default)]
struct Holds {
    /// The pointer is over them.
    hovered: bool,
    /// The focus is inside them.
    focused: bool,
    /// A panel they opened is open.
    panel: bool,
}

/// The controls over the text.
struct Controls {
    shown: bool,
    /// The last sign of the reader: a key, the pointer, a tap.
    activity: Instant,
    holds: Holds,
    timer: Option<Task<()>>,
}

/// The focus handles of the controls, to know when the focus is inside them.
struct Handles {
    back: FocusHandle,
    contents: FocusHandle,
    search: FocusHandle,
    appearance: FocusHandle,
    edit: FocusHandle,
    previous: FocusHandle,
    return_to_passage: FocusHandle,
    next: FocusHandle,
    scrubber: FocusHandle,
}

impl Handles {
    fn new(cx: &App) -> Self {
        Self {
            back: cx.focus_handle(),
            contents: cx.focus_handle(),
            search: cx.focus_handle(),
            appearance: cx.focus_handle(),
            edit: cx.focus_handle(),
            previous: cx.focus_handle(),
            return_to_passage: cx.focus_handle(),
            next: cx.focus_handle(),
            scrubber: cx.focus_handle(),
        }
    }

    /// Whether the focus is on one of the controls.
    fn focused(&self, window: &Window, cx: &App) -> bool {
        [
            &self.back,
            &self.contents,
            &self.search,
            &self.appearance,
            &self.edit,
            &self.previous,
            &self.return_to_passage,
            &self.next,
        ]
        .into_iter()
        .any(|handle| handle.is_focused(window))
            || self.scrubber.contains_focused(window, cx)
    }
}

/// The page of one open book.
pub(crate) struct ReaderPage {
    key: BookKey,
    focus_handle: FocusHandle,
    handles: Handles,
    /// The title shown while the book opens; then the title of the book.
    title: SharedString,
    phase: Phase,
    view: Entity<ChapterView>,
    keeper: Entity<PositionKeeper>,
    contents: Entity<ContentsPanel>,
    search: SearchBar<BookMatch>,
    search_origin: Option<Spot>,
    typography: Entity<TypographyPanel>,
    pictures: Arc<Pictures>,
    /// The size class the cache of pictures was sized for.
    class: SizeClass,
    picture_tasks: HashMap<String, Task<()>>,
    /// Tells a result of the opening from one for an earlier opening.
    generation: u64,
    /// Tells a chapter that arrives from one the reader has left.
    chapter_generation: u64,
    spot: Spot,
    progress: u16,
    pages: BookPages,
    scrubber: Scrubber,
    /// Where a link or an entry of the contents was followed from.
    history: Vec<Spot>,
    /// Where to continue once the book is open again.
    resume_at: Option<Place>,
    opened_ms: u64,
    suspended: bool,
    controls: Controls,
    panel: Option<Panel>,
    opening: Option<Task<()>>,
    chapter_task: Option<Task<()>>,
    follow_task: Option<Task<()>>,
    _wake: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ReaderEvent> for ReaderPage {}

impl ReaderPage {
    /// A reader for the book `key` of the library.
    pub(crate) fn new(key: BookKey, window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        keys::bind(cx);
        let title: SharedString = Library::global(cx)
            .read(cx)
            .book(key)
            .map_or_else(|| "Book".to_owned(), |book| book.row.title.clone())
            .into();
        let appearance = Appearance::global(cx);
        let (class, view) = chapter_view(&appearance, window, cx);
        let page = cx.entity().downgrade();
        let (wake, mut wakes) = unbounded::<()>();
        let pictures = Arc::new(Pictures::new(pictures::budget(class), wake));
        let table = Arc::clone(&pictures);
        let for_links = page.clone();
        let for_notes = page.clone();
        view.update(cx, |view, cx| {
            view.set_pictures(
                Arc::new(move |destination, column, page, _, _| {
                    table.resolve(destination, column, page)
                }),
                cx,
            );
            view.on_link(move |destination, _, cx| {
                for_links
                    .update(cx, |page, cx| page.follow(destination, cx))
                    .ok();
            });
            view.on_note(move |offset, window, cx| {
                for_notes
                    .update(cx, |page, cx| page.follow_note(offset, window, cx))
                    .ok();
            });
        });
        let handles = Handles::new(cx);
        let scrubber = Scrubber::new(cx);
        let search = SearchBar::new("Find in book", window, cx);
        let subscriptions = vec![
            cx.subscribe(&search.input, Self::on_search_input),
            cx.observe(&appearance, |this, appearance, cx| {
                let paged = appearance.read(cx).settings().reading.flow.is_paged();
                this.view.update(cx, |view, cx| view.set_paged(paged, cx));
                cx.notify();
            }),
            cx.subscribe(&scrubber.slider, |this, _, event, cx| {
                this.on_scrub(event, cx);
            }),
            cx.subscribe(&view, |this, _, event, cx| match event {
                ChapterViewEvent::Moved => this.sample(cx),
                ChapterViewEvent::Turn(direction) => this.screen(*direction, cx),
            }),
        ];
        let wake_task = cx.spawn(async move |this, cx| {
            while wakes.next().await.is_some() {
                if this.update(cx, Self::pump).is_err() {
                    break;
                }
            }
        });
        let now = cx.background_executor().now();
        let mut reader = Self {
            key,
            focus_handle: cx.focus_handle(),
            handles,
            title,
            phase: Phase::Opening,
            view,
            keeper: PositionKeeper::entity(key, cx),
            contents: cx.new(|_| ContentsPanel::new(page)),
            search,
            search_origin: None,
            typography: TypographyPanel::entity(cx),
            pictures,
            class,
            picture_tasks: HashMap::new(),
            generation: 0,
            chapter_generation: 0,
            spot: Spot::default(),
            progress: 0,
            pages: BookPages::default(),
            scrubber,
            history: Vec::new(),
            resume_at: None,
            opened_ms: now_ms(),
            suspended: false,
            controls: Controls {
                shown: true,
                activity: now,
                holds: Holds::default(),
                timer: None,
            },
            panel: None,
            opening: None,
            chapter_task: None,
            follow_task: None,
            _wake: wake_task,
            _subscriptions: subscriptions,
        };
        reader.start(cx);
        reader.arm(cx);
        reader
    }

    /// Writes the place and closes the file of the book, so that it can be
    /// replaced while the book is edited. The root calls it before it opens
    /// the editor.
    pub(crate) fn suspend(&mut self, cx: &mut Context<'_, Self>) {
        self.sample(cx);
        self.keeper.update(cx, PositionKeeper::flush);
        if let Phase::Ready(reading) = &mut self.phase {
            self.resume_at = reading.snapshot.place(self.spot.chapter, self.spot.offset);
            reading.source = None;
        }
        // Dropping the tasks stops the ones that have not started; the ones
        // that have hold the file only for the length of one read.
        self.picture_tasks.clear();
        self.search.hide();
        self.search_origin = None;
        drop(self.opening.take());
        drop(self.chapter_task.take());
        drop(self.follow_task.take());
        self.suspended = true;
        cx.notify();
    }

    /// Opens the file again, after the book was edited, and goes back to the
    /// place the reader left, at the block that contains it now. The root
    /// calls it when the editor closes.
    pub(crate) fn resume(&mut self, _window: &mut Window, cx: &mut Context<'_, Self>) {
        self.suspended = false;
        self.history.clear();
        self.start(cx);
    }

    /// Whether [`Self::suspend`] was called since the last [`Self::resume`].
    #[cfg(test)]
    pub(crate) fn is_suspended(&self) -> bool {
        self.suspended
    }

    /// Opens the book on the background.
    fn start(&mut self, cx: &mut Context<'_, Self>) {
        self.generation += 1;
        let generation = self.generation;
        let path = Store::global(cx).roots.book(self.key);
        self.opening = Some(cx.spawn(async move |this, cx| {
            let opened = cx
                .background_spawn(async move {
                    source::open(&path)
                        .map(|(snapshot, source)| (Arc::new(snapshot), Arc::new(source)))
                })
                .await;
            this.update(cx, |this, cx| this.opened(generation, opened, cx))
                .ok();
        }));
        cx.notify();
    }

    fn opened(
        &mut self,
        generation: u64,
        opened: Result<(Arc<BookSnapshot>, Arc<BookSource>), BookError>,
        cx: &mut Context<'_, Self>,
    ) {
        if generation != self.generation {
            return;
        }
        let (snapshot, source) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                self.phase = Phase::Failed(describe(&error));
                cx.notify();
                return;
            }
        };
        if !snapshot.title.trim().is_empty() {
            self.title = snapshot.title.clone().into();
        }
        let before = match &self.phase {
            Phase::Ready(reading) if self.resume_at.is_some() => reading.prepared.clone(),
            _ => None,
        };
        let stored = self.resume_at.take().or_else(|| {
            Library::global(cx)
                .read(cx)
                .book(self.key)?
                .state
                .place
                .clone()
        });
        let start = Place {
            chapter: String::new(),
            offset: 0,
        };
        let Some((chapter, offset)) = snapshot.locate(&stored.unwrap_or(start)) else {
            self.phase = Phase::Empty;
            cx.notify();
            return;
        };
        self.contents.update(cx, |panel, cx| {
            panel.show(
                Some(Arc::clone(&snapshot)),
                snapshot.current_entry(chapter, offset),
                cx,
            );
        });
        self.pages = BookPages::default();
        self.scrubber.reset();
        self.phase = Phase::Ready(Reading {
            snapshot,
            source: Some(source),
            chapter,
            prepared: None,
            loading: true,
            error: None,
        });
        let arrival = before.map_or(Arrival::Offset(offset), |before| {
            Arrival::Resume(offset, before)
        });
        self.load_chapter(chapter, arrival, cx);
    }

    /// Reads a chapter on the background and shows it.
    fn load_chapter(&mut self, index: usize, arrival: Arrival, cx: &mut Context<'_, Self>) {
        let Phase::Ready(reading) = &mut self.phase else {
            return;
        };
        let Some(source) = reading.source.as_ref().map(Arc::downgrade) else {
            return;
        };
        reading.chapter = index;
        reading.loading = true;
        reading.error = None;
        self.spot = Spot {
            chapter: index,
            offset: match &arrival {
                Arrival::Offset(offset) | Arrival::Resume(offset, _) => *offset,
                Arrival::Start | Arrival::End => 0,
            },
        };
        self.chapter_generation += 1;
        let generation = self.chapter_generation;
        self.chapter_task = Some(cx.spawn(async move |this, cx| {
            let prepared = cx
                .background_spawn(async move {
                    // The page holds the only lasting reference, so closing
                    // the book closes the file.
                    source
                        .upgrade()
                        .ok_or(BookError::Unusable)
                        .and_then(|source| source.prepare_chapter(index))
                })
                .await;
            this.update(cx, |this, cx| {
                this.chapter_ready(generation, arrival, prepared, cx);
            })
            .ok();
        }));
        cx.notify();
    }

    fn chapter_ready(
        &mut self,
        generation: u64,
        arrival: Arrival,
        prepared: Result<PreparedChapter, BookError>,
        cx: &mut Context<'_, Self>,
    ) {
        if generation != self.chapter_generation {
            return;
        }
        let Phase::Ready(reading) = &mut self.phase else {
            return;
        };
        reading.loading = false;
        let prepared = match prepared {
            Ok(prepared) => Arc::new(prepared),
            Err(error) => {
                reading.error = Some(describe(&error));
                cx.notify();
                return;
            }
        };
        self.picture_tasks.clear();
        self.pictures.reset(&prepared.pictures);
        reading.prepared = Some(Arc::clone(&prepared));
        let title = controls::chapter_title(&reading.snapshot, reading.chapter, self.spot.offset);
        let search = self
            .search
            .current
            .as_ref()
            .filter(|hit| hit.chapter == reading.chapter)
            .map(|hit| hit.found.source.clone());
        self.view.update(cx, |view, cx| {
            view.set_label(title, cx);
            // Before the text: a text is shown when its figures are drawn.
            view.set_figures(prepared.figures.clone(), cx);
            view.set_notes(prepared.footnotes.clone());
            view.set_chapter(&prepared.text, prepared.blocks.clone(), cx);
            view.set_search(search, cx);
            match arrival {
                Arrival::Offset(offset) => view.reveal(offset, cx),
                Arrival::Resume(offset, before) => {
                    view.reveal(prepared.after_edit(&before, offset), cx);
                }
                Arrival::Start => view.reveal_start(cx),
                Arrival::End => view.reveal_end(cx),
            }
        });
        self.sample(cx);
    }

    /// Starts the pictures that wait, as many as may run together.
    fn pump(&mut self, cx: &mut Context<'_, Self>) {
        let Phase::Ready(reading) = &self.phase else {
            return;
        };
        let Some(source) = reading.source.as_ref().map(Arc::downgrade) else {
            return;
        };
        let generation = self.chapter_generation;
        while self.picture_tasks.len() < CONCURRENCY {
            let Some(job) = self.pictures.next_job() else {
                break;
            };
            let source = source.clone();
            let work = job.clone();
            let key = task_key(&job);
            let task = cx.spawn(async move |this, cx| {
                let loaded = cx
                    .background_spawn(async move {
                        match source.upgrade() {
                            Some(source) => pictures::load(&source, work.picture, work.size),
                            None => Err(pictures::LoadError::Read(BookError::Unusable)),
                        }
                    })
                    .await;
                this.update(cx, |this, cx| {
                    this.picture_done(generation, &job, loaded, cx);
                })
                .ok();
            });
            self.picture_tasks.insert(key, task);
        }
    }

    fn picture_done(
        &mut self,
        generation: u64,
        job: &Job,
        loaded: Result<pictures::Loaded, pictures::LoadError>,
        cx: &mut Context<'_, Self>,
    ) {
        self.picture_tasks.remove(&task_key(job));
        if generation == self.chapter_generation && self.pictures.finish(job, loaded) {
            self.view.update(cx, ChapterView::pictures_changed);
        }
        self.pump(cx);
    }

    fn follow_note(&mut self, offset: u32, window: &mut Window, cx: &mut Context<'_, Self>) {
        if let Phase::Ready(reading) = &self.phase {
            let chapter = reading.chapter;
            self.touch(cx);
            self.focus_handle.focus(window, cx);
            self.jump(chapter, offset, cx);
        }
    }

    /// Follows a link of the text, with its destination as written.
    fn follow(&mut self, destination: &str, cx: &mut Context<'_, Self>) {
        let Phase::Ready(reading) = &self.phase else {
            return;
        };
        let from = reading.chapter;
        let target = reading
            .prepared
            .as_ref()
            .and_then(|prepared| prepared.links.get(destination))
            .cloned();
        match target {
            Some(Target::Deferred) => self.complete(from, destination, cx),
            Some(target) => self.go(&target, cx),
            None => {}
        }
    }

    /// Completes a destination that the tables of the book do not answer.
    fn complete(&mut self, from: usize, destination: &str, cx: &mut Context<'_, Self>) {
        let Phase::Ready(reading) = &self.phase else {
            return;
        };
        let Some(source) = reading.source.as_ref().map(Arc::downgrade) else {
            return;
        };
        let destination = destination.to_owned();
        let generation = self.chapter_generation;
        self.follow_task = Some(cx.spawn(async move |this, cx| {
            let target = cx
                .background_spawn(async move {
                    source
                        .upgrade()
                        .ok_or(BookError::Unusable)
                        .and_then(|source| source.follow(from, &destination))
                })
                .await;
            this.update(cx, |this, cx| {
                if generation == this.chapter_generation
                    && let Ok(target) = target
                    && target != Target::Deferred
                {
                    this.go(&target, cx);
                }
            })
            .ok();
        }));
    }

    /// Goes where a link leads.
    fn go(&mut self, target: &Target, cx: &mut Context<'_, Self>) {
        match target {
            Target::Place { chapter, offset } => self.jump(*chapter, *offset, cx),
            Target::External(url) => cx.open_url(url),
            Target::Picture(_) | Target::Deferred | Target::Unresolved => {}
        }
    }

    /// Goes to a place of the book and remembers where it came from.
    fn jump(&mut self, chapter: usize, offset: u32, cx: &mut Context<'_, Self>) {
        let from = self.here(cx);
        if self.history.last() != Some(&from) {
            self.history.push(from);
            if self.history.len() > HISTORY {
                self.history.remove(0);
            }
        }
        self.goto(Spot { chapter, offset }, cx);
    }

    /// Goes to a place of the book.
    fn goto(&mut self, spot: Spot, cx: &mut Context<'_, Self>) {
        let same = matches!(&self.phase, Phase::Ready(reading)
            if reading.chapter == spot.chapter && reading.prepared.is_some() && !reading.loading);
        if same {
            self.view
                .update(cx, |view, cx| view.reveal(spot.offset, cx));
            self.sample(cx);
        } else {
            self.load_chapter(spot.chapter, Arrival::Offset(spot.offset), cx);
        }
    }

    /// Where the reader is now.
    fn here(&self, cx: &App) -> Spot {
        match &self.phase {
            Phase::Ready(reading) if !reading.loading => Spot {
                chapter: reading.chapter,
                offset: self.view.read(cx).top_offset(cx),
            },
            _ => self.spot,
        }
    }

    /// Goes back to where a link was followed from. `false` when there is
    /// nowhere to go back to inside the book.
    fn back(&mut self, cx: &mut Context<'_, Self>) -> bool {
        let Some(spot) = self.history.pop() else {
            return false;
        };
        self.touch(cx);
        self.goto(spot, cx);
        true
    }

    /// Takes the entry `index` of the contents.
    fn open_entry(&mut self, index: usize, window: &mut Window, cx: &mut Context<'_, Self>) {
        let entry = match &self.phase {
            Phase::Ready(reading) => reading.snapshot.contents.get(index).cloned(),
            _ => None,
        };
        self.close_panel(window, cx);
        if let Some(entry) = entry {
            self.jump(entry.chapter, entry.offset, cx);
        }
    }

    /// Moves by a screen; at the end of the chapter, into the next.
    fn screen(&mut self, direction: Direction, cx: &mut Context<'_, Self>) {
        self.touch(cx);
        let (at_edge, loading) = {
            let view = self.view.read(cx);
            let loading = matches!(&self.phase, Phase::Ready(reading) if reading.loading);
            match direction {
                Direction::Forward => (view.at_end(cx), loading),
                Direction::Back => (view.at_start(cx), loading),
            }
        };
        if loading {
            return;
        }
        if !at_edge {
            self.view
                .update(cx, |view, cx| view.turn_page(direction, cx));
            return;
        }
        let Phase::Ready(reading) = &self.phase else {
            return;
        };
        let chapter = reading.chapter;
        match direction {
            Direction::Forward if chapter + 1 < reading.snapshot.chapters.len() => {
                self.load_chapter(chapter + 1, Arrival::Start, cx);
            }
            Direction::Back if chapter > 0 => {
                self.load_chapter(chapter - 1, Arrival::End, cx);
            }
            Direction::Forward | Direction::Back => {}
        }
    }

    /// Opens the chapter before or after this one, at its start.
    fn chapter_step(&mut self, forward: bool, cx: &mut Context<'_, Self>) {
        self.touch(cx);
        let Phase::Ready(reading) = &self.phase else {
            return;
        };
        let chapter = reading.chapter;
        let target = if forward {
            (chapter + 1 < reading.snapshot.chapters.len()).then_some(chapter + 1)
        } else {
            chapter.checked_sub(1)
        };
        if let Some(target) = target {
            self.load_chapter(target, Arrival::Start, cx);
        }
    }

    /// Records where the reader is: the status line, the contents, the
    /// stored place.
    fn sample(&mut self, cx: &mut Context<'_, Self>) {
        let Phase::Ready(reading) = &self.phase else {
            return;
        };
        if reading.loading || reading.error.is_some() || reading.prepared.is_none() {
            return;
        }
        let chapter = reading.chapter;
        let (offset, at_end, number) = {
            let view = self.view.read(cx);
            (view.top_offset(cx), view.at_end(cx), view.page_number())
        };
        if let Some((_, count)) = number {
            self.pages.record(chapter, count);
        }
        let snapshot = Arc::clone(&reading.snapshot);
        self.spot = Spot { chapter, offset };
        let previous = Library::global(cx)
            .read(cx)
            .book(self.key)
            .map_or_else(Default::default, |book| book.state.clone());
        if let Some(state) = reading_state(
            &snapshot,
            chapter,
            offset,
            at_end,
            &previous,
            self.opened_ms,
        ) {
            self.progress = state.progress_permille;
            self.keeper.update(cx, |keeper, cx| keeper.note(state, cx));
        }
        let current = snapshot.current_entry(chapter, offset);
        self.contents
            .update(cx, |panel, cx| panel.show(Some(snapshot), current, cx));
        cx.notify();
    }

    /// Leaves the reader: the place is written first.
    fn close(&mut self, cx: &mut Context<'_, Self>) {
        self.sample(cx);
        self.keeper.update(cx, PositionKeeper::flush);
        cx.emit(ReaderEvent::Close);
    }

    /// The reader showed a sign of life: the controls are shown and stay.
    fn touch(&mut self, cx: &mut Context<'_, Self>) {
        self.controls.activity = cx.background_executor().now();
        if !self.controls.shown {
            self.controls.shown = true;
            cx.notify();
        }
        self.arm(cx);
    }

    /// Starts the timer that hides the controls, if it is not running.
    fn arm(&mut self, cx: &mut Context<'_, Self>) {
        if self.controls.timer.is_some() {
            return;
        }
        self.controls.timer = Some(cx.spawn(async move |this, cx| {
            let mut wait = HIDE_AFTER;
            loop {
                cx.background_executor().timer(wait).await;
                let Ok(next) = this.update(cx, Self::hide_step) else {
                    break;
                };
                match next {
                    Some(next) => wait = next,
                    None => break,
                }
            }
        }));
    }

    /// One step of the timer: hides the controls when the time is up and
    /// nothing holds them; tells how long to wait otherwise.
    fn hide_step(&mut self, cx: &mut Context<'_, Self>) -> Option<std::time::Duration> {
        let now = cx.background_executor().now();
        let held = self.controls_held(cx);
        match controls::plan(now.saturating_duration_since(self.controls.activity), held) {
            Plan::HideNow => {
                self.controls.shown = false;
                self.controls.timer = None;
                cx.notify();
                None
            }
            Plan::Check(wait) => {
                if held {
                    self.controls.activity = now;
                }
                Some(wait)
            }
        }
    }

    /// Whether something keeps the controls on screen.
    fn controls_held(&self, cx: &App) -> bool {
        let keep = Appearance::global(cx).read(cx).settings().keep_controls;
        keep || self.controls.holds.hovered
            || self.controls.holds.focused
            || self.controls.holds.panel
            || self.scrubber.preview.is_some()
            || self.panel.is_some()
            || !matches!(&self.phase, Phase::Ready(reading) if reading.error.is_none())
    }

    /// Opens a panel in the place the size of the window asks for.
    fn set_panel(&mut self, panel: Option<Panel>, cx: &mut Context<'_, Self>) {
        self.panel = panel;
        self.controls.holds.panel = panel.is_some();
        self.touch(cx);
        cx.notify();
    }

    /// Closes the panel, whichever way it is shown.
    fn close_panel(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        use gpui_kit::component::WindowExt as _;
        if self.panel.take().is_some() || self.controls.holds.panel {
            self.controls.holds.panel = false;
            if window.has_active_sheet(cx) {
                window.close_sheet(cx);
            }
            cx.notify();
        }
    }
}

/// The key of a picture job in the tasks that run.
fn chapter_view(
    appearance: &Entity<Appearance>,
    window: &mut Window,
    cx: &mut Context<'_, ReaderPage>,
) -> (SizeClass, Entity<ChapterView>) {
    appearance.update(cx, |appearance, cx| {
        appearance.initialize_reader(window, cx);
    });
    let class = appearance.read(cx).size_class(window);
    let build = if appearance.read(cx).settings().reading.flow.is_paged() {
        ChapterView::paged
    } else {
        ChapterView::new
    };
    (class, cx.new(|cx| build(window, cx)))
}

fn task_key(job: &Job) -> String {
    let (width, height) = job.size;
    format!("{}@{width}x{height}", job.destination)
}

/// An error and its causes, as one line.
fn describe(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

impl Focusable for ReaderPage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
