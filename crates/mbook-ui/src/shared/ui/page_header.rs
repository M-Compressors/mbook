//! The bar at the top of a page.

#[cfg(not(target_os = "android"))]
use gpui_kit::component::TitleBar;
use gpui_kit::component::{ActiveTheme as _, h_flex};

use super::target_size;
use gpui_kit::{
    AnyElement, App, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderOnce, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, div, px,
};

/// The height of the bar, in pixels, below the safe area.
pub(crate) const BAR_HEIGHT: f32 = 48.0;

/// The bar at the top of a page: a control on the left, usually the back
/// control, the title as the heading of the page, and actions on the right.
///
/// On the desktop it is the title bar of the Kit, so it stands in the row of
/// the buttons of the window, leaves them their room on every platform, and
/// moves the window when it is dragged; on Android it is a plain bar. It keeps clear of the safe area the window reports at
/// the top (a notch, a status bar). The title is the heading of the page for
/// assistive technology.
#[derive(IntoElement)]
pub(crate) struct PageHeader {
    title: SharedString,
    leading: Option<AnyElement>,
    trailing: Vec<AnyElement>,
    centred: bool,
    /// The colours of a bar that stands on a page of a book: its paper and
    /// its ink.
    paper: Option<(Hsla, Hsla)>,
    bare: bool,
    opacity: f32,
}

/// A bar for a page called `title`.
pub(crate) fn page_header(title: impl Into<SharedString>) -> PageHeader {
    PageHeader {
        title: title.into(),
        leading: None,
        trailing: Vec::new(),
        centred: false,
        paper: None,
        bare: false,
        opacity: 1.0,
    }
}

impl PageHeader {
    /// The control before the title; the back control of a pushed page.
    #[must_use]
    pub(crate) fn leading(mut self, control: impl IntoElement) -> Self {
        self.leading = Some(control.into_any_element());
        self
    }

    /// An action after the title. Call it once for each action; they stand in
    /// the order they were added.
    #[must_use]
    pub(crate) fn trailing(mut self, action: impl IntoElement) -> Self {
        self.trailing.push(action.into_any_element());
        self
    }

    /// Sets the title small and in the middle, between the controls, as a
    /// reader has it.
    #[must_use]
    pub(crate) fn centred(mut self) -> Self {
        self.centred = true;
        self
    }

    /// Draws the bar on the page of a book, without a line under it: `page`
    /// is the colour of the page and `ink` the colour of its text.
    #[must_use]
    pub(crate) fn on_paper(mut self, page: Hsla, ink: Hsla) -> Self {
        self.paper = Some((page, ink));
        self
    }

    /// Leaves the bar empty: no title to see and no controls, only the room
    /// of the bar, which still moves the window. The title stays the heading
    /// of the page.
    #[must_use]
    pub(crate) fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    /// Sets the opacity of the heading and actions, leaving the window controls visible.
    #[must_use]
    pub(crate) fn content_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
}

impl RenderOnce for PageHeader {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let top = crate::shared::safe_area::insets(window).top;
        let theme = cx.theme();
        let (background, ink, line) = match self.paper {
            Some((page, ink)) => (page, ink, page),
            None => (theme.background, theme.foreground, theme.border),
        };
        let heading = div()
            .id("page-heading")
            .role(Role::Heading)
            .aria_level(1)
            .aria_label(self.title.clone())
            .test_support()
            .flex_1()
            .min_w_0()
            .truncate()
            .font_weight(FontWeight::SEMIBOLD);
        let heading = if self.bare {
            heading
        } else if self.centred {
            heading.text_center().text_sm().child(self.title)
        } else {
            heading.text_lg().child(self.title)
        };
        let (leading, trailing) = if self.bare {
            (None, Vec::new())
        } else {
            (self.leading, self.trailing)
        };
        let row = h_flex()
            .h_full()
            .items_center()
            .gap_2()
            .px_3()
            .opacity(self.opacity);
        let content = if self.centred && !self.bare {
            let rail = px(2.0 * target_size() + 10.0);
            // Fit the row inside the space left by the native window controls.
            row.w(px(0.0))
                .flex_1()
                .min_w_0()
                .child(h_flex().min_w(rail).flex_none().children(leading))
                .child(heading)
                .child(
                    h_flex()
                        .min_w(rail)
                        .flex_none()
                        .justify_end()
                        .children(trailing),
                )
        } else {
            // As above: the row takes what the window controls leave, so a
            // long title is cut short instead of pushing the actions off.
            row.w(px(0.0))
                .flex_1()
                .min_w_0()
                .children(leading)
                .child(heading)
                .children(trailing)
        };
        bar()
            .h(px(BAR_HEIGHT) + top)
            .pt(top)
            .bg(background)
            .border_color(line)
            .text_color(ink)
            .child(content)
    }
}

