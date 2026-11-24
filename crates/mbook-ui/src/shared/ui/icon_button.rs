//! Controls that show an icon only.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::{ElementId, SharedString, Styled as _, is_mobile, px};

/// The side of the smallest control for a pointer, in pixels.
const POINTER_TARGET: f32 = 32.0;
/// The side of the smallest control for a finger, in pixels.
const TOUCH_TARGET: f32 = 44.0;

/// The side, in pixels, of a control: at least 24 for a pointer (WCAG 2.5.8)
/// and 44 on a touch device.
pub(crate) fn target_size() -> f32 {
    if is_mobile() {
        TOUCH_TARGET
    } else {
        POINTER_TARGET
    }
}

/// A button that shows `icon` and nothing else.
///
/// `label` is its accessible name and the words of its tooltip, so that what
/// a screen reader says and what a pointer shows are the same. An action keeps
/// one name everywhere: pass the same words wherever the same action appears.
pub(crate) fn icon_button(
    id: impl Into<ElementId>,
    icon: IconName,
    label: impl Into<SharedString>,
) -> Button {
    let label = label.into();
    Button::new(id)
        .ghost()
        .icon(icon)
        .accessibility_label(label.clone())
        .tooltip(label)
        .size(px(target_size()))
        .cursor_pointer()
}

/// The control that goes back to the previous screen.
pub(crate) fn back_button(id: impl Into<ElementId>) -> Button {
    icon_button(id, IconName::ArrowLeft, rust_i18n::t!("ui.back"))
}

#[cfg(test)]
mod tests {
    use gpui_kit::test::TestQueryExt as _;
    use gpui_kit::{
        Context, IntoElement, ParentElement as _, Render, Role, TestAppContext, Window,
    };
    use gpui_kit::{div, px};

    use super::*;
    use crate::testing;

    struct Host;

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
            div()
                .child(icon_button("add", IconName::Plus, "Add a book"))
                .child(back_button("back"))
        }
    }

    #[test]
    fn a_control_is_at_least_the_size_of_a_pointer_target() {
        assert!(target_size() >= 24.0);
    }

    #[gpui_kit::test]
    fn an_icon_button_has_a_role_a_name_and_the_size_of_a_target(cx: &mut TestAppContext) {
        let (window, _) = testing::open(cx, 400.0, 300.0, |_, _| Host);
        testing::frame(cx, window, |window, _| {
            let button = window.find_by_label("Add a book");
            assert_eq!(button.role(), Some(Role::Button));
            let bounds = button.bounds();
            assert!(bounds.size.width >= px(24.0));
            assert!(bounds.size.height >= px(24.0));
        });
    }

    #[gpui_kit::test]
    fn the_back_control_is_named_back(cx: &mut TestAppContext) {
        let (window, _) = testing::open(cx, 400.0, 300.0, |_, _| Host);
        testing::frame(cx, window, |window, _| {
            assert_eq!(window.find_by_label("Back").role(), Some(Role::Button));
        });
    }
}
