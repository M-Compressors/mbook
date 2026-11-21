//! What is stored of a position.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::entities::book::snapshot::BookSnapshot;
use crate::entities::catalogue::ReadingState;

/// The reading state for the byte `offset` of the chapter `chapter`.
///
/// `at_end` tells that the end of the chapter is in the window; at the end of
/// the last chapter the progress is the whole book and the book is finished,
/// and it stays finished. `opened_ms` is when this reading began.
///
/// `None` when the book has no such chapter.
pub(crate) fn reading_state(
    snapshot: &BookSnapshot,
    chapter: usize,
    offset: u32,
    at_end: bool,
    previous: &ReadingState,
    opened_ms: u64,
) -> Option<ReadingState> {
    let place = snapshot.place(chapter, offset)?;
    let last = chapter + 1 == snapshot.chapters.len();
    let reached_end = at_end && last;
    let counted = if reached_end {
        snapshot.chapters.get(chapter).map_or(offset, |info| {
            u32::try_from(info.decoded_len).unwrap_or(u32::MAX)
        })
    } else {
        offset
    };
    Some(ReadingState {
        place: Some(place),
        progress_permille: snapshot.progress(chapter, counted),
        opened_ms: Some(opened_ms),
        finished: previous.finished || reached_end,
    })
}

/// The time now, in milliseconds since the Unix epoch.
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}
