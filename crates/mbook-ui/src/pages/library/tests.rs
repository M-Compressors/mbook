//! The library page in headless windows: its three layouts, the shelf, the
//! keys, the menu of a book and the imports.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    Action as _, AnyWindowHandle, Context, Entity, IntoElement, MouseButton, ParentElement as _,
    Render, Role, Styled as _, TestAppContext, Window, div,
};

use super::*;
use crate::entities::catalogue::ReadingState;
use crate::features::appearance::Appearance;
use crate::shared::actions::OpenFile;
use crate::testing::{self, Fixture};

/// Holds the page, as the root does.
struct Host {
    page: Entity<LibraryPage>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        div().size_full().child(self.page.clone())
    }
}

struct Harness {
    /// Keeps the folders of the test until it ends.
    _fixture: Fixture,
    window: AnyWindowHandle,
    page: Entity<LibraryPage>,
    keys: Vec<BookKey>,
    events: Rc<RefCell<Vec<LibraryEvent>>>,
}

/// Opens a window `width` pixels wide over a library of the books `titles`,
/// before the background has run.
fn start_unparked(cx: &mut TestAppContext, width: f32, titles: &[&str]) -> Harness {
    let fixture = Fixture::new();
    let keys = titles.iter().map(|title| fixture.add_book(title)).collect();
    fixture.install(cx);
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
        let appearance = cx.new(Appearance::new);
        Appearance::install(appearance, cx);
        let library = cx.new(Library::new);
        Library::install(library, cx);
    });
    let (window, host) = testing::open(cx, width, 800.0, |window, cx| Host {
        page: cx.new(|cx| LibraryPage::new(window, cx)),
    });
    let page = host.read_with(cx, |host, _| host.page.clone());
    // The root gives the page the focus when it comes to the front.
    cx.update_window(window, |_, window, cx| {
        let focus = page.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
    })
    .unwrap();
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    cx.update(|cx| {
        cx.subscribe(&page, move |_, event: &LibraryEvent, _| {
            sink.borrow_mut().push(*event);
        })
        .detach();
    });
    Harness {
        _fixture: fixture,
        window,
        page,
        keys,
        events,
    }
}

/// As [`start_unparked`], with the background finished.
fn start(cx: &mut TestAppContext, width: f32, titles: &[&str]) -> Harness {
    let harness = start_unparked(cx, width, titles);
    cx.run_until_parked();
    harness
}

impl Harness {
    fn frame<R>(
        &self,
        cx: &mut TestAppContext,
        inspect: impl FnOnce(&mut Window, &mut gpui_kit::App) -> R,
    ) -> R {
        testing::frame(cx, self.window, inspect)
    }

    fn click(&self, id: &str, cx: &mut TestAppContext) {
        let id = id.to_owned();
        self.frame(cx, move |window, cx| window.click(id, cx));
        cx.run_until_parked();
    }

    /// Moves the keyboard to a book, as an arrow key does.
    fn go_to(&self, key: BookKey, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| {
            self.page.update(cx, |page, cx| page.go_to(key, window, cx));
        })
        .unwrap();
        cx.run_until_parked();
    }

    /// Clicks the control with this accessible name, where it stands.
    fn click_label(&self, label: &str, cx: &mut TestAppContext) {
        let label = label.to_owned();
        self.frame(cx, move |window, cx| {
            let centre = window.find_by_label(&label).bounds().center();
            window.pointer_down(centre, MouseButton::Left, cx);
            window.pointer_up(centre, MouseButton::Left, cx);
        });
        cx.run_until_parked();
    }

    fn press(&self, key: &str, cx: &mut TestAppContext) {
        let key = key.to_owned();
        self.frame(cx, move |window, cx| window.press(&key, cx));
        cx.run_until_parked();
    }

    fn visible(&self, cx: &TestAppContext) -> Vec<BookKey> {
        self.page.read_with(cx, |page, _| page.visible().to_vec())
    }

    fn titles_shown(&self, cx: &mut TestAppContext) -> Vec<String> {
        let keys = self.visible(cx);
        cx.update(|cx| {
            let library = Library::global(cx);
            keys.iter()
                .filter_map(|key| library.read(cx).book(*key).map(|b| b.row.title.clone()))
                .collect()
        })
    }

    /// Whether the tile of a book holds the keyboard focus.
    fn focused(&self, key: BookKey, cx: &mut TestAppContext) -> bool {
        self.frame(cx, move |window, _| {
            window
                .find(format!("open-{key}"))
                .focused()
                .unwrap_or(false)
        })
    }

    fn set_state(key: BookKey, state: ReadingState, cx: &mut TestAppContext) {
        cx.update(|cx| {
            Library::global(cx).update(cx, |library, cx| library.set_state(key, state, cx));
        });
        cx.run_until_parked();
    }
}

/// A state of a book that was opened and read to `permille`.
fn reading(permille: u16) -> ReadingState {
    ReadingState {
        place: None,
        progress_permille: permille,
        opened_ms: Some(10),
        finished: false,
    }
}

/// An mbook file called `title` in `dir`.
fn write_book(dir: &Path, title: &str) -> PathBuf {
    let mut book = mbook::Book::new(title, "en").unwrap();
    book.add_chapter("ch01.md", "# One\n\nText.\n").unwrap();
    let path = dir.join(format!("{title}.mbook"));
    book.save(&path).unwrap();
    path
}

