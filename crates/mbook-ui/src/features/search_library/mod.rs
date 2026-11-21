//! Which books the shelf shows, and the words that name them.
//!
//! Pure functions over the books of the library. The page runs [`select`] on
//! the background executor with a generation counter (RFC 0004, "Threads"): a
//! library has thousands of books, and a filter is no work for the main
//! thread.

use std::cmp::Reverse;

use crate::entities::catalogue::ShelfBook;
use crate::shared::model::BookKey;

#[cfg(test)]
mod tests;

/// The part of the library the shelf shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum Section {
    /// Every book.
    #[default]
    All,
    /// Books that were opened and are not finished.
    Reading,
    /// Books marked as finished.
    Finished,
}

impl Section {
    /// Every section, in the order the sidebar lists them.
    pub(crate) const ALL: [Self; 3] = [Self::All, Self::Reading, Self::Finished];

    /// The name of the section, as the sidebar and the heading of the shelf
    /// say it.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "All books",
            Self::Reading => "Reading",
            Self::Finished => "Finished",
        }
    }

    /// Whether `book` belongs to the section.
    pub(crate) fn contains(self, book: &ShelfBook) -> bool {
        match self {
            Self::All => true,
            Self::Reading => book.state.opened_ms.is_some() && !book.state.finished,
            Self::Finished => book.state.finished,
        }
    }
}

/// The order of the shelf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum Sort {
    /// Last opened, or else last added, newest first.
    #[default]
    Recent,
    /// By title, A to Z.
    Title,
    /// By the first author, A to Z; books without an author come last.
    Author,
}

impl Sort {
    /// Every order, in the order the menu lists them.
    pub(crate) const ALL: [Self; 3] = [Self::Recent, Self::Title, Self::Author];

    /// The name of the order, as the menu says it.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Recent => "Recent",
            Self::Title => "Title",
            Self::Author => "Author",
        }
    }
}

/// How many books each section holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Counts {
    /// Every book.
    pub(crate) all: usize,
    /// Books being read.
    pub(crate) reading: usize,
    /// Books that are finished.
    pub(crate) finished: usize,
}

impl Counts {
    /// The number of books in `section`.
    pub(crate) fn of(self, section: Section) -> usize {
        match section {
            Section::All => self.all,
            Section::Reading => self.reading,
            Section::Finished => self.finished,
        }
    }
}

/// The size of each section of `books`.
pub(crate) fn counts(books: &[ShelfBook]) -> Counts {
    Counts {
        all: books.len(),
        reading: books
            .iter()
            .filter(|book| Section::Reading.contains(book))
            .count(),
        finished: books
            .iter()
            .filter(|book| Section::Finished.contains(book))
            .count(),
    }
}

/// The keys of the books of `section` that match `query`, in the order `sort`
/// gives.
///
/// A query matches a book when every one of its words, compared without
/// regard to case (Unicode lowercase), is in the title, in an author or in the
/// series. A query without words matches every book.
pub(crate) fn select(
    books: &[ShelfBook],
    query: &str,
    section: Section,
    sort: Sort,
) -> Vec<BookKey> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut chosen: Vec<&ShelfBook> = books
        .iter()
        .filter(|book| section.contains(book) && matches_words(book, &words))
        .collect();
    match sort {
        Sort::Recent => chosen.sort_by_cached_key(|book| {
            (
                Reverse(book.state.opened_ms.unwrap_or(book.row.added_ms)),
                book.row.title.to_lowercase(),
            )
        }),
        Sort::Title => {
            chosen.sort_by_cached_key(|book| (book.row.title.to_lowercase(), book.row.key));
        }
        Sort::Author => chosen.sort_by_cached_key(|book| {
            (
                // `false` sorts before `true`: books with an author first.
                book.row.authors.is_empty(),
                book.row
                    .authors
                    .first()
                    .map_or_else(String::new, |author| author.to_lowercase()),
                book.row.title.to_lowercase(),
            )
        }),
    }
    chosen.into_iter().map(|book| book.row.key).collect()
}

/// Whether every word is in the title, an author or the series of `book`.
fn matches_words(book: &ShelfBook, words: &[String]) -> bool {
    if words.is_empty() {
        return true;
    }
    let mut haystack = book.row.title.to_lowercase();
    for author in &book.row.authors {
        haystack.push('\n');
        haystack.push_str(&author.to_lowercase());
    }
    if let Some(series) = &book.row.series {
        haystack.push('\n');
        haystack.push_str(&series.to_lowercase());
    }
    words.iter().all(|word| haystack.contains(word.as_str()))
}

/// How far a book has been read, as the shelf tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Progress {
    /// Never opened.
    New,
    /// Opened; the share read, in thousandths.
    Reading(u16),
    /// Marked as finished.
    Finished,
}

impl Progress {
    /// The progress of `book`.
    pub(crate) fn of(book: &ShelfBook) -> Self {
        if book.state.finished {
            Self::Finished
        } else if book.state.opened_ms.is_some() {
            Self::Reading(book.state.progress_permille)
        } else {
            Self::New
        }
    }

    /// The words under a cover: `38%`, `New` or `Finished`.
    pub(crate) fn caption(self) -> String {
        match self {
            Self::New => "New".to_owned(),
            Self::Reading(permille) => progress_label(permille),
            Self::Finished => "Finished".to_owned(),
        }
    }
}

/// The accessible name of a book on the shelf: its title, its authors and its
/// state, such as `The Master and Margarita, by Mikhail Bulgakov, 38 percent
/// read`.
pub(crate) fn shelf_label(book: &ShelfBook) -> String {
    let mut label = book.row.title.clone();
    if !book.row.authors.is_empty() {
        label.push_str(", by ");
        label.push_str(&book.row.authors.join(", "));
    }
    match Progress::of(book) {
        Progress::New => label.push_str(", new"),
        Progress::Reading(permille) => {
            label = format!("{label}, {} percent read", permille.min(1000) / 10);
        }
        Progress::Finished => label.push_str(", finished"),
    }
    label
}

/// The share read as a whole percentage: `38%` for 380 thousandths.
pub(crate) fn progress_label(permille: u16) -> String {
    format!("{}%", permille.min(1000) / 10)
}

/// The book the "Continue reading" card shows: the one opened last that is not
/// finished.
pub(crate) fn continue_reading(books: &[ShelfBook]) -> Option<BookKey> {
    books
        .iter()
        .filter(|book| Section::Reading.contains(book))
        .max_by_key(|book| (book.state.opened_ms, Reverse(book.row.key)))
        .map(|book| book.row.key)
}
