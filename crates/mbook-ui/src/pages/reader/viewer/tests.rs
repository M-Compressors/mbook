//! The viewer, in a headless window.

use std::sync::{Arc, Mutex};

use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, Entity, IntoElement, Modifiers, PlatformInput,
    Render, ScrollDelta, ScrollEvent, TestAppContext, Window, div, point, px,
};

use super::*;
use crate::entities::figure::{FigureKind, Palette, Setting, render};
use crate::entities::picture::Bitmap;
use crate::testing;

/// A window whose only content is a viewer, as a page holds it.
struct Host {
    viewer: Entity<PictureViewer>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        div().size_full().relative().child(self.viewer.clone())
    }
}

struct Started {
    window: AnyWindowHandle,
    viewer: Entity<PictureViewer>,
    closed: Arc<Mutex<usize>>,
}

/// A grey picture of `width` by `height` pixels.
fn picture(width: u32, height: u32) -> ViewerContent {
    let pixels = vec![128; width as usize * height as usize * 4];
    let image = Bitmap::from_rgba(width, height, pixels)
        .unwrap()
        .into_render_image();
    ViewerContent::Picture {
        image,
        pixels: (width, height),
    }
}

fn formula() -> ViewerContent {
    let figure = render(
        FigureKind::Formula,
        "E = mc^2",
        Setting::Block,
        Palette::PAPER,
    )
    .unwrap();
    ViewerContent::Figure {
        figure,
        text_size: 18.0,
    }
}

/// A viewer of `content` in a window of 800 by 700 pixels, with the focus.
/// The room for the picture is 752 by 524 pixels.
fn start(cx: &mut TestAppContext, content: ViewerContent) -> Started {
    let (window, host) = testing::open(cx, 800.0, 700.0, |_, cx| Host {
        viewer: cx.new(|cx| PictureViewer::new("A grey square", content, cx)),
    });
    let viewer = host.read_with(cx, |host, _| host.viewer.clone());
    let closed = Arc::new(Mutex::new(0));
    let count = Arc::clone(&closed);
    let subscription = cx.update(|cx| {
        cx.subscribe(&viewer, move |_, event: &ViewerEvent, _| {
            assert_eq!(*event, ViewerEvent::Close);
            *count.lock().unwrap() += 1;
        })
    });
    std::mem::forget(subscription);
    cx.update_window(window, |_, window, cx| {
        let handle = viewer.focus_handle(cx);
        window.focus(&handle, cx);
    })
    .unwrap();
    let started = Started {
        window,
        viewer,
        closed,
    };
    settle(&started, cx);
    started
}

fn settle(started: &Started, cx: &mut TestAppContext) {
    for _ in 0..4 {
        cx.run_until_parked();
        testing::frame(cx, started.window, |_, _| ());
    }
}

fn zoom(started: &Started, cx: &TestAppContext) -> f32 {
    started
        .viewer
        .read_with(cx, |viewer, _| viewer.zoom().unwrap())
}

fn click(started: &Started, id: &'static str, cx: &mut TestAppContext) {
    testing::frame(cx, started.window, move |window, cx| window.click(id, cx));
    settle(started, cx);
}

fn press(started: &Started, key: &'static str, cx: &mut TestAppContext) {
    testing::frame(cx, started.window, move |window, cx| window.press(key, cx));
    settle(started, cx);
}

fn closed(started: &Started) -> usize {
    *started.closed.lock().unwrap()
}

fn close_to(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.001
}

/// The fit of a picture of 400 by 300 pixels in the room of the window.
const FIT: f32 = 524.0 / 300.0;

#[gpui_kit::test]
fn a_picture_fits_the_window_at_first(cx: &mut TestAppContext) {
    let started = start(cx, picture(400, 300));
    assert!(close_to(zoom(&started, cx), FIT));
    let (image, percent) = testing::frame(cx, started.window, |window, _| {
        (
            window.find("picture-viewer-image").bounds().size,
            window
                .find("picture-viewer-zoom")
                .label()
                .map(str::to_owned),
        )
    });
    assert!(
        (image.width.as_f32() - 400.0 * FIT).abs() < 0.5,
        "{image:?}"
    );
    assert_eq!(percent.as_deref(), Some("175%"));
}

#[gpui_kit::test]
fn the_buttons_enlarge_and_reduce_the_picture_a_step_at_a_time(cx: &mut TestAppContext) {
    let started = start(cx, picture(400, 300));
    click(&started, "picture-viewer-zoom-in", cx);
    assert!(close_to(zoom(&started, cx), FIT * 1.25));
    click(&started, "picture-viewer-zoom-out", cx);
    assert!(close_to(zoom(&started, cx), FIT));
}

