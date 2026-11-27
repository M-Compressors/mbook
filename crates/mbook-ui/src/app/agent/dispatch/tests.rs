//! The calls on the main thread: the library, a book through its session,
//! and the dialog that asks the person.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use serde_json::Value;

use super::{MainWindow, answer};
use crate::app::agent::bridge::Caller;
use crate::app::agent::bridge::{CreateBook, ExportBook, FileArgs, ListBooks, Request, Show};
use crate::app::agent::pictures::{Encoding, Source};
use crate::entities::catalogue::Library;
use crate::features::appearance::Appearance;
use crate::features::edit_book::agent::error::AgentError;
use crate::features::edit_book::agent::types::{AddChapter, ChapterArgs, MetaPatch, PathArgs};
use crate::features::edit_book::agent::{Answer, Op};
use crate::testing::{self, Blank, Fixture};

const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><rect width='8' height='8'/></svg>";

/// A library with its models, and no window.
fn library(cx: &mut TestAppContext) -> Fixture {
    let fixture = Fixture::new();
    fixture.install(cx);
    cx.update(|cx| {
        gpui_kit::init(cx);
        let appearance = cx.new(Appearance::new);
        Appearance::install(appearance, cx);
        let library = cx.new(Library::new);
        Library::install(library, cx);
    });
    cx.run_until_parked();
    fixture
}

/// A client of session 1 called "Test agent", or another.
fn caller(name: &str, session: u64) -> Caller {
    Caller {
        name: name.into(),
        session,
    }
}

/// Runs one call and waits for its answer.
fn ask(request: Request, cx: &mut TestAppContext) -> Result<Answer, AgentError> {
    let task = cx
        .update(|cx| cx.spawn(async move |cx| answer(request, &caller("Test agent", 1), cx).await));
    cx.run_until_parked();
    cx.foreground_executor().block_test(task)
}

/// Makes a book and gives its key.
fn create(cx: &mut TestAppContext) -> String {
    let created = ask(
        Request::CreateBook(CreateBook {
            title: "The Example".into(),
            language: "ru".into(),
        }),
        cx,
    )
    .unwrap();
    created.value["book"].as_str().unwrap().to_owned()
}

fn on(book: &str, op: Op) -> Request {
    Request::Book {
        book: book.to_owned(),
        op,
    }
}

/// Gives the window where dialogs are shown.
fn with_window(cx: &mut TestAppContext) -> gpui_kit::AnyWindowHandle {
    let (handle, _) = testing::open(cx, 900.0, 700.0, |_, _| Blank);
    cx.update(|cx| {
        cx.set_global(MainWindow {
            handle,
            shell: None,
        });
    });
    handle
}