/// The title bar of the Kit, which holds the buttons of the window and moves
/// the window when it is dragged.
#[cfg(not(target_os = "android"))]
fn bar() -> TitleBar {
    TitleBar::new()
}

/// A phone has no buttons to leave room for and no window to move: the bar
/// is a plain row with the border of the title bar.
#[cfg(target_os = "android")]
fn bar() -> gpui_kit::Stateful<gpui_kit::Div> {
    h_flex()
        .id("title-bar")
        .flex_shrink_0()
        .w_full()
        .items_center()
        .border_b_1()
}

#[cfg(test)]
mod tests {
    use gpui_kit::component::button::Button;
    use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
    use gpui_kit::{Context, Render, TestAppContext};

    use super::*;
    use crate::shared::ui::{back_button, icon_button};
    use crate::testing;
    use gpui_kit::assets::IconName;

    struct Host;

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
            div().child(
                page_header("Library")
                    .leading(back_button("back"))
                    .trailing(icon_button("settings", IconName::Settings, "Settings"))
                    .trailing(Button::new("add").label("Add a book")),
            )
        }
    }

    #[gpui_kit::test]
    fn the_title_is_the_heading_of_the_page(cx: &mut TestAppContext) {
        let (window, _) = testing::open(cx, 600.0, 300.0, |_, _| Host);
        testing::frame(cx, window, |window, _| {
            let heading = window.find("page-heading");
            assert_eq!(heading.role(), Some(gpui_kit::Role::Heading));
            assert_eq!(heading.label(), Some("Library"));
        });
    }

    #[gpui_kit::test]
    fn the_back_control_comes_before_the_title_and_the_actions_after_it(cx: &mut TestAppContext) {
        let (window, _) = testing::open(cx, 600.0, 300.0, |_, _| Host);
        testing::frame(cx, window, |window, _| {
            let back = window.find_by_label("Back").bounds().origin.x;
            let title = window.find("page-heading").bounds().origin.x;
            let settings = window.find_by_label("Settings").bounds().origin.x;
            assert!(back < title, "{back:?} {title:?}");
            assert!(title < settings, "{title:?} {settings:?}");
        });
    }

    /// A narrow page with a long title and a wide action, as the editor on a
    /// phone in Russian.
    struct Narrow;

    impl Render for Narrow {
        fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
            div().w_full().child(
                page_header("The Quiet Orchard of the Long Evenings")
                    .leading(back_button("back"))
                    .trailing(Button::new("save").label("Сохранить изменения")),
            )
        }
    }

    #[gpui_kit::test]
    fn a_long_title_gives_way_to_the_actions(cx: &mut TestAppContext) {
        let (window, _) = testing::open(cx, 390.0, 300.0, |_, _| Narrow);
        testing::frame(cx, window, |window, _| {
            let save = window.find("save").bounds();
            let right = save.origin.x + save.size.width;
            assert!(right <= px(390.0), "the action ends at {right:?}");
            let title = window.find("page-heading").bounds();
            assert!(title.origin.x + title.size.width <= save.origin.x);
        });
    }

    #[gpui_kit::test]
    fn the_bar_is_at_least_as_high_as_a_control(cx: &mut TestAppContext) {
        let (window, _) = testing::open(cx, 600.0, 300.0, |_, _| Host);
        testing::frame(cx, window, |window, _| {
            let heading = window.find("page-heading").bounds();
            assert!(heading.size.height >= px(24.0));
        });
    }
}
