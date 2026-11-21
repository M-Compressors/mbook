//! The text of a chapter, as the reader and the preview of the editor show it.
//!
//! [`ChapterView`] is the Markdown view of GPUI Kit in its scrolling,
//! virtualised form, set in the typeface, size, line height and page tone of
//! the [`Appearance`]. It knows nothing of files: the owner hands it Markdown,
//! and optionally a way to find the pictures of the text.
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
//!   keeps its place ([`layout::placeholder_size`]). A picture the owner does
//!   not know is not fetched: nothing is drawn for it and the view never
//!   reaches the network.
//! * **Structure for assistive technology.** The Kit's Markdown view reports
//!   no roles of its own (no heading levels, lists or tables), so the widget
//!   wraps the text in one named article. This is a gap of the Kit.
//! * **Language.** GPUI offers no way to give an element a language, so the
//!   language of a book cannot be handed to the text.

mod alt;
mod blocks;
mod layout;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::text::{TextView, TextViewState};
use gpui_kit::{
    App, AppContext as _, ClickEvent, Context, Entity, EventEmitter, ImageCacheError, ImageSource,
    InteractiveElement as _, IntoElement, ListOffset, MouseButton, ParentElement as _, Pixels,
    Render, RenderImage, Role, SharedString, SharedUri, StatefulInteractiveElement as _,
    Styled as _, Subscription, Task, TestSupportExt as _, WeakEntity, Window, div, px, relative,
};

use crate::entities::picture::Bitmap;
use crate::features::appearance::Appearance;
use crate::shared::ui::page_colors;

pub(crate) use layout::device_width;

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

/// Finds a picture of the text: its destination as written in the Markdown,
/// and the width of the text column in logical pixels. `None` for a
/// destination that is no picture of the book.
///
/// Called when the picture is drawn, so asking for it again is cheap and the
/// first ask is the request.
pub(crate) type PictureResolver =
    Arc<dyn Fn(&str, f32, &mut Window, &mut App) -> Option<PictureState> + Send + Sync>;

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
    /// The user scrolled the text.
    Scrolled,
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
    on_link: Option<LinkHandler>,
    placeholders: Placeholders,
    failures: Failures,
    landing: Option<Landing>,
    /// Frames spent waiting for the landing, and whether the text was seen
    /// laid out.
    waited: u32,
    laid_out: bool,
    /// Tells a result of the background from one for a text that is gone.
    generation: u64,
    blocks_task: Option<Task<()>>,
    alt_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ChapterViewEvent> for ChapterView {}

