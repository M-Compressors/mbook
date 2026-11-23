//! Text that is announced when it changes.
//!
//! What GPUI Fast 0.1.17 offers, as found in its source: an element has no
//! `aria_live` property (the `aria` properties of an element are the label,
//! description, value, selection, expansion, toggle, numeric range, level,
//! position and table indexes), and nothing sets AccessKit's `live` field.
//! What exists is the role. `Role::Status` is reported to macOS as
//! `AXApplicationStatus`, and `Role::Alert` as `AXApplicationAlert`; the Kit's
//! own `Alert` and its notifications use `Alert`. So the application uses
//!
//! - [`status_line`], an element with the role `Status` whose name and text
//!   are the message, for a message that stays on the page (the step of an
//!   import, "3 books"), and
//! - [`announce`], which pushes a notification of the Kit (role `Alert`) for
//!   what the user did not cause: an import that finished or failed, an update
//!   that is ready.
//!
//! Whether a platform speaks either at once depends on its adapter; this was
//! read in the sources, not heard through a screen reader.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::component::notification::{Notification, NotificationSettings};
use gpui_kit::component::{Theme, WindowExt as _};
use gpui_kit::{
    App, DispatchPhase, ElementId, Hitbox, HitboxBehavior, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Role, ScrollDelta, ScrollEvent, SharedString,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, TouchPhase, Window, canvas, div,
    px,
};

use super::swipe::Sideways;
use crate::shared::safe_area;

/// An element that says `text`, to be announced by assistive technology when
/// it changes. Style it like any `div`.
///
/// `id` must stay the same while the element means the same thing, so that a
/// change of its text is heard as a change and not as a new element.
pub(crate) fn status_line(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
) -> impl IntoElement + Styled {
    let text = text.into();
    div()
        .id(id)
        .role(Role::Status)
        .aria_label(text.clone())
        .child(text)
        .test_support()
}

/// Tells the user, and assistive technology, something that did not follow
/// from what they just did. The notification stays until it is dismissed:
/// with its close button, which shows when a pointer is over it, or with a
/// tap or a sideways swipe, which a touch screen has instead.
pub(crate) fn announce(message: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
    let swipe = Rc::new(RefCell::new(Sideways::default()));
    let notification = Notification::info(message)
        .autohide(false)
        // A notification with a click handler is dismissed by a click.
        .on_click(|_, _, _| {})
        .content(move |_, _, cx| {
            let notification = cx.entity().downgrade();
            let swipe = Rc::clone(&swipe);
            canvas(
                |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
                move |_, hitbox: Hitbox, window, _| {
                    let notification = notification.clone();
                    let swipe = Rc::clone(&swipe);
                    window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                        if phase != DispatchPhase::Capture
                            || !hitbox.is_hovered_at(event.position, window)
                        {
                            return;
                        }
                        // The view under the notification does not take the
                        // swipe for its own.
                        cx.stop_propagation();
                        let ScrollDelta::Pixels(pixels) = event.delta else {
                            return;
                        };
                        let begins = event.touch_phase == TouchPhase::Started;
                        let now = cx.background_executor().now();
                        let swiped = swipe.borrow_mut().feed(
                            pixels.x.as_f32(),
                            pixels.y.as_f32(),
                            begins,
                            now,
                        );
                        if swiped.is_some() {
                            notification
                                .update(cx, |notification, cx| notification.dismiss(window, cx))
                                .ok();
                        }
                    });
                },
            )
            // Over the whole notification, which is the nearest positioned
            // element, without taking a click or a hover from it.
            .absolute()
            .inset_0()
            .into_any_element()
        });
    window.push_notification(notification, cx);
}

/// Fits the notifications of the Kit to `window`: as wide as the window
/// allows, up to their own width, and below the system bars of a phone.
///
/// The settings belong to the whole application. `last` is what this window
/// fitted before: the settings are written only when the window's own
/// geometry changed, so that two windows do not take turns overwriting them.
pub(crate) fn fit_notifications(
    window: &Window,
    last: &mut Option<(Pixels, Pixels)>,
    cx: &mut App,
) {
    let defaults = NotificationSettings::default();
    let top = safe_area::insets(window).top;
    let fit = fitted(window.viewport_size().width, top, &defaults);
    if *last == Some(fit) {
        return;
    }
    *last = Some(fit);
    let (width, margin) = fit;
    Theme::update(cx, |theme| {
        theme.notification.width = width;
        theme.notification.margins.top = margin;
    });
}

/// The width and the top margin of notifications in a window `width` wide
/// whose system bars cover `top` of it.
fn fitted(width: Pixels, top: Pixels, defaults: &NotificationSettings) -> (Pixels, Pixels) {
    let room = width - defaults.margins.left - defaults.margins.right;
    (
        room.min(defaults.width).max(px(0.0)),
        defaults.margins.top + top,
    )
}

