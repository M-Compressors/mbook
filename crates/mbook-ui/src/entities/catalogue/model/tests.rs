use gpui_kit::{Entity, TestAppContext};

use super::*;
use crate::testing::Fixture;

fn library(cx: &mut TestAppContext) -> Entity<Library> {
    cx.new(Library::new)
}

fn titles(library: &Entity<Library>, cx: &TestAppContext) -> Vec<String> {
    library.read_with(cx, |library, _| {
        library
            .books()
            .iter()
            .map(|book| book.row.title.clone())
            .collect()
    })
}

#[gpui_kit::test]
fn the_library_is_loading_until_the_background_has_read_the_rows(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.add_book("Dune");
    fixture.install(cx);
    let library = library(cx);
    assert!(library.read_with(cx, |library, _| library.is_loading()));
    assert_eq!(titles(&library, cx).len(), 0);
    cx.run_until_parked();
    assert!(!library.read_with(cx, |library, _| library.is_loading()));
    assert_eq!(titles(&library, cx), ["Dune"]);
}

#[gpui_kit::test]
fn the_library_of_an_empty_catalogue_has_no_books_and_no_error(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.install(cx);
    let library = library(cx);
    cx.run_until_parked();
    library.read_with(cx, |library, _| {
        assert_eq!(library.books().len(), 0);
        assert!(library.error().is_none());
    });
}

#[gpui_kit::test]
fn a_book_is_found_by_its_key(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = fixture.add_book("Dune");
    fixture.install(cx);
    let library = library(cx);
    cx.run_until_parked();
    library.read_with(cx, |library, _| {
        assert_eq!(
            library.book(key).map(|book| book.row.title.as_str()),
            Some("Dune")
        );
        assert!(library.book(BookKey::generate()).is_none());
    });
}

#[gpui_kit::test]
fn a_reload_shows_the_book_added_since(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.add_book("Dune");
    fixture.install(cx);
    let library = library(cx);
    cx.run_until_parked();
    fixture.add_book("Emma");
    library.update(cx, Library::reload);
    assert_eq!(
        titles(&library, cx),
        ["Dune"],
        "the old shelf stays while reading"
    );
    cx.run_until_parked();
    assert_eq!(titles(&library, cx).len(), 2);
}

#[gpui_kit::test]
fn removing_a_book_deletes_its_row_file_and_thumbnail(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = fixture.add_book("Dune");
    let kept = fixture.add_book("Emma");
    fixture.install(cx);
    let library = library(cx);
    cx.run_until_parked();
    library.update(cx, |library, cx| library.remove(key, cx));
    assert_eq!(titles(&library, cx), ["Emma"], "the shelf changes at once");
    assert!(
        fixture.roots.book(key).exists(),
        "the file goes in the background"
    );
    cx.run_until_parked();
    assert!(!fixture.roots.book(key).exists());
    assert!(!fixture.roots.cover(key).exists());
    assert!(fixture.catalogue.book(key).unwrap().is_none());
    assert!(fixture.roots.book(kept).exists());
    assert!(fixture.catalogue.book(kept).unwrap().is_some());
}

#[gpui_kit::test]
fn removing_a_book_whose_file_is_already_gone_is_not_an_error(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = fixture.add_book("Dune");
    std::fs::remove_file(fixture.roots.book(key)).unwrap();
    fixture.install(cx);
    let library = library(cx);
    cx.run_until_parked();
    library.update(cx, |library, cx| library.remove(key, cx));
    cx.run_until_parked();
    library.read_with(cx, |library, _| assert!(library.error().is_none()));
    assert!(fixture.catalogue.book(key).unwrap().is_none());
}

#[gpui_kit::test]
fn a_removed_book_does_not_return_with_a_load_that_was_read_before(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = fixture.add_book("Dune");
    fixture.install(cx);
    let library = library(cx);
    cx.run_until_parked();
    // The reload is asked for before the removal, and reads the old rows.
    library.update(cx, |library, cx| {
        library.reload(cx);
        library.remove(key, cx);
    });
    cx.run_until_parked();
    assert_eq!(titles(&library, cx).len(), 0);
}

