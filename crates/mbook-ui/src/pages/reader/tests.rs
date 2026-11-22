//! The reader, in headless windows, over books written to a temporary root.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    Action as _, AnyWindowHandle, AppContext as _, ClickEvent, Entity, Focusable as _, Role,
    TestAppContext, TouchClickEvent,
};

use super::controls::HIDE_AFTER;
use super::pictures::LoadError;
use super::support::{FIGURE, TestResult, long_chapter, write_book};
use super::{Arrival, Phase, ReaderEvent, ReaderPage};
use crate::entities::book::error::BookError;
use crate::entities::catalogue::{Library, ReadingState};
use crate::entities::picture::PictureError;
use crate::features::appearance::Appearance;
use crate::features::reading_position::PositionKeeper;
use crate::shared::actions::GoBack;
use crate::shared::model::{BookKey, Place};
use crate::shared::theme::PageTone;
use crate::testing::{self, Fixture};

/// Where the tests read from.
struct Started {
    fixture: Fixture,
    key: BookKey,
    window: AnyWindowHandle,
    page: Entity<ReaderPage>,
    library: Entity<Library>,
    events: Entity<Events>,
}

/// What the page asked the root to do.
#[derive(Default)]
struct Events {
    seen: Vec<ReaderEvent>,
}

const CH01: &str = "# One\n\nSee [two](ch02.md#second), [the web](https://example.org) and [the note](#note).\n\n![A figure](img/fig.avif)\n\n[note]: #note\nA named paragraph.\n";

fn two_chapters() -> [(&'static str, String); 2] {
    [
        ("ch01.md", CH01.to_owned()),
        (
            "ch02.md",
            format!("{}\n## Second\n\nMore.\n", long_chapter("Two", 80)),
        ),
    ]
}

/// Writes a book with `chapters` over a library and opens a reader on it at
/// `width` pixels. The background has not run yet.
fn start_with(
    cx: &mut TestAppContext,
    width: f32,
    chapters: &[(&str, String)],
    stored: Option<ReadingState>,
) -> TestResult<Started> {
    start_with_mode(cx, width, chapters, stored, true)
}

fn start_with_mode(
    cx: &mut TestAppContext,
    width: f32,
    chapters: &[(&str, String)],
    stored: Option<ReadingState>,
    paged: bool,
) -> TestResult<Started> {
    let fixture = Fixture::new();
    if paged {
        let mut settings = crate::entities::catalogue::Settings::default();
        settings.reading.flow = crate::entities::catalogue::ReadingFlow::Pages;
        fixture.catalogue.put_settings(&settings)?;
    }
    let key = fixture.add_book("Dune");
    let chapters: Vec<(&str, &str)> = chapters
        .iter()
        .map(|(path, text)| (*path, text.as_str()))
        .collect();
    write_book(
        &fixture.roots.book(key),
        "Dune",
        "en",
        &chapters,
        Some("rotated.avif"),
    )?;
    cx.update(gpui_kit::init);
    fixture.install(cx);
    let library = cx.update(|cx| {
        let library = cx.new(Library::new);
        Library::install(library.clone(), cx);
        let appearance = cx.new(Appearance::new);
        Appearance::install(appearance, cx);
        library
    });
    cx.run_until_parked();
    if let Some(state) = stored {
        library.update(cx, |library, cx| library.set_state(key, state, cx));
    }
    let (window, page) = testing::open(cx, width, 700.0, |window, cx| {
        ReaderPage::new(key, window, cx)
    });
    let events = cx.update(|cx| cx.new(|_| Events::default()));
    let log = events.clone();
    let subscription = cx.update(|cx| {
        cx.subscribe(&page, move |_, event: &ReaderEvent, cx| {
            log.update(cx, |events, _| events.seen.push(*event));
        })
    });
    std::mem::forget(subscription);
    cx.update_window(window, |_, window, cx| {
        let handle = page.focus_handle(cx);
        window.focus(&handle, cx);
    })?;
    Ok(Started {
        fixture,
        key,
        window,
        page,
        library,
        events,
    })
}

fn start(cx: &mut TestAppContext, width: f32) -> TestResult<Started> {
    start_with(cx, width, &two_chapters(), None)
}

/// Lets the background run and draws frames until nothing changes.
fn settle(started: &Started, cx: &mut TestAppContext) {
    for _ in 0..8 {
        cx.run_until_parked();
        testing::frame(cx, started.window, |_, _| ());
    }
}

fn press(started: &Started, key: &str, cx: &mut TestAppContext) {
    let key = key.to_owned();
    testing::frame(cx, started.window, move |window, cx| window.press(&key, cx));
    settle(started, cx);
}

fn click(started: &Started, id: &str, cx: &mut TestAppContext) {
    let id = id.to_owned();
    testing::frame(cx, started.window, move |window, cx| window.click(id, cx));
    settle(started, cx);
}

fn chapter_of(started: &Started, cx: &TestAppContext) -> usize {
    started.page.read_with(cx, |page, _| page.spot.chapter)
}

fn top(started: &Started, cx: &TestAppContext) -> u32 {
    started
        .page
        .read_with(cx, |page, cx| page.view.read(cx).top_offset(cx))
}

fn stored(started: &Started, cx: &TestAppContext) -> ReadingState {
    started.library.read_with(cx, |library, _| {
        library.book(started.key).unwrap().state.clone()
    })
}

fn seen(started: &Started, cx: &TestAppContext) -> Vec<ReaderEvent> {
    started
        .events
        .read_with(cx, |events, _| events.seen.clone())
}

fn shown_text(started: &Started, cx: &TestAppContext) -> String {
    started
        .page
        .read_with(cx, |page, cx| page.view.read(cx).shown_text(cx))
}

/// The key of the platform's bracket shortcuts.
fn secondary(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("cmd-{key}")
    } else {
        format!("ctrl-{key}")
    }
}

/// Puts the pointer over the text, away from the controls.
fn rest_the_pointer(started: &Started, cx: &mut TestAppContext) {
    testing::frame(cx, started.window, |window, cx| {
        window.hover("reader-page", cx);
    });
    settle(started, cx);
}

fn has(started: &Started, id: &str, cx: &mut TestAppContext) -> bool {
    let id = id.to_owned();
    testing::frame(cx, started.window, move |window, _| {
        window.try_find(id).is_some()
    })
}

// Loading

#[gpui_kit::test]
fn the_reader_draws_its_loading_state_while_the_background_is_held_back(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(window.find("reader-page").role(), Some(Role::Main));
        let status = window.find("reader-opening");
        assert_eq!(status.role(), Some(Role::Status));
        assert_eq!(status.label(), Some("Opening Dune"));
        assert_eq!(window.find("back").role(), Some(Role::Button));
        assert!(window.try_find("chapter-text").is_none());
    });
}