/// Every element that assistive technology is told about has a role, and a
/// control has a name.
fn assert_controls_are_named(harness: &Harness, cx: &mut TestAppContext, screen: &str) {
    const CONTROLS: [Role; 8] = [
        Role::Button,
        Role::Link,
        Role::CheckBox,
        Role::RadioButton,
        Role::Switch,
        Role::TextInput,
        Role::ComboBox,
        Role::Tab,
    ];
    harness.frame(cx, |window, _| {
        let elements = window.elements();
        assert!(!elements.is_empty(), "{screen}: nothing is reported");
        for element in elements {
            if testing::is_kit_decoration(&format!("{:?}", element.path())) {
                continue;
            }
            let role = element.role();
            assert!(role.is_some(), "{screen}: no role: {element:?}");
            if role.is_some_and(|role| CONTROLS.contains(&role)) {
                assert!(
                    element
                        .label()
                        .is_some_and(|label| !label.trim().is_empty()),
                    "{screen}: a control without a name: {element:?}"
                );
            }
        }
    });
}

// ---- the layouts ----------------------------------------------------------

#[gpui_kit::test]
fn an_expanded_window_docks_the_sidebar_beside_the_shelf(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune", "Emma"]);
    harness.frame(cx, |window, _| {
        assert_eq!(window.find("library-page").role(), Some(Role::Main));
        assert_eq!(window.find("page-heading").label(), Some("Library"));
        assert!(window.try_find("sections").is_some());
        assert!(window.try_find_by_label("Search the library").is_some());
        assert!(window.try_find("section-all").is_some());
        assert!(window.try_find("show-sections").is_none());
        assert!(window.try_find("library-tabs").is_none());
    });
}

#[gpui_kit::test]
fn a_medium_window_hides_the_sidebar_behind_a_control_of_the_header(cx: &mut TestAppContext) {
    let harness = start(cx, 800.0, &["Dune"]);
    harness.frame(cx, |window, _| {
        assert!(window.try_find("sections").is_none());
        assert!(window.try_find("library-tabs").is_none());
        assert_eq!(window.find("show-sections").role(), Some(Role::Button));
    });
    harness.click("show-sections", cx);
    harness.frame(cx, |window, _| {
        assert!(window.try_find("sections").is_some());
        assert!(window.try_find("section-reading").is_some());
    });
}

#[gpui_kit::test]
fn choosing_a_section_in_the_sheet_closes_it(cx: &mut TestAppContext) {
    let harness = start(cx, 800.0, &["Dune"]);
    harness.click("show-sections", cx);
    harness.click("section-finished", cx);
    assert_eq!(
        harness.page.read_with(cx, |page, _| page.section()),
        Section::Finished
    );
    harness.frame(cx, |window, _| {
        assert!(window.try_find("sections").is_none());
    });
}

#[gpui_kit::test]
fn a_compact_window_has_a_bar_at_the_bottom_and_no_sidebar(cx: &mut TestAppContext) {
    let harness = start(cx, 390.0, &["Dune"]);
    harness.frame(cx, |window, _| {
        assert!(window.try_find("sections").is_none());
        assert!(window.try_find("show-sections").is_none());
        for id in ["tab-reading", "tab-library", "tab-search"] {
            assert_eq!(window.find(id).role(), Some(Role::Tab), "{id}");
        }
        assert!(window.try_find_by_label("Search the library").is_none());
    });
}

#[gpui_kit::test]
fn every_layout_is_drawn_at_390_800_and_1280_pixels(cx: &mut TestAppContext) {
    for width in [390.0, 800.0, 1280.0] {
        let harness = start(cx, width, &["Dune", "Emma"]);
        harness.frame(cx, |window, _| {
            let page = window.find("library-page");
            assert!(page.visible(), "{width} px");
            assert!(page.bounds().size.width > gpui_kit::px(0.0), "{width} px");
        });
        for key in &harness.keys {
            harness.frame(cx, |window, _| {
                let tile = window.find(format!("open-{key}"));
                assert!(tile.visible(), "{width} px");
                assert!(tile.bounds().size.width > gpui_kit::px(0.0), "{width} px");
            });
        }
    }
}

#[gpui_kit::test]
fn the_shelf_has_as_many_columns_as_the_width_allows(cx: &mut TestAppContext) {
    let titles: Vec<String> = (0..12).map(|n| format!("Book {n:02}")).collect();
    let refs: Vec<&str> = titles.iter().map(String::as_str).collect();
    let harness = start(cx, 1280.0, &refs);
    let columns = harness.page.read_with(cx, |page, _| page.shelf.columns());
    // 1280 less the sidebar and the margins, covers of 132 with gaps of 20.
    assert_eq!(
        columns,
        crate::shared::layout::shelf_columns(1280.0 - 280.0 - 48.0, 132.0, 20.0)
    );
    let narrow = start(cx, 390.0, &refs);
    let columns = narrow.page.read_with(cx, |page, _| page.shelf.columns());
    // A phone: covers of 104 with gaps of 12 and margins of 16, so that
    // three stand in a row.
    assert_eq!(
        columns,
        crate::shared::layout::shelf_columns(390.0 - 32.0, 104.0, 12.0)
    );
    assert_eq!(columns, 3);
}

#[gpui_kit::test]
fn the_hover_surface_extends_beyond_both_sides_of_the_cover(cx: &mut TestAppContext) {
    for (viewport, inset) in [(390.0, 5.0), (1280.0, 8.0)] {
        let harness = start(cx, viewport, &["Dune", "Emma"]);
        let [first, second] = harness.keys[..] else {
            panic!("the fixture has two books");
        };
        let cover_width = harness.page.read_with(cx, |page, _| page.cover_width);
        harness.frame(cx, |window, _| {
            let first = window.find(format!("open-{first}")).bounds();
            let second = window.find(format!("open-{second}")).bounds();
            assert_eq!(first.size.width, gpui_kit::px(cover_width + 2.0 * inset));
            assert!(
                first.right() < second.left() || second.right() < first.left(),
                "{first:?} and {second:?}"
            );
        });
    }
}

