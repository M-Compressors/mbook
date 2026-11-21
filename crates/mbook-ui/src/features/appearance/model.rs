//! The entity that holds the settings and applies them.

use std::ops::RangeInclusive;

use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui_kit::base::{ReduceMotionPreference, set_reduce_motion_preference};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    App, AppContext as _, AsyncApp, Context, Entity, Global, Pixels, SharedString, Task,
    WeakEntity, Window, WindowAppearance, px,
};

use super::theme;
use crate::entities::catalogue::{CatalogueError, Settings, Store};
use crate::shared::layout::SizeClass;
use crate::shared::theme::{Chrome, PageTone};

/// The size of an em at an interface scale of 1, in pixels: what GPUI uses
/// when nothing says otherwise.
const BASE_REM: f32 = 16.0;

/// Installed serif families, in the order used when the requested font is missing.
const READING_FONTS: [&str; 5] = [
    "Literata",
    "Charter",
    "Iowan Old Style",
    "Georgia",
    "Times New Roman",
];

/// The sizes of the text of a book, in pixels. The same bounds as the
/// catalogue applies when it reads a setting.
const TEXT_SIZE: RangeInclusive<f32> = 12.0..=32.0;
/// The line heights a book may be set at.
const LINE_HEIGHT: RangeInclusive<f32> = 1.2..=2.0;
/// The scales of the interface.
const INTERFACE_SCALE: RangeInclusive<f32> = 1.0..=2.0;

/// The settings of the catalogue and what they do to the interface.
///
/// Views read the settings and ask for a change with [`Self::update`]; they do
/// not keep a copy. A change shows at once: in memory, in the theme, in the
/// size of the interface; it reaches the catalogue on the background.
pub(crate) struct Appearance {
    settings: Settings,
    /// Whether the catalogue has answered. Until then the settings are the
    /// defaults.
    loaded: bool,
    /// Whether a change was made before the catalogue answered; the answer
    /// then does not replace it.
    changed: bool,
    error: Option<CatalogueError>,
    /// The appearance of the window, which a page tone that follows the system
    /// follows.
    system: WindowAppearance,
    /// The size class last announced to observers.
    class: SizeClass,
    /// The families of fonts the device has; `None` until they are read.
    families: Option<Vec<String>>,
    /// Whether the dark or the light theme was last given to the Kit.
    projected_dark: Option<bool>,
    writes: UnboundedSender<Settings>,
    _load: Task<()>,
    _fonts: Task<()>,
    _applying: Task<()>,
}

/// The handle of the one [`Appearance`] of the application.
struct AppearanceHandle(Entity<Appearance>);

impl Global for AppearanceHandle {}

