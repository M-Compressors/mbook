//! The editor in headless windows: each size class, the keyboard, the
//! states of opening, saving and leaving, and the errors at their controls.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use futures::executor::block_on;

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Focusable as _, Role, TestAppContext, Window,
};
use markdown_editor::{
    BlockKind, BlockPosition, BlockSelection, ColumnAlign, EditorCommand, TableEdit,
};
use mbook::{Book, BookReader, Landmark};

use super::commands::{ChapterOp, Origin, TYPING_PAUSE};
use super::contents::{
    add_entry, automatic_hint, follow_headings, in_book_order, nest, no_headings, nowhere_words,
    order_words, stored_hint, unnest,
};
use super::encode::Target;
use super::entry_prompt::{no_place, no_title, title_label};
use super::files::place_later;
use super::files::{new_chapter_label, record_save, rename_label};
use super::link_prompt::{address_label, text_label};
use super::panels::open_contents;
use super::picture_prompt::description_label;
use super::session::{Control, Phase, Section, ViewMode};
use super::visual_pictures::{choose_first, insert, no_pictures};
use super::{EditorEvent, EditorPage};
use crate::entities::catalogue::{Library, Store};
use crate::features::appearance::Appearance;
use crate::features::edit_book::command::{Command, ContentsChange, MetaChange};
use crate::features::edit_book::error::EditError;
use crate::shared::model::BookKey;
use crate::shared::roots::Roots;
use crate::testing::{self, Fixture};

/// An editor over a book of the library.
struct Setup {
    fixture: Fixture,
    key: BookKey,
    window: AnyWindowHandle,
    page: Entity<EditorPage>,
    events: Rc<RefCell<Vec<EditorEvent>>>,
}

/// The chapters every test book has.
const CHAPTERS: [(&str, &str); 3] = [
    ("ch01.md", "# One\n\nFirst text.\n"),
    ("ch02.md", "# Two\n\nSecond text.\n"),
    ("ch03.md", "# Three\n\nThird text.\n"),
];

/// Writes a book into the folder of the books, as the library keeps it.
fn write_book(roots: &Roots, chapters: &[(&str, &str)]) -> BookKey {
    let key = BookKey::generate();
    let mut book = Book::new("Dune", "en").unwrap();
    book.meta_mut().add_author("Frank Herbert").unwrap();
    for (path, text) in chapters {
        book.add_chapter(*path, *text).unwrap();
    }
    book.save(roots.book(key)).unwrap();
    key
}

/// Opens the editor of a book with `chapters`, `width` pixels wide. The
/// background has not run yet.
fn start_with(cx: &mut TestAppContext, width: f32, chapters: &[(&str, &str)]) -> Setup {
    let fixture = Fixture::new();
    let key = write_book(&fixture.roots, chapters);
    block_on(record_save(&fixture.roots, &fixture.catalogue, key)).unwrap();
    open(cx, fixture, key, width)
}

fn open(cx: &mut TestAppContext, fixture: Fixture, key: BookKey, width: f32) -> Setup {
    let (roots, catalogue) = (fixture.roots.clone(), fixture.catalogue.clone());
    let (window, page) = testing::open(cx, width, 800.0, move |window, cx| {
        Store { roots, catalogue }.install(cx);
        let appearance = cx.new(Appearance::new);
        Appearance::install(appearance, cx);
        let library = cx.new(Library::new);
        Library::install(library, cx);
        EditorPage::new(key, window, cx)
    });
    cx.update(crate::app::bind_keys);
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    cx.update(|cx| {
        cx.subscribe(&page, move |_, event: &EditorEvent, _| {
            seen.borrow_mut().push(*event);
        })
        .detach();
    });
    Setup {
        fixture,
        key,
        window,
        page,
        events,
    }
}

/// An editor whose book has loaded.
fn started(cx: &mut TestAppContext, width: f32) -> Setup {
    let setup = start_with(cx, width, &CHAPTERS);
    cx.run_until_parked();
    setup
}

/// Runs something on the page with its window.
fn act<R>(
    setup: &Setup,
    cx: &mut TestAppContext,
    run: impl FnOnce(&mut EditorPage, &mut Window, &mut gpui_kit::Context<'_, EditorPage>) -> R,
) -> R {
    let page = setup.page.clone();
    let result = cx
        .update_window(setup.window, |_, window, cx| {
            page.update(cx, |page, cx| run(page, window, cx))
        })
        .unwrap();
    cx.run_until_parked();
    result
}

fn state<R>(setup: &Setup, cx: &TestAppContext, read: impl FnOnce(&EditorPage) -> R) -> R {
    setup.page.read_with(cx, |page, _| read(page))
}

fn frame<R>(
    setup: &Setup,
    cx: &mut TestAppContext,
    inspect: impl FnOnce(&mut Window, &mut gpui_kit::App) -> R,
) -> R {
    testing::frame(cx, setup.window, inspect)
}

fn click(setup: &Setup, id: &str, cx: &mut TestAppContext) {
    let id = id.to_owned();
    frame(setup, cx, move |window, cx| window.click(id, cx));
    cx.run_until_parked();
}

fn click_named(setup: &Setup, label: &str, cx: &mut TestAppContext) {
    let label = label.to_owned();
    frame(setup, cx, move |window, cx| {
        let point = window.find_by_label(&label).bounds().center();
        window.pointer_down(point, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(point, gpui_kit::MouseButton::Left, cx);
    });
    cx.run_until_parked();
}

/// Chooses an item of the menu that is open by its words.
fn choose(setup: &Setup, label: &str, cx: &mut TestAppContext) {
    let label = label.to_owned();
    frame(setup, cx, move |window, cx| {
        let item = window
            .find_all_by_label(&label)
            .into_iter()
            .find(|item| item.role() == Some(Role::MenuItem))
            .unwrap_or_else(|| panic!("no menu item {label}"));
        let point = item.bounds().center();
        window.pointer_down(point, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(point, gpui_kit::MouseButton::Left, cx);
    });
    cx.run_until_parked();
}

fn select_visual(
    setup: &Setup,
    leaf_text: &str,
    range: std::ops::Range<usize>,
    cx: &mut TestAppContext,
) {
    let leaf_text = leaf_text.to_owned();
    act(setup, cx, move |page, _, cx| {
        page.visual.input.update(cx, |input, cx| {
            let leaf = input
                .model()
                .document()
                .leaves()
                .find(|leaf| leaf.text() == Some(leaf_text.as_str()))
                .unwrap()
                .id();
            input
                .set_selection(
                    BlockSelection::Text {
                        anchor: BlockPosition::new(leaf, range.start),
                        head: BlockPosition::new(leaf, range.end),
                    },
                    cx,
                )
                .unwrap();
        });
    });
}

fn press(setup: &Setup, key: &str, cx: &mut TestAppContext) {
    let key = key.to_owned();
    frame(setup, cx, move |window, cx| window.press(&key, cx));
    cx.run_until_parked();
}

fn press_save(setup: &Setup, cx: &mut TestAppContext) {
    let mac = cx.update(|cx| cx.platform_info().os == gpui_kit::OperatingSystem::Mac);
    press(setup, if mac { "cmd-s" } else { "ctrl-s" }, cx);
}

fn focus_editor(setup: &Setup, cx: &mut TestAppContext) {
    act(setup, cx, |page, window, cx| {
        page.text_focus(cx).focus(window, cx);
    });
}

fn editor_text(setup: &Setup, cx: &mut TestAppContext) -> String {
    setup
        .page
        .read_with(cx, |page, cx| page.preview.read(cx).source().to_string())
}

fn saved_titles_and_texts(setup: &Setup) -> (String, Vec<String>) {
    let mut reader = BookReader::open(setup.fixture.roots.book(setup.key)).unwrap();
    let book = reader.load().unwrap();
    (
        book.meta().title().to_owned(),
        book.chapters()
            .iter()
            .map(|chapter| chapter.text().to_owned())
            .collect(),
    )
}

fn dialog_is_open(setup: &Setup, cx: &mut TestAppContext) -> bool {
    cx.update_window(setup.window, |_, window, cx| window.has_active_dialog(cx))
        .unwrap()
}

#[gpui_kit::test]
fn the_landmark_dialog_saves_the_chosen_kind_title_chapter_and_anchor(cx: &mut TestAppContext) {
    let setup = start_with(
        cx,
        1280.0,
        &[
            ("ch01.md", "# Start\n"),
            ("extras/end.md", "Back matter.\n\n## Notes\n"),
        ],
    );
    cx.run_until_parked();
    click(&setup, "section-landmarks", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("section-heading").label(), Some("Landmarks"));
    });
    click(&setup, "add-landmark", cx);
    click_named(&setup, "What the place is, Choose…", cx);
    choose(&setup, "Notes", cx);
    click_named(&setup, "Name shown to readers (optional)", cx);
    frame(&setup, cx, |window, cx| {
        window.input("Explanatory notes", cx);
    });
    click_named(&setup, "Chapter, 1. Start", cx);
    choose(&setup, "2. Notes", cx);
    click_named(&setup, "Place in the chapter, Start of the chapter", cx);
    choose(&setup, "\u{2003}Notes", cx);
    click_named(&setup, "Add", cx);
    click(&setup, "save", cx);
    let mut reader = BookReader::open(setup.fixture.roots.book(setup.key)).unwrap();
    let entries = reader.read_landmarks().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, "notes");
    assert_eq!(entries[0].title, "Explanatory notes");
    assert_eq!(
        entries[0].position.chapter,
        reader.find_chapter("extras/end.md").unwrap().id
    );
    assert_eq!(entries[0].position.offset, 14);
}

#[gpui_kit::test]
fn choosing_a_kind_says_what_a_reader_gets_from_it(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-landmarks", cx);
    click(&setup, "add-landmark", cx);
    frame(&setup, cx, |window, _| {
        let effect = window
            .find("landmark-effect")
            .label()
            .unwrap_or_default()
            .to_owned();
        assert!(effect.contains("by its kind"), "{effect}");
    });
    click_named(&setup, "What the place is, Choose…", cx);
    choose(&setup, "Glossary", cx);
    frame(&setup, cx, |window, _| {
        let effect = window
            .find("landmark-effect")
            .label()
            .unwrap_or_default()
            .to_owned();
        assert!(effect.contains("“Glossary”"), "{effect}");
        assert!(
            window
                .try_find_by_label("What the place is, Glossary")
                .is_some()
        );
    });
}

#[gpui_kit::test]
fn a_landmark_without_a_kind_is_not_added(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-landmarks", cx);
    click(&setup, "add-landmark", cx);
    click_named(&setup, "Add", cx);
    assert!(dialog_is_open(&setup, cx));
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("landmark-error").label(),
            Some("Choose what the place is.")
        );
    });
    assert_eq!(state(&setup, cx, |page| page.session.landmarks().len()), 0);
}

#[gpui_kit::test]
fn marking_the_start_puts_it_in_its_own_block(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    let path = setup.fixture.roots.book(setup.key);
    click(&setup, "section-landmarks", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("landmark-start").label(),
            Some("Where the text begins, not marked")
        );
        assert!(window.try_find("edit-start").is_none());
    });
    click(&setup, "mark-start", cx);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("landmark-kind").is_none());
        assert_eq!(
            window.find("landmark-effect").label(),
            Some("A new book opens here instead of at its cover or title page.")
        );
    });
    click_named(&setup, "Chapter, 1. One", cx);
    choose(&setup, "2. Two", cx);
    click_named(&setup, "Mark", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("landmark-start").label(),
            Some("Where the text begins, 2. Two, Start of the chapter")
        );
        assert!(window.try_find("mark-start").is_none());
        assert!(window.try_find("no-other-places").is_some());
    });
    click(&setup, "save", cx);
    assert_eq!(
        Book::open(&path).unwrap().landmarks(),
        [Landmark::new("start", "", "ch02.md")]
    );
}

fn editor_with_landmarks(cx: &mut TestAppContext, landmarks: Vec<Landmark>) -> Setup {
    let fixture = Fixture::new();
    let key = write_book(&fixture.roots, &CHAPTERS);
    let path = fixture.roots.book(key);
    let mut book = Book::open(&path).unwrap();
    book.set_landmarks(landmarks).unwrap();
    book.save(&path).unwrap();
    block_on(record_save(&fixture.roots, &fixture.catalogue, key)).unwrap();
    let setup = open(cx, fixture, key, 1280.0);
    cx.run_until_parked();
    setup
}

#[gpui_kit::test]
fn retargeting_notes_preserves_the_order_and_targets_of_other_landmarks(cx: &mut TestAppContext) {
    let start = Landmark::new("start", "", "ch01.md");
    let notes = Landmark::new("notes", "", "ch02.md");
    let appendix = Landmark::new("appendix", "Appendix", "ch03.md");
    let setup = editor_with_landmarks(cx, vec![start.clone(), notes, appendix.clone()]);
    let path = setup.fixture.roots.book(setup.key);
    click(&setup, "section-landmarks", cx);
    click_named(&setup, "Edit Notes, 2. Two, Start of the chapter", cx);
    click_named(&setup, "Chapter, 2. Two", cx);
    choose(&setup, "3. Three", cx);
    click_named(&setup, "Apply", cx);
    click(&setup, "save", cx);
    let edited = Book::open(&path).unwrap();
    assert_eq!(
        edited.landmarks(),
        [start, Landmark::new("notes", "", "ch03.md"), appendix,]
    );
}

#[gpui_kit::test]
fn the_landmarks_of_a_removed_chapter_are_counted_and_removed_with_it(cx: &mut TestAppContext) {
    let start = Landmark::new("start", "", "ch01.md");
    let notes = Landmark::new("notes", "", "ch02.md");
    let glossary = Landmark::new("glossary", "", "ch02.md");
    let setup = editor_with_landmarks(cx, vec![start.clone(), notes, glossary]);
    assert_eq!(
        state(&setup, cx, |page| (
            page.session.landmarks_into("ch01.md"),
            page.session.landmarks_into("ch02.md"),
            page.session.landmarks_into("ch03.md"),
        )),
        (1, 2, 0)
    );
    act(&setup, cx, |page, window, cx| {
        page.remove_chapter("ch02.md".to_owned(), window, cx);
    });
    assert_eq!(
        state(&setup, cx, |page| page.session.landmarks().to_vec()),
        std::slice::from_ref(&start)
    );
    click(&setup, "save", cx);
    let saved = Book::open(setup.fixture.roots.book(setup.key)).unwrap();
    assert_eq!(saved.landmarks(), [start]);
}

#[gpui_kit::test]
fn the_notes_of_a_chapter_that_got_another_path_still_lead_to_it(cx: &mut TestAppContext) {
    let setup = editor_with_landmarks(cx, vec![Landmark::new("notes", "", "ch02.md")]);
    act(&setup, cx, |page, window, cx| {
        let (from, to) = ("ch02.md".to_owned(), "notes.md".to_owned());
        page.send(
            Command::MoveChapterPath {
                from: from.clone(),
                to: to.clone(),
            },
            Origin::Chapter(ChapterOp::Rename { from, to }),
            window,
            cx,
        );
    });
    click(&setup, "section-landmarks", cx);
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .try_find_by_label("Edit Notes, 2. Two, Start of the chapter")
                .is_some()
        );
    });
    click(&setup, "save", cx);
    let saved = Book::open(setup.fixture.roots.book(setup.key)).unwrap();
    assert_eq!(saved.landmarks(), [Landmark::new("notes", "", "notes.md")]);
}