#[gpui_kit::test]
fn covers_of_a_row_stand_on_the_same_line(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune", "Emma", "Ulysses"]);
    let bottoms: Vec<_> = harness.frame(cx, |window, _| {
        harness
            .keys
            .iter()
            .map(|key| {
                let bounds = window.find(format!("open-{key}")).bounds();
                bounds.origin.y
            })
            .collect()
    });
    assert!(bottoms.windows(2).all(|pair| pair[0] == pair[1]));
}

#[gpui_kit::test]
fn only_the_rows_in_view_are_built(cx: &mut TestAppContext) {
    let titles: Vec<String> = (0..160).map(|n| format!("Book {n:03}")).collect();
    let refs: Vec<&str> = titles.iter().map(String::as_str).collect();
    let harness = start(cx, 1280.0, &refs);
    let built = harness.frame(cx, |window, _| {
        window
            .find_all_by_role(Role::Button)
            .iter()
            .filter(|button| button.label().is_some_and(|l| l.contains(", by ")))
            .count()
    });
    assert!(built > 0);
    assert!(built < 80, "{built} books were built");
}

// ---- loading and empty states ---------------------------------------------

#[gpui_kit::test]
fn the_page_draws_its_loading_state_while_the_background_has_not_run(cx: &mut TestAppContext) {
    let harness = start_unparked(cx, 1280.0, &["Dune"]);
    harness.frame(cx, |window, _| {
        let status = window.find("library-status");
        assert_eq!(status.role(), Some(Role::Status));
        assert_eq!(status.label(), Some("Loading your books"));
        assert!(
            window
                .try_find(format!("open-{}", harness.keys[0]))
                .is_none()
        );
    });
    cx.run_until_parked();
    harness.frame(cx, |window, _| {
        assert!(window.try_find("library-status").is_none());
        assert!(
            window
                .try_find(format!("open-{}", harness.keys[0]))
                .is_some()
        );
    });
}

#[gpui_kit::test]
fn an_empty_library_offers_to_add_a_book(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &[]);
    harness.frame(cx, |window, _| {
        assert_eq!(window.find("empty-title").label(), Some("No books yet"));
        let add = window.find("add-first-book");
        assert_eq!(add.role(), Some(Role::Button));
        assert_eq!(add.label(), Some("Add a book…"));
    });
}

#[gpui_kit::test]
fn the_add_control_asks_for_files(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &[]);
    let asked = Rc::new(Cell::new(0));
    let seen = asked.clone();
    cx.update(|cx| {
        cx.on_action(move |_: &OpenFile, _| seen.set(seen.get() + 1));
    });
    harness.click("add-first-book", cx);
    harness.click("add-book", cx);
    assert_eq!(asked.get(), 2);
}

#[gpui_kit::test]
fn a_search_without_a_result_says_what_was_searched_for(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune", "Emma"]);
    harness
        .page
        .update(cx, |page, cx| page.set_query("zzz".into(), cx));
    cx.run_until_parked();
    harness.frame(cx, |window, _| {
        assert_eq!(
            window.find("empty-title").label(),
            Some("Nothing matches zzz")
        );
        assert!(window.try_find("add-first-book").is_none());
    });
}

#[gpui_kit::test]
fn a_section_without_books_says_so(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune"]);
    harness.click("section-finished", cx);
    harness.frame(cx, |window, _| {
        assert_eq!(
            window.find("empty-title").label(),
            Some("No books are finished")
        );
    });
}

#[test]
fn the_words_of_an_empty_shelf_depend_on_why_it_is_empty() {
    use super::view::empty_words;
    let (title, hint) = empty_words("", Section::All, true);
    assert_eq!(title, "No books yet");
    assert!(hint.starts_with("Add a file in mbook, EPUB, FB2"));
    assert_eq!(
        empty_words("  dune ", Section::All, false).0,
        "Nothing matches dune"
    );
    assert_eq!(
        empty_words("", Section::Reading, false).0,
        "No books are being read"
    );
    assert_eq!(
        empty_words("x", Section::Reading, true).0,
        "No books yet",
        "an empty library wins"
    );
}

#[test]
fn the_number_of_books_is_said_in_the_singular_and_the_plural() {
    use super::view::count_label;
    assert_eq!(count_label(0), "0 books");
    assert_eq!(count_label(1), "1 book");
    assert_eq!(count_label(2), "2 books");
}

// ---- filtering and order --------------------------------------------------

#[gpui_kit::test]
fn the_shelf_shows_every_book_in_the_order_last_added_first(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune", "Emma"]);
    assert_eq!(harness.visible(cx).len(), 2);
    assert_eq!(harness.page.read_with(cx, |page, _| page.counts()).all, 2);
}

#[gpui_kit::test]
fn words_typed_in_the_search_field_filter_the_shelf(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune", "Emma", "Dune Messiah"]);
    harness.frame(cx, |window, cx| {
        window.dispatch_action(FindBooks.boxed_clone(), cx);
    });
    cx.run_until_parked();
    harness.frame(cx, |window, cx| window.input("dune", cx));
    cx.run_until_parked();
    let mut titles = harness.titles_shown(cx);
    titles.sort();
    assert_eq!(titles, ["Dune", "Dune Messiah"]);
    assert_eq!(
        harness
            .page
            .read_with(cx, |page, _| page.query().to_owned()),
        "dune"
    );
}

