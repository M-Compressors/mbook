//! What the reader draws: the controls, the text, the panels.

use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::base::motion::{Presence, PresenceSample, Transition};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::slider::Slider;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Placement, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::{
    Anchor, AnyElement, ClickEvent, Context, DispatchPhase, Div, Hsla, InteractiveElement as _,
    IntoElement, KeyContext, MouseButton, MouseMoveEvent, ParentElement as _, Render, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window,
    canvas, div, px, relative,
};

use super::controls::{chapter_title, status_text};
use super::keys::{
    CONTEXT, ChapterEnd, ChapterStart, HIDDEN, NextChapter, PreviousChapter, SCRUBBER, ScreenBack,
    ScreenForward, ShowControls, Zone, zone,
};
use super::pagination::Layout;
use super::{Panel, Phase, ReaderEvent, ReaderPage};
use crate::features::appearance::Appearance;
use crate::shared::actions::{Find, GoBack};
use crate::shared::layout::SizeClass;
use crate::shared::ui::{
    BAR_HEIGHT, empty_state, icon_button, page_colors, page_header, status_line, target_size,
};
use crate::widgets::chapter_view::{ChapterView, Direction, column_width};

/// The height the line under the text takes, in pixels.
const FOOTER: f32 = 64.0;
/// Height of the strip that reveals the reading slider.
const SEEK_STRIP: f32 = 24.0;
/// Space reserved for the status on its own row in a compact window.
const COMPACT_STATUS: f32 = 20.0;
/// How long the reading controls take to fade.
const FADE: Duration = Duration::from_millis(160);
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
        let show = self.controls.shown || self.search.open || keep || !ready;
        let presence = Presence::new("reader-controls", show)
            .transition(Transition::new(FADE))
            .sample(window, cx);
        let bottom = window.insets().effective().bottom;
        let footer = FOOTER.max(target_size() + SEEK_STRIP)
            + if class == SizeClass::Compact {
                COMPACT_STATUS
            } else {
                0.0
            };
        self.prepare_footer(footer, class, window, cx);
        self.controls.holds.focused = self.handles.focused(window, cx);
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
            .relative()
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
            .on_action(cx.listener(|this, _: &Find, window, cx| this.show_search(window, cx)))
            .on_action(cx.listener(|this, _: &GoBack, window, cx| {
                if let Some(panel) = this.panel {
                    this.close_panel(window, cx);
                    match panel {
                        Panel::Contents => &this.handles.contents,
                        Panel::Appearance => &this.handles.appearance,
                    }
                    .focus(window, cx);
                } else if this.search.open {
                    this.close_search(window, cx);
                } else if !this.back(cx) {
                    cx.propagate();
                }
            }))
            .capture_key_down(cx.listener(|this, _, _, cx| this.touch(cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.sample(cx)),
            )
            .child(Self::pointer_activity(cx))
            .child(self.header(class, (page, ink), presence, cx))
            .children(
                self.search
                    .render(Self::step_search, Self::close_search, cx),
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
                    .h(px(footer) + bottom)
                    .pb(bottom)
                    .children(ready.then(|| self.footer(class, ink, presence, window, cx))),
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

/// The outlined group of icon controls in the title bar.
fn capsule(border: Hsla) -> Div {
    h_flex()
        .rounded_full()
        .border_1()
        .border_color(border)
        .px_1()
}

impl ReaderPage {
    /// Detects movement before the chapter's text and its clipping mask handle it.
    fn pointer_activity(cx: &mut Context<'_, Self>) -> impl IntoElement {
        let reader = cx.entity().downgrade();
        canvas(
            |bounds, _, _| bounds,
            move |_, bounds, window, _| {
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && bounds.contains(&event.position) {
                        reader.update(cx, Self::touch).ok();
                    }
                });
            },
        )
        .absolute()
        .inset_0()
    }

    /// Tracks whether the pointer is on either part of the reading controls.
    fn control_hover(id: &'static str, cx: &Context<'_, Self>) -> gpui_kit::Stateful<Div> {
        div()
            .id(id)
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.controls.holds.hovered = *hovered;
                if *hovered {
                    this.touch(cx);
                }
            }))
    }

    /// The bar over the text.
    fn header(
        &mut self,
        class: SizeClass,
        (paper, ink): (Hsla, Hsla),
        presence: PresenceSample,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let header = page_header(self.title.clone())
            .centred()
            .on_paper(paper, ink);
        if !presence.should_render() {
            return div().flex_none().child(header.bare()).into_any_element();
        }

        let ready = matches!(&self.phase, Phase::Ready(_) | Phase::Empty);
        let reading = matches!(&self.phase, Phase::Ready(reading) if reading.error.is_none());
        let border = cx.theme().border;
        let key = self.key;
        let mut leading = capsule(border).child(
            icon_button("back", IconName::ChevronLeft, "Back")
                .rounded_full()
                .track_focus(&self.handles.back)
                .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
        );
        let mut trailing = capsule(border);
        if reading {
            leading = leading.child(self.contents_control(class, cx));
            trailing = trailing.child(self.appearance_control(class, cx));
        }
        if matches!(self.phase, Phase::Ready(_)) {
            trailing = trailing.child(
                icon_button("find-book", IconName::Search, "Find in book")
                    .rounded_full()
                    .track_focus(&self.handles.search)
                    .on_click(cx.listener(|this, _, window, cx| this.show_search(window, cx))),
            );
        }
        if ready {
            trailing = trailing.child(
                icon_button("edit", IconName::Pencil, "Edit this book")
                    .rounded_full()
                    .track_focus(&self.handles.edit)
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(ReaderEvent::Edit(key)))),
            );
        }
        let mut header = header.leading(leading).content_opacity(presence.progress);
        if ready {
            header = header.trailing(trailing);
        }
        Self::control_hover("reader-header", cx)
            .flex_none()
            .child(header)
            .into_any_element()
    }

    /// The control of the contents: a popover on an expanded window, a sheet
    /// otherwise.
    fn contents_control(&mut self, class: SizeClass, cx: &mut Context<'_, Self>) -> AnyElement {
        let trigger = icon_button("contents", IconName::List, "Contents")
            .rounded_full()
            .track_focus(&self.handles.contents);
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
        let trigger = icon_button(
            "appearance",
            IconName::ALargeSmall,
            "Text and page appearance",
        )
        .rounded_full()
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
                .anchor(match which {
                    Panel::Contents => Anchor::TopLeft,
                    Panel::Appearance => Anchor::TopRight,
                })
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

    /// Updates the book page estimates and the seek slider for this frame.
    fn prepare_footer(
        &mut self,
        footer: f32,
        class: SizeClass,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let appearance = Appearance::global(cx).read(cx);
        let settings = appearance.settings();
        let viewport = window.viewport_size();
        let insets = window.insets().effective();
        let height = viewport.height - insets.top - insets.bottom - px(BAR_HEIGHT + footer);
        self.pages.set_layout(Layout {
            column: column_width(viewport.width.as_f32(), settings.text_size, class),
            height: height.as_f32().max(1.0),
            text_size: settings.text_size,
            line_height: settings.line_height,
            rem: window.rem_size(),
            font: appearance.reading_font(cx),
        });
        self.scrubber.sync(self.progress, window, cx);
    }

    /// Keeps the slider's hit area available while the track is transparent.
    fn seek_strip(&self, ink: Hsla, window: &Window, cx: &gpui_kit::App) -> impl IntoElement {
        let keyboard_focus =
            window.last_input_was_keyboard() && self.handles.scrubber.contains_focused(window, cx);
        let active = self.scrubber.preview.is_some() || keyboard_focus;
        div()
            .id("reader-seek-strip")
            .absolute()
            .bottom_0()
            .left_5()
            .right_5()
            .h(px(SEEK_STRIP))
            .px_2()
            .key_context(SCRUBBER)
            .track_focus(&self.handles.scrubber)
            .opacity(if active { 1.0 } else { 0.0 })
            .hover(|style| style.opacity(1.0))
            .child(
                Slider::new(&self.scrubber.slider)
                    .accessibility_label("Reading position")
                    .bg(ink)
                    .text_color(ink),
            )
    }

    /// The current page number and the controls around it.
    fn footer(
        &self,
        class: SizeClass,
        ink: Hsla,
        presence: PresenceSample,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let Phase::Ready(reading) = &self.phase else {
            return div().into_any_element();
        };
        let shown = presence.should_render();
        let muted = ink.opacity(0.72);
        let (chapter, offset, progress) = self
            .scrubber
            .preview
            .and_then(|progress| {
                reading
                    .snapshot
                    .at_progress(progress)
                    .map(|(chapter, offset)| (chapter, offset, progress))
            })
            .unwrap_or((self.spot.chapter, self.spot.offset, self.progress));
        let title = chapter_title(&reading.snapshot, chapter, offset);
        let text = status_text(&title, progress);
        let number = self
            .view
            .read(cx)
            .page_number()
            .filter(|_| !reading.loading)
            .map(|local| {
                let number = self.pages.label(&reading.snapshot, reading.chapter, local);
                status_line("reader-page-number", number)
                    .min_w(px(72.0))
                    .text_center()
                    .text_sm()
                    .text_color(muted)
            });
        let previous = icon_button("previous-page", IconName::ChevronLeft, "Previous page")
            .rounded_full()
            .track_focus(&self.handles.previous)
            .on_click(cx.listener(|this, _, _, cx| this.screen(Direction::Back, cx)));
        let next = icon_button("next-page", IconName::ChevronRight, "Next page")
            .rounded_full()
            .track_focus(&self.handles.next)
            .on_click(cx.listener(|this, _, _, cx| this.screen(Direction::Forward, cx)));
        let compact = class == SizeClass::Compact;
        let status = || {
            status_line("reader-status", text.clone())
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(muted)
        };
        let row = h_flex()
            .w_full()
            .min_h(px(target_size()))
            .items_center()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .opacity(presence.progress)
                    .children((shown && !compact).then(status)),
            )
            .children(shown.then(|| previous.opacity(presence.progress)))
            .children(number)
            .children(shown.then(|| next.opacity(presence.progress)))
            .child(div().flex_1().min_w_0());
        Self::control_hover("reader-footer", cx)
            .relative()
            .size_full()
            .px_5()
            .child(row)
            .children((shown && compact).then(|| {
                div()
                    .text_center()
                    .opacity(presence.progress)
                    .child(status())
            }))
            .child(self.seek_strip(ink, window, cx))
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
