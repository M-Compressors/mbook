//! What the window shows while the library opens, if it cannot be opened, and
//! once it is.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Pixels, Render,
    Styled as _, Subscription, Task, Window,
};
use rust_i18n::t;
use std::borrow::Cow;
use std::error::Error as _;

use super::open::{OpenError, Opened as OpenedCatalogue, open_catalogue};
use super::opened::Opened;
use super::root::Root;
use crate::entities::catalogue::{Library, Store};
use crate::features::appearance::Appearance;
#[cfg(feature = "self-update")]
use crate::features::auto_update::UpdateBanner;
use crate::shared::roots::Roots;
use crate::shared::ui::{announce, empty_state, fit_notifications, status_line};

/// What the user is told once when the index of the library could not be read
/// and was made again from the books.
pub(super) fn damaged_notice() -> Cow<'static, str> {
    t!("app.damaged_notice")
}

/// Where the start of the application is.
enum State {
    /// The catalogue is opening, or the library and the settings are being
    /// read.
    Opening,
    /// The library could not be opened.
    Failed(OpenError),
    /// The library is open.
    Ready(Entity<Root>),
}

/// The view at the root of the window: it opens the library, then shows the
/// [`Root`].
///
/// No file is touched on the main thread: the folders, the catalogue, the rows
/// and the settings are read on the background executor, and until they have
/// arrived the window shows that it is opening.
pub(crate) struct Shell {
    roots: Roots,
    state: State,
    /// The models that must have read their data before the first page is
    /// built.
    models: Option<(Entity<Library>, Entity<Appearance>)>,
    /// The opening in flight, held so that dropping the shell cancels it.
    opening: Option<Task<()>>,
    subscriptions: Vec<Subscription>,
    /// The width and top margin this window last gave the notifications.
    notifications: Option<(Pixels, Pixels)>,
}

impl Shell {
    /// Starts opening the library kept in `roots`.
    pub(crate) fn new(roots: Roots, window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let mut shell = Self {
            roots,
            state: State::Opening,
            models: None,
            opening: None,
            notifications: None,
            subscriptions: vec![
                cx.observe_window_appearance(window, |_, window, cx| {
                    if let Some(appearance) = Appearance::try_global(cx) {
                        appearance.update(cx, |appearance, cx| {
                            appearance.set_system(window.appearance(), cx);
                        });
                    }
                }),
                // Files handed over while the window is open.
                cx.observe_global_in::<Opened>(window, |this, window, cx| {
                    this.hand_over(window, cx);
                }),
                cx.observe_window_bounds(window, |_, window, cx| {
                    if let Some(appearance) = Appearance::try_global(cx) {
                        appearance.update(cx, |appearance, cx| {
                            appearance.observe_viewport(window, cx);
                        });
                    }
                }),
            ],
        };
        shell.open(window, cx);
        shell
    }

    /// Whether the library is open and the first page is built.
    #[cfg(any(test, feature = "visual"))]
    pub(crate) fn root(&self) -> Option<&Entity<Root>> {
        match &self.state {
            State::Ready(root) => Some(root),
            State::Opening | State::Failed(_) => None,
        }
    }

    /// Whether the library could not be opened.
    #[cfg(test)]
    pub(crate) fn failed(&self) -> bool {
        matches!(self.state, State::Failed(_))
    }

    /// Opens the catalogue on the background; the result comes to
    /// [`Self::opened`].
    fn open(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.state = State::Opening;
        let roots = self.roots.clone();
        self.opening = Some(cx.spawn_in(window, async move |this, cx| {
            let opened = cx
                .background_spawn(async move { open_catalogue(&roots) })
                .await;
            this.update_in(cx, |this, window, cx| this.opened(opened, window, cx))
                .ok();
        }));
        cx.notify();
    }