#[gpui_kit::test]
fn a_state_is_kept_in_memory_at_once_and_in_the_catalogue_later(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = fixture.add_book("Dune");
    fixture.install(cx);
    let library = library(cx);
    cx.run_until_parked();
    let state = ReadingState {
        progress_permille: 380,
        finished: false,
        ..ReadingState::default()
    };
    library.update(cx, |library, cx| library.set_state(key, state.clone(), cx));
    library.read_with(cx, |library, _| {
        assert_eq!(library.book(key).map(|book| &book.state), Some(&state));
    });
    assert_eq!(
        fixture.catalogue.state(key).unwrap(),
        ReadingState::default()
    );
    cx.run_until_parked();
    assert_eq!(fixture.catalogue.state(key).unwrap(), state);
}

#[gpui_kit::test]
fn the_last_of_two_changes_of_a_state_is_the_one_stored(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = fixture.add_book("Dune");
    fixture.install(cx);
    let library = library(cx);
    cx.run_until_parked();
    let at = |permille| ReadingState {
        progress_permille: permille,
        ..ReadingState::default()
    };
    library.update(cx, |library, cx| {
        library.set_state(key, at(100), cx);
        library.set_state(key, at(200), cx);
    });
    cx.run_until_parked();
    assert_eq!(fixture.catalogue.state(key).unwrap(), at(200));
}

#[gpui_kit::test]
fn a_state_asked_for_during_a_load_survives_it(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = fixture.add_book("Dune");
    fixture.install(cx);
    let library = library(cx);
    cx.run_until_parked();
    let state = ReadingState {
        progress_permille: 500,
        ..ReadingState::default()
    };
    library.update(cx, |library, cx| {
        library.reload(cx);
        library.set_state(key, state.clone(), cx);
    });
    cx.run_until_parked();
    library.read_with(cx, |library, _| {
        assert_eq!(library.book(key).map(|book| &book.state), Some(&state));
    });
}

#[gpui_kit::test]
fn a_state_of_a_book_the_library_does_not_have_is_ignored(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.install(cx);
    let library = library(cx);
    cx.run_until_parked();
    let stranger = BookKey::generate();
    let state = ReadingState {
        finished: true,
        ..ReadingState::default()
    };
    library.update(cx, |library, cx| library.set_state(stranger, state, cx));
    cx.run_until_parked();
    assert_eq!(
        fixture.catalogue.state(stranger).unwrap(),
        ReadingState::default()
    );
}

#[test]
fn a_library_error_keeps_its_cause() {
    use std::error::Error as _;
    let error = LibraryError::from(io::Error::other("disk"));
    assert_eq!(error.to_string(), "the file of a book could not be deleted");
    assert_eq!(error.source().unwrap().to_string(), "disk");
}

#[gpui_kit::test]
fn a_failed_read_is_kept_as_the_error_until_the_next_load_succeeds(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.add_book("Dune");
    fixture.install(cx);
    let library = library(cx);
    cx.run_until_parked();
    library.update(cx, |library, cx| {
        let generation = library.generation + 1;
        library.generation = generation;
        library.loaded(
            generation,
            Err(CatalogueError::storage(std::io::Error::other("disk gone"))),
            cx,
        );
    });
    library.read_with(cx, |library, _| {
        let error = library.error().unwrap();
        assert_eq!(
            crate::features::import_book::error_chain(error),
            "the catalogue could not be read or written: the catalogue database failed: disk gone"
        );
        assert_eq!(library.books().len(), 1, "the books already shown stay");
    });
    library.update(cx, Library::reload);
    cx.run_until_parked();
    library.read_with(cx, |library, _| assert!(library.error().is_none()));
}