impl Appearance {
    /// Starts reading the settings and the font families on the background.
    /// Until the settings arrive they are the defaults; ask
    /// [`Self::is_loaded`].
    pub(crate) fn new(cx: &mut Context<'_, Self>) -> Self {
        let catalogue = Store::global(cx).catalogue.clone();
        let load = cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_spawn(async move { catalogue.settings() })
                .await;
            this.update(cx, |this, cx| this.loaded(loaded, cx)).ok();
        });

        let text_system = cx.text_system().clone();
        let fonts = cx.spawn(async move |this, cx| {
            let mut families = cx
                .background_spawn(async move { text_system.all_font_names() })
                .await;
            families.sort_unstable();
            families.dedup();
            this.update(cx, |this, cx| {
                this.families = Some(families);
                cx.notify();
            })
            .ok();
        });

        let (writes, queue) = unbounded();
        let applying = cx.spawn(async move |this, cx| write_all(queue, this, cx).await);

        Self {
            settings: Settings::default(),
            loaded: false,
            changed: false,
            error: None,
            system: cx.window_appearance(),
            class: SizeClass::Expanded,
            families: None,
            projected_dark: None,
            writes,
            _load: load,
            _fonts: fonts,
            _applying: applying,
        }
    }

    /// The appearance of the application, as an entity to observe.
    ///
    /// # Panics
    ///
    /// When [`Self::install`] was not called: the application installs it
    /// before it builds any page.
    #[expect(
        clippy::panic,
        reason = "a page built before the appearance is a programming error, as for any GPUI global"
    )]
    pub(crate) fn global(cx: &App) -> Entity<Self> {
        Self::try_global(cx)
            .unwrap_or_else(|| panic!("the appearance is installed before any page is built"))
    }

    /// The appearance of the application, or `None` before it is installed.
    pub(crate) fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<AppearanceHandle>()
            .map(|handle| handle.0.clone())
    }

    /// Makes `appearance` the one every page reaches through [`Self::global`].
    pub(crate) fn install(appearance: Entity<Self>, cx: &mut App) {
        cx.set_global(AppearanceHandle(appearance));
    }

    /// The settings.
    pub(crate) fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Whether the settings were read from the catalogue.
    pub(crate) fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// What went wrong reading or writing the settings. The settings in
    /// memory still work.
    pub(crate) fn error(&self) -> Option<&CatalogueError> {
        self.error.as_ref()
    }

    /// Records a failure as if the catalogue had refused to read or write the
    /// settings, so that a test of a view can show it.
    #[cfg(any(test, feature = "visual"))]
    pub(crate) fn fail(&mut self, error: CatalogueError, cx: &mut Context<'_, Self>) {
        self.error = Some(error);
        cx.notify();
    }

    /// Changes the settings: in memory at once, in the theme and the size of
    /// the interface at once, in the catalogue on the background.
    ///
    /// The values are brought into their ranges after `change` has run, so a
    /// value out of range, or not a number, cannot reach a view or the
    /// catalogue. A change that leaves the settings as they were does nothing.
    pub(crate) fn update(
        &mut self,
        change: impl FnOnce(&mut Settings),
        cx: &mut Context<'_, Self>,
    ) {
        let before = self.settings.clone();
        let mut settings = before.clone();
        change(&mut settings);
        clamp(&mut settings, &before);
        if settings == before {
            return;
        }
        self.settings = settings;
        self.changed = true;
        self.apply(cx);
        // The writer ends with the entity, and so does this sender.
        self.writes.unbounded_send(self.settings.clone()).ok();
        cx.notify();
    }

    /// The page tone that is drawn: the chosen one, or, when the setting
    /// follows the system, paper for a light window and night for a dark one.
    pub(crate) fn page_tone(&self, window: &Window) -> PageTone {
        resolve(self.settings.page_tone, window.appearance())
    }

    /// The colours of the interface that go with the page tone.
    pub(crate) fn chrome(&self, window: &Window) -> Chrome {
        self.page_tone(window).chrome()
    }

    /// The size class of the window: its width over the size of the
    /// interface. Layouts take it and nothing else (RFC 0004, "Layout").
    pub(crate) fn size_class(&self, window: &Window) -> SizeClass {
        SizeClass::of(
            window.viewport_size().width.as_f32(),
            self.settings.interface_scale,
        )
    }

    /// Tells observers that the window was resized, when the size class
    /// changed with it. Call it where the window is resized.
    pub(crate) fn observe_viewport(&mut self, window: &Window, cx: &mut Context<'_, Self>) {
        let class = self.size_class(window);
        if class != self.class {
            self.class = class;
            cx.notify();
        }
    }

    /// Tells the appearance that the window changed between light and dark.
    /// It matters when the page tone follows the system.
    pub(crate) fn set_system(&mut self, system: WindowAppearance, cx: &mut Context<'_, Self>) {
        if system == self.system {
            return;
        }
        self.system = system;
        self.apply(cx);
        cx.notify();
    }

    /// The size of an em in the window: 16 px, scaled by the size of the
    /// interface.
    pub(crate) fn rem_size(&self) -> Pixels {
        px(BASE_REM * self.settings.interface_scale)
    }

    /// Gives the window the size of the interface. A view that is drawn with
    /// the window calls it each frame; it is cheap.
    pub(crate) fn sync_window(&self, window: &mut Window) {
        let rem = self.rem_size();
        if window.rem_size() != rem {
            window.set_rem_size(rem);
        }
    }

    /// The typeface for the text of books: the stored one when the device has
    /// it, then the first available serif fallback, then the interface font.
    /// The stored name is kept so the typeface applies again when it returns.
    ///
    /// Until the device's families are read, the font of the interface.
    pub(crate) fn reading_font(&self, cx: &App) -> SharedString {
        let installed = self.families();
        std::iter::once(self.settings.typeface.as_str())
            .chain(READING_FONTS)
            .find(|wanted| installed.iter().any(|family| family == wanted))
            .map_or_else(
                || cx.theme().font_family.clone(),
                |font| font.to_owned().into(),
            )
    }

    /// The families of fonts the device has, sorted; empty until they are
    /// read. The settings offer them next to the default typeface.
    pub(crate) fn families(&self) -> &[String] {
        self.families.as_deref().unwrap_or_default()
    }

    fn loaded(&mut self, loaded: Result<Settings, CatalogueError>, cx: &mut Context<'_, Self>) {
        self.loaded = true;
        match loaded {
            Ok(settings) => {
                if !self.changed {
                    self.settings = settings;
                }
            }
            Err(error) => self.error = Some(error),
        }
        self.apply(cx);
        cx.notify();
    }

    /// Gives the settings to what draws: the theme of the Kit and the flag
    /// for reduced motion.
    fn apply(&mut self, cx: &mut Context<'_, Self>) {
        let tone = resolve(self.settings.page_tone, self.system);
        let dark = tone.is_dark();
        if self.projected_dark != Some(dark) {
            self.projected_dark = Some(dark);
            theme::project(&tone.chrome(), dark, cx);
        }
        // The setting reduces motion; its absence leaves the system's choice,
        // which the Kit read and keeps following.
        let preference = if self.settings.reduce_motion {
            ReduceMotionPreference::Reduce
        } else {
            ReduceMotionPreference::System
        };
        set_reduce_motion_preference(preference, cx);
    }
}