    fn opened(
        &mut self,
        opened: Result<OpenedCatalogue, OpenError>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match opened {
            Err(error) => self.state = State::Failed(error),
            Ok(OpenedCatalogue { catalogue, damaged }) => {
                if damaged {
                    announce(damaged_notice(), window, cx);
                }
                Store {
                    roots: self.roots.clone(),
                    catalogue,
                }
                .install(cx);
                let library = cx.new(Library::new);
                let appearance = cx.new(Appearance::new);
                Library::install(library.clone(), cx);
                Appearance::install(appearance.clone(), cx);
                self.subscriptions
                    .push(cx.observe_in(&library, window, Self::refresh));
                self.subscriptions
                    .push(cx.observe_in(&appearance, window, Self::refresh));
                self.models = Some((library, appearance));
                self.refresh_now(window, cx);
            }
        }
        cx.notify();
    }

    fn refresh<T>(&mut self, _: Entity<T>, window: &mut Window, cx: &mut Context<'_, Self>) {
        // The language may have changed with the settings.
        super::keys::sync_menus(cx);
        self.refresh_now(window, cx);
    }

    /// Builds the root once the library and the settings have been read; and
    /// draws again when the settings changed.
    fn refresh_now(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if matches!(self.state, State::Ready(_)) {
            cx.notify();
            return;
        }
        let Some((library, appearance)) = &self.models else {
            return;
        };
        if library.read(cx).is_loading() || !appearance.read(cx).is_loaded() {
            return;
        }
        let appearance = appearance.clone();
        appearance.update(cx, |appearance, cx| {
            appearance.set_system(window.appearance(), cx);
            appearance.observe_viewport(window, cx);
        });
        #[cfg(feature = "self-update")]
        {
            let roots = self.roots.clone();
            let banner = cx.new(|cx| UpdateBanner::new(roots, is_busy, window, cx));
            UpdateBanner::install(banner, cx);
        }
        let root = cx.new(|cx| Root::new(window, cx));
        self.state = State::Ready(root);
        self.hand_over(window, cx);
        cx.notify();
    }

    /// Gives the files handed over by the system to the library, once it is
    /// open; until then they wait.
    fn hand_over(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let State::Ready(root) = &self.state else {
            return;
        };
        let files = Opened::take(cx);
        if !files.is_empty() {
            root.update(cx, |root, cx| root.import(files, window, cx));
        }
    }

    fn loading(cx: &Context<'_, Self>) -> AnyElement {
        let theme = cx.theme();
        let indicator = if cx.reduce_motion() {
            Icon::new(IconName::Loader).large().into_any_element()
        } else {
            Spinner::new().large().into_any_element()
        };
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .bg(theme.background)
            .text_color(theme.muted_foreground)
            .child(indicator)
            .child(status_line("opening-status", t!("app.opening")))
            .into_any_element()
    }

    fn failed_view(error: &OpenError, cx: &mut Context<'_, Self>) -> AnyElement {
        let mut reason = error.to_string();
        let mut source = error.source();
        while let Some(cause) = source {
            reason.push_str(": ");
            reason.push_str(&cause.to_string());
            source = cause.source();
        }
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                empty_state(
                    t!("app.open_failed"),
                    t!("app.open_failed_hint", reason = reason),
                )
                .action(
                    Button::new("retry")
                        .primary()
                        .label(t!("app.try_again"))
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, window, cx| this.open(window, cx))),
                ),
            )
            .into_any_element()
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        if let Some(appearance) = Appearance::try_global(cx) {
            appearance.read(cx).sync_window(window);
        }
        fit_notifications(window, &mut self.notifications, cx);
        match &self.state {
            State::Opening => Self::loading(cx),
            State::Failed(error) => Self::failed_view(error, cx),
            State::Ready(root) => root.clone().into_any_element(),
        }
    }
}

/// Whether an import is running or a book is being edited. An update waits
/// for both: it must not replace the application under a conversion, nor
/// restart it over unsaved changes.
#[cfg(feature = "self-update")]
fn is_busy(cx: &gpui_kit::App) -> bool {
    let importing = crate::features::import_book::ImportQueue::try_global(cx)
        .is_some_and(|queue| !queue.read(cx).is_idle());
    importing || super::root::Editing::any(cx)
}
