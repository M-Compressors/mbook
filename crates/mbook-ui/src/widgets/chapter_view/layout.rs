//! Measures of the text column and of a picture's place in it. Plain
//! functions, so that they are tested without a window.

use crate::shared::layout::SizeClass;

/// How wide a line of a book may be, in em (RFC 0004, "Visual direction").
const MEASURE_EM: f32 = 38.0;
/// The margin on each side of the column of a compact window, in pixels.
const COMPACT_MARGIN: f32 = 20.0;
/// The margin on each side of the column of a wider window, in pixels.
const WIDE_MARGIN: f32 = 32.0;
/// The most pixels a placeholder may have, which keeps one made for a
/// picture of an absurd size small.
const PLACEHOLDER_PIXELS: u64 = 16_000_000;

/// The margin on each side of the text column, in pixels.
pub(super) fn margin(class: SizeClass) -> f32 {
    match class {
        SizeClass::Compact => COMPACT_MARGIN,
        SizeClass::Medium | SizeClass::Expanded => WIDE_MARGIN,
    }
}

/// The width of the text column: at most 38 em, less the margins in a window
/// that is narrower. A size that is no number gives no width.
pub(super) fn column_width(viewport: f32, text_size: f32, class: SizeClass) -> f32 {
    if !(viewport.is_finite() && text_size.is_finite()) {
        return 0.0;
    }
    (viewport - 2.0 * margin(class))
        .min(MEASURE_EM * text_size)
        .max(0.0)
}

/// The size, in pixels, of the transparent image that holds the place of a
/// picture of `known` pixels in a column `column` pixels wide: the width the
/// picture will have, and its height in proportion. `None` for a picture
/// without an area.
///
/// GPUI sizes an image by its pixels, so a placeholder that has the size the
/// picture will be shown at keeps the text from moving when the picture
/// arrives.
pub(super) fn placeholder_size(known: (u32, u32), column: f32) -> Option<(u32, u32)> {
    let (width, height) = known;
    if width == 0 || height == 0 || !column.is_finite() || column < 1.0 {
        return None;
    }
    let shown = f64::from(width).min(f64::from(column).floor());
    let scaled = (f64::from(height) * shown / f64::from(width))
        .round()
        .max(1.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "both are positive and at most the size of a picture, which is a u32"
    )]
    let size = (shown as u32, scaled as u32);
    (u64::from(size.0) * u64::from(size.1) <= PLACEHOLDER_PIXELS).then_some(size)
}

/// How wide, in device pixels, a picture is made for a column of `column`
/// logical pixels on a window with the scale factor `scale`.
pub(crate) fn device_width(column: f32, scale: f32) -> u32 {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let width = (column * scale).round();
    if width.is_finite() && width >= 1.0 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a positive number of pixels of a window"
        )]
        let pixels = width.min(65_535.0) as u32;
        pixels
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_column_is_38_em_in_a_wide_window() {
        assert!((column_width(1280.0, 18.0, SizeClass::Expanded) - 684.0).abs() < 0.01);
    }

    #[test]
    fn the_column_fills_a_narrow_window_less_the_margins() {
        assert!((column_width(390.0, 18.0, SizeClass::Compact) - 350.0).abs() < 0.01);
    }

    #[test]
    fn a_compact_window_keeps_a_margin_of_20_pixels() {
        assert!((margin(SizeClass::Compact) - 20.0).abs() < f32::EPSILON);
        assert!(margin(SizeClass::Medium) > margin(SizeClass::Compact));
    }

    #[test]
    fn a_larger_text_makes_a_wider_column() {
        assert!(
            column_width(1280.0, 24.0, SizeClass::Expanded)
                > column_width(1280.0, 16.0, SizeClass::Expanded)
        );
    }

    #[test]
    fn a_window_narrower_than_its_margins_has_no_column() {
        assert!(column_width(30.0, 18.0, SizeClass::Compact).abs() < f32::EPSILON);
    }

    #[test]
    fn a_size_that_is_no_number_has_no_column() {
        assert!(column_width(f32::NAN, 18.0, SizeClass::Compact).abs() < f32::EPSILON);
        assert!(column_width(800.0, f32::INFINITY, SizeClass::Medium).abs() < f32::EPSILON);
    }

    #[test]
    fn a_wide_picture_is_held_at_the_width_of_the_column() {
        assert_eq!(placeholder_size((1600, 800), 400.0), Some((400, 200)));
    }

    #[test]
    fn a_narrow_picture_keeps_its_own_size() {
        assert_eq!(placeholder_size((100, 50), 400.0), Some((100, 50)));
    }

    #[test]
    fn a_placeholder_is_at_least_one_pixel_high() {
        assert_eq!(placeholder_size((1000, 1), 100.0), Some((100, 1)));
    }

    #[test]
    fn a_picture_without_an_area_has_no_placeholder() {
        assert_eq!(placeholder_size((0, 10), 100.0), None);
        assert_eq!(placeholder_size((10, 0), 100.0), None);
    }

    #[test]
    fn a_column_that_is_not_there_has_no_placeholder() {
        assert_eq!(placeholder_size((10, 10), 0.0), None);
        assert_eq!(placeholder_size((10, 10), f32::NAN), None);
    }

    #[test]
    fn a_placeholder_of_an_absurd_size_is_refused() {
        assert_eq!(placeholder_size((60_000, 60_000), 60_000.0), None);
    }

    #[test]
    fn a_picture_is_made_for_the_device_pixels_of_the_column() {
        assert_eq!(device_width(400.0, 2.0), 800);
        assert_eq!(device_width(400.0, 1.0), 400);
    }

    #[test]
    fn a_scale_that_is_no_number_counts_as_one() {
        assert_eq!(device_width(400.0, f32::NAN), 400);
        assert_eq!(device_width(400.0, 0.0), 400);
    }

    #[test]
    fn a_picture_is_never_made_zero_pixels_wide() {
        assert_eq!(device_width(0.0, 2.0), 1);
        assert_eq!(device_width(f32::NAN, 2.0), 1);
    }
}
