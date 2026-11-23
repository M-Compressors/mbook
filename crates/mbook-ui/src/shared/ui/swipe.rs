//! A sideways swipe of a finger or of a trackpad, told apart from scrolling.
//! A plain struct, tested without a window.
//!
//! GPUI reports a pan of a finger as the scroll events a trackpad sends,
//! with the phase of the touch, and goes on sending them while the movement
//! coasts after the finger has left. A swipe is the movement from one touch
//! to the next, or to a pause.

use std::time::{Duration, Instant};

/// How far a swipe travels sideways before it counts, in pixels: further
/// than a scrolling finger drifts.
const DISTANCE: f32 = 72.0;

/// How many times further a swipe must have gone sideways than up or down.
const RATIO: f32 = 2.0;

/// A swipe that has been quiet for this long is over.
const PAUSE: Duration = Duration::from_millis(220);

/// Which way content was swiped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    /// Towards the left, as a page of a book is turned forward.
    Left,
    /// Towards the right.
    Right,
}

/// One sideways swipe, between its events. It counts once, however long it
/// coasts; a movement mostly up or down never counts.
#[derive(Debug, Default)]
pub(crate) struct Sideways {
    /// How far the swipe has come across and along, in pixels.
    travelled: (f32, f32),
    /// Whether the swipe has counted.
    spent: bool,
    last: Option<Instant>,
}

impl Sideways {
    /// Takes one scroll event of `x` and `y` pixels at `now`; `begins` when a
    /// finger touched for it. Answers the side swiped to, when the event
    /// makes the movement a swipe.
    pub(crate) fn feed(&mut self, x: f32, y: f32, begins: bool, now: Instant) -> Option<Side> {
        let quiet = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) > PAUSE);
        self.last = Some(now);
        if begins || quiet {
            self.travelled = (0.0, 0.0);
            self.spent = false;
        }
        if self.spent || !x.is_finite() || !y.is_finite() {
            return None;
        }
        self.travelled.0 += x;
        self.travelled.1 += y;
        let (across, along) = self.travelled;
        if across.abs() < DISTANCE || across.abs() < along.abs() * RATIO {
            return None;
        }
        self.spent = true;
        // Content swiped to the left scrolls by a negative amount.
        Some(if across < 0.0 {
            Side::Left
        } else {
            Side::Right
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn a_swipe_to_the_left_and_one_to_the_right_are_told_apart() {
        let start = Instant::now();
        let mut sideways = Sideways::default();
        assert_eq!(sideways.feed(-40.0, 2.0, true, start), None);
        assert_eq!(
            sideways.feed(-40.0, 2.0, false, ms(start, 16)),
            Some(Side::Left)
        );
        assert_eq!(
            sideways.feed(80.0, 0.0, true, ms(start, 500)),
            Some(Side::Right)
        );
    }

    #[test]
    fn a_swipe_counts_once_however_long_it_coasts() {
        let start = Instant::now();
        let mut sideways = Sideways::default();
        assert!(sideways.feed(-100.0, 0.0, true, start).is_some());
        for frame in 1..20 {
            assert_eq!(
                sideways.feed(-30.0, 0.0, false, ms(start, frame * 16)),
                None
            );
        }
    }

    #[test]
    fn scrolling_up_or_down_with_a_drift_is_no_swipe() {
        let start = Instant::now();
        let mut sideways = Sideways::default();
        for frame in 0..20 {
            let side = sideways.feed(-10.0, -60.0, frame == 0, ms(start, frame * 16));
            assert_eq!(side, None);
        }
    }

    #[test]
    fn a_short_sideways_movement_is_no_swipe() {
        let start = Instant::now();
        let mut sideways = Sideways::default();
        assert_eq!(sideways.feed(-DISTANCE + 1.0, 0.0, true, start), None);
    }

    #[test]
    fn a_pause_starts_a_new_swipe() {
        let start = Instant::now();
        let mut sideways = Sideways::default();
        assert!(sideways.feed(-100.0, 0.0, true, start).is_some());
        assert_eq!(
            sideways.feed(-100.0, 0.0, false, ms(start, 1000)),
            Some(Side::Left)
        );
    }

    #[test]
    fn a_movement_that_is_not_a_number_is_ignored() {
        let start = Instant::now();
        let mut sideways = Sideways::default();
        assert_eq!(sideways.feed(f32::NAN, 0.0, true, start), None);
        assert_eq!(
            sideways.feed(0.0, f32::INFINITY, false, ms(start, 16)),
            None
        );
    }
}
