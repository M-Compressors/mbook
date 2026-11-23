//! What the main thread knows of an open book.

use super::landmark::START;
use crate::shared::model::Place;

/// What the main thread knows of an open book.
///
/// Immutable, and shared as an `Arc<BookSnapshot>`. Nothing here touches the
/// file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BookSnapshot {
    /// The title of the book.
    pub(crate) title: String,
    /// The authors, in the order the book lists them.
    pub(crate) authors: Vec<String>,
    /// The language tag of the text.
    pub(crate) language: String,
    /// The chapters, in reading order.
    pub(crate) chapters: Vec<ChapterInfo>,
    /// The table of contents, in the order the book lists it.
    pub(crate) contents: Vec<ContentsEntry>,
    /// The landmarks, in the order the book lists them; empty when the book
    /// has none or its landmarks cannot be read.
    pub(crate) landmarks: Vec<LandmarkPlace>,
    /// The size of the cover in pixels, when the book has one.
    pub(crate) cover: Option<(u32, u32)>,
}

/// One chapter of the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChapterInfo {
    /// The path of the chapter inside the book.
    pub(crate) path: String,
    /// The length of the text in bytes.
    pub(crate) decoded_len: u64,
}

/// One entry of the table of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContentsEntry {
    /// The depth in the tree, starting at 1.
    pub(crate) level: u8,
    /// The title, as plain text.
    pub(crate) title: String,
    /// The index of the chapter the entry leads to, into
    /// [`BookSnapshot::chapters`].
    pub(crate) chapter: usize,
    /// A byte offset into the text of that chapter.
    pub(crate) offset: u32,
}

/// One landmark of the snapshot: a notable place of the book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LandmarkPlace {
    /// What the place is, as the format writes it: `start`, `notes`, …
    pub(crate) kind: String,
    /// The title the book gives it, as plain text; empty when it gives none.
    pub(crate) title: String,
    /// The index of the chapter it leads to, into
    /// [`BookSnapshot::chapters`].
    pub(crate) chapter: usize,
    /// A byte offset into the text of that chapter.
    pub(crate) offset: u32,
}

impl BookSnapshot {
    /// Progress through the book at a place, in thousandths (0 to 1000).
    ///
    /// It is the decoded length of the chapters before the place plus the
    /// offset, over the decoded length of all chapters. An offset past the end
    /// of its chapter counts as the end, and a chapter past the last one as
    /// the end of the book. It is 0 for a book without text.
    pub(crate) fn progress(&self, chapter: usize, offset: u32) -> u16 {
        let total = self.decoded_len();
        if total == 0 {
            return 0;
        }
        let before: u128 = self
            .chapters
            .iter()
            .take(chapter)
            .map(|info| u128::from(info.decoded_len))
            .sum();
        let within = self.chapters.get(chapter).map_or(0, |info| {
            u128::from(offset).min(u128::from(info.decoded_len))
        });
        let thousandths = (before + within) * 1000 / total;
        u16::try_from(thousandths).map_or(1000, |value| value.min(1000))
    }

    /// The chapter and byte offset at this fraction of the decoded text.
    ///
    /// The ends go to the first and last chapter, including an empty one.
    /// Interior positions on a boundary go to the next non-empty chapter.
    /// Returns `None` when there are no chapters.
    pub(crate) fn at_progress(&self, permille: u16) -> Option<(usize, u32)> {
        let last = self.chapters.len().checked_sub(1)?;
        let total = self.decoded_len();
        if permille == 0 || (total == 0 && permille < 1000) {
            return Some((0, 0));
        }
        let mut remaining = (total * u128::from(permille.min(1000))).div_ceil(1000);
        for (index, info) in self.chapters.iter().enumerate() {
            let bytes = u128::from(info.decoded_len);
            if remaining < bytes || index == last {
                return Some((index, u32::try_from(remaining).unwrap_or(u32::MAX)));
            }
            remaining -= bytes;
        }
        None
    }

    /// The sum of the decoded lengths, including books with very large chapters.
    fn decoded_len(&self) -> u128 {
        self.chapters
            .iter()
            .map(|info| u128::from(info.decoded_len))
            .sum()
    }

    /// Where a stored place is now: the index of its chapter and the offset,
    /// clamped to the length of the chapter.
    ///
    /// A place whose chapter is gone is the start of the book. `None` for a
    /// book without chapters.
    pub(crate) fn locate(&self, place: &Place) -> Option<(usize, u32)> {
        self.chapters.first()?;
        let found = self
            .chapters
            .iter()
            .position(|info| info.path == place.chapter);
        Some(found.map_or((0, 0), |index| {
            let len = self.chapters.get(index).map_or(0, |info| info.decoded_len);
            (
                index,
                place.offset.min(u32::try_from(len).unwrap_or(u32::MAX)),
            )
        }))
    }