/// Presses a button of the dialog that is open.
fn press(window: gpui_kit::AnyWindowHandle, id: &str, cx: &mut TestAppContext) {
    let id = id.to_owned();
    testing::frame(cx, window, move |window, cx| window.click(id, cx));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_made_book_is_in_the_library_in_its_language(cx: &mut TestAppContext) {
    let _fixture = library(cx);
    let book = create(cx);
    let listed = ask(Request::ListBooks(ListBooks::default()), cx).unwrap();
    assert_eq!(listed.value["books"][0]["book"], book.as_str());
    assert_eq!(listed.value["books"][0]["title"], "The Example");
    assert_eq!(listed.value["books"][0]["open_in_editor"], false);
    let structure = ask(
        on(
            &book,
            Op::GetBook {
                open_in_editor: true,
            },
        ),
        cx,
    )
    .unwrap();
    assert_eq!(structure.value["meta"]["language"], "ru");
    assert_eq!(structure.value["unsaved"], false);
    assert_eq!(structure.value["open_in_editor"], false);
}

#[gpui_kit::test]
fn the_library_is_searched_by_title_and_limited(cx: &mut TestAppContext) {
    let _fixture = library(cx);
    create(cx);
    let none = ask(
        Request::ListBooks(ListBooks {
            query: Some("nothing".into()),
            limit: None,
        }),
        cx,
    )
    .unwrap();
    assert_eq!(none.value["books"], Value::Array(Vec::new()));
    let found = ask(
        Request::ListBooks(ListBooks {
            query: Some("EXAMPLE".into()),
            limit: Some(1),
        }),
        cx,
    )
    .unwrap();
    assert_eq!(found.value["books"].as_array().map(Vec::len), Some(1));
}

#[gpui_kit::test]
fn a_book_without_a_title_is_not_made(cx: &mut TestAppContext) {
    let _fixture = library(cx);
    let refused = ask(
        Request::CreateBook(CreateBook {
            title: " ".into(),
            language: "en".into(),
        }),
        cx,
    );
    assert_eq!(refused.map_err(|e| e.code()), Err("invalid_meta"));
}

#[gpui_kit::test]
fn a_key_the_library_does_not_have_is_refused(cx: &mut TestAppContext) {
    let _fixture = library(cx);
    let unknown = ask(on("0190a4c2-0000-7000-8000-000000000000", Op::Check), cx);
    assert_eq!(unknown.map_err(|e| e.code()), Err("no_such_book"));
    let malformed = ask(on("not a key", Op::Check), cx);
    assert_eq!(malformed.map_err(|e| e.code()), Err("no_such_book"));
}

#[gpui_kit::test]
fn a_saved_change_of_an_agent_reaches_the_library(cx: &mut TestAppContext) {
    let _fixture = library(cx);
    let book = create(cx);
    let patch = MetaPatch {
        title: Some("Renamed".into()),
        ..MetaPatch::default()
    };
    let changed = ask(on(&book, Op::SetMeta(patch)), cx).unwrap();
    assert!(changed.unsaved);
    let saved = ask(on(&book, Op::Save), cx).unwrap();
    assert_eq!(saved.value["saved"], true);
    let listed = ask(Request::ListBooks(ListBooks::default()), cx).unwrap();
    assert_eq!(listed.value["books"][0]["title"], "Renamed");
}

#[gpui_kit::test]
fn a_picture_given_as_svg_is_added_and_shown_as_png(cx: &mut TestAppContext) {
    let _fixture = library(cx);
    let book = create(cx);
    let added = ask(
        Request::AddPicture {
            book: book.clone(),
            source: Source::Svg { svg: SVG.into() },
            path: None,
            encoding: Encoding::default(),
            cover: true,
        },
        cx,
    )
    .unwrap();
    assert_eq!(added.value["path"], "img/0001.avif");
    let shown = ask(
        Request::ViewPicture {
            book,
            path: "img/0001.avif".into(),
            max_size: 4,
        },
        cx,
    )
    .unwrap();
    assert!(shown.png.is_some_and(|png| png.starts_with(b"\x89PNG")));
    assert_eq!(shown.avif, None);
    assert_eq!(shown.value["width"], 8);
}

/// Runs a call that asks the person, and answers the dialog: with the box
/// that remembers the choice ticked when `remember`.
fn ask_answering(
    request: Request,
    client: Caller,
    window: gpui_kit::AnyWindowHandle,
    button: &str,
    remember: bool,
    cx: &mut TestAppContext,
) -> Result<Answer, AgentError> {
    let task = cx.update(|cx| cx.spawn(async move |cx| answer(request, &client, cx).await));
    cx.run_until_parked();
    if remember {
        press(window, "agent-always", cx);
    }
    press(window, button, cx);
    cx.foreground_executor().block_test(task)
}

#[gpui_kit::test]
fn a_saved_book_is_exported_once_allowed_and_a_file_is_not_overwritten_unasked(
    cx: &mut TestAppContext,
) {
    let _fixture = library(cx);
    let book = create(cx);
    let window = with_window(cx);
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("out.txt");
    let export = |format: &str| {
        Request::ExportBook(ExportBook {
            book: book.clone(),
            format: format.into(),
            path: path.clone(),
        })
    };
    let written = ask_answering(
        export("txt"),
        caller("Test agent", 1),
        window,
        "agent-allow",
        true,
        cx,
    )
    .unwrap();
    assert!(written.value["bytes"].as_u64().is_some());
    assert!(path.exists());
    // Writing a new file was allowed for good; writing over one is asked.
    let over = ask_answering(
        export("txt"),
        caller("Test agent", 1),
        window,
        "agent-decline",
        false,
        cx,
    );
    assert_eq!(over.map_err(|e| e.code()), Err("declined"));
    assert_eq!(
        ask(export("nope"), cx).map_err(|e| e.code()),
        Err("invalid_arguments")
    );
    let relative = Request::ExportBook(ExportBook {
        book,
        format: "txt".into(),
        path: "out.txt".into(),
    });
    assert_eq!(
        ask(relative, cx).map_err(|e| e.code()),
        Err("invalid_arguments")
    );
}

#[gpui_kit::test]
fn a_file_is_imported_once_allowed_and_the_choice_is_remembered_for_its_session(
    cx: &mut TestAppContext,
) {
    let _fixture = library(cx);
    let window = with_window(cx);
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("given.mbook");
    let mut given = mbook::Book::new("Given", "en").unwrap();
    given.add_chapter("ch01.md", "# Given\n").unwrap();
    given.save(&file).unwrap();
    let import = || Request::ImportBook(FileArgs { path: file.clone() });
    let first = ask_answering(
        import(),
        caller("Test agent", 1),
        window,
        "agent-allow",
        true,
        cx,
    )
    .unwrap();
    // Remembered: no dialog is answered this time.
    let again = ask(import(), cx).unwrap();
    assert_eq!(first.value["book"], again.value["book"]);
    // Another session is asked, even under the same name.
    let other = ask_answering(
        import(),
        caller("Test agent", 2),
        window,
        "agent-decline",
        false,
        cx,
    );
    assert_eq!(other.map_err(|e| e.code()), Err("declined"));
    let relative = ask(
        Request::ImportBook(FileArgs {
            path: "given.mbook".into(),
        }),
        cx,
    );
    assert_eq!(relative.map_err(|e| e.code()), Err("invalid_arguments"));
}

#[gpui_kit::test]
fn a_picture_from_a_file_is_read_only_once_allowed(cx: &mut TestAppContext) {
    let _fixture = library(cx);
    let book = create(cx);
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("a.png");
    std::fs::write(&file, b"not read").unwrap();
    let add = || Request::AddPicture {
        book: book.clone(),
        source: Source::File { file: file.clone() },
        path: None,
        encoding: Encoding::default(),
        cover: false,
    };
    assert_eq!(ask(add(), cx).map_err(|e| e.code()), Err("declined"));
    let window = with_window(cx);
    let allowed = ask_answering(
        add(),
        caller("Test agent", 1),
        window,
        "agent-allow",
        false,
        cx,
    );
    // Allowed, the file is read: it is no picture.
    assert_eq!(allowed.map_err(|e| e.code()), Err("invalid_picture"));
    // Not remembered: asked again.
    let declined = ask_answering(
        add(),
        caller("Test agent", 1),
        window,
        "agent-decline",
        false,
        cx,
    );
    assert_eq!(declined.map_err(|e| e.code()), Err("declined"));
}

#[gpui_kit::test]
fn what_needs_the_person_is_declined_without_a_window(cx: &mut TestAppContext) {
    let _fixture = library(cx);
    let book = create(cx);
    let remove = on(
        &book,
        Op::RemoveChapter(ChapterArgs {
            chapter: "ch01.md".into(),
        }),
    );
    assert_eq!(ask(remove, cx).map_err(|e| e.code()), Err("declined"));
    let discard = Request::Discard { book: book.clone() };
    assert_eq!(ask(discard, cx).map_err(|e| e.code()), Err("declined"));
    let delete = Request::DeleteBook { book: book.clone() };
    assert_eq!(ask(delete, cx).map_err(|e| e.code()), Err("declined"));
    let show = Request::Show(Show {
        book,
        href: None,
        mode: None,
    });
    assert_eq!(ask(show, cx).map_err(|e| e.code()), Err("open_failed"));
}

#[gpui_kit::test]
fn a_removal_runs_once_the_person_allows_it(cx: &mut TestAppContext) {
    let _fixture = library(cx);
    let book = create(cx);
    let added = ask(
        on(
            &book,
            Op::AddChapter(AddChapter {
                text: "# Two\n".into(),
                path: None,
                position: None,
                allow_unrendered: false,
            }),
        ),
        cx,
    )
    .unwrap();
    assert_eq!(added.value["path"], "ch02.md");
    let window = with_window(cx);
    let request = on(
        &book,
        Op::RemoveChapter(ChapterArgs {
            chapter: "ch02.md".into(),
        }),
    );
    let task = cx
        .update(|cx| cx.spawn(async move |cx| answer(request, &caller("Test agent", 1), cx).await));
    cx.run_until_parked();
    press(window, "agent-allow", cx);
    assert!(cx.foreground_executor().block_test(task).is_ok());
    let structure = ask(
        on(
            &book,
            Op::GetBook {
                open_in_editor: false,
            },
        ),
        cx,
    )
    .unwrap();
    assert_eq!(
        structure.value["chapters"].as_array().map(Vec::len),
        Some(1)
    );
}

#[gpui_kit::test]
fn a_removal_the_person_declines_changes_nothing(cx: &mut TestAppContext) {
    let _fixture = library(cx);
    let book = create(cx);
    let window = with_window(cx);
    let request = on(
        &book,
        Op::RemovePicture(PathArgs {
            path: "img/none.avif".into(),
        }),
    );
    let task = cx
        .update(|cx| cx.spawn(async move |cx| answer(request, &caller("Test agent", 1), cx).await));
    cx.run_until_parked();
    press(window, "agent-decline", cx);
    let declined = cx.foreground_executor().block_test(task);
    assert_eq!(declined.map_err(|e| e.code()), Err("declined"));
}

#[gpui_kit::test]
fn a_book_the_person_allows_to_delete_leaves_the_library(cx: &mut TestAppContext) {
    let _fixture = library(cx);
    let book = create(cx);
    let window = with_window(cx);
    let request = Request::DeleteBook { book: book.clone() };
    let task = cx
        .update(|cx| cx.spawn(async move |cx| answer(request, &caller("Test agent", 1), cx).await));
    cx.run_until_parked();
    press(window, "agent-allow", cx);
    assert!(cx.foreground_executor().block_test(task).is_ok());
    let gone = ask(on(&book, Op::Check), cx);
    assert_eq!(gone.map_err(|e| e.code()), Err("no_such_book"));
}

#[gpui_kit::test]
fn a_show_needs_a_mode_it_knows(cx: &mut TestAppContext) {
    let _fixture = library(cx);
    let book = create(cx);
    with_window(cx);
    let show = Request::Show(Show {
        book,
        href: None,
        mode: Some("print".into()),
    });
    assert_eq!(
        ask(show, cx).map_err(|e| e.code()),
        Err("invalid_arguments")
    );
}

#[gpui_kit::test]
fn a_call_that_crosses_the_bridge_is_answered_on_the_main_thread(cx: &mut TestAppContext) {
    let _fixture = library(cx);
    create(cx);
    let (bridge, calls) = crate::app::agent::bridge::Bridge::new();
    let _listening = cx.update(|cx| super::listen(calls, cx));
    let asked = bridge.ask(Request::ListBooks(ListBooks::default()), caller("Test", 1));
    cx.run_until_parked();
    let answer = cx.foreground_executor().block_test(asked).unwrap();
    assert_eq!(answer.value["books"].as_array().map(Vec::len), Some(1));
    let busy = cx.update(|cx| {
        crate::features::agents::AgentStatus::global(cx)
            .read(cx)
            .busy()
    });
    assert!(!busy);
}

#[gpui_kit::test]
fn a_bridge_without_a_main_thread_says_the_application_is_closing(cx: &mut TestAppContext) {
    let (bridge, calls) = crate::app::agent::bridge::Bridge::new();
    drop(calls);
    let asked = bridge.ask(Request::ListBooks(ListBooks::default()), caller("Test", 1));
    let refused = cx.foreground_executor().block_test(asked);
    assert_eq!(refused.map_err(|e| e.code()), Err("open_failed"));
}

#[gpui_kit::test]
fn a_book_open_in_the_editor_is_not_deleted(cx: &mut TestAppContext) {
    let _fixture = library(cx);
    let book = create(cx);
    let key: crate::shared::model::BookKey = book.parse().unwrap();
    cx.update(|cx| crate::features::edit_book::sessions::Sessions::edit(key, cx));
    let refused = ask(Request::DeleteBook { book }, cx);
    assert_eq!(refused.map_err(|e| e.code()), Err("invalid_arguments"));
}

/// The window of the application over a library of one book, open.
fn application(cx: &mut TestAppContext) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let roots = crate::shared::roots::Roots::new(dir.path().join("data"), dir.path().join("cache"));
    roots.prepare().unwrap();
    let key = crate::shared::model::BookKey::generate();
    let mut book = mbook::Book::new("Dune", "en").unwrap();
    book.add_chapter("ch01.md", "# One\n").unwrap();
    book.add_chapter("ch02.md", "# Two\n").unwrap();
    book.save(roots.book(key)).unwrap();
    cx.update(crate::app::bind_keys);
    testing::open(cx, 1200.0, 800.0, move |window, cx| {
        crate::app::shell::Shell::new(roots, window, cx)
    });
    cx.run_until_parked();
    (dir, key.to_string())
}