#[gpui_kit::test]
fn an_open_book_shows_its_title_and_its_first_chapter(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(window.find("page-heading").label(), Some("Dune"));
        assert_eq!(window.find("chapter-text").label(), Some("One"));
        assert!(window.try_find("reader-opening").is_none());
    });
    assert!(shown_text(&started, cx).contains("A named paragraph."));
}

#[gpui_kit::test]
fn the_reader_works_at_each_width(cx: &mut TestAppContext) {
    for width in [390.0, 800.0, 1280.0] {
        let started = start(cx, width).unwrap();
        settle(&started, cx);
        testing::frame(cx, started.window, |window, _| {
            for label in [
                "Back",
                "Contents",
                "Text and page appearance",
                "Edit this book",
            ] {
                assert!(
                    window.try_find_by_label(label).is_some(),
                    "{width}: {label}"
                );
            }
            assert!(window.try_find("reader-status").is_some(), "{width}");
        });
        assert!(
            shown_text(&started, cx).contains("named paragraph"),
            "{width}"
        );
    }
}

#[gpui_kit::test]
fn a_book_that_cannot_be_opened_says_why_and_can_be_left(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    std::fs::write(started.fixture.roots.book(started.key), b"not a book").unwrap();
    settle(&started, cx);
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(
            window.find("empty-title").label(),
            Some("This book could not be opened")
        );
        assert!(window.try_find("retry").is_some());
    });
    click(&started, "back", cx);
    assert_eq!(seen(&started, cx), [ReaderEvent::Close]);
}

#[gpui_kit::test]
fn a_book_that_is_there_again_opens_when_the_reader_tries_again(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    let path = started.fixture.roots.book(started.key);
    std::fs::write(&path, b"not a book").unwrap();
    settle(&started, cx);
    let chapters = two_chapters();
    let chapters: Vec<(&str, &str)> = chapters.iter().map(|(p, t)| (*p, t.as_str())).collect();
    write_book(&path, "Dune", "en", &chapters, Some("rotated.avif")).unwrap();
    click(&started, "retry", cx);
    assert!(shown_text(&started, cx).contains("named paragraph"));
}

#[gpui_kit::test]
fn a_book_without_chapters_says_it_has_no_text(cx: &mut TestAppContext) {
    let started = start_with(cx, 800.0, &[], None).unwrap();
    settle(&started, cx);
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(
            window.find("empty-title").label(),
            Some("This book has no text")
        );
    });
}

#[gpui_kit::test]
fn a_chapter_that_cannot_be_read_offers_to_try_again(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    started.page.update(cx, |page, cx| {
        let generation = page.chapter_generation;
        page.chapter_ready(
            generation,
            Arrival::Start,
            Err(BookError::NoSuchChapter(9)),
            cx,
        );
    });
    settle(&started, cx);
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(
            window.find("empty-title").label(),
            Some("This chapter could not be read")
        );
    });
    click(&started, "retry", cx);
    testing::frame(cx, started.window, |window, _| {
        assert!(window.try_find("empty-title").is_none());
    });
}

// The place

#[gpui_kit::test]
fn a_book_reopens_where_it_was_left(cx: &mut TestAppContext) {
    let chapters = two_chapters();
    let target = u32::try_from(chapters[1].1.find("Paragraph 50 of").unwrap()).unwrap();
    let state = ReadingState {
        place: Some(Place {
            chapter: "ch02.md".to_owned(),
            offset: target + 7,
        }),
        ..ReadingState::default()
    };
    let started = start_with(cx, 800.0, &chapters, Some(state)).unwrap();
    settle(&started, cx);
    assert_eq!(chapter_of(&started, cx), 1);
    let first = top(&started, cx);
    assert!(
        first <= target + 7,
        "the page begins before the stored place"
    );
    press(&started, "pagedown", cx);
    assert!(
        top(&started, cx) > target + 7,
        "the next page begins after it"
    );
}

#[gpui_kit::test]
fn a_chapter_that_is_gone_opens_the_start_of_the_book(cx: &mut TestAppContext) {
    let state = ReadingState {
        place: Some(Place {
            chapter: "gone.md".to_owned(),
            offset: 99,
        }),
        ..ReadingState::default()
    };
    let started = start_with(cx, 800.0, &two_chapters(), Some(state)).unwrap();
    settle(&started, cx);
    assert_eq!(chapter_of(&started, cx), 0);
    assert_eq!(top(&started, cx), 0);
}

#[gpui_kit::test]
fn the_place_is_written_when_the_second_is_up(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::Start, cx));
    settle(&started, cx);
    for _ in 0..3 {
        press(&started, "space", cx);
    }
    assert_eq!(
        stored(&started, cx).progress_permille,
        0,
        "nothing is written before the second is up"
    );
    rest_the_pointer(&started, cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    let state = stored(&started, cx);
    let place = state.place.expect("a place is stored");
    assert_eq!(place.chapter, "ch02.md");
    assert!(place.offset > 0);
    assert!(state.progress_permille > 0);
    assert!(state.opened_ms.is_some());
    let writes = started
        .page
        .read_with(cx, |page, cx| page.keeper.read(cx).writes());
    assert_eq!(writes, 1, "three moves, one write");
}

#[gpui_kit::test]
fn leaving_the_reader_writes_the_place_at_once(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::Start, cx));
    settle(&started, cx);
    press(&started, "space", cx);
    click(&started, "back", cx);
    assert_eq!(seen(&started, cx), [ReaderEvent::Close]);
    assert_eq!(stored(&started, cx).place.unwrap().chapter, "ch02.md");
}

#[gpui_kit::test]
fn dropping_the_reader_writes_the_place(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::Start, cx));
    settle(&started, cx);
    let Started {
        library,
        key,
        page,
        window,
        events,
        fixture,
    } = started;
    cx.update_window(window, |_, window, _| window.remove_window())
        .ok();
    drop(page);
    drop(events);
    cx.update(|_| ());
    cx.run_until_parked();
    let place = library.read_with(cx, |library, _| {
        library.book(key).unwrap().state.place.clone()
    });
    assert_eq!(place.unwrap().chapter, "ch02.md");
    drop(fixture);
}

#[gpui_kit::test]
fn the_end_of_the_last_chapter_finishes_the_book(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::End, cx));
    settle(&started, cx);
    started.page.update(cx, |page, cx| {
        page.keeper.update(cx, PositionKeeper::flush);
    });
    cx.run_until_parked();
    let state = stored(&started, cx);
    assert!(state.finished, "{state:?}");
    assert_eq!(state.progress_permille, 1000);
}

// Moving

#[gpui_kit::test]
fn space_moves_a_screen_forward_and_shift_space_back(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::Start, cx));
    settle(&started, cx);
    assert_eq!(top(&started, cx), 0);
    press(&started, "space", cx);
    let moved = top(&started, cx);
    assert!(moved > 0);
    press(&started, "shift-space", cx);
    assert_eq!(top(&started, cx), 0);
}

