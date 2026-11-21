use super::*;
use crate::entities::catalogue::{BookRow, ReadingState};

fn book(title: &str, authors: &[&str]) -> ShelfBook {
    ShelfBook {
        row: BookRow {
            key: BookKey::generate(),
            id: format!("urn:test:{title}"),
            title: title.to_owned(),
            authors: authors.iter().map(|&author| author.to_owned()).collect(),
            language: "en".to_owned(),
            series: None,
            file_len: 1,
            cover: None,
            added_ms: 1,
            source_format: None,
        },
        state: ReadingState::default(),
    }
}

fn opened(mut book: ShelfBook, at: u64, permille: u16) -> ShelfBook {
    book.state.opened_ms = Some(at);
    book.state.progress_permille = permille;
    book
}

fn finished(mut book: ShelfBook) -> ShelfBook {
    book.state.opened_ms = Some(5);
    book.state.finished = true;
    book.state.progress_permille = 1000;
    book
}

fn added(mut book: ShelfBook, at: u64) -> ShelfBook {
    book.row.added_ms = at;
    book
}

fn titles(books: &[ShelfBook], keys: &[BookKey]) -> Vec<String> {
    keys.iter()
        .map(|key| {
            books
                .iter()
                .find(|book| book.row.key == *key)
                .map_or_else(String::new, |book| book.row.title.clone())
        })
        .collect()
}

fn shelf() -> Vec<ShelfBook> {
    vec![
        book("Dune", &["Frank Herbert"]),
        opened(book("Emma", &["Jane Austen"]), 10, 380),
        finished(book("Ulysses", &["James Joyce"])),
        book("Anonymous Tale", &[]),
    ]
}

#[test]
fn an_empty_query_matches_every_book() {
    let books = shelf();
    let keys = select(&books, "", Section::All, Sort::Title);
    assert_eq!(keys.len(), 4);
    assert_eq!(select(&books, "   \t", Section::All, Sort::Title), keys);
}

#[test]
fn a_query_matches_without_regard_to_case() {
    let books = shelf();
    let keys = select(&books, "DUNE", Section::All, Sort::Title);
    assert_eq!(titles(&books, &keys), ["Dune"]);
}

#[test]
fn a_query_matches_beyond_ascii_case() {
    let books = vec![book("Мастер и Маргарита", &["Михаил Булгаков"])];
    let keys = select(&books, "маргарита БУЛГАКОВ", Section::All, Sort::Title);
    assert_eq!(keys.len(), 1);
}

#[test]
fn a_query_matches_an_author() {
    let books = shelf();
    let keys = select(&books, "austen", Section::All, Sort::Title);
    assert_eq!(titles(&books, &keys), ["Emma"]);
}

#[test]
fn a_query_matches_any_author_of_a_book() {
    let books = vec![book("Good Omens", &["Terry Pratchett", "Neil Gaiman"])];
    assert_eq!(select(&books, "gaiman", Section::All, Sort::Title).len(), 1);
}

#[test]
fn a_query_matches_the_series() {
    let mut books = shelf();
    books[0].row.series = Some("Dune Chronicles".to_owned());
    books[1].row.series = Some("Chronicles".to_owned());
    let keys = select(&books, "chronicles", Section::All, Sort::Title);
    assert_eq!(titles(&books, &keys), ["Dune", "Emma"]);
}

#[test]
fn every_word_of_a_query_must_match() {
    let books = shelf();
    let keys = select(&books, "frank dune", Section::All, Sort::Title);
    assert_eq!(titles(&books, &keys), ["Dune"]);
    assert_eq!(
        select(&books, "frank emma", Section::All, Sort::Title),
        Vec::new()
    );
}

#[test]
fn the_words_of_a_query_may_come_from_the_title_and_the_author() {
    let books = shelf();
    let keys = select(&books, "joyce ulysses", Section::All, Sort::Title);
    assert_eq!(titles(&books, &keys), ["Ulysses"]);
}

#[test]
fn a_word_does_not_match_across_fields() {
    let books = vec![book("Ann", &["Bob"])];
    assert_eq!(
        select(&books, "annbob", Section::All, Sort::Title),
        Vec::new()
    );
}

#[test]
fn a_query_without_a_match_gives_nothing() {
    let books = shelf();
    assert_eq!(select(&books, "zzz", Section::All, Sort::Title), Vec::new());
}

#[test]
fn the_reading_section_holds_opened_books_that_are_not_finished() {
    let books = shelf();
    let keys = select(&books, "", Section::Reading, Sort::Title);
    assert_eq!(titles(&books, &keys), ["Emma"]);
}

#[test]
fn the_finished_section_holds_the_books_marked_finished() {
    let books = shelf();
    let keys = select(&books, "", Section::Finished, Sort::Title);
    assert_eq!(titles(&books, &keys), ["Ulysses"]);
}

#[test]
fn a_query_narrows_the_section() {
    let books = shelf();
    assert_eq!(
        select(&books, "dune", Section::Reading, Sort::Title),
        Vec::new()
    );
}

