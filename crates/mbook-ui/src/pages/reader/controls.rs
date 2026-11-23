//! When the controls of the reader show, and the words they carry. Plain
//! functions, tested without a window.

use std::time::Duration;

use crate::entities::book::snapshot::BookSnapshot;

/// How long the controls stay after the last sign of the user.
pub(super) const HIDE_AFTER: Duration = Duration::from_secs(4);

/// What the timer of the controls does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Plan {
    /// Hide them.
    HideNow,
    /// Look again after this long.
    Check(Duration),
}

/// The next step for controls whose user was last seen `idle` ago. While
/// something `held` them (the pointer over them, the focus inside them, a
/// panel open, or the setting that keeps them), they stay, and the user is
/// counted as seen.
pub(super) fn plan(idle: Duration, held: bool) -> Plan {
    if held {
        Plan::Check(HIDE_AFTER)
    } else if idle >= HIDE_AFTER {
        Plan::HideNow
    } else {
        Plan::Check(HIDE_AFTER.checked_sub(idle).unwrap_or_default())
    }
}

/// "Chapter title, 38%": what the line under the text says.
pub(super) fn status_text(title: &str, permille: u16) -> String {
    format!("{title}, {}%", permille.min(1000) / 10)
}

/// The title of the chapter at a place: the entry of the contents that
/// starts at or before it, or "Chapter n".
pub(super) fn chapter_title(snapshot: &BookSnapshot, chapter: usize, offset: u32) -> String {
    snapshot
        .current_entry(chapter, offset)
        .and_then(|entry| snapshot.contents.get(entry))
        .filter(|entry| entry.chapter == chapter && !entry.title.trim().is_empty())
        .map_or_else(
            || format!("Chapter {}", chapter + 1),
            |entry| entry.title.trim().to_owned(),
        )
}

/// A slider position as thousandths of the whole book.
pub(super) fn slider_progress(value: f32) -> u16 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the rounded slider position is clamped to the range of progress"
    )]
    let permille = (value.clamp(0.0, 100.0) * 10.0).round() as u16;
    permille
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::book::snapshot::{ChapterInfo, ContentsEntry};

    fn snapshot() -> BookSnapshot {
        let info = |path: &str| ChapterInfo {
            path: path.to_owned(),
            decoded_len: 100,
        };
        let entry = |chapter: usize, offset: u32, title: &str| ContentsEntry {
            level: 1,
            title: title.to_owned(),
            chapter,
            offset,
        };
        BookSnapshot {
            title: "T".to_owned(),
            authors: Vec::new(),
            language: "en".to_owned(),
            chapters: vec![info("a.md"), info("b.md"), info("c.md")],
            contents: vec![
                entry(0, 0, "One"),
                entry(0, 50, " Two "),
                entry(2, 0, "Four"),
            ],
            landmarks: Vec::new(),
            cover: None,
        }
    }

    #[test]
    fn controls_stay_until_the_user_has_been_idle_for_a_while() {
        assert_eq!(
            plan(Duration::from_secs(1), false),
            Plan::Check(Duration::from_secs(3))
        );
    }

    #[test]
    fn controls_hide_when_the_time_is_up() {
        assert_eq!(plan(HIDE_AFTER, false), Plan::HideNow);
        assert_eq!(plan(HIDE_AFTER * 2, false), Plan::HideNow);
    }

    #[test]
    fn controls_that_something_holds_do_not_hide() {
        assert_eq!(plan(HIDE_AFTER * 10, true), Plan::Check(HIDE_AFTER));
    }

    #[test]
    fn the_status_is_the_title_and_the_percentage() {
        assert_eq!(status_text("Two", 380), "Two, 38%");
    }

    #[test]
    fn the_percentage_rounds_down_and_stops_at_the_whole() {
        assert_eq!(status_text("A", 999), "A, 99%");
        assert_eq!(status_text("A", 5000), "A, 100%");
        assert_eq!(status_text("A", 0), "A, 0%");
    }

    #[test]
    fn a_place_is_titled_by_the_entry_it_is_in() {
        assert_eq!(chapter_title(&snapshot(), 0, 10), "One");
        assert_eq!(chapter_title(&snapshot(), 0, 60), "Two");
    }

    #[test]
    fn a_chapter_without_an_entry_of_its_own_is_numbered() {
        assert_eq!(chapter_title(&snapshot(), 1, 0), "Chapter 2");
    }

    #[test]
    fn a_chapter_after_the_last_entry_that_has_one_is_titled_by_it() {
        assert_eq!(chapter_title(&snapshot(), 2, 5), "Four");
    }

    #[test]
    fn slider_progress_rounds_and_stays_within_the_book() {
        assert_eq!(slider_progress(50.25), 503);
        assert_eq!(slider_progress(120.0), 1000);
        assert_eq!(slider_progress(-10.0), 0);
    }
}