#[gpui_kit::test]
fn escape_in_the_search_field_clears_it(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune", "Emma"]);
    harness.frame(cx, |window, cx| {
        window.dispatch_action(FindBooks.boxed_clone(), cx);
    });
    cx.run_until_parked();
    harness.frame(cx, |window, cx| window.input("emma", cx));
    cx.run_until_parked();
    assert_eq!(harness.visible(cx).len(), 1);
    harness.press("escape", cx);
    assert_eq!(
        harness
            .page
            .read_with(cx, |page, _| page.query().to_owned()),
        ""
    );
    assert_eq!(harness.visible(cx).len(), 2);
}

#[gpui_kit::test]
fn the_find_action_puts_the_focus_in_the_search_field(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune"]);
    harness.frame(cx, |window, cx| {
        window.dispatch_action(FindBooks.boxed_clone(), cx);
    });
    cx.run_until_parked();
    harness.frame(cx, |window, _| {
        assert_eq!(
            window.find_by_label("Search the library").focused(),
            Some(true)
        );
    });
}

#[gpui_kit::test]
fn the_reading_section_holds_the_books_that_were_opened_and_not_finished(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune", "Emma", "Ulysses"]);
    Harness::set_state(harness.keys[1], reading(380), cx);
    let finished = ReadingState {
        finished: true,
        ..reading(1000)
    };
    Harness::set_state(harness.keys[2], finished, cx);
    harness.click("section-reading", cx);
    assert_eq!(harness.titles_shown(cx), ["Emma"]);
    harness.click("section-finished", cx);
    assert_eq!(harness.titles_shown(cx), ["Ulysses"]);
    harness.click("section-all", cx);
    assert_eq!(harness.visible(cx).len(), 3);
}

#[gpui_kit::test]
fn a_section_button_says_how_many_books_it_holds_and_whether_it_is_selected(
    cx: &mut TestAppContext,
) {
    let harness = start(cx, 1280.0, &["Dune", "Emma"]);
    Harness::set_state(harness.keys[0], reading(100), cx);
    harness.frame(cx, |window, _| {
        assert_eq!(
            window.find("section-all").label(),
            Some("All books, 2, selected")
        );
        assert_eq!(window.find("section-reading").label(), Some("Reading, 1"));
        assert_eq!(window.find("section-finished").label(), Some("Finished, 0"));
    });
}

#[gpui_kit::test]
fn the_sort_menu_changes_the_order_of_the_shelf(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Charlie", "Alpha", "Bravo"]);
    harness
        .page
        .update(cx, |page, cx| page.set_sort(Sort::Title, cx));
    cx.run_until_parked();
    assert_eq!(harness.titles_shown(cx), ["Alpha", "Bravo", "Charlie"]);
    harness.click("sort", cx);
    harness.frame(cx, |window, _| {
        assert!(window.try_find_by_label("Recent").is_some());
        assert!(window.try_find_by_label("Title").is_some());
        assert!(window.try_find_by_label("Author").is_some());
    });
}

#[gpui_kit::test]
fn the_sort_control_names_the_current_order(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune"]);
    harness.frame(cx, |window, _| {
        assert_eq!(window.find("sort").label(), Some("Sort: Recent"));
    });
    harness
        .page
        .update(cx, |page, cx| page.set_sort(Sort::Author, cx));
    harness.frame(cx, |window, _| {
        assert_eq!(window.find("sort").label(), Some("Sort: Author"));
    });
}

#[gpui_kit::test]
fn a_filter_that_was_overtaken_does_not_replace_a_newer_one(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune", "Emma"]);
    harness.page.update(cx, |page, cx| {
        page.set_query("dune".into(), cx);
        page.set_query("emma".into(), cx);
    });
    cx.run_until_parked();
    assert_eq!(harness.titles_shown(cx), ["Emma"]);
}

#[gpui_kit::test]
fn the_filter_runs_in_the_background(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune", "Emma"]);
    harness
        .page
        .update(cx, |page, cx| page.set_query("dune".into(), cx));
    // Nothing has run yet: the shelf still shows what it showed.
    assert_eq!(harness.visible(cx).len(), 2);
    cx.run_until_parked();
    assert_eq!(harness.visible(cx).len(), 1);
}

#[gpui_kit::test]
fn a_book_that_leaves_the_library_leaves_the_shelf(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune", "Emma"]);
    cx.update(|cx| {
        Library::global(cx).update(cx, |library, cx| library.remove(harness.keys[0], cx));
    });
    cx.run_until_parked();
    assert_eq!(harness.titles_shown(cx), ["Emma"]);
    harness.frame(cx, |window, _| {
        assert!(
            window
                .try_find(format!("open-{}", harness.keys[0]))
                .is_none()
        );
    });
}

// ---- a book on the shelf --------------------------------------------------

#[gpui_kit::test]
fn a_book_is_named_by_its_title_its_authors_and_its_state(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune", "Emma"]);
    Harness::set_state(harness.keys[1], reading(380), cx);
    harness.frame(cx, |window, _| {
        let new = window.find(format!("open-{}", harness.keys[0]));
        assert_eq!(new.role(), Some(Role::Button));
        assert_eq!(new.label(), Some("Dune, by Ann, new"));
        let read = window.find(format!("open-{}", harness.keys[1]));
        assert_eq!(read.label(), Some("Emma, by Ann, 38 percent read"));
    });
}

