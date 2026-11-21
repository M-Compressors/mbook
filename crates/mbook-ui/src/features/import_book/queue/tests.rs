use std::error::Error as StdError;
use std::fmt;
use std::path::{Path, PathBuf};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};

use super::*;
use crate::testing::{self, Blank, Fixture};

/// An mbook file called `title` in `dir`.
fn write_book(dir: &Path, title: &str) -> PathBuf {
    let mut book = mbook::Book::new(title, "en").unwrap();
    book.add_chapter("ch01.md", "# One\n\nText.\n").unwrap();
    let path = dir.join(format!("{title}.mbook"));
    book.save(&path).unwrap();
    path
}

struct Setup {
    /// Keeps the folders of the test until it ends.
    _fixture: Fixture,
    window: AnyWindowHandle,
    queue: Entity<ImportQueue>,
    library: Entity<Library>,
}

fn setup(cx: &mut TestAppContext) -> Setup {
    let fixture = Fixture::new();
    cx.update(gpui_kit::init);
    fixture.install(cx);
    let library = cx.update(|cx| {
        let library = cx.new(Library::new);
        Library::install(library.clone(), cx);
        library
    });
    let (window, _) = testing::open(cx, 600.0, 400.0, |_, _| Blank);
    let queue = cx.update(|cx| cx.new(|_| ImportQueue::new(window)));
    cx.run_until_parked();
    Setup {
        _fixture: fixture,
        window,
        queue,
        library,
    }
}

fn statuses(setup: &Setup, cx: &TestAppContext) -> Vec<ImportStatus> {
    setup.queue.read_with(cx, |queue, _| {
        queue
            .entries()
            .iter()
            .map(|entry| entry.status().clone())
            .collect()
    })
}

fn add(setup: &Setup, cx: &mut TestAppContext, paths: Vec<PathBuf>) {
    setup.queue.update(cx, |queue, cx| queue.add(paths, cx));
}

fn events(
    setup: &Setup,
    cx: &mut TestAppContext,
) -> std::rc::Rc<std::cell::RefCell<Vec<ImportEvent>>> {
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = seen.clone();
    cx.update(|cx| {
        cx.subscribe(&setup.queue, move |_, event: &ImportEvent, _| {
            sink.borrow_mut().push(*event);
        })
        .detach();
    });
    seen
}

#[derive(Debug)]
struct Outer(Inner);

#[derive(Debug)]
struct Inner;

impl fmt::Display for Outer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the file could not be converted")
    }
}

impl fmt::Display for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the archive is damaged")
    }
}

impl StdError for Inner {}

impl StdError for Outer {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.0)
    }
}

/// An error that says its cause in its own words.
#[derive(Debug)]
struct Saying;

impl fmt::Display for Saying {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("could not read: the archive is damaged")
    }
}

impl StdError for Saying {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&Inner)
    }
}

#[test]
fn an_error_chain_joins_each_message_with_a_colon() {
    assert_eq!(
        error_chain(&Outer(Inner)),
        "the file could not be converted: the archive is damaged"
    );
}

#[test]
fn an_error_chain_does_not_say_a_cause_twice() {
    assert_eq!(
        error_chain(&Saying),
        "could not read: the archive is damaged"
    );
}

#[test]
fn a_sentence_starts_with_a_capital_and_ends_with_a_full_stop() {
    assert_eq!(sentence("the file is empty"), "The file is empty.");
    assert_eq!(sentence("Already done."), "Already done.");
    assert_eq!(sentence("really?"), "Really?");
    assert_eq!(sentence(""), ".");
}