#[gpui_kit::test]
fn removing_one_notes_landmark_keeps_the_other_entries(cx: &mut TestAppContext) {
    let glossary = Landmark::new("glossary", "", "ch01.md");
    let notes = Landmark::new("notes", "", "ch02.md");
    let endnotes = Landmark::new("notes", "Endnotes", "ch03.md");
    let setup = editor_with_landmarks(cx, vec![glossary.clone(), notes, endnotes.clone()]);
    let path = setup.fixture.roots.book(setup.key);
    click(&setup, "section-landmarks", cx);
    click_named(&setup, "Remove Notes, 2. Two, Start of the chapter", cx);
    click_named(&setup, "Remove", cx);
    click(&setup, "save", cx);
    assert_eq!(Book::open(&path).unwrap().landmarks(), [glossary, endnotes]);
}

#[gpui_kit::test]
fn a_new_landmark_takes_its_place_in_the_order_of_the_book(cx: &mut TestAppContext) {
    let start = Landmark::new("start", "", "ch01.md");
    let index = Landmark::new("index", "", "ch03.md");
    let setup = editor_with_landmarks(cx, vec![start.clone(), index.clone()]);
    click(&setup, "section-landmarks", cx);
    click(&setup, "add-landmark", cx);
    click_named(&setup, "What the place is, Choose…", cx);
    choose(&setup, "Glossary", cx);
    click_named(&setup, "Chapter, 1. One", cx);
    choose(&setup, "2. Two", cx);
    click_named(&setup, "Add", cx);
    click(&setup, "save", cx);
    assert_eq!(
        Book::open(setup.fixture.roots.book(setup.key))
            .unwrap()
            .landmarks(),
        [start, Landmark::new("glossary", "", "ch02.md"), index]
    );
}

#[gpui_kit::test]
fn changing_the_start_keeps_its_title_and_its_place_in_the_list(cx: &mut TestAppContext) {
    let notes = Landmark::new("notes", "", "ch03.md");
    let start = Landmark::new("start", "Begin reading", "ch01.md");
    let setup = editor_with_landmarks(cx, vec![notes.clone(), start]);
    click(&setup, "section-landmarks", cx);
    click(&setup, "edit-start", cx);
    click_named(&setup, "Chapter, 1. One", cx);
    choose(&setup, "2. Two", cx);
    click_named(&setup, "Apply", cx);
    click(&setup, "save", cx);
    assert_eq!(
        Book::open(setup.fixture.roots.book(setup.key))
            .unwrap()
            .landmarks(),
        [notes, Landmark::new("start", "Begin reading", "ch02.md")]
    );
}

#[gpui_kit::test]
fn removing_the_start_leaves_the_other_places(cx: &mut TestAppContext) {
    let notes = Landmark::new("notes", "", "ch03.md");
    let setup = editor_with_landmarks(
        cx,
        vec![Landmark::new("start", "", "ch01.md"), notes.clone()],
    );
    click(&setup, "section-landmarks", cx);
    click(&setup, "remove-start", cx);
    click_named(&setup, "Remove", cx);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("mark-start").is_some());
    });
    click(&setup, "save", cx);
    assert_eq!(
        Book::open(setup.fixture.roots.book(setup.key))
            .unwrap()
            .landmarks(),
        [notes]
    );
}

#[gpui_kit::test]
fn a_titled_landmark_shows_its_kind_and_an_unknown_kind_keeps_its_name(cx: &mut TestAppContext) {
    let appendix = Landmark::new("appendix", "Tables", "ch02.md");
    let recipes = Landmark::new("x-recipes", "", "ch03.md");
    let setup = editor_with_landmarks(cx, vec![appendix, recipes]);
    click(&setup, "section-landmarks", cx);
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .try_find_by_label("Edit Tables, Appendix, 2. Two, Start of the chapter")
                .is_some()
        );
        assert!(
            window
                .try_find_by_label("Edit x-recipes, 3. Three, Start of the chapter")
                .is_some()
        );
    });
    click_named(&setup, "Edit x-recipes, 3. Three, Start of the chapter", cx);
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .try_find_by_label("What the place is, x-recipes")
                .is_some()
        );
    });
    click_named(&setup, "Chapter, 3. Three", cx);
    choose(&setup, "1. One", cx);
    click_named(&setup, "Apply", cx);
    click(&setup, "save", cx);
    assert_eq!(
        Book::open(setup.fixture.roots.book(setup.key))
            .unwrap()
            .landmarks(),
        [
            Landmark::new("appendix", "Tables", "ch02.md"),
            Landmark::new("x-recipes", "", "ch01.md")
        ]
    );
}

#[gpui_kit::test]
fn a_landmark_changed_under_its_dialog_is_left_alone(cx: &mut TestAppContext) {
    let glossary = Landmark::new("glossary", "", "ch01.md");
    let setup = editor_with_landmarks(cx, vec![Landmark::new("notes", "", "ch02.md")]);
    click(&setup, "section-landmarks", cx);
    let replace = |setup: &Setup, cx: &mut TestAppContext| {
        let list = vec![glossary.clone()];
        act(setup, cx, move |page, window, cx| {
            page.session.landmarks_pending = true;
            page.send(Command::SetLandmarks(list), Origin::Landmarks, window, cx);
        });
    };
    click_named(&setup, "Edit Notes, 2. Two, Start of the chapter", cx);
    replace(&setup, cx);
    click_named(&setup, "Apply", cx);
    assert_eq!(
        state(&setup, cx, |page| page.session.landmarks().to_vec()),
        std::slice::from_ref(&glossary)
    );
    let setup = editor_with_landmarks(cx, vec![Landmark::new("notes", "", "ch02.md")]);
    click(&setup, "section-landmarks", cx);
    click_named(&setup, "Remove Notes, 2. Two, Start of the chapter", cx);
    replace(&setup, cx);
    click_named(&setup, "Remove", cx);
    assert_eq!(
        state(&setup, cx, |page| page.session.landmarks().to_vec()),
        std::slice::from_ref(&glossary)
    );
}

#[gpui_kit::test]
fn a_landmark_waits_for_the_change_before_it(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-landmarks", cx);
    act(&setup, cx, |page, window, cx| {
        page.session.landmarks_pending = true;
        page.put_landmark(None, Landmark::new("notes", "", "ch01.md"), window, cx);
    });
    assert_eq!(state(&setup, cx, |page| page.session.landmarks().len()), 0);
}

#[gpui_kit::test]
fn a_book_without_chapters_asks_for_no_landmark(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[]);
    cx.run_until_parked();
    act(&setup, cx, |page, window, cx| {
        page.ask_new_landmark(None, window, cx);
        page.ask_landmark_at(0, &Landmark::new("notes", "", "ch01.md"), window, cx);
    });
    assert!(!dialog_is_open(&setup, cx));
}

#[gpui_kit::test]
fn a_new_landmark_starts_at_the_first_chapter_when_none_is_open(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-landmarks", cx);
    act(&setup, cx, |page, window, cx| {
        page.session.open = None;
        page.ask_new_landmark(Some("start"), window, cx);
    });
    frame(&setup, cx, |window, _| {
        assert!(window.try_find_by_label("Chapter, 1. One").is_some());
    });
}

#[gpui_kit::test]
fn a_notes_landmark_with_a_missing_anchor_blocks_save(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-landmarks", cx);
    act(&setup, cx, |page, window, cx| {
        page.send(
            Command::SetLandmarks(vec![Landmark::new("notes", "", "ch01.md#missing")]),
            Origin::Landmarks,
            window,
            cx,
        );
    });
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .try_find_by_label("Edit Notes, 1. One, #missing (missing)")
                .is_some()
        );
    });
    click(&setup, "save", cx);
    assert_eq!(
        state(&setup, cx, |page| page.session.section),
        Section::Problems
    );
    assert!(state(&setup, cx, |page| {
        page.session.problems().iter().any(|problem| {
            problem
                .message
                .contains(r#"landmark "notes" leads nowhere"#)
        })
    }));
    let unchanged = Book::open(setup.fixture.roots.book(setup.key)).unwrap();
    assert_eq!(unchanged.landmarks(), []);
}

#[gpui_kit::test]
fn an_empty_book_explains_that_landmarks_need_a_chapter(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[]);
    cx.run_until_parked();
    click(&setup, "section-landmarks", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("no-chapters-to-mark").label(),
            Some("Add a chapter before marking places in the book.")
        );
    });
    click(&setup, "add-landmark", cx);
    assert!(!dialog_is_open(&setup, cx));
    click(&setup, "mark-start", cx);
    assert!(!dialog_is_open(&setup, cx));
}

#[gpui_kit::test]
fn a_refused_landmark_update_shows_the_problem_in_landmarks(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-landmarks", cx);
    act(&setup, cx, |page, window, cx| {
        page.session.landmarks_pending = true;
        page.send(
            Command::SetLandmarks(vec![Landmark::new("", "", "ch01.md")]),
            Origin::Landmarks,
            window,
            cx,
        );
    });
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("landmarks-error").label(),
            Some(
                r#"The book refused the change: invalid landmarks: "" is not a valid kind of landmark."#
            )
        );
    });
    assert!(!state(&setup, cx, |page| page.session.has_unsaved()));
}

#[gpui_kit::test]
fn the_page_says_that_the_book_is_opening_until_the_background_has_run(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &CHAPTERS);
    frame(&setup, cx, |window, _| {
        let status = window.find("editor-opening");
        assert_eq!(status.role(), Some(Role::Status));
        assert_eq!(status.label(), Some("Opening the book"));
        assert_eq!(window.find_by_label("Back").role(), Some(Role::Button));
        assert!(window.try_find("save").is_none());
    });
    assert_eq!(
        state(&setup, cx, |page| page.session.phase.clone()),
        Phase::Loading
    );
}

#[gpui_kit::test]
fn a_book_that_cannot_be_opened_shows_the_reason_and_can_be_left(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = BookKey::generate();
    let setup = open(cx, fixture, key, 1280.0);
    cx.run_until_parked();
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("empty-title").label(),
            Some("This book could not be opened")
        );
        assert!(window.try_find("save").is_none());
        assert_eq!(window.find_by_label("Back").role(), Some(Role::Button));
    });
    let reason = state(&setup, cx, |page| page.session.phase.clone());
    assert!(
        matches!(&reason, Phase::Failed(text) if text.contains("could not be opened")),
        "{reason:?}"
    );
    click(&setup, "back", cx);
    assert_eq!(setup.events.borrow().as_slice(), [EditorEvent::Close]);
}

#[gpui_kit::test]
fn a_file_that_is_not_a_book_shows_why(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = fixture.add_book("Fake");
    let setup = open(cx, fixture, key, 1280.0);
    cx.run_until_parked();
    let phase = state(&setup, cx, |page| page.session.phase.clone());
    assert!(
        matches!(&phase, Phase::Failed(text) if text.contains("not an mbook file")),
        "{phase:?}"
    );
}

#[gpui_kit::test]
fn an_expanded_window_shows_the_sections_the_source_and_the_preview(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-split", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("editor-page").role(), Some(Role::Main));
        assert_eq!(window.find("page-heading").label(), Some("Dune"));
        for section in Section::BOOK {
            assert_eq!(window.find(section.id()).role(), Some(Role::Button));
        }
        assert!(window.try_find(Section::Chapters.id()).is_none());
        for (path, _) in CHAPTERS {
            assert!(
                window.try_find(format!("chapter-{path}")).is_some(),
                "{path}"
            );
        }
        assert!(window.try_find("chapter-source").is_some());
        assert!(window.try_find("chapter-text").is_some());
        assert_eq!(window.find_by_label("Split").role(), Some(Role::Button));
        assert!(window.try_find("open-sections").is_none());
    });
    assert_eq!(
        state(&setup, &*cx, |page| page.session.mode),
        ViewMode::Split
    );
    assert_eq!(editor_text(&setup, cx), CHAPTERS[0].1);
}

#[gpui_kit::test]
fn a_medium_window_shows_visual_and_slides_the_sections_over_it(cx: &mut TestAppContext) {
    let setup = started(cx, 800.0);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("visual-viewport").is_some());
        assert!(
            window.try_find("chapter-text").is_none(),
            "split is for expanded windows"
        );
        assert!(window.try_find("section-details").is_none());
        assert_eq!(window.find("open-sections").role(), Some(Role::Button));
    });
    click(&setup, "open-sections", cx);
    cx.run_until_parked();
    let sheet = cx
        .update_window(setup.window, |_, window, cx| window.has_active_sheet(cx))
        .unwrap();
    assert!(sheet);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("section-details").is_some());
        assert!(window.try_find("chapter-ch02.md").is_some());
    });
    click(&setup, "chapter-ch02.md", cx);
    assert_eq!(
        state(&setup, cx, |page| page.session.open.clone()).as_deref(),
        Some("ch02.md")
    );
    assert_eq!(editor_text(&setup, cx), CHAPTERS[1].1);
}

#[gpui_kit::test]
fn selecting_the_active_preview_mode_keeps_it_selected(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.choose_section(Section::Chapters, window, cx);
    });
    click(&setup, "mode-preview", cx);
    click(&setup, "mode-preview", cx);
    assert_eq!(
        state(&setup, cx, |page| page.session.mode),
        ViewMode::Preview
    );
}

#[gpui_kit::test]
fn the_source_editor_uses_the_markdown_highlighter(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    setup.page.read_with(cx, |page, cx| {
        assert_eq!(page.text.read(cx).language_name().as_str(), "markdown");
    });
}

#[gpui_kit::test]
fn visual_text_edits_save_as_markdown_without_rewriting_the_surroundings(cx: &mut TestAppContext) {
    let original = "# One\n\nA **word** and [a link](https://example.com).\n";
    let setup = start_with(cx, 1280.0, &[("ch01.md", original)]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "A word and a link.", 3..5, cx);
    frame(&setup, cx, |window, cx| window.input("ор", cx));
    click(&setup, "save", cx);
    assert_eq!(
        saved_titles_and_texts(&setup).1,
        ["# One\n\nA **wорd** and [a link](https://example.com).\n"]
    );
}

#[gpui_kit::test]
fn enter_in_visual_prose_moves_the_caret_down_one_line_and_saves_a_paragraph(
    cx: &mut TestAppContext,
) {
    let setup = start_with(cx, 1280.0, &[("ch01.md", "First")]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    press(&setup, "end", cx);
    press(&setup, "enter", cx);
    frame(&setup, cx, |window, cx| window.input("Second", cx));

    setup.page.read_with(cx, |page, cx| {
        let paragraphs: Vec<_> = page
            .visual
            .input
            .read(cx)
            .model()
            .document()
            .leaves()
            .filter_map(|leaf| leaf.text())
            .collect();
        assert_eq!(paragraphs, ["First", "Second"]);
    });
    click(&setup, "save", cx);
    assert_eq!(saved_titles_and_texts(&setup).1, ["First\n\nSecond"]);
}

#[gpui_kit::test]
fn visual_formatting_can_be_undone_and_redone_before_switching_to_source(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "First text.", 0..5, cx);
    let mac = cx.update(|cx| cx.platform_info().os == gpui_kit::OperatingSystem::Mac);
    click(&setup, "format-bold", cx);
    assert_eq!(editor_text(&setup, cx), "# One\n\n**First** text.\n");
    press(&setup, if mac { "cmd-z" } else { "ctrl-z" }, cx);
    assert_eq!(editor_text(&setup, cx), CHAPTERS[0].1);
    click(&setup, "visual-redo", cx);
    click(&setup, "mode-source", cx);
    assert_eq!(editor_text(&setup, cx), "# One\n\n**First** text.\n");
}