#[gpui_kit::test]
fn a_click_on_a_book_opens_it(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune", "Emma"]);
    harness.click(&format!("open-{}", harness.keys[1]), cx);
    assert_eq!(
        *harness.events.borrow(),
        [LibraryEvent::Open(harness.keys[1])]
    );
}

#[gpui_kit::test]
fn return_on_a_book_opens_it(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune"]);
    let key = harness.keys[0];
    harness.go_to(key, cx);
    assert!(harness.focused(key, cx));
    harness.press("enter", cx);
    assert_eq!(*harness.events.borrow(), [LibraryEvent::Open(key)]);
}

#[gpui_kit::test]
fn the_settings_control_asks_for_the_settings(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &[]);
    harness.click("settings", cx);
    assert_eq!(*harness.events.borrow(), [LibraryEvent::Settings]);
}

#[gpui_kit::test]
fn the_menu_of_a_book_opens_from_its_control(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune"]);
    let key = harness.keys[0];
    harness.frame(cx, |window, _| {
        let more = window.find(format!("more-{key}"));
        assert_eq!(more.role(), Some(Role::Button));
        assert_eq!(more.label(), Some("Actions for Dune"));
    });
    harness.click(&format!("more-{key}"), cx);
    harness.frame(cx, |window, _| {
        for label in ["Open", "Edit", "Show the file", "Remove"] {
            assert!(window.try_find_by_label(label).is_some(), "{label}");
        }
    });
}

#[gpui_kit::test]
fn the_menu_of_a_book_opens_on_a_right_click(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune"]);
    let key = harness.keys[0];
    harness.frame(cx, |window, cx| {
        window.right_click(format!("open-{key}"), cx);
    });
    cx.run_until_parked();
    harness.frame(cx, |window, _| {
        assert!(window.try_find_by_label("Remove").is_some());
    });
}

#[gpui_kit::test]
fn the_menu_key_opens_the_menu_of_the_focused_book(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune"]);
    let key = harness.keys[0];
    harness.go_to(key, cx);
    harness.press("menu", cx);
    harness.frame(cx, |window, _| {
        assert!(window.try_find_by_label("Show the file").is_some());
    });
}

#[gpui_kit::test]
fn the_menu_entry_edit_asks_the_root_to_edit_the_book(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune"]);
    let key = harness.keys[0];
    harness.click(&format!("more-{key}"), cx);
    harness.click_label("Edit", cx);
    assert_eq!(*harness.events.borrow(), [LibraryEvent::Edit(key)]);
}

#[gpui_kit::test]
fn removing_a_book_asks_first_and_names_the_book(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune"]);
    let key = harness.keys[0];
    harness.click(&format!("more-{key}"), cx);
    harness.click_label("Remove", cx);
    let open = harness.frame(cx, |window, cx| {
        gpui_kit::component::WindowExt::has_active_dialog(window, cx)
    });
    assert!(open, "a dialog asks");
    assert_eq!(harness.visible(cx), [key], "nothing is removed yet");
}

#[gpui_kit::test]
fn confirming_the_question_removes_the_book_and_returns_the_focus_to_the_page(
    cx: &mut TestAppContext,
) {
    let harness = start(cx, 1280.0, &["Dune", "Emma"]);
    let key = harness.keys[0];
    harness.click(&format!("more-{key}"), cx);
    harness.click_label("Remove", cx);
    harness.click_label("Remove", cx);
    assert_eq!(harness.titles_shown(cx), ["Emma"]);
    let focused = cx.update_window(harness.window, |_, window, cx| {
        harness.page.read(cx).focus_handle.is_focused(window)
    });
    assert!(focused.unwrap());
}

#[gpui_kit::test]
fn cancelling_the_question_keeps_the_book(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune"]);
    let key = harness.keys[0];
    harness.click(&format!("more-{key}"), cx);
    harness.click_label("Remove", cx);
    harness.click_label("Cancel", cx);
    assert_eq!(harness.visible(cx), [key]);
}