impl ChapterView {
    /// An empty view.
    pub(crate) fn new(_window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let state = cx.new(|cx| TextViewState::markdown("", cx));
        let this = cx.entity().downgrade();
        state
            .read(cx)
            .list_state()
            .set_scroll_handler(move |_, _, cx| {
                this.update(cx, |_, cx| cx.emit(ChapterViewEvent::Scrolled))
                    .ok();
            });
        let mut subscriptions = vec![cx.observe(&state, |_, _, cx| cx.notify())];
        if let Some(appearance) = Appearance::try_global(cx) {
            subscriptions.push(cx.observe(&appearance, |this, _, cx| this.restyled(cx)));
        }
        Self {
            state,
            text: SharedString::default(),
            shown: SharedString::default(),
            blocks: Vec::new(),
            blocks_ready: true,
            label: SharedString::default(),
            pictures: None,
            on_link: None,
            placeholders: Arc::default(),
            failures: Arc::default(),
            landing: None,
            waited: 0,
            laid_out: false,
            generation: 0,
            blocks_task: None,
            alt_task: None,
            _subscriptions: subscriptions,
        }
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

    /// Tells where the pictures of the text come from.
    pub(crate) fn set_pictures(&mut self, resolver: PictureResolver, cx: &mut Context<'_, Self>) {
        self.pictures = Some(resolver);
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

    /// Scrolls so that the block that contains the byte `offset` of the
    /// source is at the top. Done as soon as the text is laid out.
    pub(crate) fn reveal(&mut self, offset: u32, cx: &mut Context<'_, Self>) {
        self.land(Landing::Offset(offset), cx);
    }

    /// Scrolls to the start of the text.
    pub(crate) fn reveal_start(&mut self, cx: &mut Context<'_, Self>) {
        self.land(Landing::Start, cx);
    }

    /// Scrolls to the end of the text.
    pub(crate) fn reveal_end(&mut self, cx: &mut Context<'_, Self>) {
        self.land(Landing::End, cx);
    }

    /// The byte offset of the block at the top of the window; the place that
    /// is kept. While the text is not laid out yet, the place it is to be put
    /// at.
    pub(crate) fn top_offset(&self, cx: &App) -> u32 {
        match self.landing {
            Some(Landing::Offset(offset)) => return offset,
            Some(Landing::Start) => return 0,
            Some(Landing::End) => return u32::try_from(self.text.len()).unwrap_or(u32::MAX),
            None => {}
        }
        let list = self.state.read(cx).list_state();
        blocks::offset_of_item(
            &self.blocks,
            self.text.len(),
            list.item_count(),
            list.logical_scroll_top().item_ix,
        )
    }

    /// Moves by a screen, less a line of the text so that the eye keeps its
    /// place.
    pub(crate) fn scroll_by_screen(&mut self, direction: Direction, cx: &mut Context<'_, Self>) {
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
        cx.emit(ChapterViewEvent::Scrolled);
        cx.notify();
    }

    /// Whether the end of the text is in the window. A text that is not laid
    /// out yet is not at its end.
    pub(crate) fn at_end(&self, cx: &App) -> bool {
        if self.landing.is_some() || !self.parsed(cx) {
            return false;
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
        self.shown = self.text.clone();
        self.landing = None;
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
        let shown = self.shown.clone();
        self.state
            .update(cx, |state, cx| state.set_text(&shown, cx));
        cx.notify();
    }

    fn land(&mut self, landing: Landing, cx: &mut Context<'_, Self>) {
        self.landing = Some(landing);
        self.laid_out = false;
        self.waited = 0;
        cx.notify();
    }

    /// The style changed: the heights of the blocks are not what they were.
    fn restyled(&mut self, cx: &mut Context<'_, Self>) {
        let at = self.top_offset(cx);
        self.state.read(cx).list_state().remeasure();
        if self.landing.is_none() {
            self.land(Landing::Offset(at), cx);
        }
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
                let shown = this.shown.clone();
                this.state
                    .update(cx, |state, cx| state.set_text(&shown, cx));
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
        match landing {
            Landing::Start => list.scroll_to(ListOffset {
                item_ix: 0,
                offset_in_item: px(0.0),
            }),
            Landing::End => list.scroll_to_end(),
            Landing::Offset(offset) => list.scroll_to(ListOffset {
                item_ix: blocks::item_of_offset(
                    &self.blocks,
                    self.text.len(),
                    list.item_count(),
                    offset,
                ),
                offset_in_item: px(0.0),
            }),
        }
        self.landing = None;
        cx.emit(ChapterViewEvent::Scrolled);
        cx.notify();
    }

    /// The source of one image of the text.
    fn image_source(
        &self,
        column: f32,
        cx: &Context<'_, Self>,
    ) -> impl Fn(&SharedUri) -> ImageSource + Send + Sync + 'static + use<> {
        let resolver = self.pictures.clone();
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
                let state = resolver
                    .as_ref()
                    .and_then(|resolver| resolver(&destination, column, window, cx));
                picture_image(
                    state,
                    column,
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
    column: f32,
    destination: &str,
    placeholders: &Placeholders,
    failures: &Failures,
    view: &WeakEntity<ChapterView>,
    cx: &mut App,
) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
    match state {
        Some(PictureState::Ready(image)) => Some(Ok(image)),
        Some(PictureState::Pending(known)) => {
            let size = layout::placeholder_size(known, column)?;
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

impl Render for ChapterView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        self.settle(window, cx);
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
                crate::entities::catalogue::Settings::default(),
                None,
                crate::shared::layout::SizeClass::Expanded,
                cx.theme().font_family.clone(),
            ),
        };
        let (page, ink) = tone.map_or((cx.theme().background, cx.theme().foreground), page_colors);
        let viewport = window.viewport_size().width.as_f32();
        let column = layout::column_width(viewport, settings.text_size, class);
        let on_link = self.on_link.clone();
        let mut view = TextView::new(&self.state)
            .scrollable(true)
            .selectable(true)
            .image_source(self.image_source(column, cx))
            .text_size(px(settings.text_size))
            .line_height(relative(settings.line_height))
            .font_family(font)
            .text_color(ink);
        if let Some(on_link) = on_link {
            view = view.on_link_click(move |destination, event, window, cx| {
                if followed(event) {
                    on_link(destination, window, cx);
                }
            });
        }
        div()
            .id("chapter-text")
            .role(Role::Article)
            .aria_label(self.label.clone())
            .test_support()
            .size_full()
            .flex()
            .justify_center()
            .bg(page)
            .child(div().h_full().w(px(column)).child(view))
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