#[test]
fn recent_puts_the_last_opened_first_and_a_book_never_opened_by_its_time_added() {
    let books = vec![
        added(book("Old", &[]), 100),
        opened(added(book("Opened", &[]), 1), 500, 10),
        added(book("Fresh", &[]), 900),
        opened(added(book("Opened early", &[]), 1), 200, 10),
    ];
    let keys = select(&books, "", Section::All, Sort::Recent);
    assert_eq!(
        titles(&books, &keys),
        ["Fresh", "Opened", "Opened early", "Old"]
    );
}

#[test]
fn recent_breaks_a_tie_by_title() {
    let books = vec![
        added(book("b", &[]), 7),
        added(book("A", &[]), 7),
        added(book("c", &[]), 7),
    ];
    let keys = select(&books, "", Section::All, Sort::Recent);
    assert_eq!(titles(&books, &keys), ["A", "b", "c"]);
}

#[test]
fn title_sorts_without_regard_to_case() {
    let books = vec![book("banana", &[]), book("Apple", &[]), book("cherry", &[])];
    let keys = select(&books, "", Section::All, Sort::Title);
    assert_eq!(titles(&books, &keys), ["Apple", "banana", "cherry"]);
}

#[test]
fn author_sorts_by_the_first_author_and_puts_books_without_one_last() {
    let books = vec![
        book("Nameless", &[]),
        book("Two", &["Zed"]),
        book("One", &["amy"]),
        book("Another by Amy", &["Amy"]),
    ];
    let keys = select(&books, "", Section::All, Sort::Author);
    assert_eq!(
        titles(&books, &keys),
        ["Another by Amy", "One", "Two", "Nameless"]
    );
}

#[test]
fn a_shelf_without_books_selects_nothing() {
    assert_eq!(select(&[], "x", Section::All, Sort::Recent), Vec::new());
}

#[test]
fn the_counts_follow_the_sections() {
    let counts = counts(&shelf());
    assert_eq!(
        counts,
        Counts {
            all: 4,
            reading: 1,
            finished: 1
        }
    );
    assert_eq!(counts.of(Section::All), 4);
    assert_eq!(counts.of(Section::Reading), 1);
    assert_eq!(counts.of(Section::Finished), 1);
}

#[test]
fn the_label_of_a_book_that_is_being_read_gives_the_percentage() {
    let book = opened(
        book("The Master and Margarita", &["Mikhail Bulgakov"]),
        5,
        380,
    );
    assert_eq!(
        shelf_label(&book),
        "The Master and Margarita, by Mikhail Bulgakov, 38 percent read"
    );
}

#[test]
fn the_label_of_a_book_never_opened_says_it_is_new() {
    let book = book("The Master and Margarita", &["Mikhail Bulgakov"]);
    assert_eq!(
        shelf_label(&book),
        "The Master and Margarita, by Mikhail Bulgakov, new"
    );
}

#[test]
fn the_label_of_a_finished_book_says_so() {
    let book = finished(book("Dune", &["Frank Herbert"]));
    assert_eq!(shelf_label(&book), "Dune, by Frank Herbert, finished");
}

#[test]
fn the_label_names_every_author_and_omits_a_missing_one() {
    assert_eq!(
        shelf_label(&book("Good Omens", &["Terry Pratchett", "Neil Gaiman"])),
        "Good Omens, by Terry Pratchett, Neil Gaiman, new"
    );
    assert_eq!(shelf_label(&book("Anonymous", &[])), "Anonymous, new");
}

#[test]
fn a_progress_label_is_a_whole_percentage() {
    assert_eq!(progress_label(0), "0%");
    assert_eq!(progress_label(380), "38%");
    assert_eq!(progress_label(389), "38%");
    assert_eq!(progress_label(1000), "100%");
}

#[test]
fn a_progress_label_never_exceeds_a_hundred_percent() {
    assert_eq!(progress_label(u16::MAX), "100%");
}

#[test]
fn the_caption_under_a_cover_is_a_percentage_new_or_finished() {
    assert_eq!(Progress::New.caption(), "New");
    assert_eq!(Progress::Reading(380).caption(), "38%");
    assert_eq!(Progress::Finished.caption(), "Finished");
}

#[test]
fn the_book_to_continue_is_the_last_opened_one_that_is_not_finished() {
    let books = vec![
        opened(book("Early", &[]), 10, 100),
        opened(book("Late", &[]), 90, 100),
        finished(book("Done", &[])),
        book("New", &[]),
    ];
    let key = continue_reading(&books).expect("a book is being read");
    assert_eq!(titles(&books, &[key]), ["Late"]);
}

#[test]
fn there_is_nothing_to_continue_when_no_book_is_being_read() {
    assert_eq!(continue_reading(&[book("New", &[])]), None);
    assert_eq!(continue_reading(&[finished(book("Done", &[]))]), None);
    assert_eq!(continue_reading(&[]), None);
}