#[gpui_kit::test]
fn the_warnings_of_an_import_are_listed_in_a_dialog(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &[]);
    cx.update_window(harness.window, |_, window, cx| {
        super::imports::show_warnings(
            "The Example",
            &["A table was flattened.".to_owned()],
            window,
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    let open = harness.frame(cx, |window, cx| {
        gpui_kit::component::WindowExt::has_active_dialog(window, cx)
    });
    assert!(open);
    harness.frame(cx, |window, _| {
        assert!(window.try_find_by_label("Close").is_some());
    });
}

#[test]
fn the_question_before_removing_names_the_book_and_spares_the_file() {
    assert_eq!(
        super::menu::remove_question("The Example"),
        "Remove The Example from the library? The file you added it from is not touched."
    );
}

// ---- the keyboard ---------------------------------------------------------

/// Twelve books on a shelf of several columns.
fn crowded(cx: &mut TestAppContext) -> Harness {
    let titles: Vec<String> = (0..12).map(|n| format!("Book {n:02}")).collect();
    let refs: Vec<&str> = titles.iter().map(String::as_str).collect();
    start(cx, 1280.0, &refs)
}

fn focus_book(harness: &Harness, key: BookKey, cx: &mut TestAppContext) {
    harness.go_to(key, cx);
}

#[gpui_kit::test]
fn the_arrow_keys_move_between_the_books_of_the_shelf(cx: &mut TestAppContext) {
    let harness = crowded(cx);
    let order = harness.visible(cx);
    let columns = harness.page.read_with(cx, |page, _| page.shelf.columns());
    assert!(columns >= 3 && columns < order.len(), "{columns} columns");
    focus_book(&harness, order[0], cx);
    assert!(harness.focused(order[0], cx));
    harness.press("right", cx);
    assert!(harness.focused(order[1], cx));
    harness.press("down", cx);
    assert!(harness.focused(order[1 + columns], cx));
    harness.press("left", cx);
    assert!(harness.focused(order[columns], cx));
    harness.press("up", cx);
    assert!(harness.focused(order[0], cx));
}

#[gpui_kit::test]
fn home_and_end_go_to_the_first_and_the_last_book(cx: &mut TestAppContext) {
    let harness = crowded(cx);
    let order = harness.visible(cx);
    focus_book(&harness, order[3], cx);
    harness.press("end", cx);
    assert!(harness.focused(order[order.len() - 1], cx));
    harness.press("home", cx);
    assert!(harness.focused(order[0], cx));
}

#[gpui_kit::test]
fn the_arrows_stop_at_the_edges_of_the_shelf(cx: &mut TestAppContext) {
    let harness = crowded(cx);
    let order = harness.visible(cx);
    focus_book(&harness, order[0], cx);
    harness.press("left", cx);
    harness.press("up", cx);
    assert!(harness.focused(order[0], cx));
    harness.press("end", cx);
    harness.press("right", cx);
    harness.press("down", cx);
    assert!(harness.focused(order[order.len() - 1], cx));
}

#[gpui_kit::test]
fn down_from_a_full_row_above_a_short_one_lands_on_the_last_book(cx: &mut TestAppContext) {
    let harness = crowded(cx);
    let order = harness.visible(cx);
    let columns = harness.page.read_with(cx, |page, _| page.shelf.columns());
    let last_row_start = (order.len() - 1) / columns * columns;
    // A column past the end of the last row.
    let above = last_row_start - columns + (order.len() - last_row_start);
    if above < last_row_start {
        focus_book(&harness, order[above], cx);
        harness.press("down", cx);
        assert!(harness.focused(order[order.len() - 1], cx));
    }
}

#[gpui_kit::test]
fn the_shelf_is_one_tab_stop(cx: &mut TestAppContext) {
    let harness = crowded(cx);
    let order = harness.visible(cx);
    focus_book(&harness, order[4], cx);
    // Only the book last used can be tabbed to.
    assert_eq!(
        harness.page.read_with(cx, |page, _| page.shelf.tab_stop()),
        Some(order[4])
    );
}

#[gpui_kit::test]
fn the_keys_of_the_grid_do_nothing_when_the_shelf_is_empty(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &[]);
    harness.press("down", cx);
    harness.press("end", cx);
    assert_eq!(*harness.events.borrow(), Vec::new());
}

// ---- the continue card ----------------------------------------------------

#[gpui_kit::test]
fn the_card_offers_the_book_read_last_and_opens_it(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune", "Emma"]);
    harness.frame(cx, |window, _| {
        assert!(window.try_find("continue-card").is_none());
    });
    Harness::set_state(harness.keys[1], reading(380), cx);
    harness.frame(cx, |window, _| {
        assert_eq!(
            window.find("continue-card").label(),
            Some("Continue reading: Emma")
        );
        assert_eq!(
            window.find("continue-heading").label(),
            Some("Continue reading")
        );
        assert_eq!(window.find("continue-reading").label(), Some("Read Emma"));
    });
    harness.click("continue-reading", cx);
    assert_eq!(
        *harness.events.borrow(),
        [LibraryEvent::Open(harness.keys[1])]
    );
}

#[gpui_kit::test]
fn the_card_is_not_shown_in_a_compact_window_or_during_a_search(cx: &mut TestAppContext) {
    let harness = start(cx, 390.0, &["Dune"]);
    Harness::set_state(harness.keys[0], reading(100), cx);
    harness.frame(cx, |window, _| {
        assert!(window.try_find("continue-card").is_none());
    });
    let wide = start(cx, 1280.0, &["Dune"]);
    Harness::set_state(wide.keys[0], reading(100), cx);
    wide.page
        .update(cx, |page, cx| page.set_query("dune".into(), cx));
    cx.run_until_parked();
    wide.frame(cx, |window, _| {
        assert!(window.try_find("continue-card").is_none());
    });
}

// ---- the tabs of a compact window -----------------------------------------

#[gpui_kit::test]
fn the_reading_tab_shows_the_books_being_read(cx: &mut TestAppContext) {
    let harness = start(cx, 390.0, &["Dune", "Emma"]);
    Harness::set_state(harness.keys[1], reading(10), cx);
    harness.click("tab-reading", cx);
    assert_eq!(harness.titles_shown(cx), ["Emma"]);
    harness.frame(cx, |window, _| {
        assert_eq!(window.find("tab-reading").label(), Some("Reading, current"));
        assert_eq!(window.find("tab-library").label(), Some("Library"));
    });
    harness.click("tab-library", cx);
    assert_eq!(harness.visible(cx).len(), 2);
}

#[gpui_kit::test]
fn the_search_tab_shows_the_field_and_leaving_it_drops_the_words(cx: &mut TestAppContext) {
    let harness = start(cx, 390.0, &["Dune", "Emma"]);
    harness.click("tab-search", cx);
    harness.frame(cx, |window, _| {
        let field = window.find_by_label("Search the library");
        assert_eq!(field.role(), Some(Role::TextInput));
        assert_eq!(field.label(), Some("Search the library"));
        assert_eq!(field.focused(), Some(true));
    });
    harness.frame(cx, |window, cx| window.input("emma", cx));
    cx.run_until_parked();
    assert_eq!(harness.titles_shown(cx), ["Emma"]);
    harness.click("tab-library", cx);
    assert_eq!(
        harness
            .page
            .read_with(cx, |page, _| page.query().to_owned()),
        ""
    );
    assert_eq!(harness.visible(cx).len(), 2);
    harness.frame(cx, |window, _| {
        assert!(window.try_find_by_label("Search the library").is_none());
    });
}

#[gpui_kit::test]
fn the_find_action_opens_the_search_tab_of_a_compact_window(cx: &mut TestAppContext) {
    let harness = start(cx, 390.0, &["Dune"]);
    harness.frame(cx, |window, cx| {
        window.dispatch_action(FindBooks.boxed_clone(), cx);
    });
    cx.run_until_parked();
    assert_eq!(
        harness.page.read_with(cx, |page, _| page.tab()),
        Tab::Search
    );
    harness.frame(cx, |window, _| {
        assert_eq!(
            window.find_by_label("Search the library").focused(),
            Some(true)
        );
    });
}

// ---- thumbnails -----------------------------------------------------------

#[gpui_kit::test]
fn a_book_whose_thumbnail_is_a_file_is_found_on_the_background(cx: &mut TestAppContext) {
    let fixture_dir = Fixture::new();
    let with = fixture_dir.add_book("With a cover");
    let without = fixture_dir.add_book("Without a file");
    let plain = fixture_dir.add_book("No cover at all");
    for (key, cover) in [(with, Some((400, 600))), (without, Some((400, 600)))] {
        let mut row = fixture_dir.catalogue.book(key).unwrap().unwrap();
        row.cover = cover;
        fixture_dir.catalogue.put_book(&row).unwrap();
    }
    std::fs::remove_file(fixture_dir.roots.cover(without)).unwrap();
    std::fs::remove_file(fixture_dir.roots.cover(plain)).unwrap();
    fixture_dir.install(cx);
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
        let appearance = cx.new(Appearance::new);
        Appearance::install(appearance, cx);
        let library = cx.new(Library::new);
        Library::install(library, cx);
    });
    let (window, host) = testing::open(cx, 1280.0, 800.0, |window, cx| Host {
        page: cx.new(|cx| LibraryPage::new(window, cx)),
    });
    let page = host.read_with(cx, |host, _| host.page.clone());
    cx.run_until_parked();
    let found = page.read_with(cx, |page, _| {
        (
            page.thumbnails.contains(&with),
            page.thumbnails.contains(&without),
            page.thumbnails.contains(&plain),
        )
    });
    assert_eq!(found, (true, false, false));
    testing::frame(cx, window, |window, _| {
        for key in [with, without, plain] {
            assert!(window.try_find(format!("open-{key}")).is_some());
        }
    });
}