#[gpui_kit::test]
fn visual_paragraph_style_changes_the_heading_level(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    focus_editor(&setup, cx);
    select_visual(&setup, "One", 0..0, cx);
    click(&setup, "visual-paragraph-style", cx);
    click_named(&setup, "Heading 2", cx);
    assert_eq!(editor_text(&setup, cx), "## One\n\nFirst text.\n");
}

#[gpui_kit::test]
fn visual_quote_formats_the_current_paragraph(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    focus_editor(&setup, cx);
    select_visual(&setup, "First text.", 0..0, cx);
    click(&setup, "format-quote", cx);
    assert_eq!(editor_text(&setup, cx), "# One\n\n> First text.\n");
}

#[gpui_kit::test]
fn visual_table_edits_keep_other_blocks_and_save_the_table(cx: &mut TestAppContext) {
    let original = "Heading  with  spaces\n=====================\n\nSee [spot] and [^note].\n\n| Label | Value |\n| :--- | ---: |\n| One | 7 |\n\n[spot]: #spot\n\n[^note]: A *footnote*.\n";
    let setup = start_with(cx, 1280.0, &[("ch01.md", original)]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "7", 0..1, cx);
    frame(&setup, cx, |window, cx| window.input("9", cx));
    act(&setup, cx, |page, _, cx| {
        page.visual.input.update(cx, |input, cx| {
            assert!(
                input
                    .run(EditorCommand::EditTable(TableEdit::AddRowBelow), cx)
                    .unwrap()
            );
            assert!(
                input
                    .run(EditorCommand::EditTable(TableEdit::AddColumnRight), cx)
                    .unwrap()
            );
            assert!(
                input
                    .run(
                        EditorCommand::EditTable(TableEdit::SetColumnAlign(ColumnAlign::Center)),
                        cx
                    )
                    .unwrap()
            );
        });
    });
    click(&setup, "save", cx);
    let source = &saved_titles_and_texts(&setup).1[0];
    assert!(
        source.starts_with(
            "Heading  with  spaces\n=====================\n\nSee [spot] and [^note].\n\n"
        ),
        "{source}"
    );
    assert!(
        source.ends_with("[spot]: #spot\n\n[^note]: A *footnote*.\n"),
        "{source}"
    );
    let markdown::mdast::Node::Root(root) =
        markdown::to_mdast(source, &markdown::ParseOptions::gfm()).unwrap()
    else {
        panic!("chapter root");
    };
    let table = root
        .children
        .iter()
        .find_map(|block| match block {
            markdown::mdast::Node::Table(table) => Some(table),
            _ => None,
        })
        .unwrap();
    assert_eq!(table.children.len(), 3);
    assert_eq!(table.align.len(), 3);
    assert!(table.align.contains(&markdown::mdast::AlignKind::Center));
    assert!(table.children[1].to_string().contains('9'));
}

#[gpui_kit::test]
fn the_visual_insert_menu_creates_a_table(cx: &mut TestAppContext) {
    let setup = start_with(cx, 390.0, &[("ch01.md", "")]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    click_named(&setup, "Insert", cx);
    click_named(&setup, "Table", cx);
    setup.page.read_with(cx, |page, cx| {
        assert_eq!(
            page.visual
                .input
                .read(cx)
                .model()
                .document()
                .root()
                .children()
                .next()
                .unwrap()
                .kind(),
            BlockKind::Table
        );
    });
}

#[gpui_kit::test]
fn the_visual_link_dialog_wraps_selected_text(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[("ch01.md", "See the book.")]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "See the book.", 4..12, cx);
    click(&setup, "visual-link", cx);
    frame(&setup, cx, |window, cx| {
        assert!(window.try_find_by_label(&text_label()).is_none());
        window.input("https://example.com", cx);
    });
    click_named(&setup, "Add the link", cx);
    assert_eq!(
        editor_text(&setup, cx),
        "See [the book](https://example.com)."
    );
}

#[gpui_kit::test]
fn a_link_can_use_a_chapter_from_the_picker(cx: &mut TestAppContext) {
    let setup = start_with(
        cx,
        1280.0,
        &[
            ("part/a.md", "Read more."),
            ("extras/next chapter.md", "More text."),
        ],
    );
    cx.run_until_parked();
    focus_editor(&setup, cx);
    select_visual(&setup, "Read more.", 0..9, cx);
    click(&setup, "visual-link", cx);
    click_named(&setup, "Chapter, Choose a chapter", cx);
    choose(&setup, "2. Untitled chapter", cx);
    click_named(&setup, "Add the link", cx);
    assert_eq!(
        editor_text(&setup, cx),
        "[Read more](../extras/next%20chapter.md)."
    );
}

#[gpui_kit::test]
fn removing_a_visual_link_keeps_its_text(cx: &mut TestAppContext) {
    let setup = start_with(
        cx,
        1280.0,
        &[("ch01.md", "See [the book](https://example.com).")],
    );
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "See the book.", 6..6, cx);
    click(&setup, "visual-link", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find_by_label("Apply").role(),
            Some(Role::Button),
            "a link at the caret is edited"
        );
    });
    click(&setup, "link-remove", cx);
    assert!(!dialog_is_open(&setup, cx));
    assert_eq!(editor_text(&setup, cx), "See the book.");
}

#[gpui_kit::test]
fn cancelling_the_visual_link_dialog_keeps_the_chapter(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[("ch01.md", "See the book.")]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "See the book.", 4..12, cx);
    click(&setup, "visual-link", cx);
    click_named(&setup, "Cancel", cx);
    assert_eq!(editor_text(&setup, cx), "See the book.");
}

#[gpui_kit::test]
fn visual_inserts_and_previews_a_picture_relative_to_the_chapter(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = BookKey::generate();
    let mut book = Book::new("Dune", "en").unwrap();
    book.add_chapter("part/ch01.md", "").unwrap();
    book.add_picture("img/sand.avif", avif_of(8, 6)).unwrap();
    book.save(fixture.roots.book(key)).unwrap();
    block_on(record_save(&fixture.roots, &fixture.catalogue, key)).unwrap();
    let setup = open(cx, fixture, key, 1280.0);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    click(&setup, "visual-image", cx);
    click_named(&setup, "img/sand.avif", cx);
    frame(&setup, cx, |window, cx| {
        assert!(window.try_find_by_label("img/sand.avif, chosen").is_some());
        window.input("Sand & [dunes]", cx);
    });
    click_named(&setup, &insert(), cx);
    click(&setup, "visual-image-preview", cx);
    frame(&setup, cx, |_, _| ());
    cx.run_until_parked();
    assert!(state(&setup, cx, |page| page
        .visual
        .pictures
        .lock()
        .unwrap()
        .has_cached("img/sand.avif", (8, 6))));
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("chapter-picture-preview").label(),
            Some("Preview of img/sand.avif")
        );
    });
    click_named(&setup, "Close", cx);
    click(&setup, "save", cx);
    assert_eq!(
        saved_titles_and_texts(&setup).1,
        ["![Sand & [dunes]](../img/sand.avif)"],
        "the editor writes the description back in its own spelling"
    );
}

#[gpui_kit::test]
fn the_chapter_picture_preview_can_switch_images(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = BookKey::generate();
    let mut book = Book::new("Pictures", "en").unwrap();
    book.add_chapter(
        "ch01.md",
        "![A pond](first.avif)\n\n![A forest][forest]\n\n[forest]: second.avif",
    )
    .unwrap();
    book.add_picture("first.avif", avif_of(8, 6)).unwrap();
    book.add_picture("second.avif", avif_of(6, 8)).unwrap();
    book.save(fixture.roots.book(key)).unwrap();
    block_on(record_save(&fixture.roots, &fixture.catalogue, key)).unwrap();
    let setup = open(cx, fixture, key, 1280.0);
    cx.run_until_parked();
    click(&setup, "visual-image-preview", cx);
    click_named(&setup, "Choose picture", cx);
    click_named(&setup, "second.avif", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("chapter-picture-preview").label(),
            Some("Preview of second.avif")
        );
    });
}

#[gpui_kit::test]
fn the_native_clipboard_actions_use_the_visual_selection(cx: &mut TestAppContext) {
    use gpui_kit::Action as _;
    use gpui_kit::component::input::{Copy, Cut, Undo};

    let setup = start_with(cx, 1280.0, &[("ch01.md", "A **word**.")]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "A word.", 2..6, cx);
    frame(&setup, cx, |window, cx| {
        window.dispatch_action(Copy.boxed_clone(), cx);
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        "**word**"
    );
    frame(&setup, cx, |window, cx| {
        window.dispatch_action(Cut.boxed_clone(), cx);
    });
    cx.run_until_parked();
    assert_eq!(editor_text(&setup, cx), "A .");
    frame(&setup, cx, |window, cx| {
        window.dispatch_action(Undo.boxed_clone(), cx);
    });
    cx.run_until_parked();
    assert_eq!(editor_text(&setup, cx), "A **word**.");
}

#[gpui_kit::test]
fn the_native_edit_actions_select_all_and_redo_in_visual(cx: &mut TestAppContext) {
    use gpui_kit::Action as _;
    use gpui_kit::component::input::{Redo, SelectAll, Undo};

    let setup = start_with(cx, 1280.0, &[("ch01.md", "First line.\n\nSecond line.")]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| {
        window.dispatch_action(SelectAll.boxed_clone(), cx);
    });
    cx.run_until_parked();
    frame(&setup, cx, |window, cx| window.input("Replaced", cx));
    assert_eq!(editor_text(&setup, cx), "Replaced");
    frame(&setup, cx, |window, cx| {
        window.dispatch_action(Undo.boxed_clone(), cx);
    });
    cx.run_until_parked();
    frame(&setup, cx, |window, cx| {
        window.dispatch_action(Redo.boxed_clone(), cx);
    });
    cx.run_until_parked();
    assert_eq!(editor_text(&setup, cx), "Replaced");
}

#[gpui_kit::test]
fn a_visual_code_block_uses_the_selected_language(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[("ch01.md", "```\nlet answer = 42;\n```")]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    select_visual(&setup, "let answer = 42;", 0..0, cx);
    click(&setup, "visual-code-language", cx);
    click_named(&setup, "Rust", cx);
    assert_eq!(editor_text(&setup, cx), "```rust\nlet answer = 42;\n```");
}

#[gpui_kit::test]
fn changing_chapters_flushes_the_visual_editor(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("New ", cx));
    click(&setup, "chapter-ch02.md", cx);
    click(&setup, "save", cx);
    assert_eq!(
        saved_titles_and_texts(&setup).1,
        ["# New One\n\nFirst text.\n", CHAPTERS[1].1, CHAPTERS[2].1]
    );
}

#[gpui_kit::test]
fn a_medium_window_offers_the_source_or_the_preview(cx: &mut TestAppContext) {
    let setup = started(cx, 800.0);
    act(&setup, cx, |page, _, cx| {
        page.session.mode = ViewMode::Preview;
        cx.notify();
    });
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("chapter-text").is_some());
        assert!(window.try_find("chapter-source").is_none());
        assert!(window.try_find_by_label("Split").is_none());
        assert!(window.try_find_by_label("Source").is_some());
    });
}

#[gpui_kit::test]
fn a_compact_window_opens_the_sections_from_its_menu_button(cx: &mut TestAppContext) {
    let setup = started(cx, 390.0);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("visual-viewport").is_some());
        assert!(window.try_find("sections").is_none());
        let menu = window.find("open-sections");
        assert_eq!(menu.role(), Some(Role::Button));
        assert_eq!(menu.label(), Some("Book and chapters"));
    });
    click(&setup, "open-sections", cx);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("sections").is_some());
        assert!(
            window.try_find("chapter-ch01.md").is_some(),
            "the list holds the chapters too"
        );
    });
    click(&setup, "section-details", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("section-heading").label(), Some("Details"));
        assert!(window.try_find("sections").is_none(), "the sheet closed");
    });
    assert!(setup.events.borrow().is_empty());
}

#[gpui_kit::test]
fn a_compact_window_opens_a_chapter_from_the_sheet(cx: &mut TestAppContext) {
    let setup = started(cx, 390.0);
    click(&setup, "open-sections", cx);
    click(&setup, "chapter-ch02.md", cx);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("visual-viewport").is_some());
        assert!(window.try_find("sections").is_none());
    });
    assert_eq!(editor_text(&setup, cx), CHAPTERS[1].1);
}

#[gpui_kit::test]
fn back_in_a_compact_window_leaves_the_editor(cx: &mut TestAppContext) {
    let setup = started(cx, 390.0);
    click(&setup, "back", cx);
    assert_eq!(setup.events.borrow().as_slice(), [EditorEvent::Close]);
}

#[gpui_kit::test]
fn an_expanded_window_shows_the_sections_beside_the_chapter(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("open-sections").is_none());
        assert!(window.try_find("sections").is_some());
    });
}

#[gpui_kit::test]
fn the_chapter_menu_opens_the_rename_dialog(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "chapter-menu", cx);
    choose(&setup, "Rename the file…", cx);
    frame(&setup, cx, |window, cx| {
        assert_eq!(
            window.find_by_label(&rename_label()).role(),
            Some(Role::TextInput)
        );
        window.press("cmd-a", cx);
        window.press("ctrl-a", cx);
    });
    act(&setup, cx, |_, window, cx| {
        window.dispatch_action(Box::new(gpui_kit::component::input::SelectAll), cx);
    });
    frame(&setup, cx, |window, cx| window.input("start.md", cx));
    click_named(&setup, "Rename", cx);
    assert_eq!(
        state(&setup, cx, |page| page.session.open.clone()).as_deref(),
        Some("start.md")
    );
}

#[gpui_kit::test]
fn renaming_a_file_to_nothing_is_refused_in_the_dialog(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |_, window, cx| {
        EditorPage::change_path(String::new(), window, cx);
    });
    click_named(&setup, "Rename", cx);
    assert!(dialog_is_open(&setup, cx));
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("path-error").label(),
            Some("Enter a file name, such as ch01.md.")
        );
    });
    click_named(&setup, "Cancel", cx);
    assert_eq!(state(&setup, cx, |page| page.session.chapters().len()), 3);
}

#[gpui_kit::test]
fn the_chapter_menu_asks_before_removing_the_chapter(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "chapter-menu", cx);
    choose(&setup, "Remove the chapter…", cx);
    assert!(dialog_is_open(&setup, cx));
    assert_eq!(
        state(&setup, cx, |page| page.chapter_name("ch01.md")),
        "“One”"
    );
    assert_eq!(state(&setup, cx, |page| page.session.chapters().len()), 3);
    click_named(&setup, "Remove", cx);
    assert_eq!(state(&setup, cx, |page| page.session.chapters().len()), 2);
}

