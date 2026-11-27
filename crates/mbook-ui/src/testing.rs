//! What the tests of several modules share: a library in a temporary root.

use std::fs;
use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, IntoElement, Point, Render,
    TestAppContext, Window, WindowBounds, WindowOptions, px, size,
};
use tempfile::TempDir;

use crate::entities::catalogue::{BookRow, Catalogue, Store};
use crate::shared::model::BookKey;
use crate::shared::roots::Roots;

/// A library in a temporary folder: its roots and its open catalogue.
pub(crate) struct Fixture {
    /// Keeps the folder until the test ends.
    pub(crate) _dir: TempDir,
    pub(crate) roots: Roots,
    pub(crate) catalogue: Arc<Catalogue>,
}

impl Fixture {
    /// Prepares the roots and opens a catalogue in them.
    pub(crate) fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let roots = Roots::new(dir.path().join("data"), dir.path().join("cache"));
        roots.prepare().unwrap();
        let catalogue = Arc::new(Catalogue::open(&roots.catalogue()).unwrap());
        Self {
            _dir: dir,
            roots,
            catalogue,
        }
    }

    /// Adds a row for a book called `title`, with a file and a thumbnail of
    /// made-up bytes, and returns its key.
    pub(crate) fn add_book(&self, title: &str) -> BookKey {
        let key = BookKey::generate();
        let row = BookRow {
            key,
            id: format!("urn:test:{title}"),
            title: title.to_owned(),
            authors: vec!["Ann".to_owned()],
            language: "en".to_owned(),
            series: None,
            file_len: 4,
            cover: None,
            added_ms: 1,
            source_format: None,
        };
        self.catalogue.put_book(&row).unwrap();
        fs::write(self.roots.book(key), b"book").unwrap();
        fs::write(self.roots.cover(key), b"jpeg").unwrap();
        key
    }

    /// Makes the store of this library the one pages reach.
    pub(crate) fn install(&self, cx: &mut TestAppContext) {
        cx.update(|cx: &mut App| {
            Store {
                roots: self.roots.clone(),
                catalogue: self.catalogue.clone(),
            }
            .install(cx);
        });
    }
}

/// Opens a headless window `width` by `height` pixels holding a view, with the
/// Kit initialised and motion reduced so that nothing waits for a clock.
pub(crate) fn open<V: Render>(
    cx: &mut TestAppContext,
    width: f32,
    height: f32,
    build: impl FnOnce(&mut Window, &mut Context<'_, V>) -> V,
) -> (AnyWindowHandle, Entity<V>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
        let bounds = Bounds {
            origin: Point::default(),
            size: size(px(width), px(height)),
        };
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..WindowOptions::default()
            },
            cx,
            |window, cx| cx.new(|cx| build(window, cx)),
        )
        .unwrap()
    })
}

/// Draws a frame of the window and lets `inspect` look at it.
pub(crate) fn frame<R>(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    inspect: impl FnOnce(&mut Window, &mut App) -> R,
) -> R {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        inspect(window, cx)
    })
    .unwrap()
}

/// A view that draws nothing, to give a model a window to be asked about.
pub(crate) struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        gpui_kit::div()
    }
}

/// Whether an element without a role is a part of a Kit control, not a
/// control of this application.
///
/// The accessibility tests demand a role of everything the application
/// reports, so that a clickable element of ours cannot go unnamed. The Kit
/// reports a few parts of its own controls without one: the frame around a
/// text field, the search field inside a select, the pieces of a slider's
/// track, the stack notifications appear in, the backdrop of a sheet. GPUI
/// tells assistive technology only about elements with a role, so these
/// reach nobody; the controls they belong to have their roles and names.
/// The Kit's window buttons also lack roles in the headless tree, and so do
/// the two groups that hold them on Linux, at the start and the end of the
/// title bar; the one at the start is empty unless the window manager puts
/// buttons there. On Windows, GPUI handles them as part of the non-client
/// area of the window.
pub(crate) fn is_kit_decoration(path: &str) -> bool {
    const PARTS: [&str; 8] = [
        "gpui_component::input",
        "Name(\"input\")",
        "NamedInteger(\"input\"",
        "slider::SliderTrack",
        "progress::ProgressTrack",
        "progress::ProgressIndicator",
        "ToastStack",
        "Sheet",
    ];
    PARTS.iter().any(|part| path.contains(part))
        || (path.contains("gpui_component::title_bar::TitleBar")
            && [
                "Name(\"title-bar\")]",
                "Name(\"bar\")]",
                "Name(\"window-controls\")]",
                "Name(\"window-controls-start\")]",
            ]
            .iter()
            .any(|part| path.ends_with(part)))
        || path
            .rsplit_once(r#"Name("gpui_component::title_bar::ControlIcon"), "#)
            .is_some_and(|(_, control)| {
                matches!(
                    control,
                    r#"Name("minimize")]"#
                        | r#"Name("maximize")]"#
                        | r#"Name("restore")]"#
                        | r#"Name("close")]"#
                )
            })
}

#[test]
fn kit_window_controls_are_outside_the_page_accessibility_checks() {
    for control in ["minimize", "maximize", "restore", "close"] {
        let path = format!(
            r#"[Name("gpui_component::title_bar::TitleBar"), Name("title-bar"), Name("gpui_component::title_bar::WindowControls"), Name("window-controls"), Name("gpui_component::title_bar::ControlIcon"), Name("{control}")]"#
        );
        assert!(is_kit_decoration(&path), "{path}");
    }
}

#[test]
fn both_groups_of_kit_window_controls_are_outside_the_page_accessibility_checks() {
    for group in ["window-controls-start", "window-controls"] {
        let path = format!(
            r#"[Name("gpui_component::window_border::WindowBorder"), Name("window-backdrop"), Name("root"), Name("library-page"), Name("mbook_ui::shared::ui::page_header::PageHeader"), Name("gpui_component::title_bar::TitleBar"), Name("title-bar"), Name("gpui_component::title_bar::WindowControls"), Name("{group}")]"#
        );
        assert!(is_kit_decoration(&path), "{path}");
    }
}

#[test]
fn a_page_element_named_like_a_window_control_group_stays_in_the_checks() {
    let path = r#"[Name("library-page"), Name("window-controls-start")]"#;
    assert!(!is_kit_decoration(path), "{path}");
}

#[test]
fn a_page_control_named_close_stays_in_the_accessibility_checks() {
    let path = r#"[Name("gpui_component::title_bar::TitleBar"), Name("title-bar"), Name("bar"), Name("close")]"#;
    assert!(!is_kit_decoration(path), "{path}");
}