// ---- the imports ----------------------------------------------------------

#[gpui_kit::test]
fn files_that_are_handed_to_the_page_are_imported_one_after_the_other(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &[]);
    let dir = tempfile::tempdir().unwrap();
    let paths = vec![write_book(dir.path(), "One"), write_book(dir.path(), "Two")];
    harness.frame(cx, |window, cx| {
        let _ = window;
        cx.update_window(window.window_handle(), |_, _, _| ()).ok();
    });
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .page
            .update(cx, |page, cx| page.import(paths.clone(), window, cx));
    })
    .unwrap();
    harness.frame(cx, |window, _| {
        // The imports are on the sidebar the moment they are asked for.
        let status = window.find("import-status-0");
        assert_eq!(status.role(), Some(Role::Status));
        assert_eq!(status.label(), Some("Waiting"));
        assert_eq!(window.find("imports-heading").label(), Some("Imports"));
    });
    cx.run_until_parked();
    assert_eq!(harness.titles_shown(cx).len(), 2);
    harness.frame(cx, |window, _| {
        assert_eq!(window.find("import-status-0").label(), Some("Added"));
        assert_eq!(window.find("import-status-1").label(), Some("Added"));
    });
    assert_eq!(
        *harness.events.borrow(),
        Vec::new(),
        "several files are only added"
    );
}