/// What the root shows: `read`, `edit` or something else.
fn shown(cx: &mut TestAppContext) -> &'static str {
    cx.update(|cx| {
        let main = cx.global::<MainWindow>().clone();
        let root = main.shell.and_then(|shell| shell.read(cx).root().cloned());
        root.map_or("none", |root| match root.read(cx).screen() {
            crate::app::root::Screen::Reader(_) => "read",
            crate::app::root::Screen::Editor(_) => "edit",
            _ => "other",
        })
    })
}

#[gpui_kit::test]
fn a_book_is_shown_in_the_reader_or_the_editor(cx: &mut TestAppContext) {
    let (_dir, book) = application(cx);
    let show = |mode: &str, href: Option<&str>| {
        Request::Show(Show {
            book: book.clone(),
            href: href.map(str::to_owned),
            mode: Some(mode.into()),
        })
    };
    assert!(ask(show("read", None), cx).is_ok());
    assert_eq!(shown(cx), "read");
    assert!(ask(show("edit", Some("ch02.md#two")), cx).is_ok());
    cx.run_until_parked();
    assert_eq!(shown(cx), "edit");
    let open = cx.update(|cx| {
        let main = cx.global::<MainWindow>().clone();
        let root = main
            .shell
            .and_then(|shell| shell.read(cx).root().cloned())
            .unwrap();
        match root.read(cx).screen() {
            crate::app::root::Screen::Editor(page) => page.read(cx).key().to_string(),
            _ => String::new(),
        }
    });
    assert_eq!(open, book);
    // The editor in front stays for the same book.
    assert!(ask(show("edit", None), cx).is_ok());
    assert_eq!(shown(cx), "edit");
}
