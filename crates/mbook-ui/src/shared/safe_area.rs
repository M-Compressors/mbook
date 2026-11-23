//! The part of a window that system bars and display cutouts leave free.
//!
//! On a phone the window of the application covers the whole screen, with
//! the status bar and the navigation bar drawn over it. GPUI reports what
//! they cover as the insets of the window. gpui-mobile does not report them
//! on Android yet, but measures them itself; this module takes the larger of
//! the two, so the shell keeps clear of the bars once either says where they
//! are. On the desktop both are zero.

use gpui_kit::{Edges, Pixels, Window};

/// How far the content of `window` keeps from each of its edges.
pub(crate) fn insets(window: &Window) -> Edges<Pixels> {
    let reported = window.insets().effective();
    #[cfg(target_os = "android")]
    let reported = larger(reported, android());
    reported
}

/// The larger of `a` and `b` on each edge.
#[cfg(any(target_os = "android", test))]
fn larger(a: Edges<Pixels>, b: Edges<Pixels>) -> Edges<Pixels> {
    Edges {
        top: a.top.max(b.top),
        right: a.right.max(b.right),
        bottom: a.bottom.max(b.bottom),
        left: a.left.max(b.left),
    }
}

/// The insets gpui-mobile measured for its window, in logical pixels, or
/// zero before the window exists.
#[cfg(target_os = "android")]
fn android() -> Edges<Pixels> {
    let Some(window) = gpui_mobile::android::jni::platform().and_then(|p| p.primary_window())
    else {
        return Edges::default();
    };
    let insets = window.safe_area_insets_logical();
    Edges {
        top: gpui_kit::px(insets.top),
        right: gpui_kit::px(insets.right),
        bottom: gpui_kit::px(insets.bottom),
        left: gpui_kit::px(insets.left),
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::px;

    use super::*;

    #[test]
    fn each_edge_keeps_the_larger_inset() {
        let reported = Edges {
            top: px(0.0),
            right: px(4.0),
            bottom: px(24.0),
            left: px(0.0),
        };
        let measured = Edges {
            top: px(43.0),
            right: px(0.0),
            bottom: px(12.0),
            left: px(0.0),
        };
        assert_eq!(
            larger(reported, measured),
            Edges {
                top: px(43.0),
                right: px(4.0),
                bottom: px(24.0),
                left: px(0.0),
            }
        );
    }

    #[test]
    fn no_insets_stay_none() {
        assert_eq!(larger(Edges::default(), Edges::default()), Edges::default());
    }
}
