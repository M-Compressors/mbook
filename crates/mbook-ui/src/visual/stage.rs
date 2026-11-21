//! A window with the application in it, and the means to drive it and to
//! take a picture of it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Entity, HeadlessAppContext, MouseButton, Pixels, Point,
    ScrollDelta, Window, point, px, size,
};
use tempfile::TempDir;

use super::world::World;
use crate::app::{Root, Shell, bind_keys};
use crate::entities::catalogue::{Catalogue, Settings};
use crate::error::{Error, Result};
use crate::shared::assets::{Assets, register_fonts};
use crate::shared::model::BookKey;
use crate::shared::roots::Roots;
use crate::shared::theme::PageTone;

/// How a screen is looked at: its size and whether the interface is dark.
#[derive(Debug, Clone, Copy)]
pub(super) struct Look {
    /// The width of the window in pixels.
    pub(super) width: f32,
    /// The height of the window in pixels.
    pub(super) height: f32,
    /// Whether the page tone is Night.
    pub(super) dark: bool,
    /// Whether motion is reduced. Screens that are drawn at rest need it;
    /// the settings are shown as they are by default.
    pub(super) calm: bool,
}

impl Look {
    /// The name a file gets for this look: `1280-dark`.
    pub(super) fn suffix(&self) -> String {
        let tone = if self.dark { "dark" } else { "light" };
        format!("{}-{tone}", self.width)
    }
}

/// The application in a headless window.
pub(super) struct Stage {
    cx: HeadlessAppContext,
    window: AnyWindowHandle,
    shell: Entity<Shell>,
    roots: Roots,
    keys: Vec<BookKey>,
    /// Keeps the copy of the library until the stage is dropped.
    _dir: TempDir,
}

/// Why a step could not be taken.
fn missing(what: &str) -> Error {
    Error::Render(format!("there is no {what} on the screen"))
}

/// The application's context: platform text system, assets, a renderer.
fn context() -> HeadlessAppContext {
    let mut cx = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        Arc::new(Assets),
        gpui_kit::platform::current_headless_renderer,
    );
    // Decoding pictures and reading files run on real threads.
    cx.allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        register_fonts(cx);
        bind_keys(cx);
    });
    cx
}

impl Stage {
    /// Opens the application over a copy of `world`, with `tweak` applied to
    /// the settings first.
    pub(super) fn open(
        world: &World,
        look: Look,
        tweak: impl FnOnce(&mut Settings),
    ) -> Result<Self> {
        let (dir, roots) = world.copy()?;
        {
            let catalogue = Catalogue::open(&roots.catalogue())
                .map_err(|error| Error::Render(error.to_string()))?;
            let mut settings = Settings {
                // Nothing may reach the network from a picture of a screen.
                check_updates: false,
                reduce_motion: look.calm,
                page_tone: Some(if look.dark {
                    PageTone::Night
                } else {
                    PageTone::Paper
                }),
                ..Settings::default()
            };
            tweak(&mut settings);
            catalogue
                .put_settings(&settings)
                .map_err(|error| Error::Render(error.to_string()))?;
        }
        let mut cx = context();
        let mut built = None;
        let for_shell = roots.clone();
        let window = cx
            .open_window(size(px(look.width), px(look.height)), |window, cx| {
                let shell = cx.new(|cx| Shell::new(for_shell, window, cx));
                built = Some(shell.clone());
                cx.new(|cx| gpui_kit::base::Root::new(shell, window, cx))
            })
            .map_err(|error| Error::Render(error.to_string()))?;
        let shell = built.ok_or_else(|| missing("window"))?;
        let mut stage = Self {
            cx,
            window: window.into(),
            shell,
            roots,
            keys: world.keys().to_vec(),
            _dir: dir,
        };
        for _ in 0..40 {
            stage.settle()?;
            if stage.root().is_ok() {
                break;
            }
        }
        stage.root()?;
        stage.settle()?;
        Ok(stage)
    }

    /// The books of the library, in the order they were added.
    pub(super) fn keys(&self) -> &[BookKey] {
        &self.keys
    }

    /// The directories of the library this stage shows.
    pub(super) fn roots(&self) -> &Roots {
        &self.roots
    }

    /// The root of the window, once the library is open.
    pub(super) fn root(&mut self) -> Result<Entity<Root>> {
        let shell = self.shell.clone();
        self.cx
            .update(|cx| shell.read(cx).root().cloned())
            .ok_or_else(|| missing("library"))
    }