#[gpui_kit::test]
fn the_keys_of_a_page_move_by_a_screen(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::Start, cx));
    settle(&started, cx);
    for forward in ["pagedown", "down", "right"] {
        press(&started, "home", cx);
        press(&started, forward, cx);
        assert!(top(&started, cx) > 0, "{forward}");
    }
    for back in ["pageup", "up", "left"] {
        press(&started, "end", cx);
        let at_end = top(&started, cx);
        press(&started, back, cx);
        assert!(top(&started, cx) < at_end, "{back}");
    }
}

#[gpui_kit::test]
fn home_and_end_go_to_the_ends_of_the_chapter(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::Start, cx));
    settle(&started, cx);
    press(&started, "end", cx);
    assert!(
        started
            .page
            .read_with(cx, |page, cx| page.view.read(cx).at_end(cx))
    );
    press(&started, "home", cx);
    assert!(
        started
            .page
            .read_with(cx, |page, cx| page.view.read(cx).at_start(cx))
    );
}

#[gpui_kit::test]
fn moving_forward_at_the_end_of_a_chapter_opens_the_next(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    assert_eq!(chapter_of(&started, cx), 0);
    press(&started, "space", cx);
    assert_eq!(
        chapter_of(&started, cx),
        1,
        "the first chapter is shorter than a screen"
    );
    assert_eq!(top(&started, cx), 0);
}

#[gpui_kit::test]
fn moving_back_at_the_start_of_a_chapter_opens_the_previous_at_its_end(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::Start, cx));
    settle(&started, cx);
    press(&started, "shift-space", cx);
    assert_eq!(chapter_of(&started, cx), 0);
    assert!(
        started
            .page
            .read_with(cx, |page, cx| page.view.read(cx).at_end(cx))
    );
}

#[gpui_kit::test]
fn moving_back_at_the_start_of_the_book_stays(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    press(&started, "shift-space", cx);
    assert_eq!(chapter_of(&started, cx), 0);
    assert_eq!(top(&started, cx), 0);
}

#[gpui_kit::test]
fn moving_forward_at_the_end_of_the_book_stays(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::End, cx));
    settle(&started, cx);
    press(&started, "space", cx);
    assert_eq!(chapter_of(&started, cx), 1);
}

#[gpui_kit::test]
fn the_bracket_keys_open_the_previous_and_the_next_chapter(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    press(&started, &secondary("]"), cx);
    assert_eq!(chapter_of(&started, cx), 1);
    press(&started, &secondary("]"), cx);
    assert_eq!(chapter_of(&started, cx), 1, "there is no third");
    press(&started, &secondary("["), cx);
    assert_eq!(chapter_of(&started, cx), 0);
    press(&started, &secondary("["), cx);
    assert_eq!(
        chapter_of(&started, cx),
        0,
        "there is none before the first"
    );
}

fn tap(started: &Started, x: f32, cx: &mut TestAppContext) {
    started
        .page
        .update_in_window(cx, started.window, move |page, window, cx| {
            let event = ClickEvent::Touch(TouchClickEvent {
                position: gpui_kit::point(gpui_kit::px(x), gpui_kit::px(300.0)),
                tap_count: 1,
                long_press: false,
            });
            page.on_tap(&event, window, cx);
        });
    settle(started, cx);
}

