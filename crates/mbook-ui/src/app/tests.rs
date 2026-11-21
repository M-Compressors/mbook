//! The start of the application and its navigation, in headless windows.

use std::path::PathBuf;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{Action as _, AnyWindowHandle, Entity, Focusable as _, Role, TestAppContext};
use redb::{ReadableDatabase as _, ReadableTable as _};
use tempfile::TempDir;

use super::root::{Editing, Root, Screen};
use super::shell::Shell;
use crate::entities::catalogue::{BookRow, Library, Store};
use crate::features::appearance::Appearance;
use crate::pages::editor::EditorEvent;
use crate::pages::reader::ReaderEvent;
use crate::shared::actions::{About, OpenSettings};
use crate::shared::model::BookKey;
use crate::shared::roots::Roots;
use crate::testing;

/// A started application.
struct Started {
    _dir: TempDir,
    roots: Roots,
    keys: Vec<BookKey>,
    window: AnyWindowHandle,
    shell: Entity<Shell>,
}

fn temp_roots() -> (TempDir, Roots) {
    let dir = tempfile::tempdir().unwrap();
    let roots = Roots::new(dir.path().join("data"), dir.path().join("cache"));
    (dir, roots)
}

/// Writes a book into the folder of the books, as an earlier run would have
/// left it.
fn save_book(roots: &Roots, title: &str) -> BookKey {
    let key = BookKey::generate();
    let mut book = mbook::Book::new(title, "en").unwrap();
    book.add_chapter("ch01.md", "# One\n\nText.\n").unwrap();
    book.save(roots.book(key)).unwrap();
    key
}

/// Opens a window `width` pixels wide over a library of the books `titles`.
/// The application is not yet started: the background has not run.
fn start(cx: &mut TestAppContext, width: f32, titles: &[&str]) -> Started {
    start_with(cx, width, titles, |_| {})
}

/// Like [`start`], with a change to the folders made before the window opens.
fn start_with(
    cx: &mut TestAppContext,
    width: f32,
    titles: &[&str],
    prepare: impl FnOnce(&Roots),
) -> Started {
    let (dir, roots) = temp_roots();
    cx.update(super::keys::bind);
    roots.prepare().unwrap();
    prepare(&roots);
    let keys = titles
        .iter()
        .map(|title| save_book(&roots, title))
        .collect();
    let for_shell = roots.clone();
    let (window, shell) = testing::open(cx, width, 800.0, |window, cx| {
        Shell::new(for_shell, window, cx)
    });
    Started {
        _dir: dir,
        roots,
        keys,
        window,
        shell,
    }
}

/// Lets the background finish and returns the root.
fn finish(started: &Started, cx: &mut TestAppContext) -> Entity<Root> {
    cx.run_until_parked();
    started
        .shell
        .read_with(cx, |shell, _| shell.root().cloned())
        .expect("the library is open")
}

fn depth(root: &Entity<Root>, cx: &TestAppContext) -> usize {
    root.read_with(cx, |root, _| root.depth())
}

fn click(started: &Started, id: &str, cx: &mut TestAppContext) {
    let id = id.to_owned();
    testing::frame(cx, started.window, move |window, cx| window.click(id, cx));
    cx.run_until_parked();
}

