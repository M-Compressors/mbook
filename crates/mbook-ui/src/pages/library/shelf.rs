//! The shelf: covers in a grid, virtualised by rows.
//!
//! A library has thousands of books, so only the rows in view are built. The
//! grid is one tab stop: Tab enters it at the book last used, and the arrow
//! keys move between books (left and right by one, up and down by a row).

use std::collections::HashMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Anchor, AnyElement, App, Context, FocusHandle, FontWeight, InteractiveElement as _,
    IntoElement, KeyDownEvent, Keystroke, ListAlignment, ListState, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, Window, div,
    list, px, relative,
};

use super::LibraryPage;
use super::cover::{cover_height, drawn_cover, thumbnail};
use super::menu::book_menu;
use crate::entities::catalogue::{ShelfBook, Store};
use crate::features::search_library::{Progress, shelf_label};
use crate::shared::model::BookKey;
use crate::shared::ui::{focus_ring, icon_button, target_size};

/// The space between two covers in a row, in pixels.
pub(super) const GAP: f32 = 20.0;
/// The space at the left and the right of the shelf, in pixels.
pub(super) const SIDE: f32 = 24.0;
/// The space under a row, in pixels.
const ROW_GAP: f32 = 24.0;
/// How many rows the page keys move by.
const PAGE_ROWS: usize = 3;
/// How tall the line of progress is, in pixels.
const LINE_HEIGHT: f32 = 4.0;
/// The space between the parts of a book, in pixels.
const PART_GAP: f32 = 4.0;

/// The focus handles of one book's controls.
struct Handles {
    tile: FocusHandle,
    more: FocusHandle,
    /// Follows the focus into the tile, so that the arrow keys start from the
    /// book the user is on.
    _focus: Subscription,
}

/// What the shelf shows and how it is laid out.
pub(super) struct Shelf {
    list: ListState,
    visible: Vec<BookKey>,
    positions: HashMap<BookKey, usize>,
    columns: usize,
    /// Whether the background has chosen the books at least once.
    ready: bool,
    /// The book the keyboard is on: the tab stop of the grid.
    current: Option<BookKey>,
    handles: HashMap<BookKey, Handles>,
    /// The book that is the tab stop of the grid, as decided when the shelf
    /// was last drawn. The list cannot be asked while it draws its rows.
    tab_key: Option<BookKey>,
}

impl Shelf {
    /// A shelf with no books.
    pub(super) fn new() -> Self {
        Self {
            list: ListState::new(0, ListAlignment::Top, px(600.0)),
            visible: Vec::new(),
            positions: HashMap::new(),
            columns: 2,
            ready: false,
            current: None,
            handles: HashMap::new(),
            tab_key: None,
        }
    }

    /// The books shown, in order.
    pub(super) fn visible(&self) -> &[BookKey] {
        &self.visible
    }

    /// Whether the books have been chosen at least once.
    pub(super) fn is_ready(&self) -> bool {
        self.ready
    }

    /// How many covers stand in a row.
    #[cfg(test)]
    pub(super) fn columns(&self) -> usize {
        self.columns
    }

    /// Makes `key` the book the keyboard is on.
    pub(super) fn set_current(&mut self, key: BookKey) {
        self.current = Some(key);
    }

    fn rows(&self) -> usize {
        self.visible.len().div_ceil(self.columns.max(1))
    }

    /// Shows these books. The list goes back to the top unless they are the
    /// ones already shown.
    pub(super) fn show(&mut self, keys: Vec<BookKey>) {
        if self.ready && keys == self.visible {
            return;
        }
        self.ready = true;
        self.positions = keys
            .iter()
            .enumerate()
            .map(|(at, key)| (*key, at))
            .collect();
        self.visible = keys;
        if self
            .current
            .is_some_and(|key| !self.positions.contains_key(&key))
        {
            self.current = None;
        }
        let positions = &self.positions;
        self.handles.retain(|key, _| positions.contains_key(key));
        self.list.reset(self.rows());
    }

    /// Lays the shelf out in `columns` covers a row.
    pub(super) fn set_columns(&mut self, columns: usize) {
        let columns = columns.max(1);
        if columns == self.columns {
            return;
        }
        let before = self.rows();
        self.columns = columns;
        self.list.splice(0..before, self.rows());
    }

    /// The book that takes the focus when the user tabs into the grid: the one
    /// last used if it is in view, else the first of the top row.
    pub(super) fn tab_stop(&self) -> Option<BookKey> {
        let in_view = self.current.filter(|key| {
            self.positions
                .get(key)
                .is_some_and(|at| self.list.bounds_for_item(at / self.columns).is_some())
        });
        in_view.or_else(|| {
            let top = self.list.logical_scroll_top().item_ix;
            self.visible
                .get(top * self.columns)
                .or_else(|| self.visible.first())
                .copied()
        })
    }