trait UpdateInWindow {
    fn update_in_window(
        &self,
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
        f: impl FnOnce(&mut ReaderPage, &mut gpui_kit::Window, &mut gpui_kit::Context<'_, ReaderPage>),
    );
}

impl UpdateInWindow for Entity<ReaderPage> {
    fn update_in_window(
        &self,
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
        f: impl FnOnce(&mut ReaderPage, &mut gpui_kit::Window, &mut gpui_kit::Context<'_, ReaderPage>),
    ) {
        let page = self.clone();
        cx.update_window(window, move |_, window, cx| {
            page.update(cx, |page, cx| f(page, window, cx));
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn a_tap_on_the_edges_moves_a_screen_and_a_tap_in_the_middle_shows_the_controls(
    cx: &mut TestAppContext,
) {
    let started = start(cx, 390.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::Start, cx));
    settle(&started, cx);
    tap(&started, 380.0, cx);
    let forward = top(&started, cx);
    assert!(forward > 0);
    tap(&started, 5.0, cx);
    assert!(top(&started, cx) < forward);

    rest_the_pointer(&started, cx);
    cx.executor().advance_clock(HIDE_AFTER * 2);
    settle(&started, cx);
    assert!(!has(&started, "back", cx));
    tap(&started, 195.0, cx);
    assert!(has(&started, "back", cx));
}

#[gpui_kit::test]
fn a_tap_that_is_a_long_press_does_nothing(cx: &mut TestAppContext) {
    let started = start(cx, 390.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::Start, cx));
    settle(&started, cx);
    started
        .page
        .update_in_window(cx, started.window, |page, window, cx| {
            let event = ClickEvent::Touch(TouchClickEvent {
                position: gpui_kit::point(gpui_kit::px(380.0), gpui_kit::px(300.0)),
                tap_count: 1,
                long_press: true,
            });
            page.on_tap(&event, window, cx);
        });
    settle(&started, cx);
    assert_eq!(top(&started, cx), 0);
}

// Links

#[gpui_kit::test]
fn a_link_to_another_place_jumps_there_and_back_returns(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.follow("ch02.md#second", cx));
    settle(&started, cx);
    assert_eq!(chapter_of(&started, cx), 1);
    assert!(top(&started, cx) > 0);

    testing::frame(cx, started.window, |window, cx| {
        window.dispatch_action(GoBack.boxed_clone(), cx);
    });
    settle(&started, cx);
    assert_eq!(chapter_of(&started, cx), 0);
    assert_eq!(
        seen(&started, cx),
        Vec::new(),
        "the page is not closed by it"
    );
}

#[gpui_kit::test]
fn going_back_with_nowhere_to_go_back_to_is_left_to_the_root(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    let handled = started.page.update(cx, ReaderPage::back);
    assert!(!handled);
}

#[gpui_kit::test]
fn a_link_that_leads_nowhere_does_nothing(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.follow("gone.md", cx));
    started
        .page
        .update(cx, |page, cx| page.follow("not a link of this page", cx));
    settle(&started, cx);
    assert_eq!(chapter_of(&started, cx), 0);
    assert!(
        started
            .page
            .read_with(cx, |page, _| page.history.is_empty())
    );
}

#[gpui_kit::test]
fn a_link_to_the_web_opens_in_the_browser_only_when_it_is_followed(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    assert_eq!(cx.opened_url(), None);
    started
        .page
        .update(cx, |page, cx| page.follow("https://example.org", cx));
    assert_eq!(cx.opened_url().as_deref(), Some("https://example.org"));
    assert_eq!(chapter_of(&started, cx), 0);
}

#[gpui_kit::test]
fn a_link_the_tables_do_not_answer_is_completed_on_the_background(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    started.page.update(cx, |page, cx| page.follow("#note", cx));
    settle(&started, cx);
    assert_eq!(started.page.read_with(cx, |page, _| page.history.len()), 1);
}

// The contents and the appearance

#[gpui_kit::test]
fn the_contents_open_as_a_popover_on_an_expanded_window(cx: &mut TestAppContext) {
    let started = start(cx, 1280.0).unwrap();
    settle(&started, cx);
    click(&started, "contents", cx);
    testing::frame(cx, started.window, |window, _| {
        assert!(window.try_find("contents-panel").is_some());
        assert!(window.try_find("sheet-content").is_none());
    });
}

#[gpui_kit::test]
fn the_contents_open_as_a_sheet_on_a_compact_window(cx: &mut TestAppContext) {
    let started = start(cx, 390.0).unwrap();
    settle(&started, cx);
    click(&started, "contents", cx);
    testing::frame(cx, started.window, |window, _| {
        assert!(window.try_find("sheet-content").is_some());
        assert!(window.try_find("contents-panel").is_some());
    });
}

#[gpui_kit::test]
fn the_contents_open_as_a_sheet_on_a_medium_window(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    click(&started, "contents", cx);
    testing::frame(cx, started.window, |window, _| {
        assert!(window.try_find("sheet-content").is_some());
    });
}

#[gpui_kit::test]
fn the_current_entry_is_marked_in_words(cx: &mut TestAppContext) {
    let started = start(cx, 1280.0).unwrap();
    settle(&started, cx);
    click(&started, "contents", cx);
    testing::frame(cx, started.window, |window, _| {
        assert!(window.try_find_by_label("One, current chapter").is_some());
        assert!(window.try_find_by_label("Two").is_some());
    });
}

#[gpui_kit::test]
fn choosing_an_entry_goes_there_and_closes_the_panel(cx: &mut TestAppContext) {
    for width in [390.0, 1280.0] {
        let started = start(cx, width).unwrap();
        settle(&started, cx);
        click(&started, "contents", cx);
        click(&started, "contents-ch02.md-0", cx);
        assert_eq!(chapter_of(&started, cx), 1, "{width}");
        testing::frame(cx, started.window, |window, _| {
            assert!(window.try_find("contents-panel").is_none(), "{width}");
        });
        assert_eq!(
            started.page.read_with(cx, |page, _| page.history.len()),
            1,
            "the entry remembers where the reader came from"
        );
    }
}

#[gpui_kit::test]
fn the_page_tone_is_chosen_by_name_and_the_choice_is_stored(cx: &mut TestAppContext) {
    let started = start(cx, 1280.0).unwrap();
    settle(&started, cx);
    click(&started, "appearance", cx);
    testing::frame(cx, started.window, |window, _| {
        for name in [
            "Follow the system",
            "Paper",
            "Neutral",
            "Sepia",
            "Night",
            "High contrast",
        ] {
            assert!(
                window.try_find_by_label(name).is_some()
                    || window
                        .try_find_by_label(&format!("{name}, selected"))
                        .is_some(),
                "{name}"
            );
        }
    });
    click(&started, "tone-sepia", cx);
    let tone = cx.update(|cx| Appearance::global(cx).read(cx).settings().page_tone);
    assert_eq!(tone, Some(PageTone::Sepia));
    testing::frame(cx, started.window, |window, _| {
        assert!(window.try_find_by_label("Sepia, selected").is_some());
    });
}

#[gpui_kit::test]
fn the_text_is_made_larger_and_smaller_within_its_bounds(cx: &mut TestAppContext) {
    let started = start(cx, 1280.0).unwrap();
    settle(&started, cx);
    click(&started, "appearance", cx);
    let size = |cx: &mut TestAppContext| {
        cx.update(|cx| Appearance::global(cx).read(cx).settings().text_size)
    };
    assert!((size(cx) - 22.0).abs() < f32::EPSILON);
    click(&started, "text-larger", cx);
    assert!((size(cx) - 23.0).abs() < f32::EPSILON);
    click(&started, "text-smaller", cx);
    click(&started, "text-smaller", cx);
    assert!((size(cx) - 21.0).abs() < f32::EPSILON);
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(window.find("text-size").label(), Some("21 pixels"));
    });
}

#[gpui_kit::test]
fn the_line_height_buttons_update_the_value(cx: &mut TestAppContext) {
    let started = start(cx, 1280.0).unwrap();
    settle(&started, cx);
    click(&started, "appearance", cx);
    click(&started, "reading-advanced", cx);
    click(&started, "line-looser", cx);
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(window.find("line-height").label(), Some("1.65"));
    });
    click(&started, "line-tighter", cx);
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(window.find("line-height").label(), Some("1.60"));
    });
}

#[gpui_kit::test]
fn the_open_chapter_keeps_its_place_when_the_text_is_restyled(cx: &mut TestAppContext) {
    let started = start(cx, 1280.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::Start, cx));
    settle(&started, cx);
    press(&started, "space", cx);
    let before = top(&started, cx);
    click(&started, "appearance", cx);
    click(&started, "text-larger", cx);
    assert_eq!(top(&started, cx), before);
}

#[gpui_kit::test]
fn the_typeface_is_chosen_from_the_ones_on_offer(cx: &mut TestAppContext) {
    let started = start(cx, 1280.0).unwrap();
    settle(&started, cx);
    cx.update(|cx| {
        Appearance::global(cx).update(cx, |appearance, cx| {
            appearance.update(|settings| settings.typeface = "Georgia".to_owned(), cx);
        });
    });
    click(&started, "appearance", cx);
    testing::frame(cx, started.window, |window, _| {
        assert!(window.try_find_by_label("Georgia, selected").is_some());
        assert!(window.try_find_by_label("Literata").is_some());
        assert!(window.try_find_by_label("Noto Sans").is_some());
    });
    click(&started, "typeface-Literata", cx);
    let face = cx.update(|cx| Appearance::global(cx).read(cx).settings().typeface.clone());
    assert_eq!(face, "Literata");
}

#[gpui_kit::test]
fn pages_are_numbered_across_chapters(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    testing::frame(cx, started.window, |window, _| {
        assert!(
            window
                .find("reader-page-number")
                .label()
                .unwrap()
                .starts_with("≈ 1 of ")
        );
    });
    let first = started
        .page
        .read_with(cx, |page, cx| page.view.read(cx).page_number().unwrap().1);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::Start, cx));
    settle(&started, cx);
    testing::frame(cx, started.window, |window, _| {
        let words = window
            .find("reader-page-number")
            .label()
            .unwrap()
            .to_owned();
        assert!(words.starts_with(&format!("{} of ", first + 1)), "{words}");
    });
    click(&started, "next-page", cx);
    testing::frame(cx, started.window, |window, _| {
        let words = window
            .find("reader-page-number")
            .label()
            .unwrap()
            .to_owned();
        assert!(words.starts_with(&format!("{} of ", first + 2)), "{words}");
    });
    click(&started, "previous-page", cx);
    testing::frame(cx, started.window, |window, _| {
        let words = window
            .find("reader-page-number")
            .label()
            .unwrap()
            .to_owned();
        assert!(words.starts_with(&format!("{} of ", first + 1)), "{words}");
    });
}