#[test]
fn a_status_says_in_words_where_the_import_is() {
    assert_eq!(ImportStatus::Queued.label(), "Waiting");
    assert_eq!(ImportStatus::Converting.label(), "Converting");
    let key = BookKey::generate();
    assert_eq!(
        ImportStatus::Done {
            key,
            warnings: Vec::new()
        }
        .label(),
        "Added"
    );
    assert_eq!(
        ImportStatus::Done {
            key,
            warnings: vec!["a".to_owned()]
        }
        .label(),
        "Added, with 1 warning"
    );
    assert_eq!(
        ImportStatus::Done {
            key,
            warnings: vec!["a".to_owned(), "b".to_owned()]
        }
        .label(),
        "Added, with 2 warnings"
    );
    assert_eq!(
        ImportStatus::Already { key }.label(),
        "Already in the library"
    );
    assert_eq!(
        ImportStatus::Failed {
            message: "No.".to_owned()
        }
        .label(),
        "No."
    );
}

#[test]
fn only_a_started_import_that_is_not_over_is_running() {
    assert!(!ImportStatus::Queued.is_running());
    assert!(ImportStatus::Probing.is_running());
    assert!(ImportStatus::Indexing.is_running());
    let key = BookKey::generate();
    assert!(!ImportStatus::Already { key }.is_running());
    assert!(ImportStatus::Already { key }.is_finished());
    assert!(!ImportStatus::Copying.is_finished());
    assert_eq!(ImportStatus::Already { key }.book(), Some(key));
    assert_eq!(ImportStatus::Queued.book(), None);
}

