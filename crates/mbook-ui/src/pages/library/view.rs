//! What the library page draws: the header, the card "Continue reading", the
//! heading of the shelf, the empty states and the bar of a compact window.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::sheet::Sheet;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{
    ActiveTheme as _, Placement, Selectable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::{
    Action as _, AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, div, px,
};

use super::imports::imports_panel;
use super::shelf::{gap, side};
use super::sidebar::{WIDTH, search_field};
use super::{FindBooks, LibraryEvent, LibraryPage, Tab, error_words};
use crate::entities::catalogue::{Library, ShelfBook};
use crate::features::appearance::Appearance;
use crate::features::search_library::{Progress, Section, Sort, continue_reading};
use crate::shared::actions::OpenFile;
use crate::shared::layout::{SizeClass, shelf_columns};
use crate::shared::ui::{empty_state, icon_button, page_header, status_line, target_size};

/// How tall the list of imports may grow above the bar of a compact window,
/// in pixels.
const COMPACT_IMPORTS_HEIGHT: f32 = 144.0;
/// The width of the cover on the card "Continue reading", in pixels.
const CARD_COVER: f32 = 84.0;
/// The height of a tab of the bar of a compact window, in pixels.
const TAB_HEIGHT: f32 = 48.0;

/// The words under the heading of the shelf: how many books it shows.
pub(super) fn count_label(count: usize) -> String {
    if count == 1 {
        "1 book".to_owned()
    } else {
        format!("{count} books")
    }
}

/// What the shelf says when it has no books to show.
pub(super) fn empty_words(query: &str, section: Section, library_empty: bool) -> (String, String) {
    let query = query.trim();
    if library_empty {
        (
            "No books yet".to_owned(),
            "Add a file in mbook, EPUB, FB2, PDF, CBZ, DOCX, HTML, Markdown or text.".to_owned(),
        )
    } else if !query.is_empty() {
        (
            format!("Nothing matches {query}"),
            "Check the spelling, or search for a title, an author or a series.".to_owned(),
        )
    } else {
        match section {
            Section::Reading => (
                "No books are being read".to_owned(),
                "A book you open and do not finish appears here.".to_owned(),
            ),
            Section::Finished => (
                "No books are finished".to_owned(),
                "A book you mark as finished appears here.".to_owned(),
            ),
            Section::All => (
                "No books yet".to_owned(),
                "Add a file in mbook, EPUB, FB2, PDF, CBZ, DOCX, HTML, Markdown or text."
                    .to_owned(),
            ),
        }
    }
}

impl LibraryPage {
    /// Opens the sidebar in a sheet over the shelf (a medium window).
    pub(super) fn show_sections(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let sidebar = self.sidebar.clone();
        window.open_sheet_at(Placement::Left, cx, move |sheet: Sheet, _, _| {
            sheet
                .title("Library")
                .size(px(WIDTH + 40.0))
                .child(sidebar.clone())
        });
    }

