//! The window of the application.

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, Result, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};

use super::shell::Shell;
use crate::shared::roots::Roots;

/// The name of the window.
const TITLE: &str = "mbook";
/// The size of a new window, in pixels.
const DEFAULT_SIZE: (f32, f32) = (1180.0, 780.0);
/// The smallest size, in pixels. A window this narrow takes the compact layout,
/// so that the layout of a phone can be reached on a desktop.
const MIN_SIZE: (f32, f32) = (360.0, 480.0);

/// Opens the window, which starts opening the library kept in `roots`.
///
/// The kit must be initialised first.
///
/// # Errors
///
/// When the platform cannot make a window.
pub(super) fn open(roots: Roots, cx: &mut App) -> Result<(AnyWindowHandle, Entity<Shell>)> {
    let bounds = Bounds::centered(None, size(px(DEFAULT_SIZE.0), px(DEFAULT_SIZE.1)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(MIN_SIZE.0), px(MIN_SIZE.1))),
        titlebar: Some(TitlebarOptions {
            title: Some(TITLE.into()),
            ..TitlebarOptions::default()
        }),
        ..WindowOptions::default()
    };
    gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| Shell::new(roots, window, cx))
    })
}