#[gpui_kit::test]
fn the_reading_slider_previews_a_chapter_and_seeks_on_release(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    let at = testing::frame(cx, started.window, |window, cx| {
        let slider = window.find_by_label("Reading position");
        assert_eq!(slider.role(), Some(Role::Slider));
        let at = slider.bounds().center();
        window.pointer_down(at, gpui_kit::MouseButton::Left, cx);
        at
    });
    settle(&started, cx);
    assert_eq!(chapter_of(&started, cx), 0);
    testing::frame(cx, started.window, |window, cx| {
        assert_eq!(window.find("reader-status").label(), Some("Two, 50%"));
        window.pointer_up(at, gpui_kit::MouseButton::Left, cx);
    });
    settle(&started, cx);
    assert_eq!(chapter_of(&started, cx), 1);
    assert!(top(&started, cx) > 0);
}

#[gpui_kit::test]
fn dragging_the_thumb_moves_to_the_target_page(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    testing::frame(cx, started.window, |window, cx| {
        let slider = window.find_by_label("Reading position").bounds();
        let center = slider.center();
        window.drag(gpui_kit::point(slider.left(), center.y), center, cx);
    });
    settle(&started, cx);
    assert_eq!(chapter_of(&started, cx), 1);
    assert!(top(&started, cx) > 0);
}

#[gpui_kit::test]
fn the_reading_slider_reaches_the_ends_with_the_keyboard(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    let focused = |cx: &mut TestAppContext| {
        testing::frame(cx, started.window, |window, _| {
            window.find_by_label("Reading position").focused() == Some(true)
        })
    };
    for _ in 0..12 {
        if focused(cx) {
            break;
        }
        press(&started, "tab", cx);
    }
    assert!(focused(cx));
    press(&started, "end", cx);
    assert_eq!(chapter_of(&started, cx), 1);
    assert_eq!(started.page.read_with(cx, |page, _| page.progress), 1000);
    press(&started, "home", cx);
    assert_eq!(chapter_of(&started, cx), 0);
    assert_eq!(top(&started, cx), 0);
}

// The controls

#[gpui_kit::test]
fn the_slider_stays_available_to_accessibility_when_the_controls_hide(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    rest_the_pointer(&started, cx);
    cx.executor()
        .advance_clock(HIDE_AFTER + Duration::from_millis(100));
    settle(&started, cx);
    testing::frame(cx, started.window, |window, _| {
        assert!(window.try_find("back").is_none());
        assert_eq!(
            window.find_by_label("Reading position").role(),
            Some(Role::Slider)
        );
    });
}

#[gpui_kit::test]
fn the_controls_hide_while_reading_and_come_back_on_a_key(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    assert!(has(&started, "back", cx));
    rest_the_pointer(&started, cx);
    cx.executor()
        .advance_clock(HIDE_AFTER + Duration::from_millis(100));
    settle(&started, cx);
    assert!(!has(&started, "back", cx));
    assert!(!has(&started, "reader-status", cx));
    assert!(has(&started, "reader-page-number", cx));
    press(&started, "home", cx);
    assert!(has(&started, "back", cx));
    assert!(has(&started, "reader-status", cx));
}

#[gpui_kit::test]
fn the_controls_stay_while_the_setting_keeps_them(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    cx.update(|cx| {
        Appearance::global(cx).update(cx, |appearance, cx| {
            appearance.update(|settings| settings.keep_controls = true, cx);
        });
    });
    rest_the_pointer(&started, cx);
    cx.executor().advance_clock(HIDE_AFTER * 3);
    settle(&started, cx);
    assert!(has(&started, "back", cx));
}

#[gpui_kit::test]
fn the_controls_stay_while_the_pointer_is_over_them(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    testing::frame(cx, started.window, |window, cx| window.hover("back", cx));
    cx.executor().advance_clock(HIDE_AFTER * 3);
    settle(&started, cx);
    assert!(has(&started, "back", cx));
}

#[gpui_kit::test]
fn the_controls_stay_while_the_focus_is_inside_them(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    rest_the_pointer(&started, cx);
    let handle = started
        .page
        .read_with(cx, |page, _| page.handles.contents.clone());
    cx.update_window(started.window, |_, window, cx| window.focus(&handle, cx))
        .unwrap();
    settle(&started, cx);
    let really = cx
        .update_window(started.window, |_, window, _| handle.is_focused(window))
        .unwrap();
    assert!(really, "the handle is focused");
    assert!(
        started
            .page
            .read_with(cx, |page, _| page.controls.holds.focused)
    );
    cx.executor().advance_clock(HIDE_AFTER * 3);
    settle(&started, cx);
    assert!(has(&started, "back", cx));
}

#[gpui_kit::test]
fn the_controls_stay_while_a_panel_is_open(cx: &mut TestAppContext) {
    let started = start(cx, 390.0).unwrap();
    settle(&started, cx);
    click(&started, "contents", cx);
    rest_the_pointer(&started, cx);
    cx.executor().advance_clock(HIDE_AFTER * 3);
    settle(&started, cx);
    assert!(started.page.read_with(cx, |page, _| page.controls.shown));
}

#[gpui_kit::test]
fn the_controls_of_a_book_that_did_not_open_never_hide(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    std::fs::write(started.fixture.roots.book(started.key), b"not a book").unwrap();
    settle(&started, cx);
    rest_the_pointer(&started, cx);
    cx.executor().advance_clock(HIDE_AFTER * 3);
    settle(&started, cx);
    assert!(has(&started, "back", cx));
}

#[gpui_kit::test]
fn the_controls_do_not_move_the_text(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    let place = |cx: &mut TestAppContext| {
        testing::frame(cx, started.window, |window, _| {
            window.find("chapter-text").bounds()
        })
    };
    let shown = place(cx);
    rest_the_pointer(&started, cx);
    cx.executor().advance_clock(HIDE_AFTER * 2);
    settle(&started, cx);
    assert_eq!(place(cx), shown);
}

#[gpui_kit::test]
fn the_edit_control_asks_the_root_to_edit_this_book(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    click(&started, "edit", cx);
    assert_eq!(seen(&started, cx), [ReaderEvent::Edit(started.key)]);
}

#[gpui_kit::test]
fn the_status_line_names_the_chapter_and_the_percentage(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    testing::frame(cx, started.window, |window, _| {
        let status = window.find("reader-status");
        assert_eq!(status.role(), Some(Role::Status));
        assert!(
            status.label().unwrap().starts_with("One, "),
            "{:?}",
            status.label()
        );
        assert!(status.label().unwrap().ends_with('%'));
    });
}

// The file