/// Clicks the control with this accessible name, where it stands.
fn click_label(started: &Started, label: &str, cx: &mut TestAppContext) {
    let label = label.to_owned();
    testing::frame(cx, started.window, move |window, cx| {
        let centre = window.find_by_label(&label).bounds().center();
        window.pointer_down(centre, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(centre, gpui_kit::MouseButton::Left, cx);
    });
    cx.run_until_parked();
}

/// Chooses "Edit" in the menu of a book on the shelf.
fn edit_from_the_shelf(started: &Started, key: BookKey, cx: &mut TestAppContext) {
    click(started, &format!("more-{key}"), cx);
    click_label(started, "Edit", cx);
}

fn press(started: &Started, key: &str, cx: &mut TestAppContext) {
    let key = key.to_owned();
    testing::frame(cx, started.window, move |window, cx| window.press(&key, cx));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_window_shows_that_the_library_is_opening_until_the_background_has_run(
    cx: &mut TestAppContext,
) {
    let started = start(cx, 1200.0, &["Dune"]);
    testing::frame(cx, started.window, |window, _| {
        let status = window.find("opening-status");
        assert_eq!(status.role(), Some(Role::Status));
        assert_eq!(status.label(), Some("Opening your library"));
        assert!(window.try_find("library-page").is_none());
    });
    assert!(
        started
            .shell
            .read_with(cx, |shell, _| shell.root().is_none())
    );
}

#[gpui_kit::test]
fn the_library_replaces_the_loading_state_once_the_background_has_run(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &["Dune", "Emma"]);
    let _root = finish(&started, cx);
    testing::frame(cx, started.window, |window, cx| {
        assert!(window.try_find("opening-status").is_none());
        assert_eq!(window.find("library-page").role(), Some(Role::Main));
        assert_eq!(Library::global(cx).read(cx).books().len(), 2);
    });
}

#[gpui_kit::test]
fn a_lost_catalogue_is_rebuilt_from_the_files_at_start(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &["Dune"]);
    let _root = finish(&started, cx);
    let titles = cx.update(|cx| {
        Library::global(cx)
            .read(cx)
            .books()
            .iter()
            .map(|book| book.row.title.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(titles, ["Dune"]);
    assert!(started.roots.catalogue().is_file());
}

#[gpui_kit::test]
fn the_settings_are_read_before_the_first_page_is_drawn(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &[]);
    let _root = finish(&started, cx);
    cx.update(|cx| {
        let appearance = Appearance::global(cx);
        assert!(appearance.read(cx).is_loaded());
        assert!(!Library::global(cx).read(cx).is_loading());
    });
}

#[gpui_kit::test]
fn a_library_that_cannot_be_opened_shows_the_reason_and_can_be_tried_again(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"a file where a folder should be").unwrap();
    let roots = Roots::new(blocker.join("data"), dir.path().join("cache"));
    let (window, shell) = testing::open(cx, 1200.0, 800.0, |window, cx| {
        Shell::new(roots, window, cx)
    });
    cx.run_until_parked();
    assert!(shell.read_with(cx, |shell, _| shell.failed()));
    testing::frame(cx, window, |window, _| {
        assert_eq!(
            window.find("empty-title").label(),
            Some("mbook could not open your library")
        );
        assert_eq!(window.find_by_label("Try again").role(), Some(Role::Button));
    });

    std::fs::remove_file(&blocker).unwrap();
    testing::frame(cx, window, |window, cx| window.click("retry", cx));
    cx.run_until_parked();
    assert!(shell.read_with(cx, |shell, _| shell.root().is_some()));
}

#[gpui_kit::test]
fn a_book_opens_in_the_reader_and_escape_returns_to_the_same_library_page(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &["Dune"]);
    let root = finish(&started, cx);
    let page = root.read_with(cx, |root, _| root.library_page().cloned().unwrap());
    let key = started.keys[0];

    click(&started, &format!("open-{key}"), cx);
    assert_eq!(depth(&root, cx), 2);
    assert!(root.read_with(cx, |root, _| matches!(root.screen(), Screen::Reader(_))));
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(window.find("reader-page").role(), Some(Role::Main));
        assert_eq!(window.find("page-heading").label(), Some("Dune"));
    });

    press(&started, "escape", cx);
    assert_eq!(depth(&root, cx), 1);
    let again = root.read_with(cx, |root, _| root.library_page().cloned().unwrap());
    assert_eq!(
        again.entity_id(),
        page.entity_id(),
        "the page is not rebuilt"
    );
    assert!(root.read_with(cx, |root, _| matches!(root.screen(), Screen::Library(_))));
}

#[gpui_kit::test]
fn the_back_control_returns_as_escape_does(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &["Dune"]);
    let root = finish(&started, cx);
    click(&started, &format!("open-{}", started.keys[0]), cx);
    click(&started, "back", cx);
    assert_eq!(depth(&root, cx), 1);
}

#[gpui_kit::test]
fn going_back_from_the_library_stays_on_the_library(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &[]);
    let root = finish(&started, cx);
    press(&started, "escape", cx);
    assert_eq!(depth(&root, cx), 1);
    testing::frame(cx, started.window, |window, _| {
        assert!(window.try_find("library-page").is_some());
    });
}

