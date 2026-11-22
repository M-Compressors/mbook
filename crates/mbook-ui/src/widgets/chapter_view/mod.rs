//! The text of a chapter, as the reader and the preview of the editor show it.
//!
//! [`ChapterView`] is the Markdown view of GPUI Kit in its virtualised form,
//! set in the typeface, size, line height and page tone of the
//! [`Appearance`]. It knows nothing of files: the owner hands it Markdown,
//! and optionally a way to find the pictures of the text.
//!
//! The text is moved through in one of two ways:
//!
//! * **scrolled**, by default in the reader and in the editor preview;
//! * **a page at a time**, when the reader enables pagination.
//!   The Kit lays the chapter out as one column and tells the height of every
//!   block; [`pages`] divides that column into pages that end between two
//!   lines. The list viewport fits the page’s visible content, leaving the
//!   rest of the sheet blank. Optional page-turn motion uses the motion
//!   of the Kit: the page slides out towards where it came from and the next
//!   one slides in.
//!
//! # What the Kit does and does not give
//!
//! * **Blocks.** The view lays out one list item for each top-level block of
//!   the document, in source order. The Kit does not say where an item starts
//!   in the source, so the widget keeps the offsets of the blocks (given by
//!   the owner, or found on the background) and maps the top item of the list
//!   to one of them while the counts agree; when they do not, it uses the
//!   position as a fraction of the text ([`blocks`]).
//! * **Pictures.** An image is sized by its pixels, and its source may be a
//!   function that answers `None` while the image is not there. The Kit then
//!   measures the image at a small square, so the text would jump when the
//!   picture arrives. The widget therefore answers with a transparent image of
//!   the size the picture will have until the real one is ready, and the text
//!   keeps its place ([`layout::picture_size`]). A picture the owner does
//!   not know is not fetched: nothing is drawn for it and the view never
//!   reaches the network.
//! * **Diagrams and formulas.** The Kit shows pictures only. A picture whose
//!   destination is a diagram or a formula of the book is taken from it by
//!   two plugins, which draw what the widget drew of its source on the
//!   background ([`figures`]). The text is handed to the Kit when its figures
//!   are drawn, so that it is laid out once, at its final heights.
//! * **Structure for assistive technology.** The Kit's Markdown view reports
//!   no roles of its own (no heading levels, lists or tables), so the widget
//!   wraps the text in one named article. This is a gap of the Kit.
//! * **Language.** GPUI offers no way to give an element a language, so the
//!   language of a book cannot be handed to the text.

mod alt;
mod blocks;
mod figures;
mod layout;
mod notes;
mod pages;
#[cfg(test)]
mod tests;
mod wheel;

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::base::animation::ease_in_cubic;
use gpui_kit::base::motion::{MotionTransform, Sequence, Transition};
use gpui_kit::base::text::{
    MarkdownExtensions, RangeHighlight, TextView, TextViewState, TextViewStyle,
};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, ClickEvent, Context, DispatchPhase, ElementId, Entity,
    EventEmitter, FontWeight, HitboxBehavior, Hsla, ImageCacheError, ImageSource,
    InteractiveElement as _, IntoElement, ListOffset, ListState, MouseButton, ParentElement as _,
    Pixels, Render, RenderImage, Role, ScrollDelta, ScrollEvent, SharedString, SharedUri,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, TestSupportExt as _,
    TouchPhase, WeakEntity, Window, canvas, div, point, px, relative, rems,
};

use self::figures::{BlockFigures, LineFigures, Stage, Table};
use self::pages::{Block, BlockKind, Measures, Page};
use self::wheel::{Travel, Wheel};
use crate::entities::catalogue::{ReadingFlow, Settings};
use crate::entities::figure::{FigureSources, Figures, Palette};
use crate::entities::picture::Bitmap;
use crate::features::appearance::Appearance;
use crate::shared::ui::page_colors;

pub(crate) use layout::picture_size;

/// How far a page slides while it is turned, in pixels.
const SLIDE: f32 = 28.0;
/// How long a page takes to leave.
const LEAVE: Duration = Duration::from_millis(90);
/// How long a page takes to arrive.
const ARRIVE: Duration = Duration::from_millis(220);
/// The room beside the text the Kit draws its scroll bar in, which a paged
/// text does not show: the bar is laid out there and cut off.
const SCROLLBAR_ROOM: f32 = 24.0;
/// The least width of a margin that shows where a click turns the page.
const MARGIN_HINT: f32 = 56.0;

/// How far a picture has come, as the owner of a text tells the view.
#[derive(Clone)]
pub(crate) enum PictureState {
    /// Not ready; its size in pixels is known from the tables of the book.
    Pending((u32, u32)),
    /// Ready to draw.
    Ready(Arc<RenderImage>),
    /// It cannot be shown; its alternative text is shown instead.
    Failed,
}

/// Finds a picture by its Markdown destination, available column width and,
/// in paged mode, available height in logical pixels. `None` for a destination
/// that is no picture of the book.
///
/// Called when the picture is drawn, so asking for it again is cheap and the
/// first ask is the request.
pub(crate) type PictureResolver = Arc<
    dyn Fn(&str, f32, Option<f32>, &mut Window, &mut App) -> Option<PictureState> + Send + Sync,
>;

/// What happens when a link of the text is followed, with its destination as
/// written.
pub(crate) type LinkHandler = Arc<dyn Fn(&str, &mut Window, &mut App) + Send + Sync>;

/// Which way to move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    /// Towards the end of the chapter.
    Forward,
    /// Towards its start.
    Back,
}

/// What the view tells the page around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChapterViewEvent {
    /// The place in the text changed, or the number of its pages.
    Moved,
    /// The reader asked for the next or the previous page with the wheel or
    /// with a click beside the text. The owner turns it, since past the last
    /// page it is the owner who knows what follows.
    Turn(Direction),
}

/// The turn of a page that is being drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Turn {
    /// The page that leaves; `None` when a page only arrives.
    from: Option<usize>,
    direction: Direction,
    /// Tells this turn from the one before it, for the motion of the Kit.
    id: u64,
}

