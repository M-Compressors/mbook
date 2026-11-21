//! What the reader draws: the controls, the text, the panels.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Placement, Sizable as _, WindowExt as _, v_flex,
};
use gpui_kit::{
    Anchor, AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, KeyContext,
    MouseButton, ParentElement as _, Render, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, div, px, relative,
};

use super::controls::{chapter_title, fill, status_text};
use super::keys::{
    CONTEXT, ChapterEnd, ChapterStart, HIDDEN, NextChapter, PreviousChapter, ScreenBack,
    ScreenForward, ShowControls, Zone, zone,
};
use super::{Panel, Phase, ReaderEvent, ReaderPage};
use crate::features::appearance::Appearance;
use crate::shared::actions::GoBack;
use crate::shared::layout::SizeClass;
use crate::shared::ui::{
    back_button, empty_state, icon_button, page_colors, page_header, status_line,
};
use crate::widgets::chapter_view::{ChapterView, Direction};

/// The height the line under the text takes, in pixels.
const FOOTER: f32 = 48.0;
/// The height the bar over the text takes below the inset, in pixels.
const HEADER: f32 = 48.0;
/// The width of the popover of a panel, in pixels.
const POPOVER_WIDTH: f32 = 340.0;

impl Render for ReaderPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let appearance = Appearance::global(cx).read(cx);
        let tone = appearance.page_tone(window);
        let class = appearance.size_class(window);
        let keep = appearance.settings().keep_controls;
        let (page, ink) = page_colors(tone);
        let ready = matches!(&self.phase, Phase::Ready(reading) if reading.error.is_none());
        let show = self.controls.shown || keep || !ready;
        let top = window.insets().effective().top;
        self.controls.holds.focused = self.handles.focused(window);
        if class != self.class {
            // A window of another class keeps another number of pictures.
            self.class = class;
            self.pictures.set_budget(super::pictures::budget(class));
        }

        v_flex()
            .id("reader-page")
            .role(Role::Main)
            .aria_label("Reader")
            .test_support()
            .key_context(reader_context(show))
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(page)
            .text_color(ink)
            .on_action(cx.listener(|this, _: &ScreenForward, _, cx| {
                this.screen(Direction::Forward, cx);
            }))
            .on_action(cx.listener(|this, _: &ScreenBack, _, cx| {
                this.screen(Direction::Back, cx);
            }))
            .on_action(cx.listener(|this, _: &ChapterStart, _, cx| {
                this.touch(cx);
                this.view.update(cx, ChapterView::reveal_start);
            }))
            .on_action(cx.listener(|this, _: &ChapterEnd, _, cx| {
                this.touch(cx);
                this.view.update(cx, ChapterView::reveal_end);
            }))
            .on_action(cx.listener(|this, _: &PreviousChapter, _, cx| this.chapter_step(false, cx)))
            .on_action(cx.listener(|this, _: &NextChapter, _, cx| this.chapter_step(true, cx)))
            .on_action(cx.listener(|this, _: &ShowControls, window, cx| {
                this.touch(cx);
                this.handles.back.focus(window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoBack, _, cx| {
                if !this.back(cx) {
                    cx.propagate();
                }
            }))
            .capture_key_down(cx.listener(|this, _, _, cx| this.touch(cx)))
            .on_mouse_move(cx.listener(|this, _, _, cx| this.touch(cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.sample(cx)),
            )
            .child(
                div()
                    .flex_none()
                    .w_full()
                    .min_h(px(HEADER) + top)
                    .when_show(show, |slot| slot.child(self.header(class, cx))),
            )
            .child(
                div()
                    .id("reader-body")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .on_click(cx.listener(Self::on_tap))
                    .child(self.body(cx)),
            )
            .child(
                div()
                    .flex_none()
                    .w_full()
                    .h(px(FOOTER))
                    .when_show(show && ready, |slot| slot.child(self.footer(cx))),
            )
    }
}

/// The key context of the page: the reader, and whether its controls are
/// hidden.
fn reader_context(show: bool) -> KeyContext {
    let names = if show {
        CONTEXT.to_owned()
    } else {
        format!("{CONTEXT} {HIDDEN}")
    };
    // Both names are plain words, which always parse.
    KeyContext::parse(&names).unwrap_or_default()
}

/// `when` for a flag, spelled for what it means here.
trait WhenShow: Sized {
    fn when_show(self, show: bool, build: impl FnOnce(Self) -> Self) -> Self {
        if show { build(self) } else { self }
    }
}

impl WhenShow for gpui_kit::Div {}

impl ReaderPage {
    /// The bar over the text.
    fn header(&mut self, class: SizeClass, cx: &mut Context<'_, Self>) -> AnyElement {
        let ready = matches!(&self.phase, Phase::Ready(_) | Phase::Empty);
        let reading = matches!(&self.phase, Phase::Ready(reading) if reading.error.is_none());
        let key = self.key;
        let mut header = page_header(self.title.clone()).leading(
            back_button("back")
                .track_focus(&self.handles.back)
                .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
        );
        if reading {
            header = header
                .trailing(self.contents_control(class, cx))
                .trailing(self.appearance_control(class, cx));
        }
        if ready {
            header = header.trailing(
                icon_button("edit", IconName::Pencil, "Edit this book")
                    .track_focus(&self.handles.edit)
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(ReaderEvent::Edit(key)))),
            );
        }
        div()
            .id("reader-header")
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.controls.holds.hovered = *hovered;
                if *hovered {
                    this.touch(cx);
                }
            }))
            .child(header)
            .into_any_element()
    }

    /// The control of the contents: a popover on an expanded window, a sheet
    /// otherwise.
    fn contents_control(&mut self, class: SizeClass, cx: &mut Context<'_, Self>) -> AnyElement {
        let trigger =
            icon_button("contents", IconName::List, "Contents").track_focus(&self.handles.contents);
        let panel = self.contents.clone();
        self.panel_control(
            Panel::Contents,
            "contents-popover",
            "Contents",
            trigger,
            move || panel.clone().into_any_element(),
            class,
            cx,
        )
    }

    /// The control of the appearance.
    fn appearance_control(&mut self, class: SizeClass, cx: &mut Context<'_, Self>) -> AnyElement {
        let trigger = Button::new("appearance")
            .ghost()
            .label("Aa")
            .accessibility_label("Aa, text and page appearance")
            .tooltip("Text and page appearance")
            .min_h(px(crate::shared::ui::target_size()))
            .cursor_pointer()
            .track_focus(&self.handles.appearance);
        let panel = self.typography.clone();
        self.panel_control(
            Panel::Appearance,
            "appearance-popover",
            "Appearance",
            trigger,
            move || panel.clone().into_any_element(),
            class,
            cx,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "one place that decides how every panel of the reader is shown"
    )]
    fn panel_control(
        &mut self,
        which: Panel,
        id: &'static str,
        title: &'static str,
        trigger: Button,
        content: impl Fn() -> AnyElement + Clone + 'static,
        class: SizeClass,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let page = cx.entity().downgrade();
        if class == SizeClass::Expanded {
            return Popover::new(id)
                .anchor(Anchor::TopRight)
                .open(self.panel == Some(which))
                .on_open_change(move |open, _, cx| {
                    page.update(cx, |page, cx| {
                        let next = if *open { Some(which) } else { None };
                        if page.panel == Some(which) || next.is_some() {
                            page.set_panel(next, cx);
                        }
                    })
                    .ok();
                })
                .trigger(trigger)
                .content(move |_, _, _| div().w(px(POPOVER_WIDTH)).child(content()))
                .into_any_element();
        }
        trigger
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.set_panel(Some(which), cx);
                let page = cx.entity().downgrade();
                let content = content.clone();
                window.open_sheet_at(Placement::Bottom, cx, move |sheet, _, _| {
                    let page = page.clone();
                    sheet
                        .title(title)
                        .size(relative(0.7))
                        .on_close(move |_, _, cx| {
                            page.update(cx, |page, cx| page.set_panel(None, cx)).ok();
                        })
                        .child(content())
                });
            }))
            .into_any_element()
    }

    /// The progress line and what it says.
    fn footer(&self, cx: &mut Context<'_, Self>) -> AnyElement {
        let Phase::Ready(reading) = &self.phase else {
            return div().into_any_element();
        };
        let title = chapter_title(&reading.snapshot, self.spot.chapter, self.spot.offset);
        let text = status_text(&title, self.progress);
        let theme = cx.theme();
        v_flex()
            .id("reader-footer")
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.controls.holds.hovered = *hovered;
                if *hovered {
                    this.touch(cx);
                }
            }))
            .w_full()
            .px_5()
            .gap_1()
            .child(
                // The track and the fill say nothing the text does not.
                div()
                    .h(px(2.0))
                    .w_full()
                    .rounded_full()
                    .bg(theme.input)
                    .child(
                        div()
                            .h_full()
                            .w(relative(fill(self.progress)))
                            .rounded_full()
                            .bg(theme.progress_bar),
                    ),
            )
            .child(
                status_line("reader-status", text)
                    .text_sm()
                    .text_color(theme.muted_foreground),
            )
            .into_any_element()
    }

    /// The middle of the page: the text, or what stands in its place.
    fn body(&mut self, cx: &mut Context<'_, Self>) -> AnyElement {
        match &self.phase {
            Phase::Opening => {
                let text: SharedString = format!("Opening {}", self.title).into();
                let indicator = if cx.reduce_motion() {
                    Icon::new(IconName::Loader).large().into_any_element()
                } else {
                    gpui_kit::component::spinner::Spinner::new()
                        .large()
                        .into_any_element()
                };
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(indicator)
                    .child(status_line("reader-opening", text))
                    .into_any_element()
            }
            Phase::Failed(reason) => empty_state(
                "This book could not be opened",
                format!("{reason}. Go back to the library and try again, or add the book again."),
            )
            .action(
                Button::new("retry")
                    .primary()
                    .label("Try again")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| this.start(cx))),
            )
            .into_any_element(),
            Phase::Empty => empty_state(
                "This book has no text",
                "It has no chapters to read. Edit the book to add some.",
            )
            .into_any_element(),
            Phase::Ready(reading) => match &reading.error {
                Some(reason) => {
                    let index = reading.chapter;
                    empty_state(
                        "This chapter could not be read",
                        format!("{reason}. Try again, or go back to the library."),
                    )
                    .action(
                        Button::new("retry")
                            .primary()
                            .label("Try again")
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.load_chapter(index, super::Arrival::Start, cx);
                            })),
                    )
                    .into_any_element()
                }
                None => self.view.clone().into_any_element(),
            },
        }
    }

    /// A tap: on the edges a screen, in the middle the controls. A click of a
    /// mouse is left to the text, where it selects.
    pub(super) fn on_tap(
        &mut self,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !matches!(event, ClickEvent::Touch(touch) if !touch.long_press) {
            return;
        }
        let width = window.viewport_size().width.as_f32();
        match zone(event.position().x.as_f32(), 0.0, width) {
            Zone::Left => self.screen(Direction::Back, cx),
            Zone::Right => self.screen(Direction::Forward, cx),
            Zone::Middle => self.touch(cx),
        }
    }
}