#[gpui_kit::test]
fn a_chapter_without_a_heading_is_asked_about_by_its_file(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[("plain.md", "No heading.\n")]);
    cx.run_until_parked();
    assert_eq!(
        state(&setup, cx, |page| page.chapter_name("plain.md")),
        "plain.md"
    );
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .try_find_by_label("Chapter 1, Untitled chapter, plain.md, current chapter")
                .is_some()
        );
    });
}

#[gpui_kit::test]
fn every_control_of_every_section_has_a_role_and_a_name(cx: &mut TestAppContext) {
    const CONTROLS: [Role; 7] = [
        Role::Button,
        Role::Link,
        Role::CheckBox,
        Role::RadioButton,
        Role::Switch,
        Role::TextInput,
        Role::ComboBox,
    ];
    for width in [1280.0, 800.0, 390.0] {
        let setup = started(cx, width);
        for section in Section::ALL {
            act(&setup, cx, |page, window, cx| {
                page.choose_section(section, window, cx);
            });
            frame(&setup, cx, |window, _| {
                let elements = window.elements();
                assert!(
                    !elements.is_empty(),
                    "{width} {section:?}: nothing is reported"
                );
                for element in elements {
                    let role = element.role();
                    let decoration = testing::is_kit_decoration(&format!("{:?}", element.path()));
                    assert!(
                        role.is_some() || decoration,
                        "{width} {section:?}: no role: {element:?}"
                    );
                    if role.is_some_and(|role| CONTROLS.contains(&role)) {
                        assert!(
                            element
                                .label()
                                .is_some_and(|label| !label.trim().is_empty()),
                            "{width} {section:?}: a control without a name: {element:?}"
                        );
                    }
                }
            });
        }
    }
}

#[gpui_kit::test]
fn the_current_chapter_and_an_edited_one_are_told_in_words(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .try_find_by_label("Chapter 1, One, ch01.md, current chapter")
                .is_some()
        );
        assert!(
            window
                .try_find_by_label("Chapter 2, Two, ch02.md")
                .is_some()
        );
        assert_eq!(
            window.find("chapter-title").label(),
            Some("Chapter 1, One, ch01.md")
        );
    });
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("More. ", cx));
    cx.run_until_parked();
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .try_find_by_label("Chapter 1, One, ch01.md, current chapter, edited")
                .is_some()
        );
        assert_eq!(
            window.find("editor-status").label(),
            Some("Unsaved changes")
        );
    });
}

#[gpui_kit::test]
fn typing_updates_the_preview_at_once_and_the_book_after_a_pause(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-split", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("Hello ", cx));
    assert!(editor_text(&setup, cx).starts_with("Hello "));
    assert!(state(&setup, cx, |page| page.session.has_pending()));
    let book_text = |setup: &Setup, cx: &mut TestAppContext| {
        act(setup, cx, |page, window, cx| {
            let reply = page.handle.chapter_text("ch01.md");
            cx.spawn_in(window, async move |_, _| drop(reply.await))
                .detach();
        });
    };
    book_text(&setup, cx);
    cx.executor()
        .advance_clock(TYPING_PAUSE + Duration::from_millis(1));
    cx.run_until_parked();
    assert!(!state(&setup, cx, |page| page.session.has_pending()));
    let outline_len = state(&setup, cx, |page| page.session.chapters()[0].len);
    assert_eq!(outline_len, "Hello ".len() + CHAPTERS[0].1.len());
}

#[gpui_kit::test]
fn opening_another_chapter_sends_the_text_first(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-source", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("Hello ", cx));
    click(&setup, "chapter-ch02.md", cx);
    assert_eq!(editor_text(&setup, cx), CHAPTERS[1].1);
    click(&setup, "chapter-ch01.md", cx);
    assert!(editor_text(&setup, cx).starts_with("Hello "));
}

#[gpui_kit::test]
fn tab_indents_and_escape_then_tab_leaves_the_editor(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-source", cx);
    focus_editor(&setup, cx);
    press(&setup, "tab", cx);
    assert!(editor_text(&setup, cx).starts_with(' '), "Tab indents");
    assert!(state(&setup, &*cx, |page| !page.escaped));
    press(&setup, "escape", cx);
    assert!(state(&setup, &*cx, |page| page.escaped));
    assert!(
        setup.events.borrow().is_empty(),
        "Escape in the editor does not leave"
    );
    let focused = act(&setup, cx, |page, window, cx| {
        page.text_focus(cx).contains_focused(window, cx)
    });
    assert!(focused);
    press(&setup, "tab", cx);
    let focused = act(&setup, cx, |page, window, cx| {
        page.text_focus(cx).contains_focused(window, cx)
    });
    assert!(!focused, "Tab after Escape moves the focus on");
    assert!(state(&setup, &*cx, |page| !page.escaped));
    assert!(setup.events.borrow().is_empty());
}

#[gpui_kit::test]
fn escape_then_tab_leaves_the_visual_table(cx: &mut TestAppContext) {
    let setup = start_with(
        cx,
        1280.0,
        &[("ch01.md", "| First | Second |\n| --- | --- |")],
    );
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "First", 0..0, cx);
    press(&setup, "escape", cx);
    assert!(state(&setup, &*cx, |page| page.escaped));
    press(&setup, "tab", cx);
    let focused = act(&setup, cx, |page, window, cx| {
        page.text_focus(cx).contains_focused(window, cx)
    });
    assert!(!focused);
    assert!(setup.events.borrow().is_empty());
}

#[gpui_kit::test]
fn the_editor_says_in_text_how_to_leave_it(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-source", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("editor-hint").label(),
            Some("Tab indents. Press Escape, then Tab, to leave the editor.")
        );
        assert_eq!(window.find("chapter-source").role(), Some(Role::Group));
    });
}

#[gpui_kit::test]
fn escape_outside_the_editor_leaves_a_book_without_changes(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.focus_handle.focus(window, cx);
    });
    press(&setup, "escape", cx);
    assert_eq!(setup.events.borrow().as_slice(), [EditorEvent::Close]);
}

#[gpui_kit::test]
fn leaving_with_unsaved_changes_asks_and_discard_leaves(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("x", cx));
    click(&setup, "back", cx);
    assert!(setup.events.borrow().is_empty());
    assert!(dialog_is_open(&setup, cx));
    frame(&setup, cx, |window, _| {
        for id in ["leave-cancel", "leave-discard", "leave-save"] {
            assert_eq!(window.find(id).role(), Some(Role::Button), "{id}");
        }
    });
    click(&setup, "leave-discard", cx);
    assert_eq!(setup.events.borrow().as_slice(), [EditorEvent::Close]);
    let (_, texts) = saved_titles_and_texts(&setup);
    assert_eq!(texts[0], CHAPTERS[0].1, "nothing was written");
}

#[gpui_kit::test]
fn cancelling_the_question_keeps_the_editor_open(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("x", cx));
    click(&setup, "back", cx);
    click(&setup, "leave-cancel", cx);
    assert!(setup.events.borrow().is_empty());
    assert!(!dialog_is_open(&setup, cx));
}

#[gpui_kit::test]
fn saving_from_the_question_saves_and_then_leaves(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-source", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("x", cx));
    click(&setup, "back", cx);
    click(&setup, "leave-save", cx);
    cx.run_until_parked();
    assert_eq!(
        setup.events.borrow().as_slice(),
        [EditorEvent::Saved(setup.key), EditorEvent::Close]
    );
    let (_, texts) = saved_titles_and_texts(&setup);
    assert!(texts[0].starts_with('x'));
}

#[gpui_kit::test]
fn the_save_key_saves_what_was_typed_without_waiting_for_the_pause(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-source", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("Typed. ", cx));
    press_save(&setup, cx);
    cx.run_until_parked();
    let (_, texts) = saved_titles_and_texts(&setup);
    assert!(texts[0].starts_with("Typed. "), "{:?}", texts[0]);
    assert_eq!(
        setup.events.borrow().as_slice(),
        [EditorEvent::Saved(setup.key)]
    );
    assert!(!state(&setup, cx, |page| page.session.has_unsaved()));
}

#[gpui_kit::test]
fn the_save_key_saves_while_the_visual_editor_has_focus(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("Chapter ", cx));
    press_save(&setup, cx);
    assert_eq!(
        saved_titles_and_texts(&setup).1[0],
        "# Chapter One\n\nFirst text.\n"
    );
}

#[gpui_kit::test]
fn save_builds_an_empty_contents_from_a_new_heading(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[("ch01.md", "")]);
    cx.run_until_parked();
    let path = setup.fixture.roots.book(setup.key);
    assert_eq!(BookReader::open(&path).unwrap().toc(), []);

    click(&setup, "mode-source", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("# Hello", cx));
    click(&setup, "save", cx);

    let reader = BookReader::open(&path).unwrap();
    let titles: Vec<_> = reader
        .toc()
        .iter()
        .map(|entry| entry.title.as_str())
        .collect();
    assert_eq!(titles, ["Hello"]);
}

#[gpui_kit::test]
fn save_keeps_the_title_of_a_stored_contents_item_when_a_heading_changes(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[("ch01.md", "# Original\n")]);
    cx.run_until_parked();
    click(&setup, "mode-source", cx);
    act(&setup, cx, |page, window, cx| {
        page.text.update(cx, |editor, cx| {
            editor.replace_all("# Revised\n", window, cx);
        });
    });
    click(&setup, "save", cx);

    let reader = BookReader::open(setup.fixture.roots.book(setup.key)).unwrap();
    let titles: Vec<_> = reader
        .toc()
        .iter()
        .map(|entry| entry.title.as_str())
        .collect();
    assert_eq!(titles, ["Original"]);
}

#[gpui_kit::test]
fn saving_with_the_title_field_focused_commits_its_pending_text(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-details", cx);
    act(&setup, cx, |page, window, cx| {
        page.fields.title.focus_handle(cx).focus(window, cx);
        page.fields.title.update(cx, |input, cx| {
            input.select_all(window, cx);
        });
    });
    frame(&setup, cx, |window, cx| window.input("A new title", cx));
    press_save(&setup, cx);
    assert_eq!(saved_titles_and_texts(&setup).0, "A new title");
}

#[gpui_kit::test]
fn saving_makes_the_row_of_the_library_again(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.send(
            Command::SetMeta(MetaChange::Title("Dune Messiah".to_owned())),
            Origin::Meta(super::session::MetaField::Title),
            window,
            cx,
        );
    });
    assert_eq!(
        state(&setup, cx, |page| page.session.title().map(str::to_owned)).as_deref(),
        Some("Dune Messiah")
    );
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("page-heading").label(), Some("Dune Messiah"));
    });
    click(&setup, "save", cx);
    cx.run_until_parked();
    let row = setup.fixture.catalogue.book(setup.key).unwrap().unwrap();
    assert_eq!(row.title, "Dune Messiah");
    assert_eq!(saved_titles_and_texts(&setup).0, "Dune Messiah");
}

#[gpui_kit::test]
fn a_book_with_problems_is_not_written_and_the_problems_are_shown(cx: &mut TestAppContext) {
    let setup = start_with(
        cx,
        1280.0,
        &[
            ("ch01.md", "# One\n\nText.\n"),
            ("ch02.md", "# Two\n\nText.\n"),
        ],
    );
    cx.run_until_parked();
    click(&setup, "mode-source", cx);
    let before = std::fs::read(setup.fixture.roots.book(setup.key)).unwrap();
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| {
        window.input("[gone](missing.md) ", cx);
    });
    click(&setup, "save", cx);
    cx.run_until_parked();
    assert_eq!(
        std::fs::read(setup.fixture.roots.book(setup.key)).unwrap(),
        before
    );
    assert!(setup.events.borrow().is_empty());
    assert_eq!(
        state(&setup, cx, |page| page.session.section),
        Section::Problems
    );
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("problems-summary").label(),
            Some("The book has 1 problem and cannot be saved until they are fixed.")
        );
        let problem = window.find("problem-0");
        assert_eq!(problem.role(), Some(Role::Button));
        let label = problem.label().unwrap();
        assert!(label.starts_with("ch01.md, line 1, column 1: "), "{label}");
        assert!(label.contains("leads nowhere"), "{label}");
        assert_eq!(
            window.find_by_label("Problems, 1, current section").role(),
            Some(Role::Button)
        );
        assert!(window.try_find("rebuild-contents").is_none());
    });
    assert!(state(&setup, cx, |page| page.session.has_unsaved()));
}

#[gpui_kit::test]
fn choosing_a_problem_opens_its_chapter_and_puts_the_caret_there(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "chapter-ch02.md", cx);
    act(&setup, cx, |page, window, cx| {
        page.text.update(cx, |state, cx| {
            state.replace_all("# Two\n\n[gone](missing.md)\n", window, cx);
        });
    });
    click(&setup, "chapter-ch01.md", cx);
    click(&setup, "section-problems", cx);
    click(&setup, "check", cx);
    frame(&setup, cx, |window, _| {
        let label = window.find("problem-0").label().unwrap().to_owned();
        assert!(label.starts_with("ch02.md, line 3, column 1: "), "{label}");
    });
    click(&setup, "problem-0", cx);
    assert_eq!(
        state(&setup, cx, |page| page.session.open.clone()).as_deref(),
        Some("ch02.md")
    );
    assert_eq!(
        state(&setup, cx, |page| page.session.section),
        Section::Chapters
    );
    let cursor = setup
        .page
        .read_with(cx, |page, cx| page.text.read(cx).cursor());
    assert_eq!(cursor, 7);
}

#[gpui_kit::test]
fn a_contents_entry_whose_heading_was_removed_offers_to_rebuild_the_contents(
    cx: &mut TestAppContext,
) {
    let setup = editor_with_three_entries(cx);
    set_chapter_text(&setup, "ch01.md", "# One\n", cx);
    click(&setup, "save", cx);
    cx.run_until_parked();
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("rebuild-contents").role(), Some(Role::Button));
        assert!(
            window
                .find("problem-0")
                .label()
                .unwrap()
                .contains("contents item")
        );
    });
    click(&setup, "rebuild-contents", cx);
    assert_eq!(state(&setup, cx, |page| page.session.problems().len()), 0);
    click(&setup, "save", cx);
    cx.run_until_parked();
    assert_eq!(
        setup.events.borrow().as_slice(),
        [EditorEvent::Saved(setup.key)]
    );
}

#[gpui_kit::test]
fn a_text_the_book_refuses_is_told_at_the_editor_with_its_place(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.text
            .update(cx, |state, cx| state.replace_all("ab\u{1}c", window, cx));
    });
    cx.executor()
        .advance_clock(TYPING_PAUSE + Duration::from_millis(1));
    cx.run_until_parked();
    frame(&setup, cx, |window, _| {
        let message = window
            .find("chapter-text-error")
            .label()
            .unwrap()
            .to_owned();
        assert!(message.contains("line 1, column 3"), "{message}");
        assert!(message.contains("control character"), "{message}");
    });
    assert_eq!(
        setup
            .page
            .read_with(cx, |page, cx| page.text.read(cx).cursor()),
        2
    );
    assert!(state(&setup, cx, |page| page.session.has_refused_text()));
}