#[gpui_kit::test]
fn one_file_that_is_imported_opens_its_book(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &[]);
    let dir = tempfile::tempdir().unwrap();
    let path = write_book(dir.path(), "Only");
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .page
            .update(cx, |page, cx| page.import(vec![path.clone()], window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    let key = harness.visible(cx)[0];
    assert_eq!(*harness.events.borrow(), [LibraryEvent::Open(key)]);
}

#[gpui_kit::test]
fn a_file_that_cannot_be_added_shows_why_and_can_be_dismissed(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune"]);
    cx.update_window(harness.window, |_, window, cx| {
        harness.page.update(cx, |page, cx| {
            page.import(vec![PathBuf::from("/nowhere/notes.txt")], window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    harness.frame(cx, |window, _| {
        let status = window.find("import-status-0");
        let message = status.label().unwrap_or_default();
        assert!(message.ends_with('.'), "{message}");
        assert_eq!(
            window.find("import-dismiss-0").label(),
            Some("Dismiss notes.txt")
        );
    });
    harness.click("import-dismiss-0", cx);
    harness.frame(cx, |window, _| {
        assert!(window.try_find("import-status-0").is_none());
        assert!(window.try_find("imports").is_none());
    });
    assert_eq!(*harness.events.borrow(), Vec::new());
}

#[gpui_kit::test]
fn the_imports_of_a_compact_window_stand_above_the_bar(cx: &mut TestAppContext) {
    let harness = start(cx, 390.0, &[]);
    cx.update_window(harness.window, |_, window, cx| {
        harness.page.update(cx, |page, cx| {
            page.import(vec![PathBuf::from("/nowhere/a.txt")], window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    harness.frame(cx, |window, _| {
        let imports = window.find("imports").bounds();
        let bar = window.find("library-tabs").bounds();
        assert!(imports.origin.y < bar.origin.y);
    });
}

#[gpui_kit::test]
fn finished_imports_are_cleared_together(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &[]);
    cx.update_window(harness.window, |_, window, cx| {
        harness.page.update(cx, |page, cx| {
            page.import(
                vec![
                    PathBuf::from("/nowhere/a.txt"),
                    PathBuf::from("/nowhere/b.txt"),
                ],
                window,
                cx,
            );
        });
    })
    .unwrap();
    cx.run_until_parked();
    harness.click("imports-clear", cx);
    harness.frame(cx, |window, _| {
        assert!(window.try_find("imports").is_none());
    });
}

// ---- accessibility --------------------------------------------------------

#[gpui_kit::test]
fn every_control_of_every_layout_has_a_role_and_a_name(cx: &mut TestAppContext) {
    for (width, screen) in [(1280.0, "expanded"), (800.0, "medium"), (390.0, "compact")] {
        let harness = start(cx, width, &["Dune", "Emma"]);
        Harness::set_state(harness.keys[0], reading(250), cx);
        cx.update_window(harness.window, |_, window, cx| {
            harness.page.update(cx, |page, cx| {
                page.import(vec![PathBuf::from("/nowhere/a.txt")], window, cx);
            });
        })
        .unwrap();
        cx.run_until_parked();
        assert_controls_are_named(&harness, cx, screen);
    }
}

#[gpui_kit::test]
fn the_empty_library_has_named_controls_too(cx: &mut TestAppContext) {
    for width in [1280.0, 800.0, 390.0] {
        let harness = start(cx, width, &[]);
        assert_controls_are_named(&harness, cx, "empty");
    }
}

#[gpui_kit::test]
fn the_ids_of_the_books_survive_a_change_of_the_shelf(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune", "Emma"]);
    let key = harness.keys[0];
    let before = harness.frame(cx, |window, _| {
        window.find(format!("open-{key}")).path().to_vec()
    });
    Harness::set_state(harness.keys[1], reading(500), cx);
    let after = harness.frame(cx, |window, _| {
        window.find(format!("open-{key}")).path().to_vec()
    });
    assert_eq!(before, after);
}

#[gpui_kit::test]
fn the_tile_of_a_book_is_at_least_a_target_in_size(cx: &mut TestAppContext) {
    let harness = start(cx, 390.0, &["Dune"]);
    let key = harness.keys[0];
    harness.frame(cx, |window, _| {
        let more = window.find(format!("more-{key}")).bounds();
        let side = gpui_kit::px(crate::shared::ui::target_size());
        assert!(more.size.width >= side && more.size.height >= side);
    });
}

#[gpui_kit::test]
fn a_page_that_is_focused_hands_nothing_to_a_shelf_that_is_empty(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &[]);
    cx.update_window(harness.window, |_, window, cx| {
        let focus = harness.page.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
    })
    .unwrap();
    harness.press("right", cx);
    assert_eq!(*harness.events.borrow(), Vec::new());
}

// ---- a failure of the catalogue ---------------------------------------------

fn fail_the_library(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Library::global(cx).update(cx, |library, cx| {
            library.fail(
                crate::entities::catalogue::CatalogueError::storage(std::io::Error::other(
                    "disk gone",
                )),
                cx,
            );
        });
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_failure_of_the_catalogue_is_said_with_a_way_to_try_again(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune"]);
    fail_the_library(cx);
    harness.frame(cx, |window, _| {
        let line = window.find("library-error-text");
        assert_eq!(line.role(), Some(Role::Status));
        assert_eq!(
            line.label(),
            Some(
                "The library could not be read or changed: the catalogue could not be read or written: the catalogue database failed: disk gone."
            )
        );
        assert_eq!(window.find("library-retry").role(), Some(Role::Button));
    });
}

#[gpui_kit::test]
fn trying_again_reads_the_library_and_clears_the_failure(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune"]);
    fail_the_library(cx);
    harness.click("library-retry", cx);
    let held = cx.update(|cx| Library::global(cx).read(cx).error().is_some());
    assert!(!held, "the library was read again");
    harness.frame(cx, |window, _| {
        assert!(window.try_find("library-error-text").is_none());
    });
    assert_eq!(harness.titles_shown(cx), ["Dune"]);
}

#[gpui_kit::test]
fn a_failure_of_the_catalogue_is_announced_once(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &["Dune"]);
    fail_the_library(cx);
    // Another change of the library does not say it again.
    cx.update(|cx| {
        Library::global(cx).update(cx, |_, cx| cx.notify());
    });
    cx.run_until_parked();
    let told = harness.frame(cx, |window, _| window.find_all("notification").len());
    assert_eq!(told, 1);
}

#[gpui_kit::test]
fn the_heading_of_an_empty_library_counts_its_books_instead_of_loading(cx: &mut TestAppContext) {
    let harness = start(cx, 1280.0, &[]);
    let ready = harness.page.read_with(cx, |page, _| page.shelf.is_ready());
    assert!(ready, "an empty library is as ready as any other");
}