#[gpui_kit::test]
fn the_keyboard_focus_follows_the_screen(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &["Dune"]);
    let root = finish(&started, cx);
    let page = root.read_with(cx, |root, _| root.library_page().cloned().unwrap());
    let focused = |cx: &mut TestAppContext, page: &Entity<crate::pages::library::LibraryPage>| {
        let page = page.clone();
        testing::frame(cx, started.window, move |window, cx| {
            page.focus_handle(cx).is_focused(window)
        })
    };
    assert!(focused(cx, &page), "the library has the focus at start");

    click(&started, &format!("open-{}", started.keys[0]), cx);
    assert!(!focused(cx, &page), "the reader has it now");
    let reader_focused = testing::frame(cx, started.window, |window, cx| {
        let Some(Screen::Reader(reader)) = root.read(cx).stack_top() else {
            return false;
        };
        reader.focus_handle(cx).is_focused(window)
    });
    assert!(reader_focused);

    press(&started, "escape", cx);
    assert!(focused(cx, &page), "going back returns it to the library");
}

#[gpui_kit::test]
fn editing_from_the_reader_closes_the_book_first_and_opens_it_after(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &["Dune"]);
    let root = finish(&started, cx);
    let key = started.keys[0];
    click(&started, &format!("open-{key}"), cx);
    let reader = root.read_with(cx, |root, _| match root.screen() {
        Screen::Reader(reader) => reader.clone(),
        _ => panic!("a reader is in front"),
    });

    reader.update(cx, |_, cx| cx.emit(ReaderEvent::Edit(key)));
    cx.run_until_parked();
    assert_eq!(depth(&root, cx), 3);
    assert!(reader.read_with(cx, |reader, _| reader.is_suspended()));
    let editor = root.read_with(cx, |root, _| match root.screen() {
        Screen::Editor(editor) => editor.clone(),
        _ => panic!("an editor is in front"),
    });

    editor.update(cx, |_, cx| cx.emit(EditorEvent::Close));
    cx.run_until_parked();
    assert_eq!(depth(&root, cx), 2);
    assert!(!reader.read_with(cx, |reader, _| reader.is_suspended()));
    assert!(root.read_with(cx, |root, _| matches!(root.screen(), Screen::Reader(_))));
}

#[gpui_kit::test]
fn an_editor_opened_from_the_library_returns_to_it(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &["Dune"]);
    let root = finish(&started, cx);
    edit_from_the_shelf(&started, started.keys[0], cx);
    assert!(root.read_with(cx, |root, _| matches!(root.screen(), Screen::Editor(_))));
    click(&started, "back", cx);
    assert!(root.read_with(cx, |root, _| matches!(root.screen(), Screen::Library(_))));
}

#[gpui_kit::test]
fn a_new_book_opens_in_the_editor_and_keeps_its_first_chapter(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &["Dune"]);
    let root = finish(&started, cx);

    click(&started, "new-book", cx);
    testing::frame(cx, started.window, |window, cx| {
        window.input("  A new story  ", cx);
    });
    click_label(&started, "Create", cx);
    assert!(root.read_with(cx, |root, _| matches!(root.screen(), Screen::Editor(_))));
    assert!(cx.update(|cx| Editing::any(cx)));

    let key = cx.update(|cx| {
        let library = Library::global(cx);
        let book = library
            .read(cx)
            .books()
            .iter()
            .find(|book| book.row.title == "A new story")
            .unwrap();
        assert_eq!(book.row.language, "und");
        assert!(book.row.added_ms > 0);
        book.row.key
    });
    let path = started.roots.book(key);
    let blank = mbook::Book::open(&path).unwrap();
    assert_eq!(blank.chapters()[0].text(), "");

    click_label(&started, "Visual editor of ch01.md", cx);
    testing::frame(cx, started.window, |window, cx| {
        window.input("It starts here.", cx);
    });
    click(&started, "save", cx);
    let book = mbook::Book::open(path).unwrap();
    assert_eq!(book.chapters()[0].text(), "It starts here.");
}

