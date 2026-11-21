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

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, Window, div,
};

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
/// from what they just did. The notification stays until it is dismissed.
pub(crate) fn announce(message: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
    window.push_notification(Notification::info(message).autohide(false), cx);
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
}
