//! The root of the window once the library is open: the screen in front and
//! the stack of screens behind it.

use std::path::PathBuf;

use gpui_kit::component::dialog::AlertDialog;
use gpui_kit::component::link::Link;
use gpui_kit::component::{WindowExt as _, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, ExternalPaths, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, TestSupportExt as _, Window,
    div,
};

use crate::entities::catalogue::Library;
#[cfg(feature = "self-update")]
use crate::features::auto_update::UpdateBanner;
use crate::features::import_book::BOOK_FILE_EXTENSIONS;
use crate::pages::editor::{EditorEvent, EditorPage};
use crate::pages::library::{LibraryEvent, LibraryPage};
use crate::pages::reader::{ReaderEvent, ReaderPage};
use crate::pages::settings::{SettingsEvent, SettingsPage};
use crate::shared::actions::{About, GoBack, OpenFile, OpenSettings};
use crate::shared::file_picker;
use crate::shared::model::BookKey;
use crate::shared::ui::announce;

/// The screen in front.
pub(crate) enum Screen {
    /// The shelf. It is the first screen and is never left behind for good:
    /// going back stops here.
    Library(Entity<LibraryPage>),
    /// One book, being read.
    Reader(Entity<ReaderPage>),
    /// One book, being edited.
    Editor(Entity<EditorPage>),
    /// The settings.
    Settings(Entity<SettingsPage>),
}

impl Screen {
    /// The focus handle of the page, which the root focuses when the screen
    /// comes to the front.
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self {
            Self::Library(page) => page.focus_handle(cx),
            Self::Reader(page) => page.focus_handle(cx),
            Self::Editor(page) => page.focus_handle(cx),
            Self::Settings(page) => page.focus_handle(cx),
        }
    }

    fn element(&self) -> AnyElement {
        match self {
            Self::Library(page) => page.clone().into_any_element(),
            Self::Reader(page) => page.clone().into_any_element(),
            Self::Editor(page) => page.clone().into_any_element(),
            Self::Settings(page) => page.clone().into_any_element(),
        }
    }
}

/// A screen, and the subscription that hears what it asks for.
struct Entry {
    screen: Screen,
    _subscription: Subscription,
}

/// Holds the current screen and a back stack, and navigates on the events of
/// the page in front.
///
/// Going back returns to the page entity that was left, not to a new one, so
/// the shelf keeps its place. Moving to a page moves the keyboard focus into
/// it; going back returns the focus to the page below.
pub(crate) struct Root {
    /// The screens, the library first, the one in front last.
    stack: Vec<Entry>,
    focus_handle: FocusHandle,
    /// The file dialog, held so that dropping the root cancels the wait.
    prompt: Option<Task<()>>,
}

impl Root {
    /// A root that shows the library.
    pub(crate) fn new(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let mut root = Self {
            stack: Vec::new(),
            focus_handle: cx.focus_handle(),
            prompt: None,
        };
        let page = cx.new(|cx| LibraryPage::new(window, cx));
        let subscription = cx.subscribe_in(&page, window, Self::on_library);
        root.enter(Screen::Library(page), subscription, window, cx);
        root
    }

    /// The screen in front.
    pub(crate) fn screen(&self) -> &Screen {
        // `new` pushes the library and `pop` never removes the first screen.
        &self.stack[self.stack.len() - 1].screen
    }

    /// The screen in front, for tests that look into its page.
    #[cfg(test)]
    pub(crate) fn stack_top(&self) -> Option<&Screen> {
        self.stack.last().map(|entry| &entry.screen)
    }

    /// How many screens there are, the one in front included.
    #[cfg(test)]
    pub(crate) fn depth(&self) -> usize {
        self.stack.len()
    }

    /// The library page, which is always the first screen.
    pub(crate) fn library_page(&self) -> Option<&Entity<LibraryPage>> {
        match self.stack.first().map(|entry| &entry.screen) {
            Some(Screen::Library(page)) => Some(page),
            _ => None,
        }
    }

    /// Puts a screen in front and moves the focus into it.
    fn enter(
        &mut self,
        screen: Screen,
        subscription: Subscription,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let focus = screen.focus_handle(cx);
        self.stack.push(Entry {
            screen,
            _subscription: subscription,
        });
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Leaves the screen in front and returns to the one below, with the
    /// focus. The library stays: there is nowhere to go back to from it.
    pub(crate) fn pop(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.stack.len() <= 1 {
            return;
        }
        let Some(left) = self.stack.pop() else {
            return;
        };
        if matches!(left.screen, Screen::Editor(_)) {
            Editing::change(cx, -1);
        }
        let top = &self.stack[self.stack.len() - 1].screen;
        // A book that was closed for the editor is opened again.
        if let (Screen::Editor(_), Screen::Reader(reader)) = (&left.screen, top) {
            reader.update(cx, |reader, cx| reader.resume(window, cx));
        }
        window.focus(&top.focus_handle(cx), cx);
        cx.notify();
    }

    fn open_reader(&mut self, key: BookKey, window: &mut Window, cx: &mut Context<'_, Self>) {
        let page = cx.new(|cx| ReaderPage::new(key, window, cx));
        let subscription = cx.subscribe_in(&page, window, Self::on_reader);
        self.enter(Screen::Reader(page), subscription, window, cx);
    }

    fn open_editor(
        &mut self,
        key: BookKey,
        start_writing: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        Editing::change(cx, 1);
        let page = cx.new(|cx| {
            let mut page = EditorPage::new(key, window, cx);
            if start_writing {
                page.open_first_chapter();
            }
            page
        });
        let subscription = cx.subscribe_in(&page, window, Self::on_editor);
        self.enter(Screen::Editor(page), subscription, window, cx);
    }

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if matches!(self.screen(), Screen::Settings(_)) {
            return;
        }
        let page = cx.new(|cx| SettingsPage::new(window, cx));
        let subscription = cx.subscribe_in(&page, window, Self::on_settings);
        self.enter(Screen::Settings(page), subscription, window, cx);
    }