#[cfg(test)]
mod tests {
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AppContext as _, Context, Render, TestAppContext};

    use super::*;
    use crate::testing;

    struct Host {
        text: SharedString,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
            div().child(status_line("import-status", self.text.clone()))
        }
    }

    #[gpui_kit::test]
    fn a_status_line_is_a_status_named_by_its_text(cx: &mut TestAppContext) {
        let (window, _) = testing::open(cx, 400.0, 200.0, |_, _| Host {
            text: "Converting".into(),
        });
        testing::frame(cx, window, |window, _| {
            let line = window.find("import-status");
            assert_eq!(line.role(), Some(Role::Status));
            assert_eq!(line.label(), Some("Converting"));
        });
    }

    #[gpui_kit::test]
    fn a_status_line_keeps_its_place_when_its_text_changes(cx: &mut TestAppContext) {
        let (window, host) = testing::open(cx, 400.0, 200.0, |_, _| Host {
            text: "Converting".into(),
        });
        let before = testing::frame(cx, window, |window, _| {
            window.find("import-status").path().to_vec()
        });
        host.update(cx, |host, cx| {
            host.text = "Done".into();
            cx.notify();
        });
        testing::frame(cx, window, |window, _| {
            let line = window.find("import-status");
            assert_eq!(line.path(), before.as_slice());
            assert_eq!(line.label(), Some("Done"));
        });
    }

    #[gpui_kit::test]
    fn an_announcement_is_an_alert_that_stays(cx: &mut TestAppContext) {
        let (window, _) = testing::open(cx, 400.0, 200.0, |_, _| Host {
            text: String::new().into(),
        });
        cx.update_window(window, |_, window, cx| announce("Added Dune", window, cx))
            .unwrap();
        testing::frame(cx, window, |window, _| {
            assert!(window.find("notification").visible());
        });
    }

    /// Lets the notification list settle after a dismissal: the Kit plays
    /// its leaving transition before it removes one.
    fn settle(window: gpui_kit::AnyWindowHandle, cx: &mut TestAppContext) {
        for _ in 0..4 {
            cx.executor()
                .advance_clock(std::time::Duration::from_secs(1));
            cx.run_until_parked();
            testing::frame(cx, window, |_, _| ());
        }
    }

    fn notifications(window: gpui_kit::AnyWindowHandle, cx: &mut TestAppContext) -> usize {
        testing::frame(cx, window, |window, cx| window.notifications(cx).len())
    }

    fn announced(cx: &mut TestAppContext) -> gpui_kit::AnyWindowHandle {
        let (window, _) = testing::open(cx, 400.0, 200.0, |_, _| Host {
            text: String::new().into(),
        });
        cx.update_window(window, |_, window, cx| announce("Added Dune", window, cx))
            .unwrap();
        testing::frame(cx, window, |_, _| ());
        window
    }

    #[gpui_kit::test]
    fn a_tap_on_an_announcement_dismisses_it(cx: &mut TestAppContext) {
        let window = announced(cx);
        assert_eq!(notifications(window, cx), 1);

        testing::frame(cx, window, |window, cx| window.click("notification", cx));
        settle(window, cx);

        assert_eq!(notifications(window, cx), 0);
    }

    #[gpui_kit::test]
    fn a_sideways_swipe_over_an_announcement_dismisses_it(cx: &mut TestAppContext) {
        let window = announced(cx);

        testing::frame(cx, window, |window, cx| {
            window.scroll(
                "notification",
                ScrollDelta::Pixels(gpui_kit::point(px(140.0), px(0.0))),
                cx,
            );
        });
        settle(window, cx);

        assert_eq!(notifications(window, cx), 0);
    }

    #[gpui_kit::test]
    fn scrolling_down_over_an_announcement_keeps_it(cx: &mut TestAppContext) {
        let window = announced(cx);

        testing::frame(cx, window, |window, cx| {
            window.scroll(
                "notification",
                ScrollDelta::Pixels(gpui_kit::point(px(0.0), px(-140.0))),
                cx,
            );
        });
        settle(window, cx);

        assert_eq!(notifications(window, cx), 1);
    }

    #[test]
    fn notifications_fit_a_narrow_window_and_keep_their_width_in_a_wide_one() {
        let defaults = NotificationSettings::default();
        let side = defaults.margins.left + defaults.margins.right;
        assert_eq!(fitted(px(381.0), px(0.0), &defaults).0, px(381.0) - side);
        assert_eq!(fitted(px(1280.0), px(0.0), &defaults).0, defaults.width);
        assert_eq!(fitted(px(10.0), px(0.0), &defaults).0, px(0.0));
    }

    #[test]
    fn notifications_start_below_the_system_bars() {
        let defaults = NotificationSettings::default();
        assert_eq!(
            fitted(px(400.0), px(0.0), &defaults).1,
            defaults.margins.top
        );
        assert_eq!(
            fitted(px(400.0), px(43.0), &defaults).1,
            defaults.margins.top + px(43.0)
        );
    }

    #[gpui_kit::test]
    fn the_notifications_of_a_narrow_window_fit_it(cx: &mut TestAppContext) {
        let (window, _) = testing::open(cx, 360.0, 400.0, |_, _| Host {
            text: String::new().into(),
        });
        let mut last = None;
        cx.update_window(window, |_, window, cx| {
            fit_notifications(window, &mut last, cx);
        })
        .unwrap();
        let width = cx.update(|cx| Theme::global(cx).notification.width);
        assert!(width <= px(360.0 - 32.0), "{width:?}");
        assert_eq!(last.map(|(width, _)| width), Some(width));
    }

    #[gpui_kit::test]
    fn a_window_whose_geometry_did_not_change_leaves_the_settings_alone(cx: &mut TestAppContext) {
        let (window, _) = testing::open(cx, 360.0, 400.0, |_, _| Host {
            text: String::new().into(),
        });
        let mut last = None;
        cx.update_window(window, |_, window, cx| {
            fit_notifications(window, &mut last, cx);
        })
        .unwrap();
        // Another window wrote its own width meanwhile.
        cx.update(|cx| Theme::update(cx, |theme| theme.notification.width = px(500.0)));
        cx.update_window(window, |_, window, cx| {
            fit_notifications(window, &mut last, cx);
        })
        .unwrap();
        assert_eq!(
            cx.update(|cx| Theme::global(cx).notification.width),
            px(500.0)
        );
    }
}
