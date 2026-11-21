//! What a page shows when it has nothing to show.

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderOnce, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, div,
};

/// A title, a hint that says what to do, and optionally the control that does
/// it, centred in the room the page has.
#[derive(IntoElement)]
pub(crate) struct EmptyState {
    title: SharedString,
    hint: SharedString,
    action: Option<AnyElement>,
}

/// An empty state: `title` says what is missing, `hint` what to do about it.
pub(crate) fn empty_state(
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
) -> EmptyState {
    EmptyState {
        title: title.into(),
        hint: hint.into(),
        action: None,
    }
}

impl EmptyState {
    /// The control that fixes it: a button with a visible label.
    #[must_use]
    pub(crate) fn action(mut self, control: impl IntoElement) -> Self {
        self.action = Some(control.into_any_element());
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .p_6()
            .text_center()
            .child(
                div()
                    .id("empty-title")
                    .role(Role::Heading)
                    .aria_level(2)
                    .aria_label(self.title.clone())
                    .test_support()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground)
                    .child(self.title),
            )
            .child(
                div()
                    .id("empty-hint")
                    .max_w(gpui_kit::px(420.0))
                    .text_color(theme.muted_foreground)
                    .child(self.hint),
            )
            .children(self.action)
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::component::button::Button;
    use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
    use gpui_kit::{Context, Render, TestAppContext};

    use super::*;
    use crate::testing;

    struct Host {
        with_action: bool,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
            let state = empty_state("No books yet", "Add a book to start reading.");
            div().size_full().child(if self.with_action {
                state.action(Button::new("add").label("Add a book"))
            } else {
                state
            })
        }
    }

    #[gpui_kit::test]
    fn the_title_is_a_heading_and_the_action_a_button(cx: &mut TestAppContext) {
        let (window, _) = testing::open(cx, 600.0, 400.0, |_, _| Host { with_action: true });
        testing::frame(cx, window, |window, _| {
            let title = window.find("empty-title");
            assert_eq!(title.role(), Some(gpui_kit::Role::Heading));
            assert_eq!(title.label(), Some("No books yet"));
            assert_eq!(
                window.find_by_label("Add a book").role(),
                Some(gpui_kit::Role::Button)
            );
        });
    }

    #[gpui_kit::test]
    fn without_an_action_there_is_no_control(cx: &mut TestAppContext) {
        let (window, _) = testing::open(cx, 600.0, 400.0, |_, _| Host { with_action: false });
        testing::frame(cx, window, |window, _| {
            assert!(window.find_all_by_role(gpui_kit::Role::Button).is_empty());
        });
    }
}
