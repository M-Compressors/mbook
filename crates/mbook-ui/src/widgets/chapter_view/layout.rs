//! Measures of the text column and of a picture's place in it. Plain
//! functions, so that they are tested without a window.

use crate::shared::layout::SizeClass;

const COMPACT_MARGIN: f32 = 20.0;
const WIDE_MARGIN: f32 = 32.0;
const PLACEHOLDER_PIXELS: u64 = 16_000_000;

/// The width in logical pixels capped by the reading font's zero advance.
pub(super) fn column_width(viewport: f32, zero: f32, measure: f32, class: SizeClass) -> f32 {
    if !(viewport.is_finite() && zero.is_finite()) {
        return 0.0;
    }
    let margin = match class {
        SizeClass::Compact if viewport <= 340.0 => 16.0,
        SizeClass::Compact => COMPACT_MARGIN,
        SizeClass::Medium | SizeClass::Expanded => WIDE_MARGIN,
    };
    let available = (viewport - 2.0 * margin).max(0.0);
    match class {
        SizeClass::Compact => available,
        SizeClass::Medium | SizeClass::Expanded => available.min(measure * zero),
    }
}

/// Fits a picture inside the column and, in the reader, the height of a page.
/// GPUI lays out a raw bitmap in logical pixels according to its pixel size.
pub(crate) fn picture_size(
    known: (u32, u32),
    column: f32,
    page: Option<f32>,
) -> Option<(u32, u32)> {
    let (width, height) = known;
    if width == 0
        || height == 0
        || !column.is_finite()
        || column < 1.0
        || page.is_some_and(|page| !page.is_finite() || page < 1.0)
    {
        return None;
    }
    let mut shown = f64::from(width).min(f64::from(column).floor());
    if let Some(page) = page {
        shown = shown.min(f64::from(page).floor() * f64::from(width) / f64::from(height));
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "shown is positive and no wider than the original u32 width"
    )]
    let shown = (shown as u32).max(1);
    let scaled = (f64::from(height) * f64::from(shown) / f64::from(width))
        .min(page.map_or(f64::from(height), f64::from));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "scaled is positive and no taller than the original u32 height"
    )]
    let size = (shown, (scaled as u32).max(1));
    (u64::from(size.0) * u64::from(size.1) <= PLACEHOLDER_PIXELS).then_some(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_column_measures_sixty_zero_advances() {
        assert!((column_width(1280.0, 12.0, 60.0, SizeClass::Expanded) - 720.0).abs() < 0.01);
    }

    #[test]
    fn the_column_fills_a_narrow_window_less_the_margins() {
        assert!((column_width(390.0, 11.0, 60.0, SizeClass::Compact) - 350.0).abs() < 0.01);
    }

    #[test]
    fn a_very_narrow_window_uses_smaller_margins() {
        assert!((column_width(320.0, 11.0, 60.0, SizeClass::Compact) - 288.0).abs() < 0.01);
    }

    #[test]
    fn a_larger_text_makes_a_wider_column() {
        assert!(
            column_width(1280.0, 24.0, 60.0, SizeClass::Expanded)
                > column_width(1280.0, 16.0, 60.0, SizeClass::Expanded)
        );
    }

    #[test]
    fn a_window_narrower_than_its_margins_has_no_column() {
        assert!(column_width(30.0, 18.0, 60.0, SizeClass::Compact).abs() < f32::EPSILON);
    }

    #[test]
    fn a_size_that_is_no_number_has_no_column() {
        assert!(column_width(f32::NAN, 18.0, 60.0, SizeClass::Compact).abs() < f32::EPSILON);
        assert!(column_width(800.0, f32::INFINITY, 60.0, SizeClass::Medium).abs() < f32::EPSILON);
    }

    #[test]
    fn a_wide_picture_is_held_at_the_width_of_the_column() {
        assert_eq!(picture_size((1600, 800), 400.0, None), Some((400, 200)));
    }

    #[test]
    fn a_narrow_picture_keeps_its_own_size() {
        assert_eq!(picture_size((100, 50), 400.0, None), Some((100, 50)));
    }

    #[test]
    fn a_placeholder_is_at_least_one_pixel_high() {
        assert_eq!(picture_size((1000, 1), 100.0, None), Some((100, 1)));
    }

    #[test]
    fn a_picture_without_an_area_has_no_placeholder() {
        assert_eq!(picture_size((0, 10), 100.0, None), None);
        assert_eq!(picture_size((10, 0), 100.0, None), None);
    }

    #[test]
    fn a_column_that_is_not_there_has_no_placeholder() {
        assert_eq!(picture_size((10, 10), 0.0, None), None);
        assert_eq!(picture_size((10, 10), f32::NAN, None), None);
    }

    #[test]
    fn a_placeholder_of_an_absurd_size_is_refused() {
        assert_eq!(picture_size((60_000, 60_000), 60_000.0, None), None);
    }

    #[test]
    fn a_square_picture_scales_to_the_page_height() {
        assert_eq!(
            picture_size((1024, 1024), 760.0, Some(515.7)),
            Some((515, 515))
        );
    }

    #[test]
    fn a_page_without_space_cannot_hold_a_picture() {
        assert_eq!(picture_size((100, 100), 400.0, Some(0.0)), None);
        assert_eq!(picture_size((100, 100), 400.0, Some(f32::NAN)), None);
    }

    #[test]
    fn a_very_tall_picture_is_at_least_one_pixel_wide() {
        assert_eq!(picture_size((1, 4096), 100.0, Some(500.0)), Some((1, 500)));
    }
}