/// The pages of a text that is shown a page at a time.
#[derive(Default)]
struct Paging {
    /// Available sheet height, independent of the visible page's extent.
    height: Rc<Cell<Pixels>>,
    /// The heights of the list items the pages were made from.
    heights: Vec<f32>,
    /// What the pages were divided by; `None` until they are made.
    measures: Option<Measures>,
    pages: Vec<Page>,
    /// The page that is shown.
    page: usize,
    /// Where the page is found again when the pages change: a list item and
    /// how far into it, from 0 to 1.
    anchor: (usize, f32),
    /// Whether the text has been shown since it was set.
    arrived: bool,
    turn: Option<Turn>,
    turns: u64,
    wheel: Wheel,
}

/// Where the text is to be put once it is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Landing {
    Offset(u32),
    Start,
    End,
}

/// How many frames a landing waits for the text before it gives up.
const LANDING_FRAMES: u32 = 180;

/// A failed picture is found while drawing, so it is collected where the
/// closure of the Kit can reach it.
type Failures = Arc<Mutex<HashSet<String>>>;
type Placeholders = Arc<Mutex<HashMap<(u32, u32), Arc<RenderImage>>>>;

/// The text of one chapter.
pub(crate) struct ChapterView {
    state: Entity<TextViewState>,
    /// The Markdown the owner gave.
    text: SharedString,
    /// What the Kit shows: `text`, with failed pictures written as their
    /// alternative text. It has the length of `text`.
    shown: SharedString,
    /// The offsets of the top-level blocks of `text`.
    blocks: Vec<u32>,
    blocks_ready: bool,
    label: SharedString,
    pictures: Option<PictureResolver>,
    picture_height: Option<f32>,
    /// The diagrams and formulas the text shows, as the owner gave them.
    figure_sources: Arc<FigureSources>,
    /// What was drawn of them, where the plugins of the Kit reach it.
    figures: Table,
    /// The colours the figures in the table were drawn in; `None` while
    /// those of `figure_sources` are not drawn.
    drawn_in: Option<Palette>,
    /// The colours the figures are being drawn in.
    drawing_in: Option<Palette>,
    /// The text is kept from the Kit until its figures are drawn.
    held: bool,
    figure_task: Option<Task<()>>,
    on_link: Option<LinkHandler>,
    note_definitions: Arc<HashMap<String, u32>>,
    on_note: Option<notes::FollowNote>,
    placeholders: Placeholders,
    failures: Failures,
    landing: Option<Landing>,
    /// Frames spent waiting for the landing, and whether the text was seen
    /// laid out.
    waited: u32,
    laid_out: bool,
    /// Tells a result of the background from one for a text that is gone.
    generation: u64,
    /// `Some` when the text is shown a page at a time.
    paging: Option<Paging>,
    /// Only a geometry change invalidates measured text.
    geometry: Option<Geometry>,
    search: Option<Range<usize>>,
    applied_search: Option<Hsla>,
    observed_scroll: Rc<Cell<Option<ListOffset>>>,
    blocks_task: Option<Task<()>>,
    alt_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

#[derive(PartialEq)]
struct Geometry {
    column: f32,
    size: f32,
    line: f32,
    gap: f32,
    weight: FontWeight,
    font: SharedString,
}

impl Paging {
    /// The page that shows the anchor, in the pages as they are now.
    fn page_of_anchor(&self, measures: Measures) -> usize {
        let (item, part) = self.anchor;
        let height = self.heights.get(item).copied().unwrap_or(0.0).max(0.0);
        let top: f32 = self.heights.iter().take(item).map(|h| h.max(0.0)).sum();
        // A little way into the line the anchor is on: a place that was kept
        // as a byte offset comes back a fraction of a line early.
        let into = (measures.line / 2.0)
            .min(height * (1.0 - part) / 2.0)
            .max(0.0);
        pages::page_at(&self.pages, top + height * part + into)
    }

    /// Remembers where the page that is shown starts.
    fn keep_anchor(&mut self) {
        let Some(page) = self.pages.get(self.page) else {
            self.anchor = (0, 0.0);
            return;
        };
        let height = self.heights.get(page.item).copied().unwrap_or(0.0);
        let part = if height > 0.0 {
            (page.offset / height).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.anchor = (page.item, part);
    }
}

/// The height of every item of `list`, or `None` while one of them has not
/// been measured. The list answers for the items from its top on, so it is
/// asked from the first item and put back where it was.
fn measured_heights(list: &ListState) -> Option<Vec<f32>> {
    let at = list.logical_scroll_top();
    let start = ListOffset {
        item_ix: 0,
        offset_in_item: px(0.0),
    };
    let moved = at.item_ix != 0 || at.offset_in_item != px(0.0);
    if moved {
        list.scroll_to(start);
    }
    let heights = (0..list.item_count())
        .map(|item| {
            list.bounds_for_item(item)
                .map(|bounds| bounds.size.height.as_f32())
        })
        .collect();
    if moved {
        list.scroll_to(at);
    }
    heights
}

impl EventEmitter<ChapterViewEvent> for ChapterView {}

impl ChapterView {
    /// An empty view whose text is scrolled.
    pub(crate) fn new(_window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        Self::build(None, cx)
    }

    /// An empty view whose text is shown a page at a time.
    pub(crate) fn paged(_window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        Self::build(Some(Paging::default()), cx)
    }

    fn build(paging: Option<Paging>, cx: &mut Context<'_, Self>) -> Self {
        let state = cx.new(|cx| TextViewState::markdown("", cx));
        let mut subscriptions = vec![cx.observe(&state, |_, _, cx| cx.notify())];
        if let Some(appearance) = Appearance::try_global(cx) {
            subscriptions.push(cx.observe(&appearance, |_, _, cx| cx.notify()));
        }
        Self {
            state,
            text: SharedString::default(),
            shown: SharedString::default(),
            blocks: Vec::new(),
            blocks_ready: true,
            label: SharedString::default(),
            pictures: None,
            picture_height: None,
            figure_sources: Arc::default(),
            figures: Table::default(),
            drawn_in: None,
            drawing_in: None,
            held: false,
            figure_task: None,
            on_link: None,
            note_definitions: Arc::default(),
            on_note: None,
            placeholders: Arc::default(),
            failures: Arc::default(),
            landing: None,
            waited: 0,
            laid_out: false,
            generation: 0,
            paging,
            geometry: None,
            search: None,
            applied_search: None,
            observed_scroll: Rc::default(),
            blocks_task: None,
            alt_task: None,
            _subscriptions: subscriptions,
        }
    }