#[gpui_kit::test]
fn suspending_closes_the_file_and_resuming_opens_it_at_the_same_place(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::Start, cx));
    settle(&started, cx);
    press(&started, "space", cx);
    press(&started, "space", cx);
    let place = top(&started, cx);
    let source = started.page.read_with(cx, |page, _| match &page.phase {
        Phase::Ready(reading) => reading.source.as_ref().map(Arc::downgrade),
        _ => None,
    });
    let source = source.expect("the book is open");

    started.page.update(cx, ReaderPage::suspend);
    cx.run_until_parked();
    assert!(source.upgrade().is_none(), "nothing holds the file");
    assert!(started.page.read_with(cx, |page, _| page.is_suspended()));
    assert_eq!(
        stored(&started, cx).place.unwrap().chapter,
        "ch02.md",
        "written at once"
    );

    started
        .page
        .update_in_window(cx, started.window, |page, window, cx| {
            page.resume(window, cx);
        });
    settle(&started, cx);
    assert!(!started.page.read_with(cx, |page, _| page.is_suspended()));
    assert_eq!(chapter_of(&started, cx), 1);
    assert_eq!(top(&started, cx), place);
}

#[gpui_kit::test]
fn a_book_that_changed_while_it_was_edited_is_opened_at_the_block_of_the_place(
    cx: &mut TestAppContext,
) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::Start, cx));
    settle(&started, cx);
    press(&started, "space", cx);
    started.page.update(cx, ReaderPage::suspend);
    cx.run_until_parked();

    let mut chapters = two_chapters();
    chapters[1].1 = format!("Added at the top.\n\n{}", chapters[1].1);
    let refs: Vec<(&str, &str)> = chapters.iter().map(|(p, t)| (*p, t.as_str())).collect();
    write_book(
        &started.fixture.roots.book(started.key),
        "Dune",
        "en",
        &refs,
        Some("rotated.avif"),
    )
    .unwrap();
    started
        .page
        .update_in_window(cx, started.window, |page, window, cx| {
            page.resume(window, cx);
        });
    settle(&started, cx);
    let blocks = started.page.read_with(cx, |page, _| match &page.phase {
        Phase::Ready(reading) => reading.prepared.as_ref().map(|p| p.blocks.clone()),
        _ => None,
    });
    assert!(
        blocks.unwrap().contains(&top(&started, cx)),
        "at the start of a block"
    );
}

// Pictures

#[gpui_kit::test]
fn a_picture_is_loaded_when_the_view_draws_it(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    assert!(
        started
            .page
            .read_with(cx, |page, _| page.pictures.cached_bytes())
            > 0
    );
}

#[gpui_kit::test]
fn a_picture_that_cannot_be_decoded_shows_its_alternative_text(cx: &mut TestAppContext) {
    let started = start_with(cx, 800.0, &two_chapters(), None).unwrap();
    started.page.update(cx, |page, _| {
        // Nothing is decoded before the failure is reported by hand.
        page.picture_tasks.clear();
    });
    settle(&started, cx);
    let job = started.page.read_with(cx, |page, _| {
        page.pictures.cached_bytes();
        super::pictures::Job {
            destination: FIGURE.to_owned(),
            picture: match &page.phase {
                Phase::Ready(reading) => *reading
                    .prepared
                    .as_ref()
                    .unwrap()
                    .pictures
                    .get(FIGURE)
                    .unwrap(),
                _ => unreachable!(),
            },
            size: (100, 100),
        }
    });
    started.page.update(cx, |page, cx| {
        let generation = page.chapter_generation;
        // A picture that is loading, then fails.
        page.pictures.set_budget(10_000);
        page.pictures.reset(&std::collections::HashMap::from([(
            FIGURE.to_owned(),
            job.picture,
        )]));
        page.pictures.resolve(FIGURE, 300.0, None);
        let job = page.pictures.next_job().unwrap();
        page.picture_done(
            generation,
            &job,
            Err(LoadError::Decode(PictureError::UnsupportedFormat)),
            cx,
        );
    });
    settle(&started, cx);
    assert!(shown_text(&started, cx).contains("A figure"));
}

// Accessibility

#[gpui_kit::test]
fn every_control_has_a_role_and_a_name(cx: &mut TestAppContext) {
    const CONTROLS: [Role; 7] = [
        Role::Button,
        Role::Link,
        Role::CheckBox,
        Role::RadioButton,
        Role::Switch,
        Role::TextInput,
        Role::ComboBox,
    ];
    for width in [390.0, 1280.0] {
        let started = start(cx, width).unwrap();
        settle(&started, cx);
        for panel in [None, Some("contents"), Some("appearance")] {
            if let Some(panel) = panel {
                click(&started, panel, cx);
            }
            testing::frame(cx, started.window, |window, _| {
                let elements = window.elements();
                assert!(!elements.is_empty());
                for element in elements {
                    let kit = testing::is_kit_decoration(&format!("{:?}", element.path()));
                    assert!(
                        kit || element.role().is_some(),
                        "{width}: no role: {element:?}"
                    );
                    if element.role().is_some_and(|role| CONTROLS.contains(&role)) {
                        assert!(
                            element
                                .label()
                                .is_some_and(|label| !label.trim().is_empty()),
                            "{width}: no name: {element:?}"
                        );
                    }
                }
            });
        }
    }
}

#[gpui_kit::test]
fn the_main_task_is_done_with_keys_alone(cx: &mut TestAppContext) {
    let started = start(cx, 1280.0).unwrap();
    settle(&started, cx);
    // Read to the end of the first chapter, into the second, and back.
    press(&started, "space", cx);
    assert_eq!(chapter_of(&started, cx), 1);
    press(&started, "end", cx);
    press(&started, "home", cx);
    press(&started, "shift-space", cx);
    assert_eq!(chapter_of(&started, cx), 0);
    press(&started, &secondary("]"), cx);
    assert_eq!(chapter_of(&started, cx), 1);
}

#[gpui_kit::test]
fn escape_closes_the_sheet_of_the_contents_and_the_reader_stays(cx: &mut TestAppContext) {
    let started = start(cx, 390.0).unwrap();
    settle(&started, cx);
    click(&started, "contents", cx);
    assert!(has(&started, "sheet-content", cx));
    press(&started, "escape", cx);
    assert!(!has(&started, "sheet-content", cx));
    assert!(started.page.read_with(cx, |page, _| page.panel.is_none()));
    assert_eq!(seen(&started, cx), Vec::new());
}

#[gpui_kit::test]
fn escape_closes_the_popover_of_the_appearance(cx: &mut TestAppContext) {
    let started = start(cx, 1280.0).unwrap();
    settle(&started, cx);
    click(&started, "appearance", cx);
    assert!(has(&started, "appearance-panel", cx));
    press(&started, "escape", cx);
    assert!(!has(&started, "appearance-panel", cx));
}

