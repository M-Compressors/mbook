//! The size class of a window, which every layout follows.

/// How much room a window gives its content.
///
/// A view takes its layout from the size class of its window and from nothing
/// else; it never tests the operating system (mbook-ui docs, "Layout").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum SizeClass {
    /// Below 600 px: a stack of screens with a bar at the bottom.
    Compact,
    /// From 600 px to 959 px: a stack; panels slide over the content.
    Medium,
    /// 960 px and more: a sidebar beside the content; panels dock.
    Expanded,
}

/// The width at which a window stops being compact.
const MEDIUM_FROM: f32 = 600.0;
/// The width at which a window becomes expanded.
const EXPANDED_FROM: f32 = 960.0;

impl SizeClass {
    /// The size class of a window `width` pixels wide whose interface is
    /// drawn at `scale`, where `1.0` is the size the design was drawn at.
    ///
    /// The width is divided by the scale, so that an enlarged interface
    /// reflows into a narrower layout instead of being cut off. A scale that
    /// is not a positive number counts as `1.0`.
    pub(crate) fn of(width: f32, scale: f32) -> Self {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let width = width / scale;
        if width >= EXPANDED_FROM {
            Self::Expanded
        } else if width >= MEDIUM_FROM {
            Self::Medium
        } else {
            Self::Compact
        }
    }

    /// Whether a sidebar stands beside the content.
    pub(crate) fn has_sidebar(self) -> bool {
        self == Self::Expanded
    }

    /// The width of a cover on the shelf, in pixels.
    pub(crate) fn cover_width(self) -> f32 {
        match self {
            Self::Compact => 104.0,
            Self::Medium | Self::Expanded => 132.0,
        }
    }
}

/// How many covers fit a shelf `width` pixels wide, with `gap` pixels between
/// them. Never less than two: a shelf of one column is a list.
pub(crate) fn shelf_columns(width: f32, cover: f32, gap: f32) -> usize {
    if !(width.is_finite() && cover > 0.0 && gap >= 0.0) {
        return 2;
    }
    let fit = ((width + gap) / (cover + gap)).floor();
    if fit < 2.0 {
        2
    } else {
        // A window is far narrower than `usize::MAX` covers.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "`fit` is a whole number of at least 2, bounded by the width of a window"
        )]
        let columns = fit as usize;
        columns
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phone_is_compact() {
        assert_eq!(SizeClass::of(390.0, 1.0), SizeClass::Compact);
        assert_eq!(SizeClass::of(599.9, 1.0), SizeClass::Compact);
    }

    #[test]
    fn a_tablet_in_portrait_is_medium() {
        assert_eq!(SizeClass::of(600.0, 1.0), SizeClass::Medium);
        assert_eq!(SizeClass::of(959.9, 1.0), SizeClass::Medium);
    }

    #[test]
    fn a_desktop_window_is_expanded() {
        assert_eq!(SizeClass::of(960.0, 1.0), SizeClass::Expanded);
        assert_eq!(SizeClass::of(2560.0, 1.0), SizeClass::Expanded);
    }

    #[test]
    fn an_enlarged_interface_reflows_into_a_narrower_class() {
        assert_eq!(SizeClass::of(1100.0, 1.0), SizeClass::Expanded);
        assert_eq!(SizeClass::of(1100.0, 1.5), SizeClass::Medium);
        assert_eq!(SizeClass::of(1100.0, 2.0), SizeClass::Compact);
    }

    #[test]
    fn a_scale_that_is_no_positive_number_counts_as_one() {
        for scale in [0.0, -2.0, f32::NAN, f32::INFINITY] {
            assert_eq!(SizeClass::of(1000.0, scale), SizeClass::Expanded);
        }
    }

    #[test]
    fn only_an_expanded_window_has_a_sidebar() {
        assert!(SizeClass::Expanded.has_sidebar());
        assert!(!SizeClass::Medium.has_sidebar());
        assert!(!SizeClass::Compact.has_sidebar());
    }

    #[test]
    fn covers_are_smaller_on_a_compact_window() {
        assert!(SizeClass::Compact.cover_width() < SizeClass::Medium.cover_width());
        assert!(
            (SizeClass::Medium.cover_width() - SizeClass::Expanded.cover_width()).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn a_shelf_holds_as_many_covers_as_fit() {
        // Three covers of 104 with two gaps of 16 need 344.
        assert_eq!(shelf_columns(344.0, 104.0, 16.0), 3);
        assert_eq!(shelf_columns(343.0, 104.0, 16.0), 2);
        assert_eq!(shelf_columns(1000.0, 132.0, 24.0), 6);
    }

    #[test]
    fn a_shelf_never_has_fewer_than_two_columns() {
        assert_eq!(shelf_columns(50.0, 104.0, 16.0), 2);
        assert_eq!(shelf_columns(f32::NAN, 104.0, 16.0), 2);
        assert_eq!(shelf_columns(500.0, 0.0, 16.0), 2);
        assert_eq!(shelf_columns(500.0, 104.0, -1.0), 2);
    }
}