    fn render_header(&self, class: SizeClass, cx: &mut Context<'_, Self>) -> AnyElement {
        let sort = self.sort;
        let page = cx.weak_entity();
        let sort_label = format!("Sort: {}", sort.label());
        let sort_button = icon_button("sort", IconName::SortAscending, sort_label).dropdown_menu(
            move |menu, _, _| {
                Sort::ALL.into_iter().fold(menu, |menu, choice| {
                    let page = page.clone();
                    menu.item(
                        PopupMenuItem::new(choice.label())
                            .checked(choice == sort)
                            .on_click(move |_, _, cx| {
                                page.update(cx, |page, cx| page.set_sort(choice, cx)).ok();
                            }),
                    )
                })
            },
        );
        let mut header = page_header("Library");
        if class == SizeClass::Medium {
            header = header.leading(
                icon_button("show-sections", IconName::PanelLeft, "Show the sections")
                    .on_click(cx.listener(|this, _, window, cx| this.show_sections(window, cx))),
            );
        }
        header
            .trailing(sort_button)
            .trailing(
                icon_button("add-book", IconName::Plus, "Add a book…")
                    .on_click(|_, window, cx| window.dispatch_action(OpenFile.boxed_clone(), cx)),
            )
            .trailing(
                icon_button("settings", IconName::Settings, "Settings")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(LibraryEvent::Settings))),
            )
            .into_any_element()
    }

    /// The card that offers the book read last.
    fn render_continue(&self, book: &ShelfBook, cx: &mut Context<'_, Self>) -> AnyElement {
        let key = book.row.key;
        let theme = cx.theme();
        let (surface, border, muted, radius) = (
            theme.popover,
            theme.border,
            theme.muted_foreground,
            theme.radius,
        );
        let progress = Progress::of(book);
        let title = book.row.title.clone();
        h_flex()
            .id("continue-card")
            .role(Role::Group)
            .aria_label(format!("Continue reading: {title}"))
            .test_support()
            .mx(px(side(self.cover_width)))
            .mt_4()
            .mb_2()
            .p_3()
            .gap_4()
            .items_center()
            .rounded(radius)
            .border_1()
            .border_color(border)
            .bg(surface)
            .child(self.cover_of(book, CARD_COVER, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .id("continue-heading")
                            .role(Role::Heading)
                            .aria_level(2)
                            .aria_label("Continue reading")
                            .test_support()
                            .text_xs()
                            .text_color(muted)
                            .child("Continue reading"),
                    )
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(title.clone()),
                    )
                    .child(
                        div()
                            .text_color(muted)
                            .truncate()
                            .child(book.row.authors.join(", ")),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(format!("{} read", progress.caption())),
                    ),
            )
            .child(
                Button::new("continue-reading")
                    .primary()
                    .label("Read")
                    .accessibility_label(format!("Read {title}"))
                    .h(px(target_size()))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.shelf_current(key);
                        cx.emit(LibraryEvent::Open(key));
                    })),
            )
            .into_any_element()
    }

    /// The row with the name of the section and how many books it shows.
    fn render_heading(&self, cx: &mut Context<'_, Self>) -> AnyElement {
        let font = Appearance::global(cx).read(cx).reading_font(cx);
        let muted = cx.theme().muted_foreground;
        let words: SharedString = if self.shelf.is_ready() {
            count_label(self.shelf.visible().len()).into()
        } else {
            "Loading".into()
        };
        h_flex()
            .w_full()
            .px(px(side(self.cover_width)))
            .pt_4()
            .pb_3()
            .gap_3()
            .items_baseline()
            .child(
                div()
                    .id("shelf-heading")
                    .role(Role::Heading)
                    .aria_level(2)
                    .aria_label(self.section.label())
                    .test_support()
                    .text_3xl()
                    .font_family(font)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.section.label()),
            )
            .child(
                status_line("shelf-count", words)
                    .text_sm()
                    .text_color(muted),
            )
            .into_any_element()
    }

    /// The bar of a compact window: Reading, Library, Search.
    fn render_bar(&self, window: &Window, cx: &mut Context<'_, Self>) -> AnyElement {
        let bottom = window.insets().effective().bottom;
        let tab = |id: &'static str, label: &'static str, icon: IconName, tab: Tab| {
            let active = self.tab == tab;
            Button::new(id)
                .ghost()
                .role(Role::Tab)
                .flex_1()
                .h(px(TAB_HEIGHT.max(target_size())))
                .icon(icon)
                .label(label)
                .accessibility_label(if active {
                    format!("{label}, current")
                } else {
                    label.to_owned()
                })
                .selected(active)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| this.set_tab(tab, window, cx)))
        };
        h_flex()
            .id("library-tabs")
            .role(Role::TabList)
            .aria_label("Library views")
            .test_support()
            .w_full()
            .flex_none()
            .pb(bottom)
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(tab(
                "tab-reading",
                "Reading",
                IconName::BookOpen,
                Tab::Reading,
            ))
            .child(tab(
                "tab-library",
                "Library",
                IconName::Library,
                Tab::Library,
            ))
            .child(tab("tab-search", "Search", IconName::Search, Tab::Search))
            .into_any_element()
    }

    /// What stands in the place of the shelf when there is nothing to show.
    fn render_empty(&self, library_empty: bool) -> AnyElement {
        let (title, hint) = empty_words(&self.query, self.section, library_empty);
        let state = empty_state(title, hint);
        if library_empty {
            state
                .action(
                    Button::new("add-first-book")
                        .primary()
                        .label("Add a book…")
                        .h(px(target_size()))
                        .cursor_pointer()
                        .on_click(|_, window, cx| {
                            window.dispatch_action(OpenFile.boxed_clone(), cx);
                        }),
                )
                .into_any_element()
        } else {
            state.into_any_element()
        }
    }

    /// The failure of the catalogue, with the control that reads it again.
    fn render_error(&self, cx: &mut Context<'_, Self>) -> Option<AnyElement> {
        let words = self.library.read(cx).error().map(error_words)?;
        let danger = cx.theme().danger;
        Some(
            h_flex()
                .id("library-error")
                .w_full()
                .px(px(side(self.cover_width)))
                .py_2()
                .gap_3()
                .items_center()
                // The button stands first: the notification that announces the
                // failure covers the right end of the row.
                .child(
                    Button::new("library-retry")
                        .label("Try again")
                        .h(px(target_size()))
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.library.update(cx, Library::reload);
                        })),
                )
                .child(
                    status_line("library-error-text", words)
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .text_color(danger),
                )
                .into_any_element(),
        )
    }

    /// Remembers the book a control of the page acted on, so that the arrow
    /// keys start from it when the user comes back.
    fn shelf_current(&mut self, key: crate::shared::model::BookKey) {
        self.shelf.set_current(key);
    }
}