    /// Changes the reader layout around the current source position.
    pub(crate) fn set_paged(&mut self, paged: bool, cx: &mut Context<'_, Self>) {
        if paged == self.paging.is_some() {
            return;
        }
        let offset = self.top_offset(cx);
        self.paging = paged.then(Paging::default);
        self.picture_height = None;
        self.state.read(cx).list_state().remeasure();
        self.land(Landing::Offset(offset), cx);
    }

    /// Whether the text is shown a page at a time.
    pub(crate) fn is_paged(&self) -> bool {
        self.paging.is_some()
    }

    /// Shows `markdown`. The blocks are found on the background; until they
    /// are, a place in the text is kept for later.
    pub(crate) fn set_text(&mut self, markdown: &str, cx: &mut Context<'_, Self>) {
        self.install(markdown, None, cx);
    }

    /// Shows `markdown` whose top-level blocks start at `blocks`, as `mbook`
    /// found them (`PreparedChapter::blocks`).
    pub(crate) fn set_chapter(
        &mut self,
        markdown: &str,
        blocks: Vec<u32>,
        cx: &mut Context<'_, Self>,
    ) {
        self.install(markdown, Some(blocks), cx);
    }

    /// Names the text for assistive technology: the title of the chapter.
    pub(crate) fn set_label(&mut self, label: impl Into<SharedString>, cx: &mut Context<'_, Self>) {
        let label = label.into();
        if self.label != label {
            self.label = label;
            cx.notify();
        }
    }

    /// Highlights a search match in the current chapter's source.
    pub(crate) fn set_search(&mut self, range: Option<Range<usize>>, cx: &mut Context<'_, Self>) {
        self.search = range;
        self.applied_search = None;
        self.state.update(cx, TextViewState::clear_range_highlights);
        cx.notify();
    }

    fn sync_search(&mut self, color: Hsla, cx: &mut Context<'_, Self>) {
        let Some(source) = self.search.clone() else {
            return;
        };
        if self.applied_search == Some(color) || self.landing.is_some() || !self.parsed(cx) {
            return;
        }
        let first = self.applied_search.is_none();
        let scroll = self.paging.is_none() && first;
        self.applied_search = Some(color);
        self.state.update(cx, |state, cx| {
            let document = state.rendered_text();
            if let Some(range) = document.range_for_source(source) {
                if scroll {
                    state.reveal_range(range.clone(), cx).ok();
                }
                state
                    .set_range_highlights([RangeHighlight::new(range, color)], cx)
                    .ok();
            }
        });
    }

    /// Sets the note definitions for the next chapter.
    pub(crate) fn set_notes(&mut self, notes: HashMap<String, u32>) {
        self.note_definitions = Arc::new(notes);
    }

    /// Sets the handler for activating an inline footnote marker.
    pub(crate) fn on_note(
        &mut self,
        follow: impl Fn(u32, &mut Window, &mut App) + Send + Sync + 'static,
    ) {
        self.on_note = Some(Arc::new(follow));
    }

    /// Tells where the pictures of the text come from.
    pub(crate) fn set_pictures(&mut self, resolver: PictureResolver, cx: &mut Context<'_, Self>) {
        self.pictures = Some(resolver);
        cx.notify();
    }

    /// Tells which diagrams and formulas the text shows and what their
    /// sources are. They are drawn on the background, in the colours of the
    /// page. Call it before the text is set: a text that is set while its
    /// figures are not drawn is shown when they are.
    pub(crate) fn set_figures(&mut self, sources: FigureSources, cx: &mut Context<'_, Self>) {
        if *self.figure_sources == sources {
            return;
        }
        self.figure_sources = Arc::new(sources);
        self.drawn_in = None;
        self.drawing_in = None;
        drop(self.figure_task.take());
        if self.figure_sources.is_empty() {
            // Nothing to draw: the text is parsed again without figures.
            self.figures_drawn(None, Figures::default(), cx);
        }
        cx.notify();
    }

    /// Tells what to do when a link is followed.
    pub(crate) fn on_link(
        &mut self,
        handler: impl Fn(&str, &mut Window, &mut App) + Send + Sync + 'static,
    ) {
        self.on_link = Some(Arc::new(handler));
    }

    /// Draws the text again: a picture arrived.
    pub(crate) fn pictures_changed(&mut self, cx: &mut Context<'_, Self>) {
        self.state.update(cx, |_, cx| cx.notify());
        cx.notify();
    }

    /// Shows the place of the byte `offset` of the source: scrolled, the
    /// block that contains it is at the top; paged, the page that shows it is
    /// the page. Done as soon as the text is laid out.
    pub(crate) fn reveal(&mut self, offset: u32, cx: &mut Context<'_, Self>) {
        self.land(Landing::Offset(offset), cx);
    }

    /// Goes to the start of the text.
    pub(crate) fn reveal_start(&mut self, cx: &mut Context<'_, Self>) {
        self.land(Landing::Start, cx);
    }

    /// Goes to the end of the text: scrolled, its last line is at the bottom;
    /// paged, the last page is the page.
    pub(crate) fn reveal_end(&mut self, cx: &mut Context<'_, Self>) {
        self.land(Landing::End, cx);
    }

    /// The byte offset of the reading place, estimated within the top block.
    /// A paged view can also retain a target within the visible page. While
    /// the text is being laid out, returns its pending destination.
    pub(crate) fn top_offset(&self, cx: &App) -> u32 {
        match self.landing {
            Some(Landing::Offset(offset)) => return offset,
            Some(Landing::Start) => return 0,
            Some(Landing::End) => return u32::try_from(self.text.len()).unwrap_or(u32::MAX),
            None => {}
        }
        let list = self.state.read(cx).list_state();
        if let Some(paging) = &self.paging {
            let (item, part) = paging.anchor;
            return blocks::offset_of_place(
                &self.blocks,
                self.text.len(),
                list.item_count(),
                item,
                part,
            );
        }
        let top = list.logical_scroll_top();
        let part = list.bounds_for_item(top.item_ix).map_or(0.0, |bounds| {
            let height = bounds.size.height.as_f32();
            if height > 0.0 {
                top.offset_in_item.as_f32() / height
            } else {
                0.0
            }
        });
        blocks::offset_of_place(
            &self.blocks,
            self.text.len(),
            list.item_count(),
            top.item_ix,
            part,
        )
    }