#[gpui_kit::test]
fn reducing_stops_at_the_own_size_of_the_picture(cx: &mut TestAppContext) {
    let started = start(cx, picture(400, 300));
    for _ in 0..5 {
        click(&started, "picture-viewer-zoom-out", cx);
    }
    assert!(close_to(zoom(&started, cx), 1.0));
    let disabled = testing::frame(cx, started.window, |window, _| {
        window.find("picture-viewer-zoom-out").disabled()
    });
    assert_eq!(disabled, Some(true));
}

#[gpui_kit::test]
fn enlarging_stops_at_four_times_the_larger_of_fit_and_own_size(cx: &mut TestAppContext) {
    let started = start(cx, picture(400, 300));
    for _ in 0..12 {
        click(&started, "picture-viewer-zoom-in", cx);
    }
    assert!(close_to(zoom(&started, cx), FIT * 4.0));
    let disabled = testing::frame(cx, started.window, |window, _| {
        window.find("picture-viewer-zoom-in").disabled()
    });
    assert_eq!(disabled, Some(true));
}

#[gpui_kit::test]
fn fit_to_window_undoes_the_zoom(cx: &mut TestAppContext) {
    let started = start(cx, picture(400, 300));
    let disabled = |cx: &mut TestAppContext| {
        testing::frame(cx, started.window, |window, _| {
            window.find("picture-viewer-fit").disabled()
        })
    };
    assert_eq!(disabled(cx), Some(true), "it fits already");
    click(&started, "picture-viewer-zoom-in", cx);
    assert_ne!(disabled(cx), Some(true));
    click(&started, "picture-viewer-fit", cx);
    assert!(close_to(zoom(&started, cx), FIT));
    assert!(
        started
            .viewer
            .read_with(cx, |viewer, _| viewer.zoom.is_none())
    );
}

#[gpui_kit::test]
fn the_keys_enlarge_reduce_and_fit(cx: &mut TestAppContext) {
    let started = start(cx, picture(400, 300));
    press(&started, "=", cx);
    assert!(close_to(zoom(&started, cx), FIT * 1.25));
    press(&started, "=", cx);
    press(&started, "-", cx);
    assert!(close_to(zoom(&started, cx), FIT * 1.25));
    press(&started, "0", cx);
    assert!(close_to(zoom(&started, cx), FIT));
}

#[gpui_kit::test]
fn going_back_and_the_close_button_ask_to_close(cx: &mut TestAppContext) {
    let started = start(cx, picture(400, 300));
    cx.update_window(started.window, |_, window, cx| {
        window.dispatch_action(GoBack.boxed_clone(), cx);
    })
    .unwrap();
    assert_eq!(closed(&started), 1);
    click(&started, "picture-viewer-close", cx);
    assert_eq!(closed(&started), 2);
}

#[gpui_kit::test]
fn finding_is_not_opened_under_the_viewer(cx: &mut TestAppContext) {
    let started = start(cx, picture(400, 300));
    cx.update_window(started.window, |_, window, cx| {
        window.dispatch_action(Find.boxed_clone(), cx);
    })
    .unwrap();
    assert_eq!(closed(&started), 0);
}

#[gpui_kit::test]
fn a_click_beside_the_picture_asks_to_close_and_one_on_it_does_not(cx: &mut TestAppContext) {
    let started = start(cx, picture(400, 300));
    click(&started, "picture-viewer-image", cx);
    assert_eq!(closed(&started), 0);
    testing::frame(cx, started.window, |window, cx| {
        window.click_at("picture-viewer-area", point(px(4.0), px(4.0)), cx);
    });
    settle(&started, cx);
    assert_eq!(closed(&started), 1);
}

/// The scroll offset of the room of the picture.
fn offset(started: &Started, cx: &TestAppContext) -> (f32, f32) {
    started.viewer.read_with(cx, |viewer, _| {
        let offset = viewer.scroll.offset();
        (offset.x.as_f32(), offset.y.as_f32())
    })
}

