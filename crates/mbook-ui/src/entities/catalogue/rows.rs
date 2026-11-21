//! The rows of the `books` and `state` tables, and how they are encoded.
//!
//! Both are encoded with the value types `redb` supports itself: tuples,
//! strings, integers, booleans and options of them (RFC 0004, "The
//! catalogue"). Nothing is serialised by a second library.
//!
//! `books`, keyed by the `u128` of the [`BookKey`]:
//!
//! | # | Field | Type |
//! | --- | --- | --- |
//! | 0 | `id` | `&str` |
//! | 1 | `title` | `&str` |
//! | 2 | `authors` | `&str`: the names joined by U+001F |
//! | 3 | `language` | `&str` |
//! | 4 | `series` | `Option<&str>` |
//! | 5 | `file_len` | `u64` |
//! | 6 | `cover` | `Option<(u32, u32)>`: width and height |
//! | 7 | `added_ms` | `u64` |
//! | 8 | `source_format` | `Option<&str>` |
//!
//! `state`, keyed by the `u128` of the [`BookKey`]:
//!
//! | # | Field | Type |
//! | --- | --- | --- |
//! | 0 | `place` | `Option<(&str, u32)>`: chapter path and byte offset |
//! | 1 | `progress_permille` | `u16` |
//! | 2 | `opened_ms` | `Option<u64>` |
//! | 3 | `finished` | `bool` |

use std::io::{Read, Seek};

use crate::shared::model::{BookKey, Place};

/// The separator of the authors in a stored row. A control character, which
/// a name does not hold; one that does is stored with a space instead.
const AUTHOR_SEPARATOR: char = '\u{1f}';

/// The stored form of a row of `books`.
pub(super) type BookValue<'a> = (
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    Option<&'a str>,
    u64,
    Option<(u32, u32)>,
    u64,
    Option<&'a str>,
);

/// The stored form of a row of `state`.
pub(super) type StateValue<'a> = (Option<(&'a str, u32)>, u16, Option<u64>, bool);

/// The most a progress can be: the whole book.
const WHOLE: u16 = 1000;

/// A book of the library, as the shelf shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BookRow {
    /// The book in the library.
    pub(crate) key: BookKey,
    /// The `id` of the book's metadata.
    pub(crate) id: String,
    /// The title.
    pub(crate) title: String,
    /// The authors, in the order of the file.
    pub(crate) authors: Vec<String>,
    /// The main language, a tag.
    pub(crate) language: String,
    /// The name of the series, with its index when it has one, as shown on
    /// the shelf: `Name #2`.
    pub(crate) series: Option<String>,
    /// The size of the file in bytes.
    pub(crate) file_len: u64,
    /// The size of the cover in pixels; `None` for a book without one.
    pub(crate) cover: Option<(u32, u32)>,
    /// When the book was added, in milliseconds since the Unix epoch.
    pub(crate) added_ms: u64,
    /// The format the book was converted from: `epub`, `fb2`, and so on.
    /// `None` for a file that was an mbook already.
    pub(crate) source_format: Option<String>,
}

impl BookRow {
    /// The row of a book that is open: from its metadata and its cover entry.
    ///
    /// Nothing is read from the source; the tables of the reader are enough.
    pub(crate) fn from_reader<R: Read + Seek>(
        key: BookKey,
        reader: &mbook::BookReader<R>,
        file_len: u64,
        added_ms: u64,
        source_format: Option<String>,
    ) -> Self {
        let meta = reader.meta();
        Self {
            key,
            id: meta.id().to_owned(),
            title: meta.title().to_owned(),
            authors: meta.authors().map(str::to_owned).collect(),
            language: meta.language().to_owned(),
            series: meta.series().map(|series| match series.index {
                Some(index) => format!("{} #{index}", series.name),
                None => series.name.to_owned(),
            }),
            file_len,
            cover: reader
                .cover()
                .map(|cover| (cover.size.width, cover.size.height)),
            added_ms,
            source_format,
        }
    }

