//! The start of the application: the window, the root view, keys and menus.

#[cfg(not(target_os = "android"))]
pub(crate) mod agent;
mod file_types;
mod keys;
mod open;
mod opened;
pub(crate) mod root;
pub(crate) mod shell;
mod window;

use std::path::PathBuf;

use gpui_kit::Application;

#[cfg(any(feature = "visual", test))]
pub(crate) use keys::bind as bind_keys;
#[cfg(feature = "visual")]
pub(crate) use root::{Root, show_about};
#[cfg(feature = "visual")]
pub(crate) use shell::Shell;

use crate::features::appearance::SystemLanguage;
use crate::shared::assets::{self, Assets};
use crate::shared::compute;
use crate::shared::roots::Roots;

/// Runs the application until its last window closes.
///
/// `application` carries the platform, which the shell chooses; `roots` are
/// the directories the library keeps its files in; `files` are the files the
/// application was started to open, as a command line names them. The window
/// opens at once and shows that the library is opening; the folders and the
/// catalogue are read on the background executor. Once it is open, `files`
/// and the files the platform hands over later are added to it, as dropped
/// files are.
pub fn run(application: Application, roots: Roots, files: Vec<PathBuf>) {
    // Before anything uses the pool. A pool built before is used as it is.
    compute::init().ok();
    let (sender, urls) = futures::channel::mpsc::unbounded();
    application.on_open_urls(move |batch| {
        // The receiver lives as long as the application.
        sender.unbounded_send(batch).ok();
    });
    application.with_assets(Assets).run(move |cx| {
        gpui_kit::init(cx);
        // Before the first screen, which is drawn in the language of the
        // system until the settings are read.
        cx.set_global(SystemLanguage::detect());
        assets::register_fonts(cx);
        keys::install(cx);
        opened::listen(urls, cx);
        opened::Opened::push(files, cx);
        #[cfg(target_os = "android")]
        opened::poll_android(cx);
        #[cfg(windows)]
        cx.background_executor()
            .spawn(async { file_types::register() })
            .detach();
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