    /// Moves on: paged, to the next or the previous page, which does nothing
    /// on the last or the first; scrolled, by a screen less a line of the
    /// text, so that the eye keeps its place.
    pub(crate) fn turn_page(&mut self, direction: Direction, cx: &mut Context<'_, Self>) {
        if self.paging.is_some() {
            self.turn_to_neighbour(direction, cx);
            return;
        }
        let list = self.state.read(cx).list_state().clone();
        let line = Appearance::try_global(cx).map_or(px(0.0), |appearance| {
            let settings = appearance.read(cx).settings();
            px(settings.text_size * settings.line_height)
        });
        let screen: Pixels = (list.viewport_bounds().size.height - line).max(px(1.0));
        list.scroll_by(match direction {
            Direction::Forward => screen,
            Direction::Back => -screen,
        });
        self.landing = None;
        cx.emit(ChapterViewEvent::Moved);
        cx.notify();
    }

    /// The page that is shown, counted from one, and how many the text has.
    /// `None` for a text that is scrolled, or not divided into pages yet.
    pub(crate) fn page_number(&self) -> Option<(usize, usize)> {
        let paging = self.paging.as_ref()?;
        (paging.measures.is_some() && self.landing.is_none())
            .then(|| (paging.page + 1, paging.pages.len().max(1)))
    }

    /// Whether the end of the text is in the window: paged, whether the last
    /// page is the page. A text that is not laid out yet is not at its end.
    pub(crate) fn at_end(&self, cx: &App) -> bool {
        if self.landing.is_some() || !self.parsed(cx) {
            return false;
        }
        if let Some(paging) = &self.paging {
            return paging.measures.is_some() && paging.page + 1 >= paging.pages.len();
        }
        let list = self.state.read(cx).list_state();
        if list.viewport_bounds().size.height <= px(0.0) {
            return false;
        }
        list.is_scrolled_to_end()
            .unwrap_or_else(|| list.max_offset_for_scrollbar().y <= px(1.0))
    }

    /// Whether the Kit has parsed the text that was set last.
    fn parsed(&self, cx: &App) -> bool {
        self.state.read(cx).rendered_text().source() == self.shown.as_ref()
    }

    /// Whether the start of the text is at the top of the window.
    pub(crate) fn at_start(&self, cx: &App) -> bool {
        if matches!(self.landing, Some(Landing::Start)) {
            return true;
        }
        if self.landing.is_some() {
            return false;
        }
        if let Some(paging) = &self.paging {
            return paging.page == 0;
        }
        let top = self.state.read(cx).list_state().logical_scroll_top();
        top.item_ix == 0 && top.offset_in_item <= px(0.5)
    }

    /// The Markdown the owner gave last, for tests.
    #[cfg(test)]
    pub(crate) fn source(&self) -> &SharedString {
        &self.text
    }

    /// The text the Kit shows, as it renders it, for tests.
    #[cfg(test)]
    pub(crate) fn shown_text(&self, cx: &App) -> String {
        self.state.read(cx).rendered_text().as_str().to_owned()
    }

    /// The sizes of the placeholders made so far, for tests.
    #[cfg(test)]
    fn placeholder_sizes(&self) -> Vec<(u32, u32)> {
        self.placeholders.lock().map_or_else(
            |_| Vec::new(),
            |placeholders| placeholders.keys().copied().collect(),
        )
    }

    /// The number of list items of the Kit's view, for tests.
    #[cfg(test)]
    fn item_count(&self, cx: &App) -> usize {
        self.state.read(cx).list_state().item_count()
    }

    fn install(&mut self, markdown: &str, blocks: Option<Vec<u32>>, cx: &mut Context<'_, Self>) {
        self.generation += 1;
        self.text = markdown.to_owned().into();
        self.observed_scroll.set(None);
        self.shown = self.text.clone();
        // A paged text is hidden until it is divided, so it always has a
        // place to be put at.
        self.landing = self.paging.is_some().then_some(Landing::Start);
        if let Some(paging) = &mut self.paging {
            let turns = paging.turns;
            *paging = Paging {
                turns,
                ..Paging::default()
            };
        }
        self.laid_out = false;
        self.waited = 0;
        drop(self.alt_task.take());
        if let Ok(mut failures) = self.failures.lock() {
            failures.clear();
        }
        if let Ok(mut placeholders) = self.placeholders.lock() {
            placeholders.clear();
        }
        if let Some(given) = blocks {
            self.blocks = given;
            self.blocks_ready = true;
            drop(self.blocks_task.take());
        } else {
            self.blocks = Vec::new();
            self.blocks_ready = false;
            let text = self.text.clone();
            let generation = self.generation;
            self.blocks_task = Some(cx.spawn(async move |this, cx| {
                let found = cx
                    .background_spawn(async move { blocks::block_offsets(&text) })
                    .await;
                this.update(cx, |this, cx| {
                    if this.generation == generation {
                        this.blocks = found;
                        this.blocks_ready = true;
                        cx.notify();
                    }
                })
                .ok();
            }));
        }
        self.held = !self.figure_sources.is_empty() && self.drawn_in.is_none();
        self.feed(cx);
        cx.notify();
    }

    /// Hands the Kit the text to show: nothing while the figures of the text
    /// are not drawn.
    fn feed(&mut self, cx: &mut Context<'_, Self>) {
        let shown = if self.held {
            SharedString::default()
        } else {
            self.shown.clone()
        };
        self.state
            .update(cx, |state, cx| state.set_text(&shown, cx));
    }