#[gpui_kit::test]
fn a_refused_text_is_kept_when_another_chapter_is_opened_and_blocks_a_save(
    cx: &mut TestAppContext,
) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.text
            .update(cx, |state, cx| state.replace_all("ab\u{1}c", window, cx));
    });
    cx.executor()
        .advance_clock(TYPING_PAUSE + Duration::from_millis(1));
    cx.run_until_parked();
    click(&setup, "chapter-ch02.md", cx);
    assert_eq!(editor_text(&setup, cx), CHAPTERS[1].1);
    click(&setup, "save", cx);
    frame(&setup, cx, |window, _| {
        let message = window.find("save-error").label().unwrap().to_owned();
        assert!(message.contains("ch01.md"), "{message}");
    });
    let (_, texts) = saved_titles_and_texts(&setup);
    assert_eq!(texts[0], CHAPTERS[0].1);
    click(&setup, "chapter-ch01.md", cx);
    assert_eq!(editor_text(&setup, cx), "ab\u{1}c");
}

#[gpui_kit::test]
fn a_chapter_moves_up_and_down_with_its_controls(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "chapter-ch02.md", cx);
    click(&setup, "chapter-menu", cx);
    choose(&setup, "Move down", cx);
    let order = |setup: &Setup, cx: &TestAppContext| {
        state(setup, cx, |page| {
            page.session
                .chapters()
                .iter()
                .map(|chapter| chapter.path.clone())
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(order(&setup, cx), ["ch01.md", "ch03.md", "ch02.md"]);
    for _ in 0..2 {
        click(&setup, "chapter-menu", cx);
        choose(&setup, "Move up", cx);
    }
    assert_eq!(order(&setup, cx), ["ch02.md", "ch01.md", "ch03.md"]);
    assert_eq!(
        state(&setup, cx, |page| page.session.open.clone()).as_deref(),
        Some("ch02.md")
    );
    assert!(state(&setup, cx, |page| page.session.has_unsaved()));
}

#[gpui_kit::test]
fn the_first_chapter_cannot_move_up_and_the_last_cannot_move_down(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.move_chapter("ch01.md", -1, window, cx);
        page.move_chapter("ch03.md", 1, window, cx);
    });
    assert!(!state(&setup, cx, |page| page.session.has_unsaved()));
    assert!(state(&setup, cx, |page| page
        .session
        .error(&Control::Chapters)
        .is_none()));
}

#[gpui_kit::test]
fn a_chapter_that_gets_another_path_keeps_its_text_and_its_place(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-source", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("Kept. ", cx));
    act(&setup, cx, |page, window, cx| {
        page.flush_text(window, cx);
        page.send(
            Command::MoveChapterPath {
                from: "ch01.md".to_owned(),
                to: "start.md".to_owned(),
            },
            Origin::Chapter(ChapterOp::Rename {
                from: "ch01.md".to_owned(),
                to: "start.md".to_owned(),
            }),
            window,
            cx,
        );
    });
    assert_eq!(
        state(&setup, cx, |page| page.session.open.clone()).as_deref(),
        Some("start.md")
    );
    assert_eq!(
        state(&setup, cx, |page| page.loaded.clone()).as_deref(),
        Some("start.md")
    );
    assert!(editor_text(&setup, cx).starts_with("Kept. "));
    let paths = state(&setup, cx, |page| {
        page.session
            .chapters()
            .iter()
            .map(|chapter| chapter.path.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(paths, ["start.md", "ch02.md", "ch03.md"]);
    assert!(state(&setup, cx, |page| page.session.is_edited("start.md")));
}

#[gpui_kit::test]
fn a_path_the_book_refuses_is_told_at_the_chapters(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.send(
            Command::AddChapter {
                path: "ch02.md".to_owned(),
                text: String::new(),
            },
            Origin::Chapter(ChapterOp::Add("ch02.md".to_owned())),
            window,
            cx,
        );
    });
    frame(&setup, cx, |window, _| {
        let message = window.find("chapters-error").label().unwrap().to_owned();
        assert!(message.contains("already used"), "{message}");
    });
    assert_eq!(state(&setup, cx, |page| page.session.chapters().len()), 3);
}

#[gpui_kit::test]
fn a_chapter_that_is_added_is_opened(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.send(
            Command::AddChapter {
                path: "ch04.md".to_owned(),
                text: String::new(),
            },
            Origin::Chapter(ChapterOp::Add("ch04.md".to_owned())),
            window,
            cx,
        );
    });
    assert_eq!(
        state(&setup, cx, |page| page.session.open.clone()).as_deref(),
        Some("ch04.md")
    );
    assert_eq!(editor_text(&setup, cx), "");
    assert_eq!(state(&setup, cx, |page| page.session.chapters().len()), 4);
}

#[gpui_kit::test]
fn removing_the_open_chapter_opens_the_next_one(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "chapter-ch02.md", cx);
    act(&setup, cx, |page, window, cx| {
        page.remove_chapter("ch02.md".to_owned(), window, cx);
    });
    assert_eq!(
        state(&setup, cx, |page| page.session.open.clone()).as_deref(),
        Some("ch03.md")
    );
    assert_eq!(editor_text(&setup, cx), CHAPTERS[2].1);
}

#[gpui_kit::test]
fn a_book_without_chapters_offers_to_add_one(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[]);
    cx.run_until_parked();
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("empty-title").label(),
            Some("This book has no chapters yet")
        );
        assert_eq!(window.find("add-first-chapter").role(), Some(Role::Button));
    });
    click(&setup, "add-first-chapter", cx);
    assert!(dialog_is_open(&setup, cx));
}

#[gpui_kit::test]
fn a_new_chapter_is_asked_for_by_its_title_and_starts_with_it_as_a_heading(
    cx: &mut TestAppContext,
) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-source", cx);
    click(&setup, "add-chapter", cx);
    assert!(dialog_is_open(&setup, cx));
    frame(&setup, cx, |window, cx| {
        assert_eq!(
            window.find_by_label(&new_chapter_label()).role(),
            Some(Role::TextInput)
        );
        assert_eq!(window.find_by_label("Cancel").role(), Some(Role::Button));
        window.input("Frost & *ice*", cx);
    });
    click_named(&setup, "Add the chapter", cx);
    assert_eq!(
        state(&setup, cx, |page| page.session.open.clone()).as_deref(),
        Some("ch04.md")
    );
    assert_eq!(editor_text(&setup, cx), "# Frost \\& \\*ice\\*\n\n");
    // The chapter is ready to be written: what is typed lands under its
    // heading, in Visual, whatever view was in front before.
    assert_eq!(
        state(&setup, cx, |page| page.session.mode),
        ViewMode::Visual
    );
    frame(&setup, cx, |window, cx| window.input("It came early.", cx));
    cx.run_until_parked();
    assert_eq!(
        editor_text(&setup, cx),
        "# Frost \\& \\*ice\\*\n\nIt came early."
    );
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .try_find_by_label("Chapter 4, Frost & *ice*, ch04.md, current chapter, edited")
                .is_some()
        );
    });
}

#[gpui_kit::test]
fn a_new_chapter_without_a_title_is_empty_and_untitled(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "add-chapter", cx);
    click_named(&setup, "Add the chapter", cx);
    assert_eq!(editor_text(&setup, cx), "");
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .try_find_by_label("Chapter 4, Untitled chapter, ch04.md, current chapter")
                .is_some()
        );
    });
}

#[gpui_kit::test]
fn the_details_form_shows_the_metadata_with_visible_labels(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-details", cx);
    frame(&setup, cx, |window, _| {
        for label in [
            "Title",
            "Authors (one per line)",
            "Language (a code such as en or ru)",
            "Series",
            "Number in the series",
            "Publisher",
            "Date",
            "Annotation",
        ] {
            let field = window.find_by_label(label);
            assert!(field.role().is_some(), "{label}");
        }
    });
    let values = setup.page.read_with(cx, EditorPage::meta_values);
    assert_eq!(values.title, "Dune");
    assert_eq!(values.authors, "Frank Herbert");
    assert_eq!(values.language, "en");
}

#[gpui_kit::test]
fn a_field_is_sent_when_it_loses_the_focus_and_a_bad_value_is_told_at_the_field(
    cx: &mut TestAppContext,
) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-details", cx);
    act(&setup, cx, |page, window, cx| {
        page.fields
            .title
            .update(cx, |state, cx| state.set_value("", window, cx));
        page.commit_meta(super::session::MetaField::Title, window, cx);
    });
    frame(&setup, cx, |window, _| {
        let message = window.find("error-title").label().unwrap().to_owned();
        assert!(message.contains("the title is empty"), "{message}");
    });
    act(&setup, cx, |page, window, cx| {
        page.fields
            .title
            .update(cx, |state, cx| state.set_value("Dune II", window, cx));
        page.commit_meta(super::session::MetaField::Title, window, cx);
    });
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("error-title").is_none());
    });
    assert_eq!(
        state(&setup, cx, |page| page.session.title().map(str::to_owned)).as_deref(),
        Some("Dune II")
    );
}

#[gpui_kit::test]
fn a_field_is_sent_after_a_pause_in_typing(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-details", cx);
    act(&setup, cx, |page, window, cx| {
        let title = page.fields.title.clone();
        title.update(cx, |state, cx| state.focus(window, cx));
    });
    frame(&setup, cx, |window, cx| window.input("!", cx));
    cx.executor()
        .advance_clock(super::commands::FIELD_PAUSE + Duration::from_millis(1));
    cx.run_until_parked();
    let title = state(&setup, cx, |page| page.session.title().map(str::to_owned));
    assert_eq!(title.as_deref(), Some("Dune!"));
}

#[gpui_kit::test]
fn a_field_that_agrees_with_the_book_sends_nothing(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.commit_meta(super::session::MetaField::Title, window, cx);
    });
    assert!(!state(&setup, cx, |page| page.session.has_unsaved()));
}

#[gpui_kit::test]
fn the_cover_section_says_when_there_is_no_cover_and_offers_to_add_one(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-cover", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("cover-state").label(),
            Some("This book has no cover.")
        );
        assert_eq!(
            window.find_by_label("Add a cover…").role(),
            Some(Role::Button)
        );
    });
}

#[gpui_kit::test]
fn the_picture_gallery_shows_a_thumbnail_and_a_remove_control(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = BookKey::generate();
    let mut book = Book::new("Dune", "en").unwrap();
    book.add_chapter("ch01.md", "# One\n").unwrap();
    let avif = avif_of(280, 280);
    book.set_cover("img/p.avif", avif).unwrap();
    book.save(fixture.roots.book(key)).unwrap();
    block_on(record_save(&fixture.roots, &fixture.catalogue, key)).unwrap();
    let setup = open(cx, fixture, key, 1280.0);
    cx.run_until_parked();
    click(&setup, "section-pictures", cx);
    frame(&setup, cx, |_, _| ());
    cx.run_until_parked();
    frame(&setup, cx, |window, _| {
        let thumb = window.find("preview-img/p.avif");
        let remove = window.find_by_label("Remove img/p.avif");
        assert_eq!(thumb.role(), Some(Role::Image));
        assert_eq!(remove.role(), Some(Role::Button));
        assert!(
            thumb.bounds().bottom() <= remove.bounds().top(),
            "{thumb:?} overlaps {remove:?}"
        );
    });
    cx.run_until_parked();
    assert!(state(&setup, cx, |page| page
        .visual
        .pictures
        .lock()
        .unwrap()
        .has_cached("img/p.avif", (192, 192))));
    click(&setup, "section-cover", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("cover-image").role(), Some(Role::Image));
        assert_eq!(window.find_by_label("Replace…").role(), Some(Role::Button));
    });
}

/// A small AVIF picture.
fn avif_of(width: u32, height: u32) -> Vec<u8> {
    use crate::entities::picture::{EncodeOptions, encode_avif};
    let picture = image::RgbImage::from_pixel(width, height, image::Rgb([200, 30, 30]));
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(picture)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let options = EncodeOptions {
        speed: 10,
        quality: 50,
        ..EncodeOptions::default()
    };
    encode_avif(&png.into_inner(), options).unwrap()
}

#[gpui_kit::test]
fn a_picture_file_is_encoded_at_the_resolution_selected_in_the_dialog(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pond.png");
    let picture = image::RgbImage::from_pixel(16, 8, image::Rgb([1, 2, 3]));
    picture.save(&file).unwrap();
    act(&setup, cx, |page, window, cx| {
        page.encode_files(vec![file.clone()], Target::Picture, window, cx);
    });
    assert_eq!(state(&setup, cx, |page| page.session.encoding), 0);
    assert!(dialog_is_open(&setup, cx));
    click_named(&setup, "Max width (px)", cx);
    frame(&setup, cx, |window, cx| window.input("8", cx));
    click_named(&setup, "Add", cx);
    click(&setup, "save", cx);
    let reader = BookReader::open(setup.fixture.roots.book(setup.key)).unwrap();
    let picture = reader.find_picture("pond.avif").unwrap();
    assert_eq!((picture.size.width, picture.size.height), (8, 4));
}

#[gpui_kit::test]
fn an_invalid_picture_resolution_keeps_the_dialog_open(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    let file = setup.fixture.roots.data().join("pond.png");
    act(&setup, cx, |page, window, cx| {
        page.encode_files(vec![file.clone()], Target::Picture, window, cx);
    });
    click_named(&setup, "Max height (px)", cx);
    frame(&setup, cx, |window, cx| window.input("0", cx));
    click_named(&setup, "Add", cx);
    assert!(dialog_is_open(&setup, cx));
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .find("picture-settings-error")
                .label()
                .unwrap()
                .contains("positive number")
        );
    });
    click_named(&setup, "Cancel", cx);
    assert!(!state(&setup, cx, |page| page.session.has_unsaved()));
}

#[gpui_kit::test]
fn a_picture_that_is_being_encoded_is_shown_as_encoding_and_counts_as_unsaved(
    cx: &mut TestAppContext,
) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-pictures", cx);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pond.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3]))
        .save(&file)
        .unwrap();
    let page = setup.page.clone();
    cx.update_window(setup.window, |_, window, cx| {
        page.update(cx, |page, cx| {
            page.encode_files(vec![file.clone()], Target::Picture, window, cx);
        });
    })
    .unwrap();
    frame(&setup, cx, |window, cx| {
        let point = window.find_by_label("Add").bounds().center();
        window.pointer_down(point, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(point, gpui_kit::MouseButton::Left, cx);
    });
    assert!(state(&setup, cx, |page| page.session.has_unsaved()));
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("encoding-pond.png").label(),
            Some("Encoding pond.png…")
        );
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_file_that_is_not_a_picture_is_reported_at_the_control_and_nothing_is_added(
    cx: &mut TestAppContext,
) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-pictures", cx);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    std::fs::write(&file, b"not a picture").unwrap();
    act(&setup, cx, |page, window, cx| {
        page.encode_files(vec![file.clone()], Target::Picture, window, cx);
    });
    click_named(&setup, "Add", cx);
    frame(&setup, cx, |window, _| {
        let message = window.find("pictures-error").label().unwrap().to_owned();
        assert_eq!(
            message,
            "notes.txt was not added: it is not an AVIF, JPEG, PNG, GIF, WebP or BMP picture."
        );
    });
    assert!(!dialog_is_open(&setup, cx));
    assert!(!state(&setup, cx, |page| page.session.has_unsaved()));
}

