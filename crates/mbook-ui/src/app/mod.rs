//! The start of the application: the window, the root view, keys and menus.

mod keys;
mod open;
mod root;
mod shell;
mod window;

use gpui_kit::Application;

#[cfg(any(feature = "visual", test))]
pub(crate) use keys::bind as bind_keys;
#[cfg(feature = "visual")]
pub(crate) use root::{Root, show_about};
#[cfg(feature = "visual")]
pub(crate) use shell::Shell;

use crate::shared::assets::{self, Assets};
use crate::shared::roots::Roots;

/// Runs the application until its last window closes.
///
/// `application` carries the platform, which the shell chooses; `roots` are
/// the directories the library keeps its files in. The window opens at once
/// and shows that the library is opening; the folders and the catalogue are
/// read on the background executor.
pub fn run(application: Application, roots: Roots) {
    application.with_assets(Assets).run(move |cx| {
        gpui_kit::init(cx);
        assets::register_fonts(cx);
        keys::install(cx);
        if window::open(roots, cx).is_err() {
            // Without a window there is nothing to show an error in.
            cx.quit();
            return;
        }
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests;