#[gpui_kit::test]
fn a_new_book_in_a_compact_window_opens_on_the_chapter_screen(cx: &mut TestAppContext) {
    let started = start(cx, 390.0, &[]);
    let _root = finish(&started, cx);
    click(&started, "create-first-book", cx);
    testing::frame(cx, started.window, |window, cx| {
        window.input("A short story", cx);
    });
    press(&started, "enter", cx);
    testing::frame(cx, started.window, |window, _| {
        assert!(
            window
                .try_find_by_label("Visual editor of ch01.md")
                .is_some()
        );
    });
}

#[gpui_kit::test]
fn new_book_requires_a_title_and_can_be_cancelled(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &[]);
    let root = finish(&started, cx);
    click(&started, "create-first-book", cx);
    testing::frame(cx, started.window, |window, cx| window.input("   ", cx));
    click_label(&started, "Create", cx);
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(
            window.find("new-book-error").label(),
            Some("Enter a title.")
        );
    });
    click_label(&started, "Cancel", cx);
    assert!(root.read_with(cx, |root, _| matches!(root.screen(), Screen::Library(_))));
    assert_eq!(std::fs::read_dir(started.roots.books()).unwrap().count(), 1);
}

#[gpui_kit::test]
fn a_creation_failure_keeps_the_library_open(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &[]);
    let root = finish(&started, cx);
    let incoming = started.roots.incoming();
    std::fs::remove_dir(&incoming).unwrap();
    std::fs::write(&incoming, "").unwrap();

    click(&started, "new-book", cx);
    testing::frame(cx, started.window, |window, cx| {
        window.input("A story", cx);
    });
    click_label(&started, "Create", cx);

    assert!(root.read_with(cx, |root, _| matches!(root.screen(), Screen::Library(_))));
    testing::frame(cx, started.window, |window, cx| {
        use gpui_kit::component::WindowExt as _;
        assert_eq!(window.notifications(cx).len(), 1);
        assert!(window.try_find("new-book").is_some());
    });
}

#[gpui_kit::test]
fn a_book_counts_as_being_edited_while_its_editor_is_open(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &["Dune"]);
    let _root = finish(&started, cx);
    assert!(!cx.update(|cx| Editing::any(cx)));

    edit_from_the_shelf(&started, started.keys[0], cx);
    assert!(cx.update(|cx| Editing::any(cx)));

    click(&started, "back", cx);
    assert!(!cx.update(|cx| Editing::any(cx)));
}

#[gpui_kit::test]
fn a_saved_book_makes_the_library_read_its_rows_again(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &["Dune"]);
    let root = finish(&started, cx);
    let key = started.keys[0];
    edit_from_the_shelf(&started, key, cx);
    let editor = root.read_with(cx, |root, _| match root.screen() {
        Screen::Editor(editor) => editor.clone(),
        _ => panic!("an editor is in front"),
    });
    // What a save does to the catalogue: the row of the book is replaced.
    cx.update(|cx| {
        let store = Store::global(cx);
        let mut row: BookRow = store.catalogue.book(key).unwrap().unwrap();
        row.title = "Dune Messiah".to_owned();
        store.catalogue.put_book(&row).unwrap();
    });
    editor.update(cx, |_, cx| cx.emit(EditorEvent::Saved(key)));
    cx.run_until_parked();
    let title = cx.update(|cx| {
        Library::global(cx)
            .read(cx)
            .book(key)
            .map(|book| book.row.title.clone())
    });
    assert_eq!(title.as_deref(), Some("Dune Messiah"));
    assert_eq!(depth(&root, cx), 2, "saving does not close the editor");
}

#[gpui_kit::test]
fn the_settings_key_opens_the_settings_once(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &[]);
    let root = finish(&started, cx);
    for _ in 0..2 {
        testing::frame(cx, started.window, |window, cx| {
            window.dispatch_action(OpenSettings.boxed_clone(), cx);
        });
        cx.run_until_parked();
    }
    assert_eq!(depth(&root, cx), 2);
    assert!(root.read_with(cx, |root, _| matches!(root.screen(), Screen::Settings(_))));
    press(&started, "escape", cx);
    assert_eq!(depth(&root, cx), 1);
}

#[gpui_kit::test]
fn the_settings_control_of_the_library_opens_the_settings(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &[]);
    let root = finish(&started, cx);
    click(&started, "settings", cx);
    assert!(root.read_with(cx, |root, _| matches!(root.screen(), Screen::Settings(_))));
    click(&started, "back", cx);
    assert_eq!(depth(&root, cx), 1);
}