    /// Draws the figures of the text in the colours of the page, unless they
    /// are drawn in them or are being drawn.
    fn draw_figures(&mut self, palette: Palette, cx: &mut Context<'_, Self>) {
        if self.figure_sources.is_empty()
            || self.drawn_in == Some(palette)
            || self.drawing_in == Some(palette)
        {
            return;
        }
        self.drawing_in = Some(palette);
        let sources = Arc::clone(&self.figure_sources);
        self.figure_task = Some(cx.spawn(async move |this, cx| {
            let wanted = Arc::clone(&sources);
            let figures = cx
                .background_spawn(async move { Figures::draw(&wanted, palette) })
                .await;
            this.update(cx, |this, cx| {
                // The owner may have named other figures meanwhile.
                if Arc::ptr_eq(&this.figure_sources, &sources) {
                    this.figures_drawn(Some(palette), figures, cx);
                }
            })
            .ok();
        }));
    }

    /// Takes the figures that were drawn. The Kit parses the text again, since
    /// which of its pictures are figures is decided while it is parsed, and
    /// the text keeps its place.
    fn figures_drawn(
        &mut self,
        palette: Option<Palette>,
        figures: Figures,
        cx: &mut Context<'_, Self>,
    ) {
        self.drawing_in = None;
        self.drawn_in = palette;
        self.figures.set(Arc::new(figures));
        let at = self.top_offset(cx);
        let waited = std::mem::take(&mut self.held);
        let shown = self.shown.clone();
        self.state.update(cx, |state, cx| {
            // The same text is no new text to the Kit: it is taken away first.
            state.set_text("", cx);
            state.set_text(&shown, cx);
        });
        if waited {
            // The text was never shown: the place it waits for stands.
            if self.landing.is_none() {
                self.land(Landing::Start, cx);
            }
        } else if self.landing.is_none() {
            self.land(Landing::Offset(at), cx);
        }
        cx.notify();
    }

    fn land(&mut self, landing: Landing, cx: &mut Context<'_, Self>) {
        self.landing = Some(landing);
        self.laid_out = false;
        self.waited = 0;
        cx.notify();
    }

    /// A picture of the text cannot be shown: write its alternative text.
    fn failures_changed(&mut self, cx: &mut Context<'_, Self>) {
        let failed: HashSet<String> = self
            .failures
            .lock()
            .map_or_else(|_| HashSet::new(), |failures| failures.clone());
        let text = self.text.clone();
        let generation = self.generation;
        self.alt_task = Some(cx.spawn(async move |this, cx| {
            let replaced = cx
                .background_spawn(async move { alt::with_alt_text(&text, &failed) })
                .await;
            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                let Some(replaced) = replaced else {
                    return;
                };
                let at = this.top_offset(cx);
                this.shown = replaced.into();
                this.feed(cx);
                this.land(Landing::Offset(at), cx);
            })
            .ok();
        }));
    }

    /// Puts the text where it is to be, once it is laid out. Runs before the
    /// Kit lays the text out in each frame.
    fn settle(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(landing) = self.landing else {
            return;
        };
        if self.held {
            // The figures of the text are being drawn; their arrival draws
            // the next frame.
            return;
        }
        self.waited += 1;
        if self.waited > LANDING_FRAMES {
            self.landing = None;
            return;
        }
        let ready = self.blocks_ready
            && self.parsed(cx)
            && (self.shown.is_empty() || self.state.read(cx).list_state().item_count() > 0);
        if !ready {
            window.request_animation_frame();
            return;
        }
        if !self.laid_out {
            // The Kit resets its list while it lays out the first frame of
            // new text, which is after this; the place is set in the next.
            self.laid_out = true;
            window.request_animation_frame();
            return;
        }
        let list = self.state.read(cx).list_state().clone();
        if self.paging.is_some() {
            if !self.land_on_page(landing, &list) {
                window.request_animation_frame();
                return;
            }
            self.landing = None;
            cx.emit(ChapterViewEvent::Moved);
            cx.notify();
            return;
        }
        match landing {
            Landing::Start => list.scroll_to(ListOffset {
                item_ix: 0,
                offset_in_item: px(0.0),
            }),
            Landing::End => list.scroll_to_end(),
            Landing::Offset(offset) => {
                let (item_ix, part) = blocks::place_of_offset(
                    &self.blocks,
                    self.text.len(),
                    list.item_count(),
                    offset,
                );
                list.scroll_to(ListOffset {
                    item_ix,
                    offset_in_item: px(0.0),
                });
                if let Some(bounds) = list.bounds_for_item(item_ix) {
                    list.scroll_to(ListOffset {
                        item_ix,
                        offset_in_item: bounds.size.height * part,
                    });
                } else if part > 0.0 && self.waited < LANDING_FRAMES {
                    window.request_animation_frame();
                    return;
                }
            }
        }
        self.landing = None;
        cx.emit(ChapterViewEvent::Moved);
        cx.notify();
    }

    /// Puts a paged text at `landing`. `false` while the text is not divided
    /// into pages yet.
    fn land_on_page(&mut self, landing: Landing, list: &ListState) -> bool {
        let (len, items) = (self.text.len(), list.item_count());
        let Some(paging) = &mut self.paging else {
            return false;
        };
        let Some(measures) = paging.measures else {
            return false;
        };
        if paging.heights.len() != items {
            return false;
        }
        paging.page = match landing {
            Landing::Start => 0,
            Landing::End => paging.pages.len().saturating_sub(1),
            Landing::Offset(offset) => {
                paging.anchor = blocks::place_of_offset(&self.blocks, len, items, offset);
                paging.page_of_anchor(measures)
            }
        };
        if !matches!(landing, Landing::Offset(_)) {
            paging.keep_anchor();
        }
        if !paging.arrived {
            // The text is shown for the first time: it slides in from the
            // side it was reached from.
            paging.arrived = true;
            paging.turns += 1;
            paging.turn = Some(Turn {
                from: None,
                direction: match landing {
                    Landing::End => Direction::Back,
                    Landing::Start | Landing::Offset(_) => Direction::Forward,
                },
                id: paging.turns,
            });
        }
        true
    }

    /// Turns to the page after or before the one that is shown.
    fn turn_to_neighbour(&mut self, direction: Direction, cx: &mut Context<'_, Self>) {
        if self.landing.is_some() {
            return;
        }
        let Some(paging) = &mut self.paging else {
            return;
        };
        let target = match direction {
            Direction::Forward => paging.page + 1,
            Direction::Back => match paging.page.checked_sub(1) {
                Some(page) => page,
                None => return,
            },
        };
        if paging.measures.is_none() || target >= paging.pages.len() {
            return;
        }
        paging.turns += 1;
        paging.turn = Some(Turn {
            // A turn that follows one still being drawn does not wait for a
            // page to leave: a held key shows page after page.
            from: paging.turn.is_none().then_some(paging.page),
            direction,
            id: paging.turns,
        });
        paging.page = target;
        paging.keep_anchor();
        cx.emit(ChapterViewEvent::Moved);
        cx.notify();
    }

