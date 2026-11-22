//! Offering an update: the entity that checks, installs and shows the bar
//! below the page header (mbook-ui docs, "Updates").
//!
//! One [`UpdateBanner`] serves the whole application. Each page shows it below
//! its header. The settings ask it to check now, so that there is one check at
//! a time. Nothing here blocks: the check and the installation run on the
//! background executor, the daily check waits on a timer of the executor.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex};
use gpui_kit::{
    Anchor, AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, Global,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, TestSupportExt as _, Window,
    div, px,
};

use super::check::{Update, check, enabled};
use super::error::UpdateError;
use super::install::{Installed, install};
use crate::features::appearance::Appearance;
use crate::shared::roots::Roots;
use crate::shared::ui::{status_line, target_size};

/// How long after a check the next one is made.
const CHECK_EVERY: Duration = Duration::from_hours(24);
/// How long to wait before asking again whether an import is still running.
const BUSY_RETRY: Duration = Duration::from_secs(5);
/// Space below update notifications for the library tabs and reader controls.
const NOTIFICATION_BOTTOM_MARGIN: f32 = 84.0;

/// What is said when a restart is asked for while the application is busy.
const RESTART_WAITS: &str =
    "Finish the import or close the editor first; then restart to finish the update";

/// What to do again when the user asks for it after a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Retry {
    /// Ask the releases again.
    Check,
    /// Install this update again.
    Install(Update),
}

/// Where the update is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UpdateState {
    /// Nothing has been asked yet.
    Idle,
    /// The releases are being asked.
    Checking,
    /// The releases were asked and nothing newer is there.
    UpToDate,
    /// A newer version is there and was not installed.
    Available(Update),
    /// The user asked for the installation, which waits for an import or a
    /// save to end.
    Waiting(Update),
    /// The archive is being downloaded, verified and installed.
    Installing(Update),
    /// The files are replaced; the application restarts to use them.
    Installed(Update),
    /// A check or an installation failed.
    Failed {
        /// What went wrong, as a sentence of its own.
        reason: String,
        /// What "Try again" does.
        retry: Retry,
    },
}

impl UpdateState {
    /// The sentence that says where the update is, for the bar and for the
    /// settings. Empty while nothing has been asked.
    pub(crate) fn summary(&self) -> String {
        match self {
            Self::Idle => String::new(),
            Self::Checking => "Checking for updates…".to_owned(),
            Self::UpToDate => "mbook is up to date".to_owned(),
            Self::Available(update) => format!("Version {} is available", update.version),
            Self::Waiting(_) => {
                "The update will be installed when the import or the edit is finished".to_owned()
            }
            Self::Installing(_) => "Downloading and verifying…".to_owned(),
            Self::Installed(_) => "Restart to finish".to_owned(),
            Self::Failed {
                reason,
                retry: Retry::Check,
            } => format!("Could not check for updates: {reason}"),
            Self::Failed {
                reason,
                retry: Retry::Install(_),
            } => format!("The update failed: {reason}"),
        }
    }

    /// Whether a check, a download or an installation is under way, or the
    /// application waits to be restarted: no new check starts then.
    pub(crate) fn is_working(&self) -> bool {
        matches!(
            self,
            Self::Checking | Self::Waiting(_) | Self::Installing(_) | Self::Installed(_)
        )
    }

    /// Whether the bar shows this state. A check that failed on its own is
    /// not shown: being offline is not news. It shows in the settings.
    fn is_shown(&self) -> bool {
        match self {
            Self::Idle | Self::Checking | Self::UpToDate => false,
            Self::Failed { retry, .. } => matches!(retry, Retry::Install(_)),
            Self::Available(_) | Self::Waiting(_) | Self::Installing(_) | Self::Installed(_) => {
                true
            }
        }
    }
}

/// The message of an error and of everything it was caused by, as a sentence:
/// `The update could not be downloaded: connection refused`.
pub(crate) fn reason(error: &dyn Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => text,
    }
}

type Check = dyn Fn() -> Result<Option<Update>, UpdateError> + Send + Sync;
type Install = dyn Fn(&Update, &Roots) -> Result<Installed, UpdateError> + Send + Sync;