    fn handles_for(
        &mut self,
        key: BookKey,
        window: &mut Window,
        cx: &mut Context<'_, LibraryPage>,
    ) -> (FocusHandle, FocusHandle) {
        let handles = self.handles.entry(key).or_insert_with(|| {
            let tile = cx.focus_handle();
            let focus = cx.on_focus_in(&tile, window, move |this, _, cx| {
                if this.shelf.current != Some(key) {
                    this.shelf.current = Some(key);
                    cx.notify();
                }
            });
            Handles {
                tile,
                more: cx.focus_handle(),
                _focus: focus,
            }
        });
        (handles.tile.clone(), handles.more.clone())
    }
}

impl LibraryPage {
    /// The width the shelf has, in pixels, given the width of the window and
    /// of what stands beside it.
    pub(super) fn shelf_width(window: &Window, beside: f32) -> f32 {
        (window.viewport_size().width.as_f32() - beside - 2.0 * SIDE).max(0.0)
    }

    /// The list of rows, in its box, with the keys that move through it.
    pub(super) fn render_shelf(&mut self, cx: &mut Context<'_, Self>) -> AnyElement {
        use gpui_kit::component::scroll::ScrollableElement as _;
        self.shelf.tab_key = self.shelf.tab_stop();
        let state = self.shelf.list.clone();
        div()
            .id("shelf")
            .role(gpui_kit::Role::List)
            .aria_label("Books")
            .test_support()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.on_shelf_key(event, window, cx);
            }))
            .child(
                list(
                    state.clone(),
                    cx.processor(|this, row: usize, window, cx| this.render_row(row, window, cx)),
                )
                .size_full(),
            )
            .vertical_scrollbar(&state)
            .into_any_element()
    }

    /// One row of covers, with the books of the row, aligned at the bottom.
    fn render_row(
        &mut self,
        row: usize,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let columns = self.shelf.columns;
        let from = row * columns;
        let keys: Vec<BookKey> = self
            .shelf
            .visible
            .iter()
            .skip(from)
            .take(columns)
            .copied()
            .collect();
        let books: Vec<ShelfBook> = keys
            .iter()
            .filter_map(|key| self.index.get(key).and_then(|at| self.books.get(*at)))
            .cloned()
            .collect();
        let cover_width = self.cover_width;
        let area = books
            .iter()
            .map(|book| cover_height(cover_width, book.row.cover))
            .fold(0.0_f32, f32::max);
        let tab_stop = self.shelf.tab_key;
        h_flex()
            .id(("shelf-row", row))
            .w_full()
            .items_start()
            .gap(px(GAP))
            .px(px(SIDE))
            .pb(px(ROW_GAP))
            .children(books.iter().map(|book| {
                self.render_tile(book, area, tab_stop == Some(book.row.key), window, cx)
            }))
            .into_any_element()
    }

    /// The cover of a book at `width` pixels: its thumbnail when it has one,
    /// else a plate drawn from its title.
    pub(super) fn cover_of(&self, book: &ShelfBook, width: f32, cx: &App) -> AnyElement {
        let height = cover_height(width, book.row.cover);
        let author = book.row.authors.first().map(String::as_str);
        if self.thumbnails.contains(&book.row.key) {
            let path = Store::global(cx).roots.cover(book.row.key);
            thumbnail(path, &book.row.title, author, width, height, cx)
        } else {
            drawn_cover(&book.row.title, author, width, height, cx)
        }
    }

    /// The part of a tile that opens the book: the cover, the line of
    /// progress, the state, the title and the authors.
    fn tile_content(&self, book: &ShelfBook, area: f32, cx: &App) -> impl IntoElement + use<> {
        let progress = Progress::of(book);
        let cover = self.cover_of(book, self.cover_width, cx);
        let theme = cx.theme();
        let (muted, accent, track, radius) = (
            theme.muted_foreground,
            theme.primary,
            theme.input,
            theme.radius,
        );
        let target = px(target_size());
        let line = div()
            .w_full()
            .h(px(LINE_HEIGHT))
            .rounded(radius)
            .bg(track)
            .child(
                div()
                    .h_full()
                    .w(relative(progress.fraction()))
                    .rounded(radius)
                    .bg(accent),
            );
        // The caption stops short of the `…` control that stands at its end.
        let caption = h_flex()
            .w_full()
            .h(target)
            .items_center()
            .pr(target)
            .text_xs()
            .text_color(muted)
            .child(progress.caption());
        // The width is the cover's, not "full": inside the button, "full"
        // follows the longest line of a title, which then runs out of the tile.
        v_flex()
            .w(px(self.cover_width))
            .items_start()
            .gap(px(PART_GAP))
            .whitespace_normal()
            .text_left()
            .child(div().w_full().h(px(area)).flex().items_end().child(cover))
            .child(line)
            .child(caption)
            .child(
                div()
                    .w_full()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .line_clamp(2)
                    .text_ellipsis()
                    .child(book.row.title.clone()),
            )
            .child(
                div()
                    .w_full()
                    .text_xs()
                    .text_color(muted)
                    .truncate()
                    .child(book.row.authors.join(", ")),
            )
    }

    /// One book: its cover and captions, which open it, and the `…` control
    /// and the right click, which open its menu.
    fn render_tile(
        &mut self,
        book: &ShelfBook,
        area: f32,
        tab_stop: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let key = book.row.key;
        let width = self.cover_width;
        let (open_focus, more_focus) = self.shelf.handles_for(key, window, cx);
        let title = book.row.title.clone();
        let page = cx.weak_entity();
        let open = Button::new(format!("open-{key}"))
            .ghost()
            .accessibility_id(key.to_string())
            .accessibility_label(shelf_label(book))
            .track_focus(&open_focus)
            .tab_stop(tab_stop)
            .w(px(width))
            .h_auto()
            .p_0()
            .items_start()
            .cursor_pointer()
            .focus_visible(focus_ring(cx))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.shelf.current = Some(key);
                cx.emit(super::LibraryEvent::Open(key));
            }))
            .child(self.tile_content(book, area, cx));
        let more_title = title.clone();
        let more_page = page.clone();
        let more = icon_button(
            format!("more-{key}"),
            IconName::Ellipsis,
            format!("Actions for {title}"),
        )
        .track_focus(&more_focus)
        .tab_stop(false)
        .dropdown_menu_with_anchor(Anchor::BottomRight, move |menu, _, _| {
            book_menu(menu, key, &more_title, &more_page)
        });
        div()
            .id(format!("tile-{key}"))
            .relative()
            .flex_none()
            .w(px(width))
            .child(open)
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top(px(area + 2.0 * PART_GAP + LINE_HEIGHT))
                    .child(more),
            )
            .context_menu(move |menu, _, _| book_menu(menu, key, &title, &page))
            .into_any_element()
    }

    /// Moves the keyboard to a book, brings its row into view and focuses it.
    pub(super) fn go_to(&mut self, key: BookKey, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(&at) = self.shelf.positions.get(&key) else {
            return;
        };
        self.shelf.current = Some(key);
        self.shelf
            .list
            .scroll_to_reveal_item(at / self.shelf.columns);
        let (tile, _) = self.shelf.handles_for(key, window, cx);
        window.focus(&tile, cx);
        cx.notify();
    }

    /// The book `step` away from the current one in reading order, held
    /// within the shelf; a step down from the row above the last row's gap
    /// lands on the last book.
    fn neighbour(&self, from: usize, key: &str) -> Option<usize> {
        let count = self.shelf.visible.len();
        let columns = self.shelf.columns;
        if count == 0 {
            return None;
        }
        let last = count - 1;
        Some(match key {
            "left" => from.saturating_sub(1),
            "right" => (from + 1).min(last),
            "up" => from.saturating_sub(columns),
            "down" => {
                if from + columns <= last {
                    from + columns
                } else if from / columns < last / columns {
                    last
                } else {
                    from
                }
            }
            "pageup" => from.saturating_sub(columns * PAGE_ROWS),
            "pagedown" => (from + columns * PAGE_ROWS).min(last),
            "home" => 0,
            "end" => last,
            _ => return None,
        })
    }

    /// The keys of the grid.
    pub(super) fn on_shelf_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if modifiers.control || modifiers.alt || modifiers.platform || modifiers.function {
            return;
        }
        let key = keystroke.key.as_str();
        let Some(current) = self
            .shelf
            .current
            .or_else(|| self.shelf.visible.first().copied())
        else {
            return;
        };
        if key == "menu" || (key == "f10" && modifiers.shift) {
            self.open_menu_of(current, window, cx);
            cx.stop_propagation();
            return;
        }
        let from = self.shelf.positions.get(&current).copied().unwrap_or(0);
        let Some(to) = self.neighbour(from, key) else {
            return;
        };
        cx.stop_propagation();
        if let Some(&target) = self.shelf.visible.get(to) {
            self.go_to(target, window, cx);
        }
    }

    /// Opens the menu of a book from the keyboard: the focus goes to its `…`
    /// control, which a press of Return opens.
    fn open_menu_of(&mut self, key: BookKey, window: &mut Window, cx: &mut Context<'_, Self>) {
        let (_, more) = self.shelf.handles_for(key, window, cx);
        // The press of Return reaches the page's own key handler: it must not
        // run inside this update.
        window.defer(cx, move |window, cx| {
            window.focus(&more, cx);
            if let Ok(enter) = Keystroke::parse("enter") {
                window.dispatch_keystroke(enter, cx);
            }
        });
    }
}