    /// Divides a paged text into pages again when the heights of its blocks,
    /// the height of the window or the lines of the text are not what the
    /// pages were made from. Runs before a frame is drawn, on what the frame
    /// before laid out.
    fn repaginate(&mut self, line: f32, gap: f32, cx: &mut Context<'_, Self>) {
        let Some(paging) = &self.paging else {
            return;
        };
        // The list still holds the text before this one until the Kit has
        // laid the new one out.
        let stale = paging.measures.is_none() && !self.laid_out;
        if stale || !self.parsed(cx) {
            return;
        }
        let list = self.state.read(cx).list_state().clone();
        let page = paging.height.get().as_f32();
        let Some(heights) = measured_heights(&list) else {
            return;
        };
        if page < 1.0 {
            return;
        }
        let measures = Measures { page, line, gap };
        let kinds = self.kinds(heights.len());
        let Some(paging) = &mut self.paging else {
            return;
        };
        if paging.measures == Some(measures) && paging.heights == heights {
            return;
        }
        let blocks: Vec<Block> = heights
            .iter()
            .zip(kinds)
            .map(|(&height, kind)| Block { height, kind })
            .collect();
        let before = (paging.page, paging.pages.len());
        paging.pages = pages::paginate(&blocks, measures);
        paging.heights = heights;
        paging.measures = Some(measures);
        paging.page = paging.page_of_anchor(measures);
        if before != (paging.page, paging.pages.len()) {
            cx.emit(ChapterViewEvent::Moved);
        }
    }

    /// The kind of each of the `items` blocks of the list, by its first line.
    /// When the Kit and the owner do not agree on the blocks, each counts as
    /// a paragraph, and is divided only if its height is whole lines.
    fn kinds(&self, items: usize) -> Vec<BlockKind> {
        if self.blocks.len() != items {
            return vec![BlockKind::Text; items];
        }
        self.blocks
            .iter()
            .map(|&start| {
                self.shown
                    .get(start as usize..)
                    .map_or(BlockKind::Text, pages::kind_of)
            })
            .collect()
    }

    /// The page to draw in this frame and how it is moved: the page that is
    /// shown, or the one that leaves while a turn has just begun.
    fn sample_turn(&mut self, window: &mut Window, cx: &mut App) -> (usize, MotionTransform) {
        let rest = MotionTransform::identity();
        let Some(paging) = &mut self.paging else {
            return (0, rest);
        };
        let Some(turn) = paging.turn else {
            return (paging.page, rest);
        };
        let slide = match turn.direction {
            Direction::Forward => SLIDE,
            Direction::Back => -SLIDE,
        };
        let aside = |x: f32| MotionTransform {
            translation: point(px(x), px(0.0)),
            opacity: 0.0,
            ..rest
        };
        let id = ElementId::NamedInteger("page-turn".into(), turn.id);
        let arrive = Transition::new(ARRIVE);
        let sequence = match turn.from {
            Some(_) => Sequence::new(id, rest)
                .with_step(aside(-slide), Transition::new(LEAVE).ease(ease_in_cubic))
                .with_step(aside(slide), Transition::new(Duration::ZERO))
                .with_step(rest, arrive),
            None => Sequence::new(id, aside(slide)).with_step(rest, arrive),
        };
        let sample = sequence.sample(window, cx);
        let leaving = turn
            .from
            .filter(|_| sample.step() == 0 && !sample.is_finished());
        if sample.is_finished() {
            paging.turn = None;
        }
        (leaving.unwrap_or(paging.page), *sample.value())
    }

    /// The wheel moved over a paged text: it does not scroll, it asks for a
    /// page.
    fn on_wheel(&mut self, event: &ScrollEvent, cx: &mut Context<'_, Self>) {
        let Some(paging) = &mut self.paging else {
            return;
        };
        let travel = match event.delta {
            ScrollDelta::Lines(lines) => Travel::Lines(wheel::dominant(lines.x, lines.y)),
            ScrollDelta::Pixels(pixels) => {
                Travel::Pixels(wheel::dominant(pixels.x.as_f32(), pixels.y.as_f32()))
            }
        };
        let begins = event.touch_phase == TouchPhase::Started;
        let now = cx.background_executor().now();
        if let Some(direction) = paging.wheel.feed(travel, begins, now) {
            cx.emit(ChapterViewEvent::Turn(direction));
        }
    }

    /// The source of one image of the text.
    fn image_source(
        &self,
        column: f32,
        cx: &Context<'_, Self>,
    ) -> impl Fn(&SharedUri) -> ImageSource + Send + Sync + 'static + use<> {
        let resolver = self.pictures.clone();
        let height = self.picture_height;
        let placeholders = Arc::clone(&self.placeholders);
        let failures = Arc::clone(&self.failures);
        let this = cx.entity().downgrade();
        move |uri| {
            let destination = uri.to_string();
            let resolver = resolver.clone();
            let placeholders = Arc::clone(&placeholders);
            let failures = Arc::clone(&failures);
            let this = this.clone();
            ImageSource::Custom(Arc::new(move |window, cx| {
                if column < 1.0 || height.is_some_and(|height| height < 1.0) {
                    return None;
                }
                let state = resolver
                    .as_ref()
                    .and_then(|resolver| resolver(&destination, column, height, window, cx));
                picture_image(
                    state,
                    (column, height),
                    &destination,
                    &placeholders,
                    &failures,
                    &this,
                    cx,
                )
            }))
        }
    }
}