#[gpui_kit::test]
fn an_enlarged_picture_grows_around_its_middle_and_is_moved_with_a_drag(cx: &mut TestAppContext) {
    let started = start(cx, picture(400, 300));
    for _ in 0..3 {
        click(&started, "picture-viewer-zoom-in", cx);
    }
    // 400 by 300 at FIT * 1.25^3: 1364 by 1023 in a room of 800 by 700.
    let shown = (400.0 * FIT * 1.953_125, 300.0 * FIT * 1.953_125);
    let (x, y) = offset(&started, cx);
    assert!((x - (400.0 - shown.0 / 2.0)).abs() < 1.0, "{x}");
    assert!((y - (350.0 - shown.1 / 2.0)).abs() < 1.0, "{y}");
    testing::frame(cx, started.window, |window, cx| {
        window.drag(point(px(400.0), px(350.0)), point(px(300.0), px(300.0)), cx);
    });
    settle(&started, cx);
    let (moved_x, moved_y) = offset(&started, cx);
    assert!((moved_x - (x - 100.0)).abs() < 1.0, "{moved_x}");
    assert!((moved_y - (y - 50.0)).abs() < 1.0, "{moved_y}");
    assert_eq!(closed(&started), 0, "the release of a drag closes nothing");
}

#[gpui_kit::test]
fn a_small_move_of_a_press_is_no_drag(cx: &mut TestAppContext) {
    let started = start(cx, picture(400, 300));
    for _ in 0..3 {
        click(&started, "picture-viewer-zoom-in", cx);
    }
    let before = offset(&started, cx);
    testing::frame(cx, started.window, |window, cx| {
        window.drag(point(px(400.0), px(350.0)), point(px(402.0), px(351.0)), cx);
    });
    settle(&started, cx);
    assert_eq!(offset(&started, cx), before);
}

#[gpui_kit::test]
fn the_wheel_zooms_only_with_the_secondary_key(cx: &mut TestAppContext) {
    let started = start(cx, picture(400, 300));
    testing::frame(cx, started.window, |window, cx| {
        window.scroll(
            "picture-viewer-area",
            ScrollDelta::Lines(point(0.0, 1.0)),
            cx,
        );
    });
    settle(&started, cx);
    assert!(close_to(zoom(&started, cx), FIT));
    testing::frame(cx, started.window, |window, cx| {
        window.change_modifiers(Modifiers::secondary_key(cx), cx);
        window.scroll(
            "picture-viewer-area",
            ScrollDelta::Lines(point(0.0, 1.0)),
            cx,
        );
    });
    settle(&started, cx);
    assert!(
        close_to(zoom(&started, cx), FIT * 1.1),
        "{}",
        zoom(&started, cx)
    );
}

#[gpui_kit::test]
fn a_pinch_enlarges_the_picture(cx: &mut TestAppContext) {
    let started = start(cx, picture(400, 300));
    testing::frame(cx, started.window, |window, cx| {
        window.dispatch_event(
            PlatformInput::Pinch(PinchEvent {
                position: point(px(400.0), px(350.0)),
                delta: 0.5,
                ..PinchEvent::default()
            }),
            cx,
        );
    });
    settle(&started, cx);
    assert!(close_to(zoom(&started, cx), FIT * 1.5));
}

#[test]
fn a_wheel_turned_away_enlarges_and_one_turned_back_reduces() {
    let event = |delta| ScrollEvent {
        delta,
        ..ScrollEvent::default()
    };
    assert!(close_to(
        wheel_factor(&event(ScrollDelta::Lines(point(0.0, 2.0)))),
        1.21
    ));
    let back = wheel_factor(&event(ScrollDelta::Pixels(point(px(0.0), px(-10.0)))));
    assert!(back < 1.0 && close_to(back, 1.01_f32.powf(-10.0)));
}

#[gpui_kit::test]
fn a_picture_that_is_decoding_shows_that_it_is_coming(cx: &mut TestAppContext) {
    let started = start(cx, ViewerContent::Loading((400, 300)));
    assert!(close_to(zoom(&started, cx), FIT), "the room is kept for it");
    let image = testing::frame(cx, started.window, |window, _| {
        window.try_find("picture-viewer-image").is_some()
    });
    assert!(!image);
    started
        .viewer
        .update(cx, |viewer, cx| viewer.set_content(picture(400, 300), cx));
    settle(&started, cx);
    let image = testing::frame(cx, started.window, |window, _| {
        window.try_find("picture-viewer-image").is_some()
    });
    assert!(image);
}

#[gpui_kit::test]
fn a_picture_that_failed_says_so(cx: &mut TestAppContext) {
    let started = start(cx, ViewerContent::Failed);
    assert!(
        started
            .viewer
            .read_with(cx, |viewer, _| viewer.zoom().is_none())
    );
    let text = testing::frame(cx, started.window, |window, _| {
        window.find("picture-viewer").label().map(str::to_owned)
    });
    assert_eq!(text.as_deref(), Some("A grey square"));
    // Nothing to zoom: the steps do nothing.
    click(&started, "picture-viewer-zoom-in", cx);
    assert!(
        started
            .viewer
            .read_with(cx, |viewer, _| viewer.zoom.is_none())
    );
}

