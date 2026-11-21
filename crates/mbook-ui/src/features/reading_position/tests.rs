//! The state of a position, and its writing.

use std::time::Duration;

use gpui_kit::{AppContext as _, Entity, TestAppContext};

use super::keeper::WRITE_EVERY;
use super::{PositionKeeper, reading_state};
use crate::entities::book::snapshot::{BookSnapshot, ChapterInfo};
use crate::entities::catalogue::{Library, ReadingState};
use crate::shared::model::{BookKey, Place};
use crate::testing::Fixture;

fn snapshot() -> BookSnapshot {
    BookSnapshot {
        title: "T".to_owned(),
        authors: Vec::new(),
        language: "en".to_owned(),
        chapters: vec![
            ChapterInfo {
                path: "a.md".to_owned(),
                decoded_len: 100,
            },
            ChapterInfo {
                path: "b.md".to_owned(),
                decoded_len: 300,
            },
        ],
        contents: Vec::new(),
        cover: None,
    }
}

fn state(snapshot: &BookSnapshot, chapter: usize, offset: u32, at_end: bool) -> ReadingState {
    reading_state(
        snapshot,
        chapter,
        offset,
        at_end,
        &ReadingState::default(),
        7,
    )
    .unwrap()
}

#[test]
fn the_place_is_the_path_of_the_chapter_and_the_offset() {
    let state = state(&snapshot(), 1, 42, false);
    assert_eq!(
        state.place,
        Some(Place {
            chapter: "b.md".to_owned(),
            offset: 42
        })
    );
}

#[test]
fn the_progress_counts_the_chapters_before_and_the_offset() {
    assert_eq!(state(&snapshot(), 1, 100, false).progress_permille, 500);
}

#[test]
fn the_opening_time_is_the_one_given() {
    assert_eq!(state(&snapshot(), 0, 0, false).opened_ms, Some(7));
}

#[test]
fn a_book_is_not_finished_in_the_middle() {
    assert!(!state(&snapshot(), 1, 10, false).finished);
}

#[test]
fn the_end_of_a_chapter_that_is_not_the_last_does_not_finish_the_book() {
    let state = state(&snapshot(), 0, 90, true);
    assert!(!state.finished);
    assert_eq!(state.progress_permille, 225);
}

#[test]
fn the_end_of_the_last_chapter_finishes_the_book_and_fills_the_progress() {
    let state = state(&snapshot(), 1, 20, true);
    assert!(state.finished);
    assert_eq!(state.progress_permille, 1000);
}

#[test]
fn a_finished_book_stays_finished_when_it_is_read_again() {
    let previous = ReadingState {
        finished: true,
        ..ReadingState::default()
    };
    let state = reading_state(&snapshot(), 0, 0, false, &previous, 1).unwrap();
    assert!(state.finished);
}

#[test]
fn a_chapter_the_book_does_not_have_has_no_state() {
    assert_eq!(
        reading_state(&snapshot(), 5, 0, false, &ReadingState::default(), 1),
        None
    );
}

fn started(cx: &mut TestAppContext) -> (Fixture, BookKey, Entity<Library>) {
    let fixture = Fixture::new();
    let key = fixture.add_book("Dune");
    cx.update(gpui_kit::init);
    fixture.install(cx);
    let library = cx.update(|cx| {
        let library = cx.new(Library::new);
        Library::install(library.clone(), cx);
        library
    });
    cx.run_until_parked();
    (fixture, key, library)
}

fn stored(library: &Entity<Library>, key: BookKey, cx: &TestAppContext) -> ReadingState {
    library.read_with(cx, |library, _| library.book(key).unwrap().state.clone())
}

fn at(permille: u16) -> ReadingState {
    ReadingState {
        progress_permille: permille,
        ..ReadingState::default()
    }
}

#[gpui_kit::test]
fn a_state_is_written_when_the_second_is_up(cx: &mut TestAppContext) {
    let (_fixture, key, library) = started(cx);
    let keeper = cx.update(|cx| PositionKeeper::entity(key, cx));
    keeper.update(cx, |keeper, cx| keeper.note(at(100), cx));
    assert_eq!(stored(&library, key, cx).progress_permille, 0);
    cx.executor().advance_clock(WRITE_EVERY);
    cx.run_until_parked();
    assert_eq!(stored(&library, key, cx).progress_permille, 100);
}

#[gpui_kit::test]
fn many_states_in_a_second_are_one_write_of_the_newest(cx: &mut TestAppContext) {
    let (_fixture, key, library) = started(cx);
    let keeper = cx.update(|cx| PositionKeeper::entity(key, cx));
    for permille in [100, 200, 300] {
        keeper.update(cx, |keeper, cx| keeper.note(at(permille), cx));
        cx.executor().advance_clock(Duration::from_millis(100));
    }
    cx.executor().advance_clock(WRITE_EVERY);
    cx.run_until_parked();
    assert_eq!(keeper.read_with(cx, |keeper, _| keeper.writes()), 1);
    assert_eq!(stored(&library, key, cx).progress_permille, 300);
}

#[gpui_kit::test]
fn a_flush_writes_at_once(cx: &mut TestAppContext) {
    let (_fixture, key, library) = started(cx);
    let keeper = cx.update(|cx| PositionKeeper::entity(key, cx));
    keeper.update(cx, |keeper, cx| keeper.note(at(400), cx));
    keeper.update(cx, PositionKeeper::flush);
    assert_eq!(stored(&library, key, cx).progress_permille, 400);
    assert!(!keeper.read_with(cx, |keeper, _| keeper.has_pending()));
}

#[gpui_kit::test]
fn a_flush_with_nothing_to_write_writes_nothing(cx: &mut TestAppContext) {
    let (_fixture, key, _library) = started(cx);
    let keeper = cx.update(|cx| PositionKeeper::entity(key, cx));
    keeper.update(cx, PositionKeeper::flush);
    assert_eq!(keeper.read_with(cx, |keeper, _| keeper.writes()), 0);
}

#[gpui_kit::test]
fn a_state_waiting_is_written_when_the_keeper_is_dropped(cx: &mut TestAppContext) {
    let (_fixture, key, library) = started(cx);
    let keeper = cx.update(|cx| PositionKeeper::entity(key, cx));
    keeper.update(cx, |keeper, cx| keeper.note(at(600), cx));
    drop(keeper);
    cx.update(|_| ());
    cx.run_until_parked();
    assert_eq!(stored(&library, key, cx).progress_permille, 600);
}

#[gpui_kit::test]
fn a_state_for_a_book_the_library_lost_is_ignored(cx: &mut TestAppContext) {
    let (_fixture, _key, library) = started(cx);
    let other = BookKey::generate();
    let keeper = cx.update(|cx| PositionKeeper::entity(other, cx));
    keeper.update(cx, |keeper, cx| {
        keeper.note(at(500), cx);
        keeper.flush(cx);
    });
    assert!(library.read_with(cx, |library, _| library.book(other).is_none()));
}