/// What the Kit is given for a picture in `state`.
fn picture_image(
    state: Option<PictureState>,
    (column, height): (f32, Option<f32>),
    destination: &str,
    placeholders: &Placeholders,
    failures: &Failures,
    view: &WeakEntity<ChapterView>,
    cx: &mut App,
) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
    match state {
        Some(PictureState::Ready(image)) => Some(Ok(image)),
        Some(PictureState::Pending(known)) => {
            let size = picture_size(known, column, height)?;
            let mut placeholders = placeholders.lock().ok()?;
            let image = placeholders
                .entry(size)
                .or_insert_with(|| placeholder_image(size));
            Some(Ok(Arc::clone(image)))
        }
        Some(PictureState::Failed) => {
            let new = failures
                .lock()
                .is_ok_and(|mut failures| failures.insert(destination.to_owned()));
            if new {
                let view = view.clone();
                cx.defer(move |cx| {
                    view.update(cx, ChapterView::failures_changed).ok();
                });
            }
            Some(Err(ImageCacheError::Asset(
                "the picture cannot be shown".into(),
            )))
        }
        // Not a picture of the book: never fetched from the network.
        None => Some(Err(ImageCacheError::Asset(
            "the picture is not part of the book".into(),
        ))),
    }
}

/// A transparent image of `size` pixels, to hold the place of a picture.
fn placeholder_image(size: (u32, u32)) -> Arc<RenderImage> {
    let pixels = vec![0; size.0 as usize * size.1 as usize * 4];
    Bitmap::from_rgba(size.0, size.1, pixels).map_or_else(
        || Arc::new(RenderImage::new(Vec::new())),
        Bitmap::into_render_image,
    )
}

pub(crate) fn reading_column(
    window: &Window,
    cx: &App,
    settings: &Settings,
    class: crate::shared::layout::SizeClass,
    family: &SharedString,
) -> f32 {
    let mut font = gpui_kit::font(family.clone());
    font.weight = reading_weight(settings);
    let system = cx.text_system();
    let zero = system
        .advance(system.resolve_font(&font), px(settings.text_size), '0')
        .map_or(settings.text_size * 0.54, |advance| advance.width.as_f32());
    let insets = window.insets().effective();
    let viewport = (window.viewport_size().width - insets.left - insets.right).as_f32();
    layout::column_width(viewport, zero, settings.reading.column, class)
}

fn reading_weight(settings: &Settings) -> FontWeight {
    if settings.reading.heavier {
        FontWeight::MEDIUM
    } else {
        FontWeight::NORMAL
    }
}

impl ChapterView {
    fn measure(&mut self, geometry: Geometry, cx: &App) {
        let line = geometry.line;
        let gap = geometry.gap;
        if self.geometry.as_ref() != Some(&geometry) {
            self.geometry = Some(geometry);
            self.state.read(cx).list_state().remeasure();
        }
        if let Some(paging) = &self.paging
            && (self.pictures.is_some() || !self.figure_sources.is_empty())
        {
            let list = self.state.read(cx).list_state();
            let page = paging.height.get().as_f32();
            // Leave room for the paragraph gap and the line that can precede an inline image.
            let height = if page >= 1.0 {
                (page - gap - line).max(1.0)
            } else {
                0.0
            };
            if self.picture_height != Some(height) {
                self.picture_height = Some(height);
                list.remeasure();
            }
        }
    }
}

impl Render for ChapterView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let (settings, tone, class, font) = match Appearance::try_global(cx) {
            Some(appearance) => {
                let appearance = appearance.read(cx);
                (
                    appearance.settings().clone(),
                    Some(appearance.page_tone(window)),
                    appearance.size_class(window),
                    appearance.reading_font(cx),
                )
            }
            None => (
                Settings::default(),
                None,
                crate::shared::layout::SizeClass::Expanded,
                cx.theme().font_family.clone(),
            ),
        };
        // What GPUI makes of a relative line height: whole pixels.
        let line = (settings.text_size * settings.line_height).round();
        let gap = settings.text_size * settings.reading.paragraph_gap;
        let weight = reading_weight(&settings);
        let column = reading_column(window, cx, &settings, class, &font);
        self.measure(
            Geometry {
                column,
                size: settings.text_size,
                line,
                gap,
                weight,
                font: font.clone(),
            },
            cx,
        );
        self.repaginate(line, gap, cx);
        self.settle(window, cx);
        let (page, ink) = tone.map_or((cx.theme().background, cx.theme().foreground), page_colors);
        self.draw_figures(
            tone.map_or_else(|| Palette::of(page, ink), Palette::from),
            cx,
        );
        let viewport = window.viewport_size().width.as_f32();
        let on_link = self.on_link.clone();
        let stage = Stage {
            table: self.figures.clone(),
            column,
            page: self.picture_height,
            text_size: settings.text_size,
            gap,
        };
        let extensions = self.extensions(stage, tone.map_or(ink, |tone| tone.colors().link.into()));
        let style = self.page_style(tone, gap, (page, ink), window, cx);
        let mut view = TextView::new(&self.state)
            .style(style)
            .scrollable(true)
            .selectable(true)
            .markdown_extensions(extensions)
            .image_source(self.image_source(column, cx))
            .text_size(px(settings.text_size))
            .line_height(relative(settings.line_height))
            .font_family(font)
            .font_weight(weight)
            .text_color(ink);
        if let Some(on_link) = on_link {
            view = view.on_link_click(move |destination, event, window, cx| {
                if followed(event) {
                    on_link(destination, window, cx);
                }
            });
        }
        let article = div()
            .id("chapter-text")
            .role(Role::Article)
            .aria_label(self.label.clone())
            .test_support()
            .size_full()
            .flex()
            .bg(page);
        if self.paging.is_none() {
            return article
                .relative()
                .justify_center()
                .child(div().h_full().w(px(column)).child(view))
                .child(self.watch_scroll(cx));
        }
        let hints = (viewport - column) / 2.0 >= MARGIN_HINT;
        article
            .relative()
            .child(Self::wheel_catcher(cx))
            .child(Self::margin(Direction::Back, hints, ink, cx))
            .child(self.sheet(view, column, window, cx))
            .child(Self::margin(Direction::Forward, hints, ink, cx))
    }
}