/// The page tone for a choice and the appearance of the window.
fn resolve(chosen: Option<PageTone>, system: WindowAppearance) -> PageTone {
    chosen.unwrap_or(match system {
        WindowAppearance::Light | WindowAppearance::VibrantLight => PageTone::Paper,
        WindowAppearance::Dark | WindowAppearance::VibrantDark => PageTone::Night,
    })
}

/// Brings every setting into its range. A value that is no number, or a
/// typeface that is blank, keeps what it was.
fn clamp(settings: &mut Settings, before: &Settings) {
    settings.text_size = within(settings.text_size, &TEXT_SIZE, before.text_size);
    settings.line_height = within(settings.line_height, &LINE_HEIGHT, before.line_height);
    settings.interface_scale = within(
        settings.interface_scale,
        &INTERFACE_SCALE,
        before.interface_scale,
    );
    if settings.typeface.trim().is_empty() {
        settings.typeface.clone_from(&before.typeface);
    }
}

fn within(value: f32, range: &RangeInclusive<f32>, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(*range.start(), *range.end())
    } else {
        fallback
    }
}

/// Writes the settings one after another, in the order they were changed.
async fn write_all(
    mut queue: UnboundedReceiver<Settings>,
    this: WeakEntity<Appearance>,
    cx: &mut AsyncApp,
) {
    while let Some(settings) = queue.next().await {
        let Ok(catalogue) = this.read_with(cx, |_, cx| {
            Store::try_global(cx).map(|store| store.catalogue.clone())
        }) else {
            return;
        };
        let Some(catalogue) = catalogue else {
            continue;
        };
        let result = cx
            .background_spawn(async move { catalogue.put_settings(&settings) })
            .await;
        // A write that succeeds is the answer to the failure before it.
        let told = this.update(cx, |this, cx| {
            let changed = match result {
                Ok(()) => this.error.take().is_some(),
                Err(error) => {
                    this.error = Some(error);
                    true
                }
            };
            if changed {
                cx.notify();
            }
        });
        if told.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests;