#[test]
fn the_notice_of_a_catalogue_that_was_made_again_says_what_was_lost() {
    assert_eq!(
        super::shell::DAMAGED_NOTICE,
        "The library index was damaged and has been rebuilt. Reading places and settings were reset."
    );
}

#[gpui_kit::test]
fn a_damaged_catalogue_is_moved_aside_rebuilt_and_announced_once(cx: &mut TestAppContext) {
    let started = start_with(cx, 1200.0, &["Dune"], |roots| {
        std::fs::write(roots.catalogue(), vec![0xAB; 8192]).unwrap();
    });
    let _root = finish(&started, cx);
    assert_eq!(
        std::fs::read(started.roots.data().join("catalog.redb.damaged")).unwrap(),
        vec![0xAB; 8192]
    );
    let titles = cx.update(|cx| {
        Library::global(cx)
            .read(cx)
            .books()
            .iter()
            .map(|book| book.row.title.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(titles, ["Dune"]);
    testing::frame(cx, started.window, |window, cx| {
        use gpui_kit::component::WindowExt as _;
        assert_eq!(window.notifications(cx).len(), 1);
        let notice = window.find("notification");
        assert_eq!(notice.role(), Some(Role::Alert));
        assert!(notice.visible());
    });
}

#[gpui_kit::test]
fn a_catalogue_that_opens_is_not_announced(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &["Dune"]);
    let _root = finish(&started, cx);
    testing::frame(cx, started.window, |window, cx| {
        use gpui_kit::component::WindowExt as _;
        assert_eq!(window.notifications(cx).len(), 0);
    });
    assert!(!started.roots.data().join("catalog.redb.damaged").exists());
}

#[gpui_kit::test]
fn a_catalogue_of_a_newer_schema_keeps_the_error_state_and_is_not_touched(cx: &mut TestAppContext) {
    let started = start_with(cx, 1200.0, &["Dune"], |roots| {
        let db = redb::Database::create(roots.catalogue()).unwrap();
        let txn = db.begin_write().unwrap();
        let schema = redb::TableDefinition::<&str, u32>::new("schema");
        txn.open_table(schema)
            .unwrap()
            .insert("version", 99)
            .unwrap();
        txn.commit().unwrap();
    });
    cx.run_until_parked();
    assert!(started.shell.read_with(cx, |shell, _| shell.failed()));
    let db = redb::Database::create(started.roots.catalogue()).unwrap();
    let txn = db.begin_read().unwrap();
    let schema = redb::TableDefinition::<&str, u32>::new("schema");
    let table = txn.open_table(schema).unwrap();
    let version = table.get("version").unwrap().map(|version| version.value());
    assert_eq!(version, Some(99));
    assert!(!started.roots.data().join("catalog.redb.damaged").exists());
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(
            window.find("empty-title").label(),
            Some("mbook could not open your library")
        );
        assert_eq!(window.find_by_label("Try again").role(), Some(Role::Button));
    });
}

#[gpui_kit::test]
fn the_about_dialog_names_the_application_its_version_its_licence_and_its_repository(
    cx: &mut TestAppContext,
) {
    let started = start(cx, 1200.0, &[]);
    let _root = finish(&started, cx);
    testing::frame(cx, started.window, |window, cx| {
        window.dispatch_action(About.boxed_clone(), cx);
    });
    cx.run_until_parked();
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(
            window.find("about-version").label(),
            Some(format!("Version {}", env!("CARGO_PKG_VERSION")).as_str())
        );
        assert_eq!(
            window.find("about-licence").label(),
            Some("MIT or Apache-2.0")
        );
        let link = window.find("about-repository");
        assert_eq!(link.role(), Some(Role::Link));
        assert_eq!(link.label(), Some("https://github.com/M-Compressors/mbook"));
        assert_eq!(window.find_by_label("Close").role(), Some(Role::Button));
    });
}

#[gpui_kit::test]
fn escape_closes_the_about_dialog(cx: &mut TestAppContext) {
    use gpui_kit::Window;
    use gpui_kit::component::WindowExt;
    let started = start(cx, 1200.0, &[]);
    let _root = finish(&started, cx);
    testing::frame(cx, started.window, |window, cx| {
        window.dispatch_action(About.boxed_clone(), cx);
    });
    cx.run_until_parked();
    press(&started, "escape", cx);
    cx.run_until_parked();
    let open = testing::frame(cx, started.window, <Window as WindowExt>::has_active_dialog);
    assert!(!open);
}