    fn on_library(
        &mut self,
        _: &Entity<LibraryPage>,
        event: &LibraryEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match *event {
            LibraryEvent::Open(key) => self.open_reader(key, window, cx),
            LibraryEvent::Edit(key) => self.open_editor(key, false, window, cx),
            LibraryEvent::Write(key) => {
                if matches!(self.screen(), Screen::Library(_)) {
                    self.open_editor(key, true, window, cx);
                }
            }
            LibraryEvent::Settings => self.open_settings(window, cx),
        }
    }

    fn on_reader(
        &mut self,
        reader: &Entity<ReaderPage>,
        event: &ReaderEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match *event {
            ReaderEvent::Close => self.pop(window, cx),
            ReaderEvent::Edit(key) => {
                // The file must be closed before the editor replaces it.
                reader.update(cx, ReaderPage::suspend);
                self.open_editor(key, false, window, cx);
            }
        }
    }

    fn on_editor(
        &mut self,
        _: &Entity<EditorPage>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match *event {
            EditorEvent::Close => self.pop(window, cx),
            EditorEvent::Saved(_) => {
                Library::global(cx).update(cx, Library::reload);
            }
        }
    }

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "GPUI hands an event to its subscriber by reference"
    )]
    fn on_settings(
        &mut self,
        _: &Entity<SettingsPage>,
        event: &SettingsEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match event {
            SettingsEvent::Close => self.pop(window, cx),
        }
    }

    /// Hands files that were chosen or dropped to the import of the library.
    pub(crate) fn import(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if paths.is_empty() {
            return;
        }
        if let Some(page) = self.library_page() {
            page.update(cx, |page, cx| page.import(paths, window, cx));
        }
    }

    /// Asks which files to add, in the dialog of the platform.
    fn on_open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.prompt = Some(cx.spawn_in(window, async move |this, cx| {
            match file_picker::choose("Add books", "Books", BOOK_FILE_EXTENSIONS, true).await {
                Ok(Some(paths)) => {
                    this.update_in(cx, |this, window, cx| this.import(paths, window, cx))
                        .ok();
                }
                Ok(None) => {}
                Err(error) => {
                    this.update_in(cx, |_, window, cx| announce(error.to_string(), window, cx))
                        .ok();
                }
            }
        }));
    }

    fn on_open_settings(
        &mut self,
        _: &OpenSettings,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.open_settings(window, cx);
    }

    fn on_go_back(&mut self, _: &GoBack, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.pop(window, cx);
    }
}

/// The licence of the application, as the dialog says it.
const LICENCE: &str = "MIT or Apache-2.0";
/// The repository of the application.
const REPOSITORY: &str = "https://github.com/M-Compressors/mbook";

/// The text of a line of the dialog, reported with its text as its name.
fn about_line(id: &'static str, text: impl Into<gpui_kit::SharedString>) -> impl IntoElement {
    let text = text.into();
    div()
        .id(id)
        .role(Role::Label)
        .aria_label(text.clone())
        .test_support()
        .child(text)
}

/// Tells what the application is, in a dialog: its name, its version, its
/// licence and where its repository is.
pub(crate) fn show_about(_: &About, window: &mut Window, cx: &mut App) {
    window.open_alert_dialog(cx, |alert: AlertDialog, _, _| {
        alert
            .title("mbook")
            .description(
                v_flex()
                    .gap_1()
                    .child(about_line(
                        "about-version",
                        format!("Version {}", env!("CARGO_PKG_VERSION")),
                    ))
                    .child(about_line("about-licence", LICENCE))
                    .child(
                        div()
                            .id("about-repository")
                            .role(Role::Link)
                            .aria_label(REPOSITORY)
                            .test_support()
                            .child(
                                Link::new("about-repository-link")
                                    .href(REPOSITORY)
                                    .child(REPOSITORY),
                            ),
                    ),
            )
            .ok_text("Close")
    });
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        // The bar that offers an update stands above every page.
        #[cfg(feature = "self-update")]
        let banner = UpdateBanner::try_global(cx).map(IntoElement::into_any_element);
        #[cfg(not(feature = "self-update"))]
        let banner: Option<AnyElement> = None;
        v_flex()
            .id("root")
            .key_context("Root")
            .track_focus(&self.focus_handle)
            .size_full()
            .on_action(cx.listener(Self::on_go_back))
            .on_action(cx.listener(Self::on_open_file))
            .on_action(cx.listener(Self::on_open_settings))
            .on_action(show_about)
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.import(paths.paths().to_vec(), window, cx);
            }))
            .children(banner)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(self.screen().element()),
            )
    }
}

/// How many books are being edited.
///
/// An update is not installed, and the application is not restarted for one,
/// while a book is open in the editor: its unsaved changes would be lost.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Editing(i32);

impl gpui_kit::Global for Editing {}

impl Editing {
    /// Whether a book is open in the editor.
    #[cfg(any(feature = "self-update", test))]
    pub(crate) fn any(cx: &App) -> bool {
        cx.try_global::<Self>().is_some_and(|editing| editing.0 > 0)
    }

    /// Counts an editor that was opened (`1`) or left (`-1`).
    fn change(cx: &mut App, by: i32) {
        let count = cx.try_global::<Self>().map_or(0, |editing| editing.0);
        cx.set_global(Self(count.saturating_add(by).max(0)));
    }
}