    /// The stored authors: the names joined by the separator.
    pub(super) fn joined_authors(&self) -> String {
        let names: Vec<String> = self
            .authors
            .iter()
            .filter(|name| !name.is_empty())
            .map(|name| name.replace(AUTHOR_SEPARATOR, " "))
            .collect();
        names.join(AUTHOR_SEPARATOR.encode_utf8(&mut [0; 4]))
    }

    /// The row a stored value stands for.
    pub(super) fn decode(key: u128, value: BookValue<'_>) -> Self {
        let (id, title, authors, language, series, file_len, cover, added_ms, source_format) =
            value;
        Self {
            key: BookKey::from_u128(key),
            id: id.to_owned(),
            title: title.to_owned(),
            authors: authors
                .split(AUTHOR_SEPARATOR)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect(),
            language: language.to_owned(),
            series: series.map(str::to_owned),
            file_len,
            cover,
            added_ms,
            source_format: source_format.map(str::to_owned),
        }
    }
}

/// How far a book has been read, and where.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ReadingState {
    /// The last place read; `None` for a book not opened yet.
    pub(crate) place: Option<Place>,
    /// How much of the book is behind the place, from 0 to 1000.
    pub(crate) progress_permille: u16,
    /// When the book was last opened, in milliseconds since the Unix epoch.
    pub(crate) opened_ms: Option<u64>,
    /// Whether the reader marked the book as finished.
    pub(crate) finished: bool,
}

impl ReadingState {
    /// The state a stored value stands for. A progress above the whole book
    /// is brought down to it.
    pub(super) fn decode(value: StateValue<'_>) -> Self {
        let (place, progress, opened_ms, finished) = value;
        Self {
            place: place.map(|(chapter, offset)| Place {
                chapter: chapter.to_owned(),
                offset,
            }),
            progress_permille: progress.min(WHOLE),
            opened_ms,
            finished,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(authors: &[&str]) -> BookRow {
        BookRow {
            key: BookKey::generate(),
            id: "urn:x".to_owned(),
            title: "T".to_owned(),
            authors: authors.iter().map(|name| (*name).to_owned()).collect(),
            language: "en".to_owned(),
            series: None,
            file_len: 1,
            cover: None,
            added_ms: 2,
            source_format: None,
        }
    }

    #[test]
    fn authors_are_joined_by_the_unit_separator() {
        assert_eq!(row(&["A", "B"]).joined_authors(), "A\u{1f}B");
    }

    #[test]
    fn no_authors_are_stored_as_an_empty_text() {
        assert_eq!(row(&[]).joined_authors(), "");
    }

    #[test]
    fn a_separator_inside_a_name_is_replaced_by_a_space() {
        assert_eq!(row(&["A\u{1f}B"]).joined_authors(), "A B");
    }

    #[test]
    fn an_empty_name_is_not_stored() {
        assert_eq!(row(&["A", "", "B"]).joined_authors(), "A\u{1f}B");
    }

    #[test]
    fn authors_survive_their_stored_form() {
        let source = row(&["Ann", "Bo"]);
        let joined = source.joined_authors();
        let decoded = BookRow::decode(
            source.key.as_u128(),
            ("urn:x", "T", &joined, "en", None, 1, None, 2, None),
        );
        assert_eq!(decoded, source);
    }

    #[test]
    fn an_empty_authors_text_gives_no_authors() {
        let decoded = BookRow::decode(0, ("", "", "", "", None, 0, None, 0, None));
        assert_eq!(decoded.authors, Vec::<String>::new());
    }

    #[test]
    fn a_progress_beyond_the_whole_book_is_brought_down() {
        let state = ReadingState::decode((None, 1500, None, false));
        assert_eq!(state.progress_permille, 1000);
    }

    #[test]
    fn a_default_state_is_unread() {
        let state = ReadingState::default();
        assert_eq!(state.place, None);
        assert_eq!(state.progress_permille, 0);
        assert_eq!(state.opened_ms, None);
        assert!(!state.finished);
    }
}