#[cfg(feature = "self-update")]
mod updates {
    use gpui_kit::AppContext as _;

    use super::*;
    use crate::features::auto_update::UpdateBanner;

    #[gpui_kit::test]
    fn the_banner_of_updates_is_installed_with_the_first_page(cx: &mut TestAppContext) {
        let started = start(cx, 1200.0, &[]);
        let _root = finish(&started, cx);
        assert!(cx.update(|cx| UpdateBanner::try_global(cx).is_some()));
    }

    /// Puts a banner that offers `0.2.0` in the place of the real one, and
    /// lets it ask.
    fn offer_an_update(started: &Started, root: &Entity<Root>, cx: &mut TestAppContext) {
        testing::frame(cx, started.window, |window, cx| {
            let banner = cx.new(|cx| UpdateBanner::scripted(true, Ok(Some("0.2.0")), window, cx));
            banner.update(cx, UpdateBanner::check_now);
            UpdateBanner::install(banner, cx);
        });
        root.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn the_banner_stands_above_the_page_and_leaves_it_the_rest(cx: &mut TestAppContext) {
        let started = start(cx, 1200.0, &["Dune"]);
        let root = finish(&started, cx);
        offer_an_update(&started, &root, cx);
        testing::frame(cx, started.window, |window, _| {
            let banner = window.find("update-banner").bounds();
            let page = window.find("library-page").bounds();
            assert!(banner.origin.y < page.origin.y, "{banner:?} {page:?}");
            assert!(
                banner.origin.y + banner.size.height <= page.origin.y + gpui_kit::px(0.5),
                "{banner:?} {page:?}"
            );
            let height = window.bounds().size.height;
            assert!(page.origin.y + page.size.height <= height + gpui_kit::px(0.5));
        });
    }

    #[gpui_kit::test]
    fn the_banner_stays_above_every_screen(cx: &mut TestAppContext) {
        let started = start(cx, 1200.0, &[]);
        let root = finish(&started, cx);
        offer_an_update(&started, &root, cx);
        testing::frame(cx, started.window, |window, cx| {
            window.dispatch_action(OpenSettings.boxed_clone(), cx);
        });
        cx.run_until_parked();
        testing::frame(cx, started.window, |window, _| {
            assert!(window.try_find("update-banner").is_some());
            assert!(window.try_find("settings-page").is_some());
        });
    }

    #[gpui_kit::test]
    fn the_page_is_drawn_without_a_bar_when_nothing_is_offered(cx: &mut TestAppContext) {
        let started = start(cx, 1200.0, &[]);
        let _root = finish(&started, cx);
        testing::frame(cx, started.window, |window, _| {
            assert!(window.try_find("update-banner").is_none());
            assert!(window.try_find("library-page").is_some());
        });
    }
}

#[gpui_kit::test]
fn the_about_action_opens_a_dialog(cx: &mut TestAppContext) {
    use gpui_kit::Window;
    use gpui_kit::component::WindowExt;
    let started = start(cx, 1200.0, &[]);
    let _root = finish(&started, cx);
    testing::frame(cx, started.window, |window, cx| {
        window.dispatch_action(About.boxed_clone(), cx);
    });
    cx.run_until_parked();
    let open = testing::frame(cx, started.window, <Window as WindowExt>::has_active_dialog);
    assert!(open);
}

#[gpui_kit::test]
fn files_that_are_chosen_or_dropped_reach_the_library_page(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &[]);
    let root = finish(&started, cx);
    let paths = vec![
        PathBuf::from("/books/a.epub"),
        PathBuf::from("/books/b.fb2"),
    ];
    testing::frame(cx, started.window, |window, cx| {
        root.update(cx, |root, cx| root.import(paths.clone(), window, cx));
    });
    let received = root.read_with(cx, |root, cx| {
        root.library_page().unwrap().read(cx).received().to_vec()
    });
    assert_eq!(received, paths);
}