    /// Runs `step` with the window.
    pub(super) fn with_window<R>(
        &mut self,
        step: impl FnOnce(&mut Window, &mut App) -> R,
    ) -> Result<R> {
        self.cx
            .update_window(self.window, |_, window, cx| step(window, cx))
            .map_err(|error| Error::Render(error.to_string()))
    }

    /// Runs `step` with the app.
    pub(super) fn with_app<R>(&mut self, step: impl FnOnce(&mut App) -> R) -> R {
        self.cx.update(step)
    }

    /// Lets the background run and draws a few frames.
    pub(super) fn settle(&mut self) -> Result<()> {
        for _ in 0..6 {
            self.cx.run_until_parked();
            self.cx.advance_clock(Duration::from_millis(30));
            self.with_window(TestWindowExt::render_frame)?;
        }
        Ok(())
    }

    /// Lets `time` pass on the clock of the application, as a user would wait.
    pub(super) fn wait(&mut self, time: Duration) -> Result<()> {
        self.cx.run_until_parked();
        self.cx.advance_clock(time);
        self.settle()
    }

    /// Whether an element with this id is on the screen.
    pub(super) fn has(&mut self, id: &'static str) -> Result<bool> {
        self.with_window(|window, _| window.try_find(id).is_some())
    }

    /// The bounds of an element, for a step that needs a place.
    fn centre_of(&mut self, id: &'static str) -> Result<Point<Pixels>> {
        self.with_window(|window, _| window.try_find(id).map(|found| found.bounds().center()))?
            .ok_or_else(|| missing(id))
    }

    /// Clicks the element with this id.
    pub(super) fn click(&mut self, id: &'static str) -> Result<()> {
        let at = self.centre_of(id)?;
        self.press_at(at)
    }

    /// Clicks an element whose id is made at run time.
    pub(super) fn click_named(&mut self, id: &str) -> Result<()> {
        let id = id.to_owned();
        let at = self
            .with_window(|window, _| {
                window
                    .try_find(id.clone())
                    .map(|found| found.bounds().center())
            })?
            .ok_or_else(|| missing(&id))?;
        self.press_at(at)
    }

    /// Clicks the control with this accessible name.
    pub(super) fn click_label(&mut self, label: &str) -> Result<()> {
        let label = label.to_owned();
        let at = self
            .with_window(|window, _| {
                window
                    .find_all_by_label(&label)
                    .first()
                    .map(|found| found.bounds().center())
            })?
            .ok_or_else(|| missing(&label))?;
        self.press_at(at)
    }

    fn press_at(&mut self, at: Point<Pixels>) -> Result<()> {
        self.with_window(|window, cx| {
            window.pointer_down(at, MouseButton::Left, cx);
            window.pointer_up(at, MouseButton::Left, cx);
        })?;
        self.settle()
    }

    /// Puts the pointer over an element.
    pub(super) fn hover(&mut self, id: &str) -> Result<()> {
        let id = id.to_owned();
        self.with_window(|window, cx| {
            if window.try_find(id.clone()).is_none() {
                return Err(missing(&id));
            }
            window.hover(id, cx);
            Ok(())
        })??;
        self.settle()
    }

    /// Types text where the focus is.
    pub(super) fn type_text(&mut self, text: &str) -> Result<()> {
        let text = text.to_owned();
        self.with_window(move |window, cx| window.input(&text, cx))?;
        self.settle()
    }

    /// Scrolls an element by `lines`, down for a negative number.
    pub(super) fn scroll(&mut self, id: &'static str, lines: f32) -> Result<()> {
        if !self.has(id)? {
            return Err(missing(id));
        }
        self.with_window(|window, cx| {
            window.scroll(id, ScrollDelta::Lines(point(0.0, lines)), cx);
        })?;
        self.settle()
    }

    /// Writes the picture of the window to `path`.
    pub(super) fn capture(&mut self, path: &Path) -> Result<PathBuf> {
        self.settle()?;
        let image = self
            .cx
            .capture_screenshot(self.window)
            .map_err(|error| Error::Render(error.to_string()))?;
        image
            .save(path)
            .map_err(|error| Error::Render(error.to_string()))?;
        Ok(path.to_owned())
    }
}