impl ChapterView {
    fn extensions(&self, stage: Stage, link_ink: Hsla) -> MarkdownExtensions {
        // The plugins read the table when the text is parsed, so the parser
        // is the same whatever the table holds: its revision never changes.
        let extensions = MarkdownExtensions::default()
            .plugin(BlockFigures(stage.clone()))
            .plugin(LineFigures(stage));
        let extensions = if let Some(follow) = self.on_note.clone()
            && !self.note_definitions.is_empty()
        {
            extensions.plugin(notes::Notes {
                definitions: self.note_definitions.clone(),
                follow,
                ink: link_ink,
            })
        } else {
            extensions
        };
        extensions.parser_revision(1)
    }

    fn page_style(
        &mut self,
        tone: Option<crate::shared::theme::PageTone>,
        gap: f32,
        (page, ink): (Hsla, Hsla),
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> TextViewStyle {
        let mut style =
            TextViewStyle::default().with_paragraph_gap(rems(gap / window.rem_size().as_f32()));
        if let Some(tone) = tone {
            let colors = tone.colors();
            self.sync_search(colors.search.into(), cx);
            style = style
                .with_dark(tone.is_dark())
                .with_code_background(page)
                .with_inline_code(gpui_kit::HighlightStyle {
                    background_color: Some(page),
                    ..Default::default()
                })
                .with_border(colors.muted.into())
                .with_foreground(ink)
                .with_muted_foreground(ink)
                .with_link(colors.link.into())
                .with_selection(colors.selection.into());
        }
        style
    }

    /// Samples the list after it lays out. The Kit can update its list
    /// directly, for example during a scrollbar drag or a touch gesture.
    fn watch_scroll(&self, cx: &Context<'_, Self>) -> impl IntoElement {
        let entity = cx.entity().downgrade();
        let previous = self.observed_scroll.clone();
        let list = self.state.read(cx).list_state().clone();
        canvas(
            move |_, window, cx| {
                let offset = list.logical_scroll_top();
                if previous.replace(Some(offset)) != Some(offset) {
                    let entity = entity.clone();
                    window.defer(cx, move |_, cx| {
                        entity
                            .update(cx, |view, cx| {
                                if !view.is_paged() && view.landing.is_none() {
                                    cx.emit(ChapterViewEvent::Moved);
                                }
                            })
                            .ok();
                    });
                }
            },
            |_, (), _, _| {},
        )
        .absolute()
        .size_0()
    }

    /// Shows one page at the top of a sheet of the available height.
    fn sheet(
        &mut self,
        view: TextView,
        column: f32,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let (shown, motion) = if Appearance::try_global(cx).is_some_and(|appearance| {
            appearance.read(cx).settings().reading.flow == ReadingFlow::AnimatedPages
        }) {
            self.sample_turn(window, cx)
        } else {
            let shown = self.paging.as_mut().map_or(0, |paging| {
                paging.turn = None;
                paging.page
            });
            (shown, MotionTransform::default())
        };
        let list = self.state.read(cx).list_state().clone();
        let ready = self.landing.is_none();
        let page = self
            .paging
            .as_ref()
            .filter(|paging| paging.measures.is_some())
            .and_then(|paging| paging.pages.get(shown).copied());
        if let Some(page) = page {
            let top = ListOffset {
                item_ix: page.item,
                offset_in_item: px(page.offset),
            };
            let at = list.logical_scroll_top();
            if at.item_ix != top.item_ix || at.offset_in_item != top.offset_in_item {
                list.scroll_to(top);
            }
        }
        let height = self.paging.as_ref().map(|paging| paging.height.clone());
        let seen_offset = list.max_offset_for_scrollbar().y;
        let watch = canvas(
            move |bounds, window, _| {
                let resized = height
                    .as_ref()
                    .is_some_and(|height| height.replace(bounds.size.height) != bounds.size.height);
                if resized || list.max_offset_for_scrollbar().y != seen_offset {
                    window.request_animation_frame();
                }
            },
            |_, (), _, _| {},
        )
        .absolute()
        .inset_0();
        let visible = ready && page.is_some();
        div()
            .relative()
            .flex_none()
            .w(px(column))
            .h_full()
            .left(motion.translation.x)
            .opacity(if visible { motion.opacity } else { 0.0 })
            .child(
                div()
                    .size_full()
                    // The list backfills a viewport taller than the remaining
                    // text, pulling in the preceding page near chapter end.
                    .when_some(page.filter(|page| page.extent > 0.0), |view, page| {
                        view.h(px(page.extent))
                    })
                    .overflow_hidden()
                    .child(view.w(px(column + SCROLLBAR_ROOM)).pr(px(SCROLLBAR_ROOM))),
            )
            .child(watch)
            .into_any_element()
    }

    /// Takes the wheel before the text does. The Kit scrolls its list while
    /// an event is on its way down, so the view claims the event earlier: a
    /// listener of an element that is painted first is asked first.
    fn wheel_catcher(cx: &mut Context<'_, Self>) -> AnyElement {
        let view = cx.entity().downgrade();
        canvas(
            |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |_, hitbox, window, _| {
                window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture && hitbox.should_handle_scroll(window) {
                        cx.stop_propagation();
                        view.update(cx, |view, cx| view.on_wheel(event, cx)).ok();
                    }
                });
            },
        )
        .absolute()
        .inset_0()
        .into_any_element()
    }

    /// The margin beside a paged text: a click on it turns the page, and
    /// while the pointer is over it, it shows which way.
    fn margin(
        direction: Direction,
        hints: bool,
        ink: Hsla,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let (group, icon) = match direction {
            Direction::Back => ("margin-back", IconName::ChevronLeft),
            Direction::Forward => ("margin-forward", IconName::ChevronRight),
        };
        div()
            .group(group)
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _, _, cx| cx.emit(ChapterViewEvent::Turn(direction))),
            )
            .children(hints.then(|| {
                div()
                    .text_color(ink)
                    .opacity(0.0)
                    .group_hover(group, |style| style.opacity(0.45))
                    .child(Icon::new(icon).large())
            }))
            .into_any_element()
    }
}

/// Whether a click on a link follows it: the main or middle button, a key, or
/// a tap that is not a long press. The Kit's own rule.
fn followed(event: &ClickEvent) -> bool {
    match event {
        ClickEvent::Mouse(click) => {
            matches!(click.up.button, MouseButton::Left | MouseButton::Middle)
        }
        ClickEvent::Keyboard(_) => true,
        ClickEvent::Touch(click) => !click.long_press,
    }
}