#[gpui_kit::test]
fn files_are_not_handed_over_when_none_were_chosen(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &[]);
    let root = finish(&started, cx);
    testing::frame(cx, started.window, |window, cx| {
        root.update(cx, |root, cx| root.import(Vec::new(), window, cx));
    });
    let received = root.read_with(cx, |root, cx| {
        root.library_page().unwrap().read(cx).received().len()
    });
    assert_eq!(received, 0);
}

#[gpui_kit::test]
fn a_change_of_the_interface_size_reaches_the_window(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &[]);
    let _root = finish(&started, cx);
    cx.update(|cx| {
        Appearance::global(cx).update(cx, |appearance, cx| {
            appearance.update(|settings| settings.interface_scale = 1.5, cx);
        });
    });
    cx.run_until_parked();
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(window.rem_size(), gpui_kit::px(24.0));
    });
}

#[gpui_kit::test]
fn a_dark_page_tone_makes_the_whole_window_dark(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &[]);
    let _root = finish(&started, cx);
    cx.update(|cx| {
        Appearance::global(cx).update(cx, |appearance, cx| {
            appearance.update(
                |settings| settings.page_tone = Some(crate::shared::theme::PageTone::Dusk),
                cx,
            );
        });
        assert!(cx.theme().is_dark());
    });
}

#[gpui_kit::test]
fn every_layout_is_drawn_at_390_800_and_1280_pixels(cx: &mut TestAppContext) {
    for width in [390.0, 800.0, 1280.0] {
        let started = start(cx, width, &["Dune"]);
        let _root = finish(&started, cx);
        testing::frame(cx, started.window, |window, _| {
            let page = window.find("library-page");
            assert!(page.visible(), "{width} px");
            assert!(page.bounds().size.width > gpui_kit::px(0.0));
        });
    }
}

/// Every element that assistive technology is told about has a role, and a
/// control has a name.
fn assert_controls_are_named(started: &Started, cx: &mut TestAppContext, screen: &str) {
    const CONTROLS: [Role; 7] = [
        Role::Button,
        Role::Link,
        Role::CheckBox,
        Role::RadioButton,
        Role::Switch,
        Role::TextInput,
        Role::ComboBox,
    ];
    testing::frame(cx, started.window, |window, _| {
        let elements = window.elements();
        assert!(!elements.is_empty(), "{screen}: nothing is reported");
        for element in elements {
            let role = element.role();
            let decoration = testing::is_kit_decoration(&format!("{:?}", element.path()));
            assert!(
                role.is_some() || decoration,
                "{screen}: no role: {element:?}"
            );
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

#[gpui_kit::test]
fn every_control_of_every_screen_has_a_role_and_a_name(cx: &mut TestAppContext) {
    let started = start(cx, 1200.0, &["Dune"]);
    let root = finish(&started, cx);
    let key = started.keys[0];

    assert_controls_are_named(&started, cx, "library");
    testing::frame(cx, started.window, |window, _| {
        assert!(!window.find_all_by_role(Role::Button).is_empty());
        assert_eq!(window.find("add-book").role(), Some(Role::Button));
        assert!(window.try_find_by_label("Add a book…").is_some());
        assert!(window.try_find_by_label("Settings").is_some());
    });

    click(&started, &format!("open-{key}"), cx);
    assert_controls_are_named(&started, cx, "reader");
    testing::frame(cx, started.window, |window, _| {
        assert!(window.try_find_by_label("Back").is_some());
        assert!(window.try_find_by_label("Edit this book").is_some());
    });

    click(&started, "edit", cx);
    assert_controls_are_named(&started, cx, "editor");
    click(&started, "back", cx);
    click(&started, "back", cx);

    click(&started, "settings", cx);
    assert_controls_are_named(&started, cx, "settings");
    assert!(depth(&root, cx) >= 2);
}

#[gpui_kit::test]
fn the_window_of_the_application_opens_and_starts_the_library(cx: &mut TestAppContext) {
    let (dir, roots) = temp_roots();
    let _keep = dir;
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let (_window, shell) = cx.update(|cx| super::window::open(roots, cx).unwrap());
    assert!(shell.read_with(cx, |shell, _| shell.root().is_none()));
    cx.run_until_parked();
    assert!(shell.read_with(cx, |shell, _| shell.root().is_some()));
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
}
