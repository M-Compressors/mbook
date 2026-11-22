//! How the reader is moved: its actions, its keys and its touch zones.

use std::rc::Rc;

use gpui_kit::{Action, App, Global, KeyBinding, KeyBindingContextPredicate, actions};

use crate::shared::ui::search_bar::KEY_CONTEXT as FIND_BAR;

/// The key context of the reader.
pub(super) const CONTEXT: &str = "Reader";
/// Added to the key context while the controls are hidden, so that Tab can
/// bring them back instead of moving the focus to something that cannot be
/// seen.
pub(super) const HIDDEN: &str = "ControlsHidden";
/// The slider owns the navigation keys while it has focus.
pub(super) const SCRUBBER: &str = "ReadingPosition";

actions!(
    reader,
    [
        /// Moves a screen towards the end.
        ScreenForward,
        /// Moves a screen towards the start.
        ScreenBack,
        /// Goes to the start of the chapter.
        ChapterStart,
        /// Goes to the end of the chapter.
        ChapterEnd,
        /// Opens the previous chapter.
        PreviousChapter,
        /// Opens the next chapter.
        NextChapter,
        /// Brings the hidden controls back and puts the focus on them.
        ShowControls,
    ]
);

/// Marks that the keys are bound, so that a second reader does not bind them
/// again.
struct Bound;

impl Global for Bound {}

/// Binds the keys of the reader, once. `secondary-` is Command on macOS and
/// Control elsewhere.
///
/// On macOS `cmd-[` is also the key that goes back to the previous screen;
/// inside the reader the key of the reader is the more specific, so it moves
/// to the previous chapter there.
pub(crate) fn bind(cx: &mut App) {
    if cx.has_global::<Bound>() {
        return;
    }
    cx.set_global(Bound);
    let keys: [(&str, Box<dyn Action>); 12] = [
        ("space", Box::new(ScreenForward)),
        ("pagedown", Box::new(ScreenForward)),
        ("down", Box::new(ScreenForward)),
        ("right", Box::new(ScreenForward)),
        ("shift-space", Box::new(ScreenBack)),
        ("pageup", Box::new(ScreenBack)),
        ("up", Box::new(ScreenBack)),
        ("left", Box::new(ScreenBack)),
        ("home", Box::new(ChapterStart)),
        ("end", Box::new(ChapterEnd)),
        ("secondary-[", Box::new(PreviousChapter)),
        ("secondary-]", Box::new(NextChapter)),
    ];
    let context =
        KeyBindingContextPredicate::parse(&format!("{CONTEXT} && !{SCRUBBER} && !{FIND_BAR}"))
            .ok()
            .map(Rc::new);
    // A key the platform cannot parse is left out; the controls still reach
    // every action.
    let bindings: Vec<KeyBinding> = keys
        .into_iter()
        .filter_map(|(keys, action)| {
            KeyBinding::load(keys, action, context.clone(), false, None, cx).ok()
        })
        .collect();
    let hidden = KeyBindingContextPredicate::parse(&format!("{CONTEXT} && {HIDDEN}"))
        .ok()
        .map(Rc::new);
    let tab: Vec<KeyBinding> = ["tab", "shift-tab"]
        .into_iter()
        .filter_map(|keys| {
            KeyBinding::load(
                keys,
                Box::new(ShowControls),
                hidden.clone(),
                false,
                None,
                cx,
            )
            .ok()
        })
        .collect();
    cx.bind_keys(bindings.into_iter().chain(tab));
}

/// Which third of the text a tap fell on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Zone {
    /// The left edge: a screen back.
    Left,
    /// The middle: the controls.
    Middle,
    /// The right edge: a screen forward.
    Right,
}

/// How much of the width each edge takes.
const EDGE: f32 = 0.25;

/// The zone of a tap at `x`, in a body that starts at `left` and is `width`
/// wide. A body without width has only a middle.
pub(super) fn zone(x: f32, left: f32, width: f32) -> Zone {
    if !(width.is_finite() && width > 0.0 && x.is_finite() && left.is_finite()) {
        return Zone::Middle;
    }
    let at = (x - left) / width;
    if at < EDGE {
        Zone::Left
    } else if at > 1.0 - EDGE {
        Zone::Right
    } else {
        Zone::Middle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tap_on_the_left_edge_goes_back() {
        assert_eq!(zone(10.0, 0.0, 400.0), Zone::Left);
    }

    #[test]
    fn a_tap_on_the_right_edge_goes_forward() {
        assert_eq!(zone(390.0, 0.0, 400.0), Zone::Right);
    }

    #[test]
    fn a_tap_in_the_middle_shows_the_controls() {
        assert_eq!(zone(200.0, 0.0, 400.0), Zone::Middle);
    }

    #[test]
    fn the_edges_are_a_quarter_of_the_width_each() {
        assert_eq!(zone(99.0, 0.0, 400.0), Zone::Left);
        assert_eq!(zone(100.0, 0.0, 400.0), Zone::Middle);
        assert_eq!(zone(300.0, 0.0, 400.0), Zone::Middle);
        assert_eq!(zone(301.0, 0.0, 400.0), Zone::Right);
    }

    #[test]
    fn the_zones_follow_where_the_body_starts() {
        assert_eq!(zone(250.0, 200.0, 400.0), Zone::Left);
    }

    #[test]
    fn a_body_without_width_has_only_a_middle() {
        assert_eq!(zone(10.0, 0.0, 0.0), Zone::Middle);
        assert_eq!(zone(f32::NAN, 0.0, 400.0), Zone::Middle);
    }
}