#[gpui_kit::test]
fn going_back_with_a_popover_open_closes_it_first(cx: &mut TestAppContext) {
    let started = start(cx, 1280.0).unwrap();
    settle(&started, cx);
    click(&started, "contents", cx);
    assert!(has(&started, "contents-panel", cx));
    cx.update_window(started.window, |_, window, cx| {
        window.focus(&started.page.read(cx).focus_handle.clone(), cx);
        window.dispatch_action(GoBack.boxed_clone(), cx);
    })
    .unwrap();
    settle(&started, cx);
    assert!(!has(&started, "contents-panel", cx));
}

#[gpui_kit::test]
fn the_tab_key_brings_the_controls_back(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    rest_the_pointer(&started, cx);
    cx.executor().advance_clock(HIDE_AFTER * 2);
    settle(&started, cx);
    assert!(!has(&started, "back", cx));
    press(&started, "a", cx);
    assert!(has(&started, "back", cx), "a plain key");
    cx.executor().advance_clock(HIDE_AFTER * 2);
    settle(&started, cx);
    assert!(!has(&started, "back", cx));
    press(&started, "tab", cx);
    assert!(has(&started, "back", cx), "the tab key");
    let focus = started
        .page
        .read_with(cx, |page, _| page.handles.back.clone());
    let focused = cx
        .update_window(started.window, |_, window, _| focus.is_focused(window))
        .unwrap();
    assert!(focused, "the focus is on the first control");
}

#[gpui_kit::test]
fn the_pointer_moving_brings_the_controls_back(cx: &mut TestAppContext) {
    let started = start(cx, 800.0).unwrap();
    settle(&started, cx);
    rest_the_pointer(&started, cx);
    cx.executor().advance_clock(HIDE_AFTER * 2);
    settle(&started, cx);
    assert!(!has(&started, "back", cx));
    testing::frame(cx, started.window, |window, cx| {
        window.hover("chapter-text", cx);
    });
    settle(&started, cx);
    assert!(has(&started, "back", cx));
}

fn enter_book_query(started: &Started, query: &str, cx: &mut TestAppContext) {
    testing::frame(cx, started.window, |window, cx| window.input(query, cx));
    cx.run_until_parked();
    cx.executor()
        .advance_clock(crate::shared::ui::search_bar::TYPING_PAUSE);
    settle(started, cx);
}

#[gpui_kit::test]
fn book_search_reaches_other_chapters_and_wraps_in_both_directions(cx: &mut TestAppContext) {
    let chapters = [
        (
            "one.md",
            "# First\n\nHere is the first **needle**.\n".to_owned(),
        ),
        ("two.md", "# Middle\n\nNothing here.\n".to_owned()),
        (
            "three.md",
            "# Last\n\nHere is the last **needle**.\n".to_owned(),
        ),
    ];
    let started = start_with(cx, 900.0, &chapters, None).unwrap();
    settle(&started, cx);
    click(&started, "find-book", cx);
    enter_book_query(&started, "NEEDLE", cx);
    assert_eq!(chapter_of(&started, cx), 0);
    press(&started, "enter", cx);
    assert_eq!(chapter_of(&started, cx), 2);
    click(&started, "find-next", cx);
    assert_eq!(chapter_of(&started, cx), 0);
    click(&started, "find-previous", cx);
    assert_eq!(chapter_of(&started, cx), 2);
    click(&started, "find-close", cx);
    assert!(!has(&started, "find-status", cx));
}

#[gpui_kit::test]
fn the_find_shortcut_brings_back_hidden_reader_controls(cx: &mut TestAppContext) {
    cx.update(crate::app::bind_keys);
    let started = start(cx, 900.0).unwrap();
    settle(&started, cx);
    rest_the_pointer(&started, cx);
    cx.executor().advance_clock(HIDE_AFTER * 2);
    settle(&started, cx);
    assert!(!has(&started, "find-book", cx));

    press(&started, &secondary("f"), cx);
    assert!(has(&started, "find-book", cx));
    enter_book_query(&started, "not in this book", cx);
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(window.find("find-status").label(), Some("No matches"));
    });
    press(&started, "escape", cx);
    assert!(!has(&started, "find-status", cx));
    assert_eq!(seen(&started, cx), []);
}

#[gpui_kit::test]
fn clearing_a_book_query_discards_the_pending_search(cx: &mut TestAppContext) {
    let started = start(cx, 900.0).unwrap();
    settle(&started, cx);
    click(&started, "find-book", cx);
    testing::frame(cx, started.window, |window, cx| {
        window.input("More", cx);
    });
    click(&started, "find-close", cx);
    click(&started, "find-book", cx);
    press(&started, "backspace", cx);
    cx.executor()
        .advance_clock(crate::shared::ui::search_bar::TYPING_PAUSE);
    settle(&started, cx);
    assert_eq!(chapter_of(&started, cx), 0);
    assert!(!has(&started, "find-status", cx));
}

#[gpui_kit::test]
fn unreadable_chapters_leave_book_search_incomplete(cx: &mut TestAppContext) {
    let started = start(cx, 900.0).unwrap();
    settle(&started, cx);
    std::fs::OpenOptions::new()
        .write(true)
        .open(started.fixture.roots.book(started.key))
        .unwrap()
        .set_len(1)
        .unwrap();
    click(&started, "find-book", cx);
    enter_book_query(&started, "More", cx);
    testing::frame(cx, started.window, |window, _| {
        assert!(
            window
                .find("find-status")
                .label()
                .unwrap()
                .starts_with("Search incomplete.")
        );
    });
}

#[gpui_kit::test]
fn a_book_match_opens_a_page_deep_in_another_chapter(cx: &mut TestAppContext) {
    let started = start(cx, 900.0).unwrap();
    settle(&started, cx);
    click(&started, "find-book", cx);
    enter_book_query(&started, "Second", cx);
    assert_eq!(chapter_of(&started, cx), 1);
    assert!(top(&started, cx) > 0);
}

#[gpui_kit::test]
fn the_compact_reader_title_has_room_between_the_controls(cx: &mut TestAppContext) {
    let started = start(cx, 390.0).unwrap();
    started.page.update(cx, |page, cx| {
        page.title = "The Quiet Orchard".into();
        cx.notify();
    });
    settle(&started, cx);
    testing::frame(cx, started.window, |window, _| {
        let heading = window.find("page-heading").bounds();
        let appearance = window.find("appearance").bounds();
        assert!(heading.right() <= appearance.left());
        assert!(window.find("edit").bounds().right() <= window.viewport_size().width);
        assert!(window.find("find-book").visible());
    });
}

#[gpui_kit::test]
fn a_new_reader_uses_scrolling_and_keeps_the_initial_size_when_resized(cx: &mut TestAppContext) {
    for (width, expected) in [(390.0, 20.0), (1280.0, 22.0)] {
        let started = start_with_mode(cx, width, &two_chapters(), None, false).unwrap();
        settle(&started, cx);
        assert!(
            !started
                .page
                .read_with(cx, |page, cx| page.view.read(cx).is_paged())
        );
        testing::frame(cx, started.window, |window, _| {
            window.resize(gpui_kit::size(gpui_kit::px(768.0), gpui_kit::px(390.0)));
        });
        settle(&started, cx);
        let saved = started.fixture.catalogue.settings().unwrap();
        assert!((saved.text_size - expected).abs() < f32::EPSILON);
    }
}

