//! What the library page draws: the header, the card "Continue reading", the
//! heading of the shelf, the empty states and the bar of a compact window.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::sheet::Sheet;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Placement, Selectable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::{
    Action as _, AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, div, px,
};
use rust_i18n::t;

use super::imports::imports_panel;
use super::shelf::{gap, side};
use super::sidebar::{WIDTH, search_field};
use super::{LibraryEvent, LibraryPage, Tab, error_words};
use crate::entities::catalogue::{Library, ShelfBook};
use crate::features::appearance::Appearance;
use crate::features::search_library::{Progress, Section, Sort, continue_reading};
use crate::features::update_banner;
use crate::shared::actions::{Find, OpenFile};
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
    crate::shared::i18n::count_text("library.books", count)
}

/// What the shelf says when it has no books to show.
pub(super) fn empty_words(query: &str, section: Section, library_empty: bool) -> (String, String) {
    let query = query.trim();
    let (title, hint) = if library_empty {
        (t!("library.no_books"), t!("library.no_books_hint"))
    } else if !query.is_empty() {
        (
            t!("library.nothing_matches", query = query),
            t!("library.nothing_matches_hint"),
        )
    } else {
        match section {
            Section::Reading => (t!("library.none_reading"), t!("library.none_reading_hint")),
            Section::Finished => (
                t!("library.none_finished"),
                t!("library.none_finished_hint"),
            ),
            Section::All => (t!("library.no_books"), t!("library.no_books_hint")),
        }
    };
    (title.into_owned(), hint.into_owned())
}

impl LibraryPage {
    /// Opens the sidebar in a sheet over the shelf (a medium window).
    pub(super) fn show_sections(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let sidebar = self.sidebar.clone();
        window.open_sheet_at(Placement::Left, cx, move |sheet: Sheet, _, _| {
            sheet
                .title(SharedString::from(t!("library.title")))
                .size(px(WIDTH + 40.0))
                .child(sidebar.clone())
        });
    }

    fn render_header(&self, class: SizeClass, cx: &mut Context<'_, Self>) -> AnyElement {
        let sort = self.sort;
        let page = cx.weak_entity();
        let sort_label = t!("library.sort_by", sort = sort.label());
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
        let mut header = page_header(t!("library.title"));
        if class == SizeClass::Medium {
            header = header.leading(
                icon_button(
                    "show-sections",
                    IconName::PanelLeft,
                    t!("library.show_sections"),
                )
                .on_click(cx.listener(|this, _, window, cx| this.show_sections(window, cx))),
            );
        }
        header
            .trailing(sort_button)
            .trailing(
                icon_button("add-book", IconName::Plus, t!("library.add_book"))
                    .on_click(|_, window, cx| window.dispatch_action(OpenFile.boxed_clone(), cx)),
            )
            .trailing(
                icon_button("new-book", IconName::FilePenLine, t!("library.new_book"))
                    .disabled(self.creating.is_some())
                    .on_click(cx.listener(|this, _, window, cx| this.ask_to_create(window, cx))),
            )
            .trailing(
                icon_button("settings", IconName::Settings, t!("library.settings"))
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
            .aria_label(t!("library.continue_reading_book", title = title))
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
                            .aria_label(t!("library.continue_reading"))
                            .test_support()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(t!("library.continue_reading"))),
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
                            .child(SharedString::from(t!(
                                "library.progress_read",
                                progress = progress.caption()
                            ))),
                    ),
            )
            .child(
                Button::new("continue-reading")
                    .primary()
                    .label(t!("library.read"))
                    .accessibility_label(t!("library.read_book", title = title))
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
            t!("library.loading").into()
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
        let bottom = crate::shared::safe_area::insets(window).bottom;
        let tab = |id: &'static str, label: SharedString, icon: IconName, tab: Tab| {
            let active = self.tab == tab;
            Button::new(id)
                .ghost()
                .role(Role::Tab)
                .flex_1()
                .h(px(TAB_HEIGHT.max(target_size())))
                .icon(icon)
                .label(label.clone())
                .accessibility_label(if active {
                    t!("library.tab_current", tab = label).into()
                } else {
                    label
                })
                .selected(active)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| this.set_tab(tab, window, cx)))
        };
        h_flex()
            .id("library-tabs")
            .role(Role::TabList)
            .aria_label(t!("library.views"))
            .test_support()
            .w_full()
            .flex_none()
            .pb(bottom)
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(tab(
                "tab-reading",
                t!("library.tab_reading").into(),
                IconName::BookOpen,
                Tab::Reading,
            ))
            .child(tab(
                "tab-library",
                t!("library.title").into(),
                IconName::Library,
                Tab::Library,
            ))
            .child(tab(
                "tab-search",
                t!("library.search").into(),
                IconName::Search,
                Tab::Search,
            ))
            .into_any_element()
    }

    /// What stands in the place of the shelf when there is nothing to show.
    fn render_empty(&self, library_empty: bool, cx: &mut Context<'_, Self>) -> AnyElement {
        let (title, hint) = empty_words(&self.query, self.section, library_empty);
        let state = empty_state(title, hint);
        if library_empty {
            state
                .action(
                    h_flex()
                        .flex_wrap()
                        .justify_center()
                        .gap_2()
                        .child(
                            Button::new("add-first-book")
                                .primary()
                                .label(t!("library.add_book"))
                                .h(px(target_size()))
                                .cursor_pointer()
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(OpenFile.boxed_clone(), cx);
                                }),
                        )
                        .child(
                            Button::new("create-first-book")
                                .label(t!("library.new_book"))
                                .h(px(target_size()))
                                .cursor_pointer()
                                .disabled(self.creating.is_some())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.ask_to_create(window, cx);
                                })),
                        ),
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
                        .label(t!("library.try_again"))
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
                .child(status_line("library-status", t!("library.loading_books")).text_color(muted))
                .child(Spinner::new())
                .into_any_element()
        } else if self.shelf.visible().is_empty() {
            self.render_empty(library_empty, cx)
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
            .aria_label(t!("library.title"))
            .test_support()
            .key_context("Library")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &Find, window, cx| this.find(window, cx)))
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
            .children(update_banner(cx))
            .child(body)
            .children(compact_imports)
            .children(bar)
    }
}
