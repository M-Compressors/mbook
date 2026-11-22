//! The measures of a picture in the viewer: how large it is shown, how far
//! one step enlarges it, and where the view moves so that the part under the
//! pointer stays there. Plain functions, so that they are tested without a
//! window.

/// How much one step enlarges or reduces the picture.
pub(super) const STEP: f32 = 1.25;
/// The most a picture is enlarged to fit the screen, past its own size.
const MOST_FIT: f32 = 4.0;
/// How far past the larger of the fitted and the own size a picture may be
/// enlarged.
const MOST_ZOOM: f32 = 4.0;
/// The most pixels a picture is decoded with for the viewer; a larger one is
/// decoded smaller and scaled up.
const VIEWER_PIXELS: u64 = 16_000_000;
/// The most pixels a figure is drawn with for the viewer.
const RASTER_PIXELS: f32 = 16_000_000.0;

/// The scales a picture may be shown at, as parts of its own size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Scales {
    /// The whole picture in the room it has.
    pub(super) fit: f32,
    /// The smallest it may be made.
    pub(super) least: f32,
    /// The largest it may be made.
    pub(super) most: f32,
}

impl Scales {
    /// `zoom` brought within the scales.
    pub(super) fn clamp(&self, zoom: f32) -> f32 {
        if zoom.is_finite() {
            zoom.clamp(self.least, self.most)
        } else {
            self.fit
        }
    }
}

fn usable(length: f32) -> bool {
    length.is_finite() && length > 0.0
}

/// The scales of a picture of `natural` logical pixels in a `room` of
/// logical pixels. It fits the room, and is enlarged to do so at most
/// [`MOST_FIT`] times. `None` while either has no size.
pub(super) fn scales(natural: (f32, f32), room: (f32, f32)) -> Option<Scales> {
    if !(usable(natural.0) && usable(natural.1) && usable(room.0) && usable(room.1)) {
        return None;
    }
    let fit = (room.0 / natural.0).min(room.1 / natural.1).min(MOST_FIT);
    Some(Scales {
        fit,
        least: fit.min(1.0),
        most: fit.max(1.0) * MOST_ZOOM,
    })
}

/// The scale one step from `zoom`: larger when `larger`, else smaller. A step
/// that passes the fitted or the own size stops there, so that both can be
/// reached again.
pub(super) fn step(zoom: f32, scales: Scales, larger: bool) -> f32 {
    let next = if larger { zoom * STEP } else { zoom / STEP };
    let (low, high) = if larger { (zoom, next) } else { (next, zoom) };
    let stop = [scales.fit, 1.0]
        .into_iter()
        .filter(|&stop| stop > low * 1.001 && stop < high / 1.001)
        .reduce(|a, b| if larger { a.min(b) } else { a.max(b) });
    scales.clamp(stop.unwrap_or(next))
}

/// The scroll offset along one axis after the picture changes from `before`
/// to `after` logical pixels long, so that the point `at` of a room `room`
/// long shows the same part of the picture. `offset` is the one GPUI keeps:
/// zero, or negative once the picture is scrolled. A picture shorter than
/// the room is centred in it and needs no offset.
pub(super) fn anchored(offset: f32, room: f32, before: f32, after: f32, at: f32) -> f32 {
    if !(usable(room) && usable(before) && usable(after)) || after <= room {
        return 0.0;
    }
    let lead = ((room - before) / 2.0).max(0.0);
    let part = ((at - lead - offset) / before).clamp(0.0, 1.0);
    (at - part * after).clamp(room - after, 0.0)
}

/// The scroll offset along one axis while a drag moves the picture `moved`
/// logical pixels from `start`, kept within the picture.
pub(super) fn dragged(start: f32, moved: f32, room: f32, length: f32) -> f32 {
    if !(usable(room) && usable(length)) || length <= room {
        return 0.0;
    }
    (start + moved).clamp(room - length, 0.0)
}

/// The size a picture of `pixels` is decoded at for the viewer: its own,
/// unless it has more than [`VIEWER_PIXELS`].
pub(crate) fn decoded_size((width, height): (u32, u32)) -> (u32, u32) {
    let pixels = u64::from(width) * u64::from(height);
    if pixels <= VIEWER_PIXELS || pixels == 0 {
        return (width.max(1), height.max(1));
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "a scale only needs to be close; the pixels are far below 2^52"
    )]
    let scale = (VIEWER_PIXELS as f64 / pixels as f64).sqrt();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the scale is below one, so the result is positive and below the u32 sides"
    )]
    let side = |length: u32| ((f64::from(length) * scale).floor() as u32).max(1);
    let (width, height) = (side(width), side(height));
    // A side that was made one pixel long lets the other keep too many.
    let most = VIEWER_PIXELS / u64::from(height);
    (
        u32::try_from(u64::from(width).min(most)).unwrap_or(u32::MAX),
        height,
    )
}