/// What the banner asks the world; tests put their own in place.
#[derive(Clone)]
struct Backend {
    /// Whether this build checks for updates at all.
    enabled: bool,
    check: Arc<Check>,
    install: Arc<Install>,
}

impl Backend {
    fn releases() -> Self {
        Self {
            enabled: enabled(),
            check: Arc::new(check),
            install: Arc::new(install),
        }
    }
}

/// The handle of the one [`UpdateBanner`] of the application.
struct UpdateBannerHandle(Entity<UpdateBanner>);

impl Global for UpdateBannerHandle {}

/// Checks the releases, offers an update and installs it; draws the bar that
/// says so.
pub(crate) struct UpdateBanner {
    backend: Backend,
    roots: Roots,
    /// Whether an import or a save is running, which an installation waits
    /// for.
    is_busy: Box<dyn Fn(&App) -> bool>,
    /// The window that the end of an install is announced in.
    window: AnyWindowHandle,
    state: UpdateState,
    /// Whether the user chose "Later" for what the bar shows.
    dismissed: bool,
    /// The version that was announced, so that a daily check does not
    /// announce it again.
    announced: Option<String>,
    /// Counts the operations, so that the answer of one the user has moved
    /// on from is dropped.
    generation: u64,
    /// Whether the setting last said that releases are checked.
    checks_on: bool,
    work: Option<Task<()>>,
    retry: Option<Task<()>>,
    _daily: Task<()>,
    _subscription: Option<Subscription>,
}

impl UpdateBanner {
    /// A banner that checks the releases now, and then once a day, when the
    /// build updates itself and the setting is on.
    ///
    /// `is_busy` tells whether an import or a save is running: an
    /// installation waits until it says no.
    pub(crate) fn new(
        roots: Roots,
        is_busy: impl Fn(&App) -> bool + 'static,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        Self::with_backend(Backend::releases(), roots, Box::new(is_busy), window, cx)
    }

    fn with_backend(
        backend: Backend,
        roots: Roots,
        is_busy: Box<dyn Fn(&App) -> bool>,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let daily = cx.spawn(async move |this, cx| {
            loop {
                if this.update(cx, Self::check_if_due).is_err() {
                    return;
                }
                cx.background_executor().timer(CHECK_EVERY).await;
            }
        });
        let appearance = Appearance::try_global(cx);
        let checks_on = appearance
            .as_ref()
            .is_none_or(|appearance| appearance.read(cx).settings().check_updates);
        let subscription = appearance.map(|appearance| {
            cx.observe(&appearance, |this, appearance, cx| {
                let on = appearance.read(cx).settings().check_updates;
                let turned_on = on && !this.checks_on;
                this.checks_on = on;
                if turned_on {
                    this.check_if_due(cx);
                }
            })
        });
        Self {
            backend,
            roots,
            is_busy,
            window: window.window_handle(),
            state: UpdateState::Idle,
            dismissed: false,
            announced: None,
            generation: 0,
            checks_on,
            work: None,
            retry: None,
            _daily: daily,
            _subscription: subscription,
        }
    }

    /// A banner whose releases answer as scripted, for the tests of pages
    /// that use it: `Ok(Some(version))` offers that version, `Ok(None)` finds
    /// nothing newer, `Err(text)` fails with that text as its cause.
    #[cfg(any(test, feature = "visual"))]
    pub(crate) fn scripted(
        enabled: bool,
        answer: Result<Option<&'static str>, &'static str>,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let backend = Backend {
            enabled,
            check: Arc::new(move || match answer {
                Ok(version) => Ok(version.map(|version| Update {
                    version: version.to_owned(),
                    archive: format!("mbook-v{version}.tar.gz"),
                    url: format!("https://example.com/mbook-v{version}.tar.gz"),
                })),
                Err(text) => Err(UpdateError::Releases(text.into())),
            }),
            install: Arc::new(|_, _| Ok(Installed::Restart)),
        };
        let roots = Roots::new("/nowhere/data", "/nowhere/cache");
        Self::with_backend(backend, roots, Box::new(|_| false), window, cx)
    }