impl Render for LibraryPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let class = Appearance::global(cx).read(cx).size_class(window);
        self.cover_width = class.cover_width();
        let beside = if class.has_sidebar() { WIDTH } else { 0.0 };
        self.shelf.set_columns(shelf_columns(
            Self::shelf_width(window, beside, self.cover_width),
            self.cover_width,
            gap(self.cover_width),
        ));

        let library = self.library.read(cx);
        let library_empty = library.books().is_empty();
        let loading = library.is_loading() && library_empty;
        let searching = !self.query.trim().is_empty();
        let failure = self.render_error(cx);
        let theme = cx.theme();
        let (background, foreground, muted) =
            (theme.background, theme.foreground, theme.muted_foreground);

        let card = (class != SizeClass::Compact && self.section == Section::All && !searching)
            .then(|| continue_reading(&self.books))
            .flatten()
            .and_then(|key| {
                self.index
                    .get(&key)
                    .and_then(|at| self.books.get(*at))
                    .cloned()
            })
            .map(|book| self.render_continue(&book, cx));

        let books_area: AnyElement = if loading || (!self.shelf.is_ready() && !library_empty) {
            div()
                .p_6()
                .child(status_line("library-status", "Loading your books").text_color(muted))
                .child(Spinner::new())
                .into_any_element()
        } else if self.shelf.visible().is_empty() {
            self.render_empty(library_empty)
        } else {
            self.render_shelf(cx)
        };

        let search_row = (class == SizeClass::Compact && self.tab == Tab::Search).then(|| {
            div()
                .w_full()
                .px(px(side(self.cover_width)))
                .pt_3()
                .child(search_field(&self.search))
        });

        let content = v_flex()
            .id("library-content")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .h_full()
            .children(failure)
            .children(search_row)
            .children(card)
            .child(self.render_heading(cx))
            .child(books_area);

        let body = if class.has_sidebar() {
            h_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(self.sidebar.clone())
                .child(content)
                .into_any_element()
        } else {
            h_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(content)
                .into_any_element()
        };

        let compact_imports = (class == SizeClass::Compact)
            .then(|| imports_panel(&self.queue, COMPACT_IMPORTS_HEIGHT, cx))
            .flatten();
        let bar = (class == SizeClass::Compact).then(|| self.render_bar(window, cx));

        v_flex()
            .id("library-page")
            .role(Role::Main)
            .aria_label("Library")
            .test_support()
            .key_context("Library")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &FindBooks, window, cx| this.find(window, cx)))
            .on_key_down(
                cx.listener(|this, event: &gpui_kit::KeyDownEvent, window, cx| {
                    // Only when the page itself holds the focus: the arrows of a
                    // field or a menu are theirs.
                    if this.focus_handle.is_focused(window) {
                        this.on_shelf_key(event, window, cx);
                    }
                }),
            )
            .size_full()
            .bg(background)
            .text_color(foreground)
            .child(self.render_header(class, cx))
            .child(body)
            .children(compact_imports)
            .children(bar)
    }
}