#[gpui_kit::test]
fn a_cover_file_that_cannot_be_read_is_reported_at_the_cover(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-cover", cx);
    let missing = setup.fixture.roots.data().join("missing.png");
    act(&setup, cx, |page, window, cx| {
        page.encode_files(vec![missing.clone()], Target::Cover, window, cx);
    });
    click_named(&setup, "Add", cx);
    frame(&setup, cx, |window, _| {
        let message = window.find("cover-error").label().unwrap().to_owned();
        assert!(
            message.starts_with("missing.png was not added: it could not be read"),
            "{message}"
        );
    });
}

#[gpui_kit::test]
fn a_picture_is_added_to_the_book_by_its_path(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-pictures", cx);
    let avif = avif_of(8, 6);
    act(&setup, cx, |page, window, cx| {
        page.send(
            Command::AddPicture {
                path: "img/p.avif".to_owned(),
                avif: avif.clone(),
            },
            Origin::AddPicture,
            window,
            cx,
        );
    });
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find_by_label("Remove img/p.avif").role(),
            Some(Role::Button)
        );
        assert!(window.try_find("no-pictures").is_none());
    });
    act(&setup, cx, |page, window, cx| {
        page.send(
            Command::AddPicture {
                path: "img/p.avif".to_owned(),
                avif,
            },
            Origin::AddPicture,
            window,
            cx,
        );
    });
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .find("pictures-error")
                .label()
                .unwrap()
                .contains("already used")
        );
    });
}

#[gpui_kit::test]
fn a_save_waits_for_the_pictures_that_are_being_encoded(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.session.encoding = 1;
        page.save(window, cx);
    });
    assert!(state(&setup, cx, |page| page.saving.requested));
    assert!(!state(&setup, cx, |page| page.saving.running));
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("editor-status").label(), Some("Saving"));
    });
    act(&setup, cx, |page, window, cx| {
        page.session.encoding = 0;
        page.continue_save(window, cx);
    });
    assert!(!state(&setup, cx, |page| page.saving.requested));
    assert_eq!(
        setup.events.borrow().as_slice(),
        [EditorEvent::Saved(setup.key)]
    );
}

#[gpui_kit::test]
#[expect(
    clippy::used_underscore_binding,
    reason = "the field is only held, so that dropping the page ends the session; the test replaces it"
)]
fn dropping_the_task_that_owns_the_book_ends_the_session(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    let handle = state(&setup, cx, |page| page.handle.clone());
    // The page holds the task; when the page goes, this is what is dropped.
    act(&setup, cx, |page, _, _| {
        page._actor = gpui_kit::Task::ready(());
    });
    let reply = handle.chapter_text("ch01.md");
    let ended = cx.foreground_executor().block_test(reply);
    assert!(matches!(ended, Err(EditError::Ended)), "{ended:?}");
}

#[gpui_kit::test]
fn the_preview_is_the_chapter_view_of_the_reader(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-preview", cx);
    frame(&setup, cx, |window, _| {
        let preview = window.find("chapter-text");
        assert_eq!(preview.role(), Some(Role::Article));
        assert_eq!(preview.label(), Some("Preview of ch01.md"));
    });
}

#[gpui_kit::test]
fn record_save_keeps_the_day_and_the_format_of_the_row(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    let mut row = setup.fixture.catalogue.book(setup.key).unwrap().unwrap();
    row.added_ms = 42;
    row.source_format = Some("epub".to_owned());
    setup.fixture.catalogue.put_book(&row).unwrap();
    let mut book = BookReader::open(setup.fixture.roots.book(setup.key))
        .unwrap()
        .load()
        .unwrap();
    book.meta_mut().set_title("Changed").unwrap();
    book.save(setup.fixture.roots.book(setup.key)).unwrap();
    block_on(record_save(
        &setup.fixture.roots,
        &setup.fixture.catalogue,
        setup.key,
    ))
    .unwrap();
    let after = setup.fixture.catalogue.book(setup.key).unwrap().unwrap();
    assert_eq!(after.title, "Changed");
    assert_eq!(after.added_ms, 42);
    assert_eq!(after.source_format.as_deref(), Some("epub"));
}

#[gpui_kit::test]
fn record_save_makes_the_thumbnail_of_the_cover_and_removes_it_without_one(
    cx: &mut TestAppContext,
) {
    let setup = started(cx, 1280.0);
    let thumbnail = setup.fixture.roots.cover(setup.key);
    let mut book = BookReader::open(setup.fixture.roots.book(setup.key))
        .unwrap()
        .load()
        .unwrap();
    book.set_cover("c.avif", avif_of(16, 16)).unwrap();
    book.save(setup.fixture.roots.book(setup.key)).unwrap();
    block_on(record_save(
        &setup.fixture.roots,
        &setup.fixture.catalogue,
        setup.key,
    ))
    .unwrap();
    assert!(thumbnail.is_file());
    let row = setup.fixture.catalogue.book(setup.key).unwrap().unwrap();
    assert_eq!(row.cover, Some((16, 16)));
}

#[gpui_kit::test]
fn record_save_reports_a_file_that_is_gone(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    std::fs::remove_file(setup.fixture.roots.book(setup.key)).unwrap();
    let error = block_on(record_save(
        &setup.fixture.roots,
        &setup.fixture.catalogue,
        setup.key,
    ))
    .unwrap_err();
    assert_eq!(error.to_string(), "the saved file could not be measured");
}

/// A book with one chapter in a folder and one picture.
fn editor_with_a_picture(cx: &mut TestAppContext, text: &str) -> Setup {
    let fixture = Fixture::new();
    let key = BookKey::generate();
    let mut book = Book::new("Dune", "en").unwrap();
    book.add_chapter("part/ch01.md", text).unwrap();
    book.add_picture("img/sand.avif", avif_of(8, 6)).unwrap();
    book.save(fixture.roots.book(key)).unwrap();
    block_on(record_save(&fixture.roots, &fixture.catalogue, key)).unwrap();
    let setup = open(cx, fixture, key, 1280.0);
    cx.run_until_parked();
    setup
}

#[gpui_kit::test]
fn the_picture_dialog_asks_for_a_picture_before_it_inserts(cx: &mut TestAppContext) {
    let setup = editor_with_a_picture(cx, "");
    click(&setup, "visual-image", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find_by_label(&description_label()).role(),
            Some(Role::TextInput)
        );
        assert_eq!(window.find("picture-upload").role(), Some(Role::Button));
    });
    click_named(&setup, &insert(), cx);
    assert!(dialog_is_open(&setup, cx));
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("picture-choice-error").label(),
            Some(choose_first().as_ref())
        );
    });
    click_named(&setup, "img/sand.avif", cx);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("picture-choice-error").is_none());
    });
    click_named(&setup, &insert(), cx);
    assert!(!dialog_is_open(&setup, cx));
    assert_eq!(editor_text(&setup, cx), "![](../img/sand.avif)");
}

#[gpui_kit::test]
fn the_picture_dialog_of_a_book_without_pictures_offers_a_file(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "visual-image", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("no-book-pictures").label(),
            Some(no_pictures().as_ref())
        );
        assert!(window.try_find_by_label(&description_label()).is_none());
        assert_eq!(window.find("picture-upload").role(), Some(Role::Button));
    });
    click_named(&setup, "Cancel", cx);
    assert_eq!(editor_text(&setup, cx), CHAPTERS[0].1);
}

/// Writes a small PNG file and returns the folder that keeps it.
fn png_file(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(name);
    image::RgbImage::from_pixel(16, 8, image::Rgb([1, 2, 3]))
        .save(&file)
        .unwrap();
    (dir, file)
}

#[gpui_kit::test]
fn a_picture_file_dropped_on_a_chapter_joins_the_book_and_the_text(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[("part/ch01.md", "")]);
    cx.run_until_parked();
    let (_dir, file) = png_file("pond.png");
    let book = std::path::PathBuf::from("/books/other.epub");
    act(&setup, cx, |page, window, cx| {
        page.drop_files(std::slice::from_ref(&book), window, cx);
    });
    assert!(!dialog_is_open(&setup, cx), "a book is not a picture");
    act(&setup, cx, |page, window, cx| {
        page.drop_files(&[book, file.clone()], window, cx);
    });
    assert!(dialog_is_open(&setup, cx));
    frame(&setup, cx, |window, cx| {
        assert_eq!(
            window.find_by_label(&description_label()).role(),
            Some(Role::TextInput)
        );
        window.input("A pond", cx);
    });
    click_named(&setup, "Add", cx);
    assert_eq!(editor_text(&setup, cx), "![A pond](../pond.avif)");
    assert_eq!(
        state(&setup, cx, |page| page.session.pictures().len()),
        1,
        "the picture is one of the book"
    );
    click(&setup, "save", cx);
    assert_eq!(
        saved_titles_and_texts(&setup).1,
        ["![A pond](../pond.avif)"]
    );
}

#[gpui_kit::test]
fn a_picture_that_arrives_after_its_chapter_was_left_is_kept_out_of_the_text(
    cx: &mut TestAppContext,
) {
    let setup = started(cx, 1280.0);
    let (_dir, file) = png_file("pond.png");
    act(&setup, cx, |page, window, cx| {
        page.encode_files(vec![file.clone()], Target::Chapter, window, cx);
    });
    // The chapter is left while the dialog is open: the picture belonged to
    // the first one.
    act(&setup, cx, |page, window, cx| {
        page.open_chapter("ch02.md", window, cx);
    });
    click_named(&setup, "Add", cx);
    assert_eq!(state(&setup, cx, |page| page.session.pictures().len()), 1);
    assert_eq!(editor_text(&setup, cx), CHAPTERS[1].1);
    assert!(state(&setup, cx, |page| page.encoder.placing.is_none()));
    assert_ne!(place_later(), "");
}

#[gpui_kit::test]
fn a_file_that_is_not_a_picture_is_reported_at_the_chapter_it_was_meant_for(
    cx: &mut TestAppContext,
) {
    let setup = started(cx, 1280.0);
    let file = setup.fixture.roots.data().join("notes.png");
    std::fs::write(&file, b"not a picture").unwrap();
    act(&setup, cx, |page, window, cx| {
        page.encode_files(vec![file.clone()], Target::Chapter, window, cx);
    });
    click_named(&setup, "Add", cx);
    frame(&setup, cx, |window, _| {
        let message = window
            .find("chapter-picture-error")
            .label()
            .unwrap()
            .to_owned();
        assert!(message.contains("notes.png was not added"), "{message}");
    });
    assert_eq!(editor_text(&setup, cx), CHAPTERS[0].1);
}

#[gpui_kit::test]
fn a_picture_the_book_refuses_is_reported_at_the_chapter(cx: &mut TestAppContext) {
    let setup = editor_with_a_picture(cx, "");
    let (_dir, file) = png_file("sand.png");
    act(&setup, cx, |page, window, cx| {
        page.encode_files(vec![file.clone()], Target::Chapter, window, cx);
    });
    // The name the dialog proposes is free; the one typed here is taken.
    click_named(&setup, &super::picture_prompt::name_label(), cx);
    act(&setup, cx, |_, window, cx| {
        window.dispatch_action(Box::new(gpui_kit::component::input::SelectAll), cx);
    });
    frame(&setup, cx, |window, cx| window.input("img/sand.avif", cx));
    click_named(&setup, "Add", cx);
    frame(&setup, cx, |window, _| {
        let message = window
            .find("chapter-picture-error")
            .label()
            .unwrap()
            .to_owned();
        assert!(message.contains("already used"), "{message}");
    });
    assert_eq!(editor_text(&setup, cx), "");
    assert!(state(&setup, cx, |page| page.encoder.placing.is_none()));
}

#[gpui_kit::test]
fn a_link_without_a_selection_asks_for_its_words_and_inserts_them(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[("ch01.md", "See .")]);
    cx.run_until_parked();
    focus_editor(&setup, cx);
    select_visual(&setup, "See .", 4..4, cx);
    click(&setup, "visual-link", cx);
    click_named(&setup, "Add the link", cx);
    assert!(dialog_is_open(&setup, cx));
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("link-error").label(),
            Some("Enter the text of the link.")
        );
    });
    click_named(&setup, &text_label(), cx);
    frame(&setup, cx, |window, cx| window.input("the *book*", cx));
    click_named(&setup, &address_label(), cx);
    frame(&setup, cx, |window, cx| {
        window.input("https://example.com", cx);
    });
    click_named(&setup, "Add the link", cx);
    assert!(!dialog_is_open(&setup, cx));
    assert_eq!(
        editor_text(&setup, cx),
        "See [the \\*book\\*](https://example.com)."
    );
}

#[gpui_kit::test]
fn a_link_can_lead_to_a_heading_inside_a_chapter(cx: &mut TestAppContext) {
    let setup = start_with(
        cx,
        1280.0,
        &[
            ("ch01.md", "Read more."),
            ("ch02.md", "# Two\n\nText.\n\n## The pond\n"),
        ],
    );
    cx.run_until_parked();
    focus_editor(&setup, cx);
    select_visual(&setup, "Read more.", 0..9, cx);
    click(&setup, "visual-link", cx);
    click_named(&setup, "Chapter, Choose a chapter", cx);
    choose(&setup, "2. Two", cx);
    click_named(&setup, "Place in the chapter, Start of the chapter", cx);
    choose(&setup, "\u{2003}The pond", cx);
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .try_find_by_label("Place in the chapter, The pond")
                .is_some()
        );
    });
    click_named(&setup, "Add the link", cx);
    assert_eq!(editor_text(&setup, cx), "[Read more](ch02.md#the-pond).");
}

#[gpui_kit::test]
fn the_contents_follow_the_headings_and_lead_to_their_chapters(cx: &mut TestAppContext) {
    let setup = start_with(
        cx,
        1280.0,
        &[("ch01.md", "# One\n\n## Inside\n"), ("ch02.md", "# Two\n")],
    );
    cx.run_until_parked();
    // A book that was written keeps its contents as a list: this one is
    // told to follow its headings.
    act(&setup, cx, |page, window, cx| {
        page.follow_headings(window, cx);
    });
    click(&setup, "section-contents", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("section-heading").label(), Some("Contents"));
        assert_eq!(
            window.find("contents-hint").label(),
            Some(automatic_hint().as_ref())
        );
        assert!(window.try_find("follow-headings").is_none());
        assert!(window.try_find("no-contents").is_none());
        for name in [
            "One, level 1, in 1. One",
            "Inside, level 2, in 1. One",
            "Two, level 1, in 2. Two",
        ] {
            assert_eq!(
                window.find_by_label(name).role(),
                Some(Role::Button),
                "{name}"
            );
        }
    });
    click_named(&setup, "Two, level 1, in 2. Two", cx);
    assert_eq!(
        state(&setup, cx, |page| (
            page.session.section,
            page.session.open.clone()
        )),
        (Section::Chapters, Some("ch02.md".to_owned()))
    );
}

#[gpui_kit::test]
fn what_went_wrong_with_the_contents_is_told_in_their_section(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-contents", cx);
    act(&setup, cx, |page, _, cx| {
        page.session
            .set_error(Control::Contents, "The contents were refused.".to_owned());
        cx.notify();
    });
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("contents-error").label(),
            Some("The contents were refused.")
        );
    });
}