    /// Where a book that was never read opens: the first landmark of kind
    /// `start`. `None` when the book marks no start.
    pub(crate) fn start(&self) -> Option<Place> {
        self.landmarks
            .iter()
            .find(|landmark| landmark.kind == START)
            .and_then(|landmark| self.place(landmark.chapter, landmark.offset))
    }

    /// The stored form of a position; `None` when the chapter does not exist.
    pub(crate) fn place(&self, chapter: usize, offset: u32) -> Option<Place> {
        self.chapters.get(chapter).map(|info| Place {
            chapter: info.path.clone(),
            offset,
        })
    }

    /// The index into [`contents`](Self::contents) of the entry a position is
    /// in: the entry that is the last at or before it in the book.
    ///
    /// When several entries lead to the same place, the later one in the list.
    pub(crate) fn current_entry(&self, chapter: usize, offset: u32) -> Option<usize> {
        self.contents
            .iter()
            .enumerate()
            .filter(|(_, entry)| (entry.chapter, entry.offset) <= (chapter, offset))
            .max_by_key(|(index, entry)| (entry.chapter, entry.offset, *index))
            .map(|(index, _)| index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(path: &str, decoded_len: u64) -> ChapterInfo {
        ChapterInfo {
            path: path.to_owned(),
            decoded_len,
        }
    }

    fn entry(chapter: usize, offset: u32) -> ContentsEntry {
        ContentsEntry {
            level: 1,
            title: format!("{chapter}:{offset}"),
            chapter,
            offset,
        }
    }

    fn snapshot(chapters: Vec<ChapterInfo>, contents: Vec<ContentsEntry>) -> BookSnapshot {
        BookSnapshot {
            title: "T".to_owned(),
            authors: Vec::new(),
            language: "en".to_owned(),
            chapters,
            contents,
            landmarks: Vec::new(),
            cover: None,
        }
    }

    fn two_chapters() -> BookSnapshot {
        snapshot(vec![info("a.md", 100), info("b.md", 300)], Vec::new())
    }

    fn place(chapter: &str, offset: u32) -> Place {
        Place {
            chapter: chapter.to_owned(),
            offset,
        }
    }

    #[test]
    fn progress_is_zero_at_the_start() {
        assert_eq!(two_chapters().progress(0, 0), 0);
    }

    #[test]
    fn progress_counts_the_chapters_before_and_the_offset() {
        // 100 + 100 of 400.
        assert_eq!(two_chapters().progress(1, 100), 500);
    }

    #[test]
    fn progress_is_the_whole_at_the_end_of_the_last_chapter() {
        assert_eq!(two_chapters().progress(1, 300), 1000);
    }

    #[test]
    fn progress_clamps_an_offset_past_the_end_of_its_chapter() {
        assert_eq!(two_chapters().progress(0, 5000), 250);
    }

    #[test]
    fn progress_of_a_chapter_past_the_last_is_the_whole() {
        assert_eq!(two_chapters().progress(9, 0), 1000);
    }

    #[test]
    fn progress_of_a_book_without_chapters_is_zero() {
        assert_eq!(snapshot(Vec::new(), Vec::new()).progress(0, 0), 0);
    }

    #[test]
    fn progress_of_a_book_without_text_is_zero() {
        assert_eq!(
            snapshot(vec![info("a.md", 0)], Vec::new()).progress(0, 10),
            0
        );
    }

    #[test]
    fn progress_does_not_overflow_on_huge_chapters() {
        let huge = snapshot(
            vec![info("a.md", u64::MAX), info("b.md", u64::MAX)],
            Vec::new(),
        );
        assert_eq!(huge.progress(1, u32::MAX), 500);
    }

    #[test]
    fn progress_seeks_across_chapter_boundaries() {
        assert_eq!(two_chapters().at_progress(250), Some((1, 0)));
        assert_eq!(two_chapters().at_progress(500), Some((1, 100)));
    }

    #[test]
    fn seeking_to_the_edges_includes_empty_chapters() {
        let book = snapshot(
            vec![info("first", 0), info("middle", 100), info("last", 0)],
            Vec::new(),
        );
        assert_eq!(book.at_progress(0), Some((0, 0)));
        assert_eq!(book.at_progress(1000), Some((2, 0)));
    }

    #[test]
    fn a_positive_seek_rounds_up_in_a_short_book() {
        let book = snapshot(vec![info("short", 1)], Vec::new());
        assert_eq!(book.at_progress(1), Some((0, 1)));
    }

    #[test]
    fn no_progress_position_exists_without_chapters() {
        assert_eq!(snapshot(Vec::new(), Vec::new()).at_progress(500), None);
    }

    #[test]
    fn a_book_of_empty_chapters_seeks_to_its_start_or_end() {
        let book = snapshot(vec![info("first", 0), info("last", 0)], Vec::new());
        assert_eq!(book.at_progress(500), Some((0, 0)));
        assert_eq!(book.at_progress(1000), Some((1, 0)));
    }

    #[test]
    fn a_known_chapter_is_located_at_its_offset() {
        assert_eq!(two_chapters().locate(&place("b.md", 42)), Some((1, 42)));
    }

    #[test]
    fn an_offset_past_the_end_is_clamped_to_the_chapter() {
        assert_eq!(two_chapters().locate(&place("a.md", 1000)), Some((0, 100)));
    }

    #[test]
    fn an_offset_at_the_end_is_kept() {
        assert_eq!(two_chapters().locate(&place("a.md", 100)), Some((0, 100)));
    }

    #[test]
    fn a_chapter_that_is_gone_is_the_start_of_the_book() {
        assert_eq!(two_chapters().locate(&place("gone.md", 42)), Some((0, 0)));
    }

    #[test]
    fn nothing_is_located_in_a_book_without_chapters() {
        assert_eq!(
            snapshot(Vec::new(), Vec::new()).locate(&place("a.md", 0)),
            None
        );
    }

    #[test]
    fn a_position_is_stored_by_the_path_of_its_chapter() {
        assert_eq!(two_chapters().place(1, 7), Some(place("b.md", 7)));
    }

    #[test]
    fn a_position_in_a_missing_chapter_has_no_stored_form() {
        assert_eq!(two_chapters().place(2, 7), None);
    }

    fn with_contents() -> BookSnapshot {
        snapshot(
            vec![info("a.md", 100), info("b.md", 100)],
            vec![entry(0, 0), entry(0, 50), entry(1, 0), entry(1, 20)],
        )
    }

    #[test]
    fn the_entry_at_a_position_is_the_one_that_starts_there() {
        assert_eq!(with_contents().current_entry(0, 50), Some(1));
    }

    #[test]
    fn the_entry_before_a_position_is_the_last_one_at_or_before_it() {
        assert_eq!(with_contents().current_entry(0, 49), Some(0));
        assert_eq!(with_contents().current_entry(1, 10), Some(2));
    }

    #[test]
    fn a_position_in_a_later_chapter_belongs_to_that_chapters_entry() {
        assert_eq!(with_contents().current_entry(1, 99), Some(3));
    }

    #[test]
    fn a_position_before_the_first_entry_has_none() {
        let book = snapshot(vec![info("a.md", 100)], vec![entry(0, 10)]);
        assert_eq!(book.current_entry(0, 5), None);
    }

    #[test]
    fn a_book_without_contents_has_no_current_entry() {
        assert_eq!(two_chapters().current_entry(1, 5), None);
    }

    #[test]
    fn of_entries_at_one_place_the_later_one_is_current() {
        let book = snapshot(vec![info("a.md", 100)], vec![entry(0, 0), entry(0, 0)]);
        assert_eq!(book.current_entry(0, 0), Some(1));
    }

    fn landmark(kind: &str, chapter: usize, offset: u32) -> LandmarkPlace {
        LandmarkPlace {
            kind: kind.to_owned(),
            title: String::new(),
            chapter,
            offset,
        }
    }

    #[test]
    fn a_book_that_marks_its_start_opens_there() {
        let mut book = two_chapters();
        book.landmarks = vec![
            landmark("notes", 0, 10),
            landmark(START, 1, 42),
            landmark(START, 0, 0),
        ];
        assert_eq!(book.start(), Some(place("b.md", 42)));
    }

    #[test]
    fn a_book_without_a_start_has_no_place_to_open_at() {
        let mut book = two_chapters();
        assert_eq!(book.start(), None);
        book.landmarks = vec![landmark("notes", 0, 10)];
        assert_eq!(book.start(), None);
    }

    #[test]
    fn a_start_in_a_chapter_that_is_not_there_is_passed_over() {
        let mut book = two_chapters();
        book.landmarks = vec![landmark(START, 7, 0)];
        assert_eq!(book.start(), None);
    }
}