    /// The banner of the application, or `None` before it is installed.
    pub(crate) fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<UpdateBannerHandle>()
            .map(|handle| handle.0.clone())
    }

    /// Makes `banner` the one every page reaches through [`Self::try_global`].
    pub(crate) fn install(banner: Entity<Self>, cx: &mut App) {
        cx.set_global(UpdateBannerHandle(banner));
    }

    /// Whether this build checks for updates at all.
    pub(crate) fn is_enabled(&self) -> bool {
        self.backend.enabled
    }

    /// Where the update is.
    pub(crate) fn state(&self) -> &UpdateState {
        &self.state
    }

    /// Whether the bar is on the screen.
    pub(crate) fn is_visible(&self) -> bool {
        !self.dismissed && self.state.is_shown()
    }

    /// Asks the releases now, whatever the setting says. Nothing happens
    /// while a check or an installation is under way, or in a build that does
    /// not update itself.
    pub(crate) fn check_now(&mut self, cx: &mut Context<'_, Self>) {
        if self.backend.enabled {
            self.begin_check(true, cx);
        }
    }

    /// The check at the start and every day after it, when the setting says
    /// so.
    fn check_if_due(&mut self, cx: &mut Context<'_, Self>) {
        let wanted = Appearance::try_global(cx)
            .is_none_or(|appearance| appearance.read(cx).settings().check_updates);
        if self.backend.enabled && wanted {
            self.begin_check(false, cx);
        }
    }

    fn begin_check(&mut self, manual: bool, cx: &mut Context<'_, Self>) {
        if self.state.is_working() {
            return;
        }
        self.generation += 1;
        let generation = self.generation;
        self.state = UpdateState::Checking;
        self.dismissed = false;
        let ask = self.backend.check.clone();
        self.work = Some(cx.spawn(async move |this, cx| {
            let found = cx.background_spawn(async move { ask() }).await;
            this.update(cx, |this, cx| this.checked(generation, manual, found, cx))
                .ok();
        }));
        cx.notify();
    }

    fn checked(
        &mut self,
        generation: u64,
        manual: bool,
        found: Result<Option<Update>, UpdateError>,
        cx: &mut Context<'_, Self>,
    ) {
        if generation != self.generation {
            return;
        }
        match found {
            Ok(Some(update)) => {
                let version = update.version.clone();
                self.state = UpdateState::Available(update);
                if self.announced.as_deref() != Some(version.as_str()) || manual {
                    self.announced = Some(version.clone());
                    self.announce(format!("Version {version} is available"), cx);
                }
            }
            Ok(None) => self.state = UpdateState::UpToDate,
            Err(error) => {
                self.state = UpdateState::Failed {
                    reason: reason(&error),
                    retry: Retry::Check,
                };
            }
        }
        cx.notify();
    }

    /// Starts installing the update on offer, or waits for the import to end.
    fn begin_install(&mut self, cx: &mut Context<'_, Self>) {
        let update = match &self.state {
            UpdateState::Available(update)
            | UpdateState::Waiting(update)
            | UpdateState::Failed {
                retry: Retry::Install(update),
                ..
            } => update.clone(),
            _ => return,
        };
        self.generation += 1;
        if (self.is_busy)(cx) {
            self.state = UpdateState::Waiting(update);
            self.retry = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(BUSY_RETRY).await;
                this.update(cx, Self::begin_install).ok();
            }));
            cx.notify();
            return;
        }
        self.retry = None;
        let generation = self.generation;
        self.state = UpdateState::Installing(update.clone());
        let roots = self.roots.clone();
        let run = self.backend.install.clone();
        self.work = Some(cx.spawn(async move |this, cx| {
            let to_install = update.clone();
            let done = cx
                .background_spawn(async move { run(&to_install, &roots) })
                .await;
            this.update(cx, |this, cx| this.installed(generation, &update, done, cx))
                .ok();
        }));
        cx.notify();
    }

    fn installed(
        &mut self,
        generation: u64,
        update: &Update,
        done: Result<Installed, UpdateError>,
        cx: &mut Context<'_, Self>,
    ) {
        if generation != self.generation {
            return;
        }
        match done {
            Ok(Installed::Restart) => {
                self.state = UpdateState::Installed(update.clone());
                self.dismissed = false;
                self.announce(
                    format!(
                        "Version {} is installed. Restart to finish.",
                        update.version
                    ),
                    cx,
                );
            }
            Ok(Installed::Quit) => {
                self.announce(
                    format!("Version {} is being installed.", update.version),
                    cx,
                );
                cx.quit();
            }
            Err(error) => {
                let reason = reason(&error);
                self.dismissed = false;
                self.announce(format!("The update failed: {reason}"), cx);
                self.state = UpdateState::Failed {
                    reason,
                    retry: Retry::Install(update.clone()),
                };
            }
        }
        cx.notify();
    }

    /// Restarts the application into the version that was installed, unless
    /// an import is running or a book is being edited: then it says so and
    /// waits to be asked again.
    fn restart(&mut self, cx: &mut Context<'_, Self>) {
        if (self.is_busy)(cx) {
            self.announce(RESTART_WAITS.to_owned(), cx);
        } else {
            cx.restart();
        }
    }

    /// Hides the bar until the next check.
    fn later(&mut self, cx: &mut Context<'_, Self>) {
        self.dismissed = true;
        cx.notify();
    }

    /// Does again what failed.
    fn try_again(&mut self, cx: &mut Context<'_, Self>) {
        let retry = match &self.state {
            UpdateState::Failed { retry, .. } => retry.clone(),
            _ => return,
        };
        match retry {
            Retry::Check => self.begin_check(true, cx),
            Retry::Install(_) => self.begin_install(cx),
        }
    }

    /// Announces update status away from the window controls and the update banner.
    fn announce(&self, message: String, cx: &mut Context<'_, Self>) {
        let window = self.window;
        cx.defer(move |cx| {
            window
                .update(cx, |_, window, cx| {
                    window.push_notification(
                        Notification::info(message)
                            .placement(Anchor::BottomCenter)
                            .mb(px(NOTIFICATION_BOTTOM_MARGIN))
                            .autohide(false),
                        cx,
                    );
                })
                .ok();
        });
    }

    /// The buttons of the bar, by id and label.
    fn buttons(&self, cx: &mut Context<'_, Self>) -> Vec<AnyElement> {
        let target = px(target_size());
        let primary = |id: &'static str, label: &'static str| {
            Button::new(id)
                .primary()
                .label(label)
                .min_h(target)
                .cursor_pointer()
        };
        let plain = |id: &'static str, label: &'static str| {
            Button::new(id).label(label).min_h(target).cursor_pointer()
        };
        match &self.state {
            UpdateState::Available(_) => vec![
                primary("update-install", "Update")
                    .on_click(cx.listener(|this, _, _, cx| this.begin_install(cx)))
                    .into_any_element(),
                plain("update-later", "Later")
                    .on_click(cx.listener(|this, _, _, cx| this.later(cx)))
                    .into_any_element(),
            ],
            UpdateState::Installed(_) => vec![
                primary("update-restart", "Restart")
                    .on_click(cx.listener(|this, _, _, cx| this.restart(cx)))
                    .into_any_element(),
            ],
            UpdateState::Failed { .. } => vec![
                primary("update-retry", "Try again")
                    .on_click(cx.listener(|this, _, _, cx| this.try_again(cx)))
                    .into_any_element(),
                plain("update-later", "Later")
                    .on_click(cx.listener(|this, _, _, cx| this.later(cx)))
                    .into_any_element(),
            ],
            _ => Vec::new(),
        }
    }
}

impl Render for UpdateBanner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        if !self.is_visible() {
            return div().into_any_element();
        }
        let theme = cx.theme();
        let (background, text) = (theme.secondary, theme.secondary_foreground);
        let border = theme.border;
        let message: SharedString = self.state.summary().into();
        h_flex()
            .id("update-banner")
            .role(Role::Region)
            .aria_label("Application update")
            .test_support()
            .w_full()
            .flex_none()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(border)
            .bg(background)
            .text_color(text)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(status_line("update-message", message)),
            )
            .children(self.buttons(cx))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