/// The density a figure of `units` is drawn at for the screen when it needs
/// `needed` device pixels for each of its units: the next power of two, so
/// that it is drawn again only when it is twice as large or as small, and
/// never with more than [`RASTER_PIXELS`].
pub(super) fn raster_density(needed: f32, units: (f32, f32)) -> Option<f32> {
    if !(usable(needed) && usable(units.0) && usable(units.1)) {
        return None;
    }
    let most = (RASTER_PIXELS / (units.0 * units.1)).sqrt();
    let tier = needed.log2().ceil().exp2();
    Some(tier.min(most))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.001
    }

    #[test]
    fn a_large_picture_is_reduced_to_fit_the_room() {
        let s = scales((4000.0, 2000.0), (1000.0, 800.0)).unwrap();
        assert!(close(s.fit, 0.25));
        assert!(close(s.least, 0.25));
        assert!(close(s.most, 4.0));
    }

    #[test]
    fn a_small_picture_is_enlarged_to_fit_but_only_so_far() {
        let s = scales((200.0, 100.0), (1000.0, 800.0)).unwrap();
        assert!(close(s.fit, 4.0), "{s:?}");
        assert!(close(s.least, 1.0));
        assert!(close(s.most, 16.0));
        let s = scales((400.0, 100.0), (1000.0, 800.0)).unwrap();
        assert!(close(s.fit, 2.5));
    }

    #[test]
    fn a_picture_or_a_room_without_a_size_has_no_scales() {
        assert!(scales((0.0, 10.0), (100.0, 100.0)).is_none());
        assert!(scales((10.0, 10.0), (100.0, f32::NAN)).is_none());
        assert!(scales((10.0, 10.0), (-1.0, 100.0)).is_none());
    }

    #[test]
    fn a_zoom_that_is_not_a_number_falls_back_to_the_fit() {
        let s = scales((100.0, 100.0), (50.0, 50.0)).unwrap();
        assert!(close(s.clamp(f32::NAN), 0.5));
        assert!(close(s.clamp(100.0), s.most));
        assert!(close(s.clamp(0.0), s.least));
    }

    #[test]
    fn a_step_enlarges_and_reduces_by_a_quarter() {
        let s = scales((100.0, 100.0), (50.0, 50.0)).unwrap();
        assert!(close(step(1.25, s, true), 1.5625));
        assert!(close(step(1.5625, s, false), 1.25));
    }

    #[test]
    fn a_step_stops_at_the_own_size_and_at_the_fit() {
        let s = scales((1000.0, 1000.0), (900.0, 900.0)).unwrap();
        // From the fit of 0.9 the next step would be 1.125: it stops at 1.
        assert!(close(step(s.fit, s, true), 1.0));
        assert!(close(step(1.0, s, false), 0.9));
        let s = scales((300.0, 300.0), (1000.0, 1000.0)).unwrap();
        assert!(close(step(3.0, s, true), 3.333));
        assert!(close(step(1.0, s, true), 1.25));
    }

    #[test]
    fn a_step_never_leaves_the_scales() {
        let s = scales((100.0, 100.0), (50.0, 50.0)).unwrap();
        assert!(close(step(s.least, s, false), s.least));
        assert!(close(step(s.most, s, true), s.most));
    }

    #[test]
    fn the_point_under_the_pointer_stays_while_the_picture_grows() {
        // The picture fills the room and is scrolled 100 pixels; the point
        // 200 pixels into the room is 300 into the picture, or 3/8 of it.
        let offset = anchored(-100.0, 400.0, 800.0, 1600.0, 200.0);
        assert!(close(offset, 200.0 - 600.0));
    }

    #[test]
    fn a_picture_that_grows_from_the_middle_of_the_room_keeps_its_middle() {
        // 200 long in a room of 400: it starts 100 in. Its middle is at 200.
        let offset = anchored(0.0, 400.0, 200.0, 800.0, 200.0);
        assert!(close(offset, -200.0));
    }

    #[test]
    fn a_picture_shorter_than_the_room_is_not_scrolled() {
        assert!(close(anchored(-50.0, 400.0, 800.0, 300.0, 200.0), 0.0));
        assert!(close(anchored(0.0, 0.0, 800.0, 900.0, 200.0), 0.0));
    }

    #[test]
    fn the_offset_stays_within_the_picture() {
        // A point beside the picture anchors at its nearer edge.
        assert!(close(anchored(0.0, 400.0, 200.0, 800.0, 10.0), 0.0));
        assert!(close(anchored(0.0, 400.0, 200.0, 800.0, 390.0), -400.0));
    }

    #[test]
    fn a_drag_moves_the_picture_within_its_edges() {
        assert!(close(dragged(-100.0, 30.0, 400.0, 800.0), -70.0));
        assert!(close(dragged(-100.0, 300.0, 400.0, 800.0), 0.0));
        assert!(close(dragged(-100.0, -900.0, 400.0, 800.0), -400.0));
        assert!(close(dragged(-100.0, 30.0, 400.0, 300.0), 0.0));
    }

    #[test]
    fn a_picture_is_decoded_at_its_own_size_unless_it_is_huge() {
        assert_eq!(decoded_size((4000, 3000)), (4000, 3000));
        let (width, height) = decoded_size((10_000, 5_000));
        assert!(u64::from(width) * u64::from(height) <= VIEWER_PIXELS);
        assert_eq!((width, height), (5656, 2828));
        assert_eq!(decoded_size((0, 0)), (1, 1));
        assert_eq!(decoded_size((200_000_000, 1)), (16_000_000, 1));
    }

    #[test]
    fn a_figure_is_drawn_at_the_next_power_of_two() {
        assert!(close(raster_density(3.0, (100.0, 100.0)).unwrap(), 4.0));
        assert!(close(raster_density(4.0, (100.0, 100.0)).unwrap(), 4.0));
        assert!(close(raster_density(0.3, (100.0, 100.0)).unwrap(), 0.5));
    }

    #[test]
    fn a_figure_is_never_drawn_with_too_many_pixels() {
        let density = raster_density(64.0, (1000.0, 1000.0)).unwrap();
        assert!(close(density, 4.0));
        assert!(raster_density(0.0, (10.0, 10.0)).is_none());
        assert!(raster_density(2.0, (0.0, 10.0)).is_none());
    }
}