#[gpui_kit::test]
fn a_file_waits_in_the_queue_until_the_background_has_run(cx: &mut TestAppContext) {
    let setup = setup(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = write_book(dir.path(), "The Example");
    add(&setup, cx, vec![path]);
    assert_eq!(statuses(&setup, cx), [ImportStatus::Queued]);
    assert!(!setup.queue.read_with(cx, |queue, _| queue.is_idle()));
}

#[gpui_kit::test]
fn an_mbook_file_becomes_a_book_of_the_library(cx: &mut TestAppContext) {
    let setup = setup(cx);
    let dir = tempfile::tempdir().unwrap();
    add(&setup, cx, vec![write_book(dir.path(), "The Example")]);
    cx.run_until_parked();
    let statuses = statuses(&setup, cx);
    assert!(matches!(
        statuses.as_slice(),
        [ImportStatus::Done { warnings, .. }] if warnings.is_empty()
    ));
    let titles: Vec<String> = setup.library.read_with(cx, |library, _| {
        library
            .books()
            .iter()
            .map(|book| book.row.title.clone())
            .collect()
    });
    assert_eq!(titles, ["The Example"]);
    assert!(setup.queue.read_with(cx, |queue, _| queue.is_idle()));
    let title = setup
        .queue
        .read_with(cx, |queue, _| queue.entries()[0].display_name().to_owned());
    assert_eq!(title, "The Example");
}

#[gpui_kit::test]
fn a_finished_import_is_announced(cx: &mut TestAppContext) {
    let setup = setup(cx);
    let dir = tempfile::tempdir().unwrap();
    add(&setup, cx, vec![write_book(dir.path(), "The Example")]);
    cx.run_until_parked();
    testing::frame(cx, setup.window, |window, _| {
        assert!(window.find("notification").visible());
    });
}

#[gpui_kit::test]
fn the_same_book_added_twice_is_already_in_the_library(cx: &mut TestAppContext) {
    let setup = setup(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = write_book(dir.path(), "The Example");
    add(&setup, cx, vec![path.clone(), path]);
    cx.run_until_parked();
    let statuses = statuses(&setup, cx);
    let (ImportStatus::Done { key, .. }, ImportStatus::Already { key: again }) =
        (&statuses[0], &statuses[1])
    else {
        panic!("unexpected statuses: {statuses:?}");
    };
    assert_eq!(key, again);
    assert_eq!(setup.library.read_with(cx, |l, _| l.books().len()), 1);
}

#[gpui_kit::test]
fn a_file_that_is_not_there_fails_with_a_sentence(cx: &mut TestAppContext) {
    let setup = setup(cx);
    add(&setup, cx, vec![PathBuf::from("/nowhere/notes.txt")]);
    cx.run_until_parked();
    let statuses = statuses(&setup, cx);
    let [ImportStatus::Failed { message }] = statuses.as_slice() else {
        panic!("unexpected statuses: {statuses:?}");
    };
    assert!(
        message.starts_with("A file could not be read or written: "),
        "{message}"
    );
    assert!(message.ends_with('.'), "{message}");
    assert_eq!(setup.library.read_with(cx, |l, _| l.books().len()), 0);
    assert!(setup.queue.read_with(cx, |queue, _| queue.is_idle()));
}

#[gpui_kit::test]
fn a_file_in_no_known_format_fails_and_the_next_one_still_runs(cx: &mut TestAppContext) {
    let setup = setup(cx);
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.bin");
    std::fs::write(&junk, [0_u8, 159, 146, 150, 1, 2, 3]).unwrap();
    add(&setup, cx, vec![junk, write_book(dir.path(), "Second")]);
    cx.run_until_parked();
    let statuses = statuses(&setup, cx);
    assert!(
        matches!(statuses[0], ImportStatus::Failed { .. }),
        "{statuses:?}"
    );
    assert!(
        matches!(statuses[1], ImportStatus::Done { .. }),
        "{statuses:?}"
    );
}

#[gpui_kit::test]
fn a_failed_import_is_announced_with_the_name_of_the_file(cx: &mut TestAppContext) {
    let setup = setup(cx);
    add(&setup, cx, vec![PathBuf::from("/nowhere/notes.txt")]);
    cx.run_until_parked();
    testing::frame(cx, setup.window, |window, _| {
        assert!(window.find("notification").visible());
    });
    let name = setup
        .queue
        .read_with(cx, |queue, _| queue.entries()[0].name().to_owned());
    assert_eq!(name, "notes.txt");
}

#[gpui_kit::test]
fn one_file_that_was_added_opens_its_book(cx: &mut TestAppContext) {
    let setup = setup(cx);
    let seen = events(&setup, cx);
    let dir = tempfile::tempdir().unwrap();
    add(&setup, cx, vec![write_book(dir.path(), "The Example")]);
    cx.run_until_parked();
    let key = statuses(&setup, cx)[0].book().unwrap();
    assert_eq!(*seen.borrow(), [ImportEvent::Opened(key)]);
}

#[gpui_kit::test]
fn one_file_that_was_there_already_opens_the_existing_book(cx: &mut TestAppContext) {
    let setup = setup(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = write_book(dir.path(), "The Example");
    add(&setup, cx, vec![path.clone()]);
    cx.run_until_parked();
    let seen = events(&setup, cx);
    add(&setup, cx, vec![path]);
    cx.run_until_parked();
    let key = statuses(&setup, cx)[1].book().unwrap();
    assert!(matches!(
        statuses(&setup, cx)[1],
        ImportStatus::Already { .. }
    ));
    assert_eq!(*seen.borrow(), [ImportEvent::Opened(key)]);
}

#[gpui_kit::test]
fn several_files_are_only_added(cx: &mut TestAppContext) {
    let setup = setup(cx);
    let seen = events(&setup, cx);
    let dir = tempfile::tempdir().unwrap();
    add(
        &setup,
        cx,
        vec![write_book(dir.path(), "One"), write_book(dir.path(), "Two")],
    );
    cx.run_until_parked();
    assert_eq!(setup.library.read_with(cx, |l, _| l.books().len()), 2);
    assert_eq!(*seen.borrow(), Vec::new());
}

#[gpui_kit::test]
fn one_file_that_failed_opens_nothing(cx: &mut TestAppContext) {
    let setup = setup(cx);
    let seen = events(&setup, cx);
    add(&setup, cx, vec![PathBuf::from("/nowhere/notes.txt")]);
    cx.run_until_parked();
    assert_eq!(*seen.borrow(), Vec::new());
}

#[gpui_kit::test]
fn no_files_add_nothing(cx: &mut TestAppContext) {
    let setup = setup(cx);
    add(&setup, cx, Vec::new());
    cx.run_until_parked();
    assert_eq!(statuses(&setup, cx), Vec::new());
    assert!(setup.queue.read_with(cx, |queue, _| queue.is_idle()));
}

#[gpui_kit::test]
fn a_finished_entry_can_be_dismissed_and_a_waiting_one_cannot(cx: &mut TestAppContext) {
    let setup = setup(cx);
    let dir = tempfile::tempdir().unwrap();
    add(&setup, cx, vec![write_book(dir.path(), "One")]);
    setup.queue.update(cx, |queue, cx| queue.dismiss(0, cx));
    assert_eq!(statuses(&setup, cx).len(), 1, "a waiting entry stays");
    cx.run_until_parked();
    setup.queue.update(cx, |queue, cx| queue.dismiss(5, cx));
    assert_eq!(statuses(&setup, cx).len(), 1, "no such entry");
    setup.queue.update(cx, |queue, cx| queue.dismiss(0, cx));
    assert_eq!(statuses(&setup, cx), Vec::new());
}

#[gpui_kit::test]
fn finished_entries_are_cleared_together(cx: &mut TestAppContext) {
    let setup = setup(cx);
    let dir = tempfile::tempdir().unwrap();
    add(
        &setup,
        cx,
        vec![
            write_book(dir.path(), "One"),
            PathBuf::from("/nowhere/two.txt"),
        ],
    );
    cx.run_until_parked();
    assert_eq!(setup.queue.read_with(cx, |q, _| q.finished_count()), 2);
    add(&setup, cx, vec![write_book(dir.path(), "Three")]);
    setup.queue.update(cx, ImportQueue::clear_finished);
    assert_eq!(statuses(&setup, cx), [ImportStatus::Queued]);
}

#[gpui_kit::test]
fn the_queue_keeps_working_after_it_went_idle(cx: &mut TestAppContext) {
    let setup = setup(cx);
    let dir = tempfile::tempdir().unwrap();
    add(&setup, cx, vec![write_book(dir.path(), "One")]);
    cx.run_until_parked();
    add(&setup, cx, vec![write_book(dir.path(), "Two")]);
    cx.run_until_parked();
    assert_eq!(setup.library.read_with(cx, |l, _| l.books().len()), 2);
}

#[gpui_kit::test]
fn dropping_the_queue_cancels_what_has_not_started(cx: &mut TestAppContext) {
    let setup = setup(cx);
    let dir = tempfile::tempdir().unwrap();
    add(&setup, cx, vec![write_book(dir.path(), "One")]);
    let Setup { queue, library, .. } = setup;
    drop(queue);
    cx.run_until_parked();
    assert_eq!(library.read_with(cx, |l, _| l.books().len()), 0);
}

#[gpui_kit::test]
fn there_is_no_queue_before_the_library_page_makes_one(cx: &mut TestAppContext) {
    let _setup = setup(cx);
    assert!(cx.update(|cx| ImportQueue::try_global(cx)).is_none());

    let (window, _) = testing::open(cx, 300.0, 200.0, |_, _| Blank);
    let made = cx
        .update_window(window, |_, window, cx| ImportQueue::install(window, cx))
        .unwrap();

    let found = cx.update(|cx| ImportQueue::try_global(cx));
    assert_eq!(found.map(|queue| queue.entity_id()), Some(made.entity_id()));
}

#[gpui_kit::test]
fn the_application_has_one_queue(cx: &mut TestAppContext) {
    let _setup = setup(cx);
    let (window, _) = testing::open(cx, 300.0, 200.0, |_, _| Blank);
    let (first, second) = cx
        .update_window(window, |_, window, cx| {
            (
                ImportQueue::install(window, cx),
                ImportQueue::install(window, cx),
            )
        })
        .unwrap();
    assert_eq!(first.entity_id(), second.entity_id());
    let global = cx.update(|cx| ImportQueue::global(cx));
    assert_eq!(global.entity_id(), first.entity_id());
}