#[gpui_kit::test]
fn a_book_without_headings_says_how_the_contents_are_made(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[("ch01.md", "No heading.\n")]);
    cx.run_until_parked();
    click(&setup, "section-contents", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("no-contents").label(),
            Some(no_headings().as_ref())
        );
    });
}

#[gpui_kit::test]
fn stored_contents_are_shown_and_can_be_made_to_follow_the_headings(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = BookKey::generate();
    let mut book = Book::new("Dune", "en").unwrap();
    book.add_chapter("ch01.md", "# One\n").unwrap();
    book.set_toc(mbook::Toc::Explicit(vec![mbook::TocItem::new(
        1, "Opening", "ch01.md",
    )]));
    book.save(fixture.roots.book(key)).unwrap();
    block_on(record_save(&fixture.roots, &fixture.catalogue, key)).unwrap();
    let setup = open(cx, fixture, key, 1280.0);
    cx.run_until_parked();
    click(&setup, "section-contents", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("contents-hint").label(),
            Some(stored_hint().as_ref())
        );
        assert_eq!(
            window.find_by_label("Opening, level 1, in 1. One").role(),
            Some(Role::Button)
        );
        assert_eq!(
            window.find_by_label(&follow_headings()).role(),
            Some(Role::Button)
        );
    });
    click(&setup, "follow-headings", cx);
    assert!(dialog_is_open(&setup, cx));
    click_named(&setup, "Replace", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("contents-hint").label(),
            Some(automatic_hint().as_ref())
        );
        assert!(
            window
                .try_find_by_label("One, level 1, in 1. One")
                .is_some()
        );
        assert!(window.try_find("follow-headings").is_none());
    });
    assert!(state(&setup, cx, |page| page.session.has_unsaved()));
}

#[gpui_kit::test]
fn a_contents_entry_whose_heading_is_gone_says_so(cx: &mut TestAppContext) {
    let setup = editor_with_three_entries(cx);
    set_chapter_text(&setup, "ch01.md", "# One\n", cx);
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .try_find_by_label("Inside, level 2, leads nowhere")
                .is_some()
        );
    });
}

#[gpui_kit::test]
fn removing_a_chapter_removes_its_entries_of_the_contents(cx: &mut TestAppContext) {
    let setup = editor_with_three_entries(cx);
    assert_eq!(
        state(&setup, cx, |page| (
            page.session.stored_entries_of("ch01.md"),
            page.session.stored_entries_of("ch02.md"),
            page.session.stored_entries_of("gone.md"),
        )),
        (2, 1, 0)
    );
    act(&setup, cx, |page, window, cx| {
        page.remove_chapter("ch01.md".to_owned(), window, cx);
    });
    assert_eq!(shown_entries(&setup, cx), ["1 Second -> ch02.md"]);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("contents-nowhere").is_none());
    });
    click(&setup, "save", cx);
    assert_eq!(saved_entries(&setup), ["1 Second"]);
    assert_eq!(
        setup.events.borrow().as_slice(),
        [EditorEvent::Saved(setup.key)]
    );
}

#[gpui_kit::test]
fn contents_that_follow_the_headings_have_no_entries_to_lose(cx: &mut TestAppContext) {
    let setup = editor_with_three_entries(cx);
    act(&setup, cx, |page, window, cx| {
        page.follow_headings(window, cx);
    });
    assert_eq!(
        state(&setup, cx, |page| page.session.stored_entries_of("ch01.md")),
        0
    );
}

/// An editor that shows the Contents section of a book which keeps these
/// entries as a list of its own.
fn editor_with_contents(
    cx: &mut TestAppContext,
    chapters: &[(&str, &str)],
    items: Vec<mbook::TocItem>,
) -> Setup {
    let fixture = Fixture::new();
    let key = BookKey::generate();
    let mut book = Book::new("Dune", "en").unwrap();
    for (path, text) in chapters {
        book.add_chapter(*path, *text).unwrap();
    }
    book.set_toc(mbook::Toc::Explicit(items));
    book.save(fixture.roots.book(key)).unwrap();
    block_on(record_save(&fixture.roots, &fixture.catalogue, key)).unwrap();
    let setup = open(cx, fixture, key, 1280.0);
    cx.run_until_parked();
    click(&setup, "section-contents", cx);
    setup
}

/// Two chapters, the first with a heading inside, and an entry for each
/// chapter.
fn editor_with_two_entries(cx: &mut TestAppContext) -> Setup {
    editor_with_contents(
        cx,
        &[("ch01.md", "# One\n\n## Inside\n"), ("ch02.md", "# Two\n")],
        vec![
            mbook::TocItem::new(1, "Opening", "ch01.md"),
            mbook::TocItem::new(1, "Second", "ch02.md"),
        ],
    )
}

/// A chapter with a heading inside, another chapter, and an entry for each
/// of the three places.
fn editor_with_three_entries(cx: &mut TestAppContext) -> Setup {
    editor_with_contents(
        cx,
        &[("ch01.md", "# One\n\n## Inside\n"), ("ch02.md", "# Two\n")],
        vec![
            mbook::TocItem::new(1, "Opening", "ch01.md"),
            mbook::TocItem::new(2, "Inside", "ch01.md#inside"),
            mbook::TocItem::new(1, "Second", "ch02.md"),
        ],
    )
}

/// Gives a chapter another text, as a pause in typing does.
fn set_chapter_text(setup: &Setup, path: &str, text: &str, cx: &mut TestAppContext) {
    let (path, text) = (path.to_owned(), text.to_owned());
    act(setup, cx, move |page, window, cx| {
        page.send(
            Command::SetChapterText {
                path: path.clone(),
                text: text.clone(),
            },
            Origin::Text { path, text },
            window,
            cx,
        );
    });
}

/// The entries the section shows, as `level title -> destination`.
fn shown_entries(setup: &Setup, cx: &TestAppContext) -> Vec<String> {
    state(setup, cx, |page| {
        let contents = page.session.contents().unwrap();
        contents
            .entries
            .iter()
            .map(|entry| {
                let anchor = entry
                    .anchor
                    .as_ref()
                    .map_or_else(String::new, |anchor| format!("#{anchor}"));
                format!(
                    "{} {} -> {}{anchor}",
                    entry.level, entry.title, entry.chapter
                )
            })
            .collect()
    })
}

/// The contents of the saved file, as `level title`.
fn saved_entries(setup: &Setup) -> Vec<String> {
    let reader = BookReader::open(setup.fixture.roots.book(setup.key)).unwrap();
    reader
        .toc()
        .iter()
        .map(|entry| format!("{} {}", entry.level, entry.title))
        .collect()
}

#[gpui_kit::test]
fn an_entry_of_a_stored_list_gets_another_title_and_place_from_its_menu(cx: &mut TestAppContext) {
    let setup = editor_with_two_entries(cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("contents-hint").label(),
            Some(stored_hint().as_ref())
        );
        assert_eq!(
            window.find_by_label("Entry actions, Second").role(),
            Some(Role::Button)
        );
        assert!(window.try_find("contents-nowhere").is_none());
        assert!(window.try_find("contents-order").is_none());
    });
    click_named(&setup, "Entry actions, Second", cx);
    choose(&setup, "Edit…", cx);
    assert!(dialog_is_open(&setup, cx));
    click_named(&setup, &title_label(), cx);
    press(&setup, "end", cx);
    frame(&setup, cx, |window, cx| window.input(" part", cx));
    click_named(&setup, "Chapter, 2. Two", cx);
    choose(&setup, "1. One", cx);
    click_named(&setup, "Place in the chapter, Start of the chapter", cx);
    choose(&setup, "\u{2003}Inside", cx);
    click_named(&setup, "Apply", cx);
    assert!(!dialog_is_open(&setup, cx));
    assert_eq!(
        shown_entries(&setup, cx),
        ["1 Opening -> ch01.md", "1 Second part -> ch01.md#inside"]
    );
    assert!(!state(&setup, cx, |page| page.session.contents_pending));
    click(&setup, "save", cx);
    assert_eq!(saved_entries(&setup), ["1 Opening", "1 Second part"]);
}

#[gpui_kit::test]
fn a_new_entry_takes_the_title_of_the_heading_it_leads_to_and_its_place_in_the_list(
    cx: &mut TestAppContext,
) {
    let setup = editor_with_two_entries(cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("add-entry").label(), Some(add_entry().as_ref()));
    });
    click(&setup, "add-entry", cx);
    assert!(dialog_is_open(&setup, cx));
    // The dialog starts at the chapter being edited and offers its headings.
    click_named(&setup, "Place in the chapter, Start of the chapter", cx);
    choose(&setup, "\u{2003}Inside", cx);
    click_named(&setup, "Add the entry", cx);
    assert_eq!(
        shown_entries(&setup, cx),
        [
            "1 Opening -> ch01.md",
            "1 Inside -> ch01.md#inside",
            "1 Second -> ch02.md"
        ]
    );
    assert!(state(&setup, cx, |page| page.session.has_unsaved()));
}

#[gpui_kit::test]
fn a_title_the_author_typed_is_kept_when_another_place_is_chosen(cx: &mut TestAppContext) {
    let setup = editor_with_two_entries(cx);
    click(&setup, "add-entry", cx);
    click_named(&setup, &title_label(), cx);
    press(&setup, "end", cx);
    frame(&setup, cx, |window, cx| window.input(" more", cx));
    click_named(&setup, "Chapter, 1. One", cx);
    choose(&setup, "2. Two", cx);
    click_named(&setup, "Add the entry", cx);
    assert_eq!(
        shown_entries(&setup, cx),
        [
            "1 Opening -> ch01.md",
            "1 Second -> ch02.md",
            "1 One more -> ch02.md"
        ]
    );
}

#[gpui_kit::test]
fn a_place_without_a_heading_takes_the_suggested_title_away(cx: &mut TestAppContext) {
    let setup = editor_with_contents(
        cx,
        &[("ch01.md", "# One\n"), ("ch02.md", "No heading.\n")],
        vec![mbook::TocItem::new(1, "Opening", "ch01.md")],
    );
    click(&setup, "add-entry", cx);
    click_named(&setup, "Chapter, 1. One", cx);
    choose(&setup, "2. Untitled chapter", cx);
    click_named(&setup, "Add the entry", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("entry-error").label(),
            Some(no_title().as_ref())
        );
    });
    click_named(&setup, &title_label(), cx);
    frame(&setup, cx, |window, cx| window.input("Loose pages", cx));
    click_named(&setup, "Add the entry", cx);
    assert_eq!(
        shown_entries(&setup, cx),
        ["1 Opening -> ch01.md", "1 Loose pages -> ch02.md"]
    );
}

#[gpui_kit::test]
fn an_entry_without_a_title_is_refused_in_the_dialog(cx: &mut TestAppContext) {
    let setup = editor_with_two_entries(cx);
    click_named(&setup, "Entry actions, Opening", cx);
    choose(&setup, "Edit…", cx);
    act(&setup, cx, |_, window, cx| {
        window.dispatch_action(Box::new(gpui_kit::component::input::SelectAll), cx);
    });
    press(&setup, "backspace", cx);
    click_named(&setup, "Apply", cx);
    assert!(dialog_is_open(&setup, cx));
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("entry-error").label(),
            Some(no_title().as_ref())
        );
    });
    assert_eq!(
        shown_entries(&setup, cx),
        ["1 Opening -> ch01.md", "1 Second -> ch02.md"]
    );
}

#[gpui_kit::test]
fn an_entry_moves_in_and_out_from_its_menu(cx: &mut TestAppContext) {
    let setup = editor_with_two_entries(cx);
    click_named(&setup, "Entry actions, Second", cx);
    choose(&setup, &nest(), cx);
    assert_eq!(
        shown_entries(&setup, cx),
        ["1 Opening -> ch01.md", "2 Second -> ch02.md"]
    );
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .try_find_by_label("Second, level 2, in 2. Two")
                .is_some()
        );
    });
    click_named(&setup, "Entry actions, Second", cx);
    choose(&setup, &unnest(), cx);
    assert_eq!(
        shown_entries(&setup, cx),
        ["1 Opening -> ch01.md", "1 Second -> ch02.md"]
    );
}

#[gpui_kit::test]
fn the_first_entry_cannot_move_in_or_out(cx: &mut TestAppContext) {
    for item in [nest(), unnest()] {
        let setup = editor_with_two_entries(cx);
        click_named(&setup, "Entry actions, Opening", cx);
        choose(&setup, &item, cx);
        assert_eq!(
            shown_entries(&setup, cx),
            ["1 Opening -> ch01.md", "1 Second -> ch02.md"],
            "{item}"
        );
        assert!(!state(&setup, cx, |page| page.session.has_unsaved()));
    }
}

#[gpui_kit::test]
fn an_entry_is_removed_after_a_question(cx: &mut TestAppContext) {
    let setup = editor_with_two_entries(cx);
    click_named(&setup, "Entry actions, Opening", cx);
    choose(&setup, "Remove…", cx);
    assert!(dialog_is_open(&setup, cx));
    assert_eq!(
        shown_entries(&setup, cx),
        ["1 Opening -> ch01.md", "1 Second -> ch02.md"]
    );
    click_named(&setup, "Remove", cx);
    assert_eq!(shown_entries(&setup, cx), ["1 Second -> ch02.md"]);
    assert!(!state(&setup, cx, |page| {
        page.session.contents().unwrap().automatic
    }));
}

#[gpui_kit::test]
fn removing_the_last_entry_lets_the_contents_follow_the_headings(cx: &mut TestAppContext) {
    let setup = editor_with_contents(
        cx,
        &[("ch01.md", "# One\n")],
        vec![mbook::TocItem::new(1, "Opening", "ch01.md")],
    );
    click_named(&setup, "Entry actions, Opening", cx);
    choose(&setup, "Remove…", cx);
    click_named(&setup, "Remove", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("contents-hint").label(),
            Some(automatic_hint().as_ref())
        );
        assert!(window.try_find("add-entry").is_none());
        assert!(window.try_find_by_label("Entry actions, One").is_none());
    });
    assert_eq!(shown_entries(&setup, cx), ["1 One -> ch01.md#one"]);
}

#[gpui_kit::test]
fn entries_whose_place_is_gone_are_counted_and_given_another_place(cx: &mut TestAppContext) {
    let setup = editor_with_three_entries(cx);
    set_chapter_text(&setup, "ch01.md", "# One\n", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("contents-nowhere").label(),
            Some(nowhere_words(1).as_str())
        );
    });
    click_named(&setup, "Entry actions, Inside", cx);
    choose(&setup, "Edit…", cx);
    // The place it had is told as missing, and is not taken.
    click_named(&setup, "Apply", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("entry-error").label(),
            Some(no_place().as_ref())
        );
    });
    click_named(&setup, "Place in the chapter, #inside (missing)", cx);
    choose(&setup, "Start of the chapter", cx);
    click_named(&setup, "Apply", cx);
    assert_eq!(
        shown_entries(&setup, cx),
        [
            "1 Opening -> ch01.md",
            "2 Inside -> ch01.md",
            "1 Second -> ch02.md"
        ]
    );
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("contents-nowhere").is_none());
    });
    click(&setup, "save", cx);
    assert_eq!(saved_entries(&setup), ["1 Opening", "2 Inside", "1 Second"]);
}