#[gpui_kit::test]
fn a_figure_is_shown_at_the_size_of_the_text_and_drawn_for_its_zoom(cx: &mut TestAppContext) {
    let started = start(cx, formula());
    // A formula one line high is enlarged four times to fit.
    assert!(close_to(zoom(&started, cx), 4.0));
    let first = started.viewer.read_with(cx, |viewer, _| {
        viewer.rasters.current.as_ref().map(|(key, _)| *key)
    });
    assert!(first.is_some());
    click(&started, "picture-viewer-zoom-in", cx);
    let (current, previous) = started.viewer.read_with(cx, |viewer, _| {
        (
            viewer.rasters.current.as_ref().map(|(key, _)| *key),
            viewer.rasters.previous.is_some(),
        )
    });
    // From 4 to 5 times: drawn again, with the old drawing standing in.
    assert_ne!(current, first);
    assert!(previous);
}

#[gpui_kit::test]
fn a_figure_is_drawn_again_only_for_a_new_density(cx: &mut TestAppContext) {
    let started = start(cx, formula());
    let ViewerContent::Figure { figure, .. } = formula() else {
        unreachable!();
    };
    started.viewer.update(cx, |viewer, cx| {
        let (one, _) = viewer.raster(&figure, 3.0, cx);
        let (again, previous) = viewer.raster(&figure, 3.0, cx);
        assert!(Arc::ptr_eq(&one, &again));
        let (other, previous_now) = viewer.raster(&figure, 6.0, cx);
        assert!(!Arc::ptr_eq(&one, &other));
        assert!(previous_now.is_some_and(|previous| Arc::ptr_eq(&previous, &one)));
        drop(previous);
        viewer.raster(&figure, 12.0, cx);
        viewer.release(cx);
        assert!(viewer.rasters.current.is_none() && viewer.rasters.previous.is_none());
    });
}

#[gpui_kit::test]
fn releasing_a_picture_lets_its_image_go(cx: &mut TestAppContext) {
    let started = start(cx, picture(40, 30));
    started.viewer.update(cx, |viewer, cx| {
        viewer.release(cx);
        assert!(matches!(viewer.content(), ViewerContent::Picture { .. }));
    });
}

#[gpui_kit::test]
fn a_picture_without_words_is_called_a_picture(cx: &mut TestAppContext) {
    let (window, host) = testing::open(cx, 800.0, 700.0, |_, cx| Host {
        viewer: cx.new(|cx| PictureViewer::new("", picture(40, 30), cx)),
    });
    let _ = host;
    let label = testing::frame(cx, window, |window, _| {
        window.find("picture-viewer").label().map(str::to_owned)
    });
    assert_eq!(label.as_deref(), Some("Picture"));
}

#[gpui_kit::test]
fn a_picture_that_is_decoding_spins_when_motion_is_allowed(cx: &mut TestAppContext) {
    let started = start(cx, ViewerContent::Loading((400, 300)));
    cx.update(|cx| cx.set_reduce_motion(false));
    settle(&started, cx);
    let image = testing::frame(cx, started.window, |window, _| {
        window.try_find("picture-viewer-image").is_some()
    });
    assert!(!image);
}

#[gpui_kit::test]
fn a_press_released_outside_the_window_ends_the_drag(cx: &mut TestAppContext) {
    let started = start(cx, picture(400, 300));
    for _ in 0..3 {
        click(&started, "picture-viewer-zoom-in", cx);
    }
    let before = offset(&started, cx);
    testing::frame(cx, started.window, |window, cx| {
        window.pointer_down(point(px(400.0), px(350.0)), MouseButton::Left, cx);
        window.pointer_move(point(px(300.0), px(300.0)), None, cx);
    });
    settle(&started, cx);
    assert_eq!(offset(&started, cx), before);
    assert!(
        started
            .viewer
            .read_with(cx, |viewer, _| viewer.press.is_none())
    );
}

#[gpui_kit::test]
fn a_pinch_that_does_not_change_the_size_moves_nothing(cx: &mut TestAppContext) {
    let started = start(cx, picture(400, 300));
    testing::frame(cx, started.window, |window, cx| {
        window.dispatch_event(
            PlatformInput::Pinch(PinchEvent {
                position: point(px(10.0), px(10.0)),
                delta: 0.0,
                ..PinchEvent::default()
            }),
            cx,
        );
    });
    settle(&started, cx);
    assert!(
        started
            .viewer
            .read_with(cx, |viewer, _| viewer.zoom.is_none())
    );
}