#[gpui_kit::test]
fn scrolling_keeps_the_passage_across_reflow_and_reset(cx: &mut TestAppContext) {
    let text =
        "Тихое утро у реки. Анна открыла ворота и увидела ёжика. API и за́мок.\n\n".repeat(850);
    let target = u32::try_from(text.find("\n\n").unwrap() + 2).unwrap() * 420;
    let chapters = [("one.md", text)];
    let started = start_with_mode(cx, 1280.0, &chapters, None, false).unwrap();
    settle(&started, cx);
    started.page.update(cx, |page, cx| {
        page.goto(
            super::Spot {
                chapter: 0,
                offset: target,
            },
            cx,
        );
    });
    settle(&started, cx);
    assert_eq!(top(&started, cx), target);
    cx.update(|cx| {
        Appearance::global(cx).update(cx, |appearance, cx| {
            appearance.update(
                |settings| {
                    settings.text_size = 44.0;
                    settings.line_height = 2.0;
                    settings.typeface = "Noto Sans".to_owned();
                    settings.reading.heavier = true;
                    settings.reading.paragraph_gap = 2.0;
                    settings.reading.column = 45.0;
                },
                cx,
            );
        });
    });
    testing::frame(cx, started.window, |window, _| {
        window.resize(gpui_kit::size(gpui_kit::px(390.0), gpui_kit::px(844.0)));
    });
    settle(&started, cx);
    assert_eq!(top(&started, cx), target);
    click(&started, "appearance", cx);
    press(&started, "escape", cx);
    assert_eq!(top(&started, cx), target);
    testing::frame(cx, started.window, |window, cx| {
        Appearance::global(cx).update(cx, |appearance, cx| appearance.reset_reader(window, cx));
    });
    settle(&started, cx);
    assert_eq!(top(&started, cx), target);
    started.page.update(cx, ReaderPage::close);
    settle(&started, cx);
    assert_eq!(
        started
            .fixture
            .catalogue
            .state(started.key)
            .unwrap()
            .place
            .unwrap()
            .offset,
        target
    );
}

#[gpui_kit::test]
fn search_closes_back_at_the_passage_it_started_from(cx: &mut TestAppContext) {
    let chapters = [
        ("one.md", "# Начало\n\nМы пошли домой.\n".to_owned()),
        (
            "two.md",
            "# Продолжение\n\nАнна увидела **ёжика**.\n".to_owned(),
        ),
    ];
    let started = start_with_mode(cx, 390.0, &chapters, None, false).unwrap();
    settle(&started, cx);
    click(&started, "find-book", cx);
    enter_book_query(&started, "ЁЖИКА", cx);
    assert_eq!(chapter_of(&started, cx), 1);
    click(&started, "find-close", cx);
    assert_eq!(chapter_of(&started, cx), 0);
    assert_eq!(top(&started, cx), 0);
}

#[gpui_kit::test]
fn a_footnote_leads_to_its_definition_and_back(cx: &mut TestAppContext) {
    let middle = "Текст главы продолжается.\n\n".repeat(60);
    let text = format!(
        "# Заметки\n\nАнна сверилась с журналом.[^журнал]\n\n{middle}[^журнал]: Запись за вторник.\n"
    );
    let footnote = u32::try_from(text.find("[^журнал]:").unwrap()).unwrap();
    let chapters = [("one.md", text)];
    let started = start_with_mode(cx, 800.0, &chapters, None, false).unwrap();
    settle(&started, cx);
    let origin = top(&started, cx);
    testing::frame(cx, started.window, |window, cx| {
        let at = window.find_by_label("Footnote журнал").bounds().center();
        window.pointer_down(at, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(at, gpui_kit::MouseButton::Left, cx);
    });
    settle(&started, cx);
    assert!(top(&started, cx) > footnote / 2);
    click(&started, "reader-return", cx);
    assert_eq!(top(&started, cx), origin);
}

#[gpui_kit::test]
fn scrolling_with_the_wheel_updates_the_saved_place(cx: &mut TestAppContext) {
    let started = start_with_mode(cx, 800.0, &two_chapters(), None, false).unwrap();
    settle(&started, cx);
    started
        .page
        .update(cx, |page, cx| page.load_chapter(1, Arrival::Start, cx));
    settle(&started, cx);
    testing::frame(cx, started.window, |window, cx| {
        window.scroll(
            "chapter-text",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.0), gpui_kit::px(-250.0))),
            cx,
        );
    });
    settle(&started, cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(&started, cx);
    let offset = top(&started, cx);
    assert!(offset > 0);
    assert_eq!(
        started
            .fixture
            .catalogue
            .state(started.key)
            .unwrap()
            .place
            .unwrap()
            .offset,
        offset,
    );
}

#[gpui_kit::test]
fn the_appearance_controls_apply_and_reset_the_saved_preferences(cx: &mut TestAppContext) {
    let started = start_with_mode(cx, 1280.0, &two_chapters(), None, false).unwrap();
    testing::frame(cx, started.window, |window, _| {
        window.resize(gpui_kit::size(gpui_kit::px(1280.0), gpui_kit::px(1100.0)));
    });
    settle(&started, cx);
    click(&started, "appearance", cx);
    click(&started, "text-scale", cx);
    let settings = || started.fixture.catalogue.settings().unwrap();
    assert!((settings().text_size - 44.0).abs() < f32::EPSILON);
    click(&started, "text-scale", cx);
    click(&started, "typeface-System", cx);
    click(&started, "reading-advanced", cx);
    for id in [
        "column-wider",
        "column-narrower",
        "paragraph-tighter",
        "paragraph-looser",
        "reading-heavy",
        "reading-paged",
        "reading-page-motion",
        "reading-reduced-motion",
    ] {
        click(&started, id, cx);
    }
    let scroll = |amount, cx: &mut TestAppContext| {
        testing::frame(cx, started.window, |window, cx| {
            window.scroll(
                "appearance-panel",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(
                    gpui_kit::px(0.0),
                    gpui_kit::px(amount),
                )),
                cx,
            );
        });
    };
    scroll(-500.0, cx);
    click(&started, "reading-progress", cx);
    let changed = settings();
    assert!(changed.reading.heavier);
    assert!(changed.reading.hide_progress);
    assert_eq!(
        changed.reading.flow,
        crate::entities::catalogue::ReadingFlow::AnimatedPages
    );
    assert_eq!(changed.typeface, "System");
    scroll(500.0, cx);
    click(&started, "reading-reset", cx);
    let mut expected = crate::entities::catalogue::Settings {
        text_size: 22.0,
        ..crate::entities::catalogue::Settings::default()
    };
    expected.reading.size_set = true;
    assert_eq!(settings(), expected);
}