#[gpui_kit::test]
fn the_entries_of_a_moved_chapter_move_with_it(cx: &mut TestAppContext) {
    let setup = editor_with_two_entries(cx);
    act(&setup, cx, |page, window, cx| {
        page.move_chapter("ch02.md", -1, window, cx);
    });
    assert_eq!(
        shown_entries(&setup, cx),
        ["1 Second -> ch02.md", "1 Opening -> ch01.md"]
    );
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("contents-order").is_none());
        assert!(
            window
                .try_find_by_label("Second, level 1, in 1. Two")
                .is_some()
        );
    });
    click(&setup, "save", cx);
    assert_eq!(saved_entries(&setup), ["1 Second", "1 Opening"]);
}

#[gpui_kit::test]
fn the_entries_of_a_chapter_that_got_another_path_still_lead_to_it(cx: &mut TestAppContext) {
    let setup = editor_with_two_entries(cx);
    act(&setup, cx, |page, window, cx| {
        let (from, to) = ("ch02.md".to_owned(), "second.md".to_owned());
        page.send(
            Command::MoveChapterPath {
                from: from.clone(),
                to: to.clone(),
            },
            Origin::Chapter(ChapterOp::Rename { from, to }),
            window,
            cx,
        );
    });
    assert_eq!(
        shown_entries(&setup, cx),
        ["1 Opening -> ch01.md", "1 Second -> second.md"]
    );
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("contents-nowhere").is_none());
    });
    click(&setup, "save", cx);
    assert_eq!(saved_entries(&setup), ["1 Opening", "1 Second"]);
}

#[gpui_kit::test]
fn entries_that_swapped_headings_left_out_of_order_are_sorted_by_one_control(
    cx: &mut TestAppContext,
) {
    let setup = editor_with_contents(
        cx,
        &[("ch01.md", "# One\n\n## Early\n\n## Late\n")],
        vec![
            mbook::TocItem::new(1, "Opening", "ch01.md"),
            mbook::TocItem::new(2, "Early", "ch01.md#early"),
            mbook::TocItem::new(2, "Late", "ch01.md#late"),
        ],
    );
    act(&setup, cx, |page, window, cx| {
        let (path, text) = (
            "ch01.md".to_owned(),
            "# One\n\n## Late\n\n## Early\n".to_owned(),
        );
        page.send(
            Command::SetChapterText {
                path: path.clone(),
                text: text.clone(),
            },
            Origin::Text { path, text },
            window,
            cx,
        );
    });
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("contents-order").label(),
            Some(order_words(1).as_str())
        );
        assert_eq!(
            window.find("contents-in-order").label(),
            Some(in_book_order().as_ref())
        );
        assert!(
            window
                .try_find_by_label("Late, level 2, in 1. One, not in the order of the book")
                .is_some()
        );
    });
    click(&setup, "contents-in-order", cx);
    assert_eq!(
        shown_entries(&setup, cx),
        [
            "1 Opening -> ch01.md",
            "2 Late -> ch01.md#late",
            "2 Early -> ch01.md#early"
        ]
    );
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("contents-order").is_none());
        assert!(window.try_find("contents-in-order").is_none());
    });
    click(&setup, "save", cx);
    assert_eq!(saved_entries(&setup), ["1 Opening", "2 Late", "2 Early"]);
}

#[gpui_kit::test]
fn a_change_of_the_list_waits_for_the_one_before_it(cx: &mut TestAppContext) {
    let setup = editor_with_two_entries(cx);
    let page = setup.page.clone();
    cx.update_window(setup.window, |_, window, cx| {
        page.update(cx, |page, cx| {
            page.edit_contents(ContentsChange::Remove { index: 0 }, window, cx);
            assert!(page.session.contents_pending);
            assert!(page.session.has_unsaved());
            // The second names a position of a list that is about to change.
            page.edit_contents(ContentsChange::Remove { index: 0 }, window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(shown_entries(&setup, cx), ["1 Second -> ch02.md"]);
    assert!(!state(&setup, cx, |page| page.session.contents_pending));
}

#[gpui_kit::test]
fn a_refused_change_of_the_list_is_told_in_the_section(cx: &mut TestAppContext) {
    let setup = editor_with_two_entries(cx);
    act(&setup, cx, |page, window, cx| {
        page.edit_contents(ContentsChange::Remove { index: 7 }, window, cx);
    });
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("contents-error").label(),
            Some("The contents were not changed: the list has no entry at position 7.")
        );
    });
    assert!(!state(&setup, cx, |page| page.session.contents_pending));
}

#[gpui_kit::test]
fn the_dialog_of_an_entry_that_is_gone_does_not_open(cx: &mut TestAppContext) {
    let setup = editor_with_two_entries(cx);
    act(&setup, cx, |page, window, cx| {
        page.ask_contents_entry(Some(9), window, cx);
    });
    assert!(!dialog_is_open(&setup, cx));
}

#[gpui_kit::test]
fn removing_the_only_chapter_takes_its_entries_and_the_list_away(cx: &mut TestAppContext) {
    let setup = editor_with_contents(
        cx,
        &[("ch01.md", "# One\n")],
        vec![mbook::TocItem::new(1, "Opening", "ch01.md")],
    );
    act(&setup, cx, |page, window, cx| {
        page.remove_chapter("ch01.md".to_owned(), window, cx);
    });
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("contents-hint").label(),
            Some(automatic_hint().as_ref())
        );
        assert!(window.try_find("add-entry").is_none());
    });
    // Without a chapter there is nowhere for an entry to lead.
    act(&setup, cx, |page, window, cx| {
        page.ask_contents_entry(None, window, cx);
    });
    assert!(!dialog_is_open(&setup, cx));
}

#[gpui_kit::test]
fn a_problem_of_the_contents_leads_to_their_section(cx: &mut TestAppContext) {
    let setup = editor_with_three_entries(cx);
    set_chapter_text(&setup, "ch01.md", "# One\n", cx);
    click(&setup, "save", cx);
    cx.run_until_parked();
    assert_eq!(
        state(&setup, cx, |page| page.session.section),
        Section::Problems
    );
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("open-contents").label(),
            Some(open_contents().as_ref())
        );
    });
    click(&setup, "open-contents", cx);
    assert_eq!(
        state(&setup, cx, |page| page.session.section),
        Section::Contents
    );
}

#[gpui_kit::test]
fn a_diagram_can_be_previewed_edited_and_removed_in_its_section(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-diagrams", cx);
    click(&setup, "new-figure", cx);
    click_named(&setup, "Add", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("figure-draft-error").label(),
            Some("Enter the source.")
        );
    });
    click_named(&setup, "Mermaid source", cx);
    frame(&setup, cx, |window, cx| {
        window.input("flowchart LR\n  A --> B", cx);
    });
    click(&setup, "draw-figure", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("figure-source-preview").role(),
            Some(Role::Image)
        );
        // A small drawing fits as it is.
        assert!(window.try_find("figure-zoom").is_none());
    });
    click_named(&setup, "Add", cx);
    click(&setup, "edit-figure-dia/diagram.mmd", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find_by_label("Mermaid source").role(),
            Some(Role::MultilineTextInput)
        );
    });
    act(&setup, cx, |_, window, cx| {
        window.dispatch_action(Box::new(gpui_kit::component::input::SelectAll), cx);
    });
    frame(&setup, cx, |window, cx| {
        window.input("flowchart LR\n  C --> D", cx);
    });
    click_named(&setup, "Apply", cx);
    click(&setup, "save", cx);
    let path = setup.fixture.roots.book(setup.key);
    assert_eq!(
        Book::open(&path)
            .unwrap()
            .diagram("dia/diagram.mmd")
            .unwrap()
            .source(),
        "flowchart LR\n  C --> D"
    );
    click(&setup, "remove-figure-dia/diagram.mmd", cx);
    click_named(&setup, "Remove", cx);
    click(&setup, "save", cx);
    assert_eq!(Book::open(path).unwrap().diagrams(), []);
}

#[gpui_kit::test]
fn an_edited_diagram_is_drawn_as_its_dialog_opens(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-diagrams", cx);
    click(&setup, "new-figure", cx);
    frame(&setup, cx, |window, cx| {
        window.input("flowchart LR\n  A --> B", cx);
    });
    click_named(&setup, "Add", cx);
    click(&setup, "edit-figure-dia/diagram.mmd", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("figure-source-preview").role(),
            Some(Role::Image)
        );
    });
}

#[gpui_kit::test]
fn a_large_diagram_is_fitted_and_can_be_seen_at_its_own_size(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-diagrams", cx);
    click(&setup, "new-figure", cx);
    let chain = (0..30)
        .map(|step| format!("  N{step} --> N{}", step + 1))
        .collect::<Vec<_>>()
        .join("\n");
    frame(&setup, cx, |window, cx| {
        window.input(&format!("flowchart TD\n{chain}"), cx);
    });
    click(&setup, "draw-figure", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("figure-zoom").label(), Some("Actual size"));
    });
    click(&setup, "figure-zoom", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("figure-zoom").label(), Some("Fit"));
        assert_eq!(
            window.find("figure-source-preview").role(),
            Some(Role::Image)
        );
    });
}

#[gpui_kit::test]
fn a_narrow_window_shows_the_drawing_under_the_source(cx: &mut TestAppContext) {
    let setup = started(cx, 800.0);
    act(&setup, cx, |page, _, cx| {
        page.session.section = Section::Figures(crate::entities::figure::FigureKind::Diagram);
        cx.notify();
    });
    click(&setup, "new-figure", cx);
    frame(&setup, cx, |window, _| {
        let controls = window.find("draw-figure").bounds();
        let drawing = window.find("figure-preview-box").bounds();
        assert!(
            drawing.top() > controls.bottom(),
            "{controls:?} {drawing:?}"
        );
    });
}

#[gpui_kit::test]
fn a_source_the_renderer_refuses_shows_its_error_in_the_preview(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-diagrams", cx);
    click(&setup, "new-figure", cx);
    frame(&setup, cx, |window, cx| {
        window.input("this is not a diagram", cx);
    });
    click(&setup, "draw-figure", cx);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("figure-preview-error").is_some());
        assert!(window.try_find("figure-source-preview").is_none());
    });
}

#[gpui_kit::test]
fn a_formula_inserted_in_visual_is_drawn_in_the_draft_preview(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[("part/ch01.md", "")]);
    cx.run_until_parked();
    click(&setup, "section-formulas", cx);
    click(&setup, "new-figure", cx);
    frame(&setup, cx, |window, cx| window.input("x^2", cx));
    click_named(&setup, "Add", cx);
    click(&setup, "chapter-part/ch01.md", cx);
    click(&setup, "visual-formula", cx);
    frame(&setup, cx, |window, cx| window.input("x squared", cx));
    click_named(&setup, "Insert formula", cx);
    assert_eq!(editor_text(&setup, cx), "![x squared](../math/formula.tex)");
    click(&setup, "mode-preview", cx);
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    frame(&setup, cx, |_, _| ());
    cx.run_until_parked();
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .find_all_by_role(Role::Image)
                .iter()
                .any(|node| node.label() == Some("x squared"))
        );
    });
    click(&setup, "save", cx);
    let book = Book::open(setup.fixture.roots.book(setup.key)).unwrap();
    assert_eq!(book.formula("math/formula.tex").unwrap().source(), "x^2");
}

#[gpui_kit::test]
fn inserting_figures_with_empty_descriptions_uses_the_resource_kind(cx: &mut TestAppContext) {
    use crate::entities::figure::FigureKind;

    for (kind, source, button, label, description, expected) in [
        (
            FigureKind::Formula,
            "a^2",
            "visual-formula",
            "Insert formula",
            "",
            "![Formula](../fig/source)",
        ),
        (
            FigureKind::Diagram,
            "flowchart LR\n  A --> B",
            "visual-diagram",
            "Insert diagram",
            "  ",
            "![Diagram](../fig/source)",
        ),
    ] {
        let setup = start_with(cx, 1280.0, &[("part/ch01.md", "")]);
        cx.run_until_parked();
        act(&setup, cx, |page, window, cx| {
            page.send(
                Command::AddFigure {
                    kind,
                    path: "fig/source".to_owned(),
                    source: source.to_owned(),
                },
                Origin::Figure(kind),
                window,
                cx,
            );
        });
        click(&setup, button, cx);
        frame(&setup, cx, |window, cx| window.input(description, cx));
        click_named(&setup, label, cx);
        assert_eq!(editor_text(&setup, cx), expected);
    }
}

fn enter_chapter_query(setup: &Setup, query: &str, cx: &mut TestAppContext) {
    frame(setup, cx, |window, cx| window.input(query, cx));
    cx.run_until_parked();
    finish_chapter_search(cx);
}

fn finish_chapter_search(cx: &mut TestAppContext) {
    cx.executor()
        .advance_clock(crate::shared::ui::search_bar::TYPING_PAUSE);
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_chapter_find_shortcut_searches_unsaved_visual_text(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    select_visual(&setup, "First text.", 0..0, cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("Café ", cx));
    cx.run_until_parked();
    let mac = cx.update(|cx| cx.platform_info().os == gpui_kit::OperatingSystem::Mac);
    press(&setup, if mac { "cmd-f" } else { "ctrl-f" }, cx);
    assert!(state(&setup, cx, |page| page.search.open));
    enter_chapter_query(&setup, "CAFÉ", cx);
    setup.page.read_with(cx, |page, cx| {
        let visual = page.visual.input.read(cx);
        assert!(
            matches!(visual.selection(), BlockSelection::Text { anchor, head }
            if anchor.offset == 0 && head.offset == "Café".len())
        );
        assert!(page.search.current.is_some());
    });
    press(&setup, "escape", cx);
    assert!(!state(&setup, cx, |page| page.search.open));
    assert!(setup.events.borrow().is_empty());
}

#[gpui_kit::test]
fn source_search_selects_the_markdown_match(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-source", cx);
    focus_editor(&setup, cx);
    let mac = cx.update(|cx| cx.platform_info().os == gpui_kit::OperatingSystem::Mac);
    press(&setup, if mac { "cmd-f" } else { "ctrl-f" }, cx);
    assert!(state(&setup, cx, |page| page.search.open));
    enter_chapter_query(&setup, "# One", cx);
    setup.page.read_with(cx, |page, cx| {
        assert_eq!(page.text.read(cx).selected_range(), 0..5);
    });
}

#[gpui_kit::test]
fn chapter_search_does_not_find_text_from_another_chapter(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "find-chapter", cx);
    enter_chapter_query(&setup, "Second text", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("find-status").label(), Some("No matches"));
    });
}

#[gpui_kit::test]
fn chapter_find_tracks_the_current_view_and_closes_on_chapter_change(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "find-chapter", cx);
    enter_chapter_query(&setup, "First", cx);
    click(&setup, "mode-preview", cx);
    finish_chapter_search(cx);
    assert!(state(&setup, cx, |page| page.search.current.is_some()));
    click(&setup, "find-next", cx);
    assert!(state(&setup, cx, |page| page.search.current.is_some()));
    act(&setup, cx, |page, window, cx| {
        page.open_chapter("ch02.md", window, cx);
    });
    assert!(!state(&setup, cx, |page| page.search.open));
}
