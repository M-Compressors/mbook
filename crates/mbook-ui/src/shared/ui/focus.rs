//! The outline of keyboard focus.
//!
//! What the Kit draws, found in `gpui-component` 0.10: a focused control gets
//! a border in `Theme::ring` and, outside it, a ring 3 px wide in `ring` at
//! 50 % opacity. The border is opaque but 1 px; the ring is wide but
//! translucent, so over the canvas it falls below the 3:1 of WCAG 1.4.11. The
//! application sets `ring` to the accent, which keeps the border right, and
//! gives its own focusable elements (a cover on the shelf) [`focus_ring`]: 2 px,
//! opaque, in the accent.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{App, BoxShadow, Pixels, StyleRefinement, Styled as _, px};

/// The width of the outline of keyboard focus.
pub(crate) const FOCUS_RING_WIDTH: Pixels = px(2.0);

/// The style of an element that has the keyboard focus: a ring of
/// [`FOCUS_RING_WIDTH`] in the accent, drawn outside the element, so that
/// nothing moves when it appears.
///
/// Apply it with `focus_visible`, which acts for focus that came from the
/// keyboard and not for a click:
///
/// ```ignore
/// div().track_focus(&handle).focus_visible(focus_ring(cx))
/// ```
pub(crate) fn focus_ring(cx: &App) -> impl FnOnce(StyleRefinement) -> StyleRefinement + use<> {
    let accent = cx.theme().ring;
    move |style| {
        style.shadow(vec![
            BoxShadow::new(px(0.0), px(0.0), accent).spread_radius(FOCUS_RING_WIDTH),
        ])
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;

    use super::*;

    #[gpui_kit::test]
    fn the_ring_is_two_pixels_wide_in_the_ring_colour(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let style = cx.update(|cx| focus_ring(cx)(StyleRefinement::default()));
        let shadows = style.box_shadow.unwrap();
        assert_eq!(shadows.len(), 1);
        assert_eq!(shadows[0].spread_radius, FOCUS_RING_WIDTH);
        assert_eq!(shadows[0].blur_radius, px(0.0));
        let ring = cx.update(|cx| cx.theme().ring);
        assert_eq!(shadows[0].color, ring);
    }
}
