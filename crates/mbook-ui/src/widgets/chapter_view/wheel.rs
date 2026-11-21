//! A wheel or a swipe of a trackpad, read as the turn of a page. Plain
//! functions, tested without a window.
//!
//! * A wheel with notches reports lines: every notch turns a page, but not
//!   more often than a page can be looked at.
//! * A trackpad reports pixels, and goes on reporting them after the fingers
//!   have left it: one swipe turns one page, however long it coasts. The next
//!   swipe starts when the fingers touch again, or after a pause.

use std::time::{Duration, Instant};

use super::Direction;

/// How far a swipe travels before it turns the page, in pixels.
const SWIPE: f32 = 36.0;
/// The least time between two pages turned by a wheel with notches.
const BETWEEN_NOTCHES: Duration = Duration::from_millis(140);
/// A swipe that has been quiet for this long is over.
const PAUSE: Duration = Duration::from_millis(220);

/// What the wheel reported, along the axis it moved most: lines of a wheel
/// with notches, or pixels of a trackpad. Negative moves towards the end of
/// the text, as a scrolled text would.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Travel {
    Lines(f32),
    Pixels(f32),
}

/// The wheel of one view, between its events.
#[derive(Debug, Default)]
pub(super) struct Wheel {
    /// How far the swipe has come, in pixels.
    travelled: f32,
    /// Whether the swipe has turned its page.
    spent: bool,
    last: Option<Instant>,
    turned: Option<Instant>,
}

impl Wheel {
    /// Takes one event at `now`; `begins` when fingers touched the trackpad
    /// for it. Answers the way to turn, when the event turns a page.
    pub(super) fn feed(&mut self, travel: Travel, begins: bool, now: Instant) -> Option<Direction> {
        let quiet = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) > PAUSE);
        self.last = Some(now);
        match travel {
            Travel::Lines(lines) => {
                self.travelled = 0.0;
                self.spent = false;
                let direction = direction(lines)?;
                let rested = self
                    .turned
                    .is_none_or(|turned| now.saturating_duration_since(turned) >= BETWEEN_NOTCHES);
                rested.then(|| {
                    self.turned = Some(now);
                    direction
                })
            }
            Travel::Pixels(pixels) => {
                if begins || quiet {
                    self.travelled = 0.0;
                    self.spent = false;
                }
                if self.spent || !pixels.is_finite() {
                    return None;
                }
                self.travelled += pixels;
                if self.travelled.abs() < SWIPE {
                    return None;
                }
                self.spent = true;
                self.turned = Some(now);
                direction(self.travelled)
            }
        }
    }
}

/// The way a movement turns: towards the end when it is negative.
fn direction(amount: f32) -> Option<Direction> {
    if amount < 0.0 {
        Some(Direction::Forward)
    } else if amount > 0.0 {
        Some(Direction::Back)
    } else {
        None
    }
}

/// What an event of `x` and `y` reports along the axis it moved most.
pub(super) fn dominant(x: f32, y: f32) -> f32 {
    if x.abs() > y.abs() { x } else { y }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn a_notch_down_turns_forward_and_a_notch_up_back() {
        let start = Instant::now();
        let mut wheel = Wheel::default();
        assert_eq!(
            wheel.feed(Travel::Lines(-1.0), false, start),
            Some(Direction::Forward)
        );
        assert_eq!(
            wheel.feed(Travel::Lines(1.0), false, ms(start, 500)),
            Some(Direction::Back)
        );
    }

    #[test]
    fn notches_that_follow_at_once_turn_one_page() {
        let start = Instant::now();
        let mut wheel = Wheel::default();
        assert!(wheel.feed(Travel::Lines(-1.0), false, start).is_some());
        assert_eq!(wheel.feed(Travel::Lines(-1.0), false, ms(start, 30)), None);
        assert!(
            wheel
                .feed(Travel::Lines(-1.0), false, ms(start, 200))
                .is_some()
        );
    }

    #[test]
    fn a_notch_that_moves_nothing_turns_nothing() {
        let mut wheel = Wheel::default();
        assert_eq!(wheel.feed(Travel::Lines(0.0), false, Instant::now()), None);
    }

    #[test]
    fn a_swipe_turns_once_it_has_travelled_far_enough() {
        let start = Instant::now();
        let mut wheel = Wheel::default();
        assert_eq!(wheel.feed(Travel::Pixels(-20.0), true, start), None);
        assert_eq!(
            wheel.feed(Travel::Pixels(-20.0), false, ms(start, 16)),
            Some(Direction::Forward)
        );
    }

    #[test]
    fn a_swipe_that_coasts_turns_one_page() {
        let start = Instant::now();
        let mut wheel = Wheel::default();
        assert!(wheel.feed(Travel::Pixels(-80.0), true, start).is_some());
        for frame in 1..40 {
            assert_eq!(
                wheel.feed(Travel::Pixels(-30.0), false, ms(start, frame * 16)),
                None
            );
        }
    }

    #[test]
    fn fingers_that_touch_again_start_a_new_swipe() {
        let start = Instant::now();
        let mut wheel = Wheel::default();
        assert!(wheel.feed(Travel::Pixels(80.0), true, start).is_some());
        assert_eq!(
            wheel.feed(Travel::Pixels(80.0), true, ms(start, 60)),
            Some(Direction::Back)
        );
    }

    #[test]
    fn a_pause_ends_a_swipe() {
        let start = Instant::now();
        let mut wheel = Wheel::default();
        assert_eq!(wheel.feed(Travel::Pixels(-30.0), false, start), None);
        // The rest of the way is not added to a swipe that ended.
        assert_eq!(
            wheel.feed(Travel::Pixels(-30.0), false, ms(start, 600)),
            None
        );
        assert!(
            wheel
                .feed(Travel::Pixels(-30.0), false, ms(start, 620))
                .is_some()
        );
    }

    #[test]
    fn a_swipe_back_and_forth_turns_nothing() {
        let start = Instant::now();
        let mut wheel = Wheel::default();
        assert_eq!(wheel.feed(Travel::Pixels(-30.0), true, start), None);
        assert_eq!(wheel.feed(Travel::Pixels(30.0), false, ms(start, 16)), None);
        assert_eq!(
            wheel.feed(Travel::Pixels(f32::NAN), false, ms(start, 32)),
            None
        );
    }

    #[test]
    fn the_axis_that_moved_most_counts() {
        assert!((dominant(-40.0, 3.0) + 40.0).abs() < f32::EPSILON);
        assert!((dominant(2.0, -9.0) + 9.0).abs() < f32::EPSILON);
        assert!(dominant(0.0, 0.0).abs() < f32::EPSILON);
    }
}
