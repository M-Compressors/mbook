//! The appearance panel: page tone, size and line height of the text, typeface.
//!
//! Every change goes through [`Appearance::update`], so the open chapter is
//! drawn again at once and the choice is stored.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Selectable as _, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Role, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, Window, div, px,
};

use crate::entities::catalogue::{LINE_HEIGHT, READING_FACES, ReadingFlow, Settings, TEXT_SIZE};
use crate::features::appearance::Appearance;
use crate::shared::theme::PageTone;
use crate::shared::ui::{icon_button, page_colors, target_size};

const MINIMUM_READER_SIZE: f32 = 16.0;
const STANDARD_MAX: f32 = 36.0;
const LINE_STEP: f32 = 0.05;

/// `size` one step larger or smaller, kept in its range.
pub(super) fn step_size(size: f32, larger: bool) -> f32 {
    let step = if larger { 1.0 } else { -1.0 };
    (size + step).clamp(MINIMUM_READER_SIZE, TEXT_SIZE.1)
}

/// `height` one step looser or tighter, kept in its range and rounded to twentieths.
pub(super) fn step_line_height(height: f32, looser: bool) -> f32 {
    let step = if looser { LINE_STEP } else { -LINE_STEP };
    (((height + step) * 20.0).round() / 20.0).clamp(LINE_HEIGHT.0, LINE_HEIGHT.1)
}

/// The size as it is read out: "18 pixels".
pub(super) fn size_text(size: f32) -> String {
    format!("{size:.0} pixels")
}

/// The line height as it is read out: "1.6".
pub(super) fn line_height_text(height: f32) -> String {
    format!("{height:.2}")
}

/// The typefaces on offer: the default and the stored one when they differ,
/// and the one used before it, so that a reader can go back and forth.
pub(super) fn typefaces(current: &str, before: Option<&str>) -> Vec<String> {
    let mut faces = READING_FACES.map(str::to_owned).to_vec();
    for face in [Some(current), before].into_iter().flatten() {
        if !face.trim().is_empty() && !faces.iter().any(|known| known == face) {
            faces.push(face.to_owned());
        }
    }
    faces
}

/// The panel.
pub(super) struct TypographyPanel {
    /// The typeface that was used before the current one.
    advanced: bool,
    before: Option<String>,
    last: String,
    _appearance: Subscription,
}

impl TypographyPanel {
    /// A panel over the appearance of the application.
    pub(super) fn new(cx: &mut Context<'_, Self>) -> Self {
        let appearance = Appearance::global(cx);
        let last = appearance.read(cx).settings().typeface.clone();
        Self {
            before: None,
            advanced: false,
            last,
            _appearance: cx.observe(&appearance, |this, appearance, cx| {
                let now = appearance.read(cx).settings().typeface.clone();
                if now != this.last {
                    this.before = Some(std::mem::replace(&mut this.last, now));
                }
                cx.notify();
            }),
        }
    }

    /// A panel entity.
    pub(super) fn entity(cx: &mut gpui_kit::App) -> Entity<Self> {
        cx.new(Self::new)
    }
}

/// Changes the settings of the appearance.
fn change(cx: &mut gpui_kit::App, change: impl FnOnce(&mut Settings)) {
    Appearance::global(cx).update(cx, |appearance, cx| appearance.update(change, cx));
}

/// One of the two buttons of a stepper.
#[derive(Clone, Copy)]
struct Step {
    id: &'static str,
    label: &'static str,
    icon: IconName,
    disabled: bool,
    action: fn(&mut gpui_kit::App),
}

impl Step {
    fn new(
        id: &'static str,
        label: &'static str,
        icon: IconName,
        disabled: bool,
        action: fn(&mut gpui_kit::App),
    ) -> Self {
        Self {
            id,
            label,
            icon,
            disabled,
            action,
        }
    }
}

fn step_button(step: Step) -> Button {
    icon_button(step.id, step.icon, step.label)
        .outline()
        .disabled(step.disabled)
        .on_click(move |_, _, cx| (step.action)(cx))
}

/// The title of a group of choices.
fn section(id: &'static str, title: &'static str, muted: Hsla) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Heading)
        .aria_level(3)
        .aria_label(title)
        .test_support()
        .text_sm()
        .text_color(muted)
        .child(title)
}

/// A smaller and a larger button around the value they change.
fn stepper(id: &'static str, less: Step, more: Step, value: String) -> impl IntoElement {
    h_flex()
        .gap_2()
        .items_center()
        .child(step_button(less))
        .child(
            div()
                .id(id)
                .role(Role::Status)
                .aria_label(value.clone())
                .test_support()
                .min_w(px(84.0))
                .text_center()
                .child(value),
        )
        .child(step_button(more))
}

/// One of the choices of a group: named, marked as selected in its name and
/// by a check, and as wide as the panel.
fn choice(
    id: String,
    name: &str,
    selected: bool,
    apply: impl Fn(&mut gpui_kit::App) + 'static,
) -> Button {
    let button = Button::new(id)
        .outline()
        .label(name.to_owned())
        .accessibility_label(if selected {
            format!("{name}, selected")
        } else {
            name.to_owned()
        })
        .toggled(selected)
        .selected(selected)
        .w_full()
        .min_h(px(target_size()))
        .justify_start()
        .cursor_pointer()
        .on_click(move |_, _, cx| apply(cx));
    if selected {
        button.icon(IconName::Check)
    } else {
        button
    }
}

/// A sample of the paper and ink, or a system appearance icon.
fn tone_swatch(tone: Option<PageTone>, selected: bool, cx: &gpui_kit::App) -> Button {
    let name = tone.map_or("Follow the system", PageTone::label);
    let label = if selected {
        format!("{name}, selected")
    } else {
        name.to_owned()
    };
    let id = format!("tone-{}", tone.map_or("system", PageTone::name));
    let theme = cx.theme();
    let (paper, ink) = tone.map_or((theme.background, theme.foreground), page_colors);
    let button = if tone.is_some() {
        Button::new(id)
            .ghost()
            .accessibility_label(label.clone())
            .tooltip(label)
            .child(div().text_color(ink).child("A"))
            .cursor_pointer()
    } else {
        icon_button(id, IconName::SunMoon, label)
    };
    button
        .selected(selected)
        .toggled(selected)
        .size(px(target_size()))
        .rounded_full()
        .border_1()
        .border_color(if selected {
            theme.primary
        } else {
            tone.map_or(theme.border, |tone| tone.colors().muted.into())
        })
        .bg(paper)
        .on_click(move |_, _, cx| change(cx, |settings| settings.page_tone = tone))
}

/// A named group of choices.
fn group(id: &'static str, label: &'static str, choices: Vec<Button>) -> impl IntoElement {
    v_flex()
        .id(id)
        .role(Role::Group)
        .aria_label(label)
        .test_support()
        .gap_1()
        .children(choices)
}

/// The current and recently used typefaces, in the order they are offered.
fn face_choices(current: &str, before: Option<&str>) -> impl IntoElement {
    let faces = typefaces(current, before)
        .into_iter()
        .map(|face| {
            let selected = face == current;
            choice(
                format!("typeface-{face}"),
                &face.clone(),
                selected,
                move |cx| {
                    let face = face.clone();
                    change(cx, |s| s.typeface = face);
                },
            )
        })
        .collect();
    group("typefaces", "Typeface", faces)
}

impl Render for TypographyPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let settings = Appearance::global(cx).read(cx).settings().clone();
        let foreground = cx.theme().foreground;
        v_flex()
            .id("appearance-panel")
            .key_context("ReaderAppearance")
            .role(Role::Group)
            .aria_label("Text and page appearance")
            .test_support()
            .w_full()
            .max_h(px(window.viewport_size().height.as_f32() * 0.61))
            .overflow_y_scroll()
            .gap_3()
            .children((!self.advanced).then(|| basic(&settings, self.before.as_deref(), cx)))
            .child(
                Button::new("reading-advanced")
                    .outline()
                    .label(if self.advanced {
                        "Basic appearance"
                    } else {
                        "Advanced appearance"
                    })
                    .min_h(px(target_size()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.advanced = !this.advanced;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("reading-reset")
                    .outline()
                    .label("Reset appearance")
                    .min_h(px(target_size()))
                    .on_click(|_, window, cx| {
                        Appearance::global(cx)
                            .update(cx, |appearance, cx| appearance.reset_reader(window, cx));
                    }),
            )
            .children(self.advanced.then(|| advanced(&settings, foreground)))
    }
}

fn basic(settings: &Settings, before: Option<&str>, cx: &gpui_kit::App) -> impl IntoElement {
    let foreground = cx.theme().foreground;
    let size = settings.text_size;
    let tones = h_flex()
        .id("tones")
        .role(Role::Group)
        .aria_label("Page colour")
        .test_support()
        .flex_wrap()
        .gap_2()
        .children(
            std::iter::once(None)
                .chain(PageTone::ALL.map(Some))
                .map(|tone| tone_swatch(tone, settings.page_tone == tone, cx)),
        );
    v_flex()
        .gap_3()
        .child(section("tone-heading", "Page colour", foreground))
        .child(tones)
        .child(section("size-heading", "Text size", foreground))
        .child(stepper(
            "text-size",
            Step::new(
                "text-smaller",
                "Decrease text size",
                IconName::AArrowDown,
                size <= MINIMUM_READER_SIZE,
                |cx| {
                    change(cx, |s| s.text_size = step_size(s.text_size, false));
                },
            ),
            Step::new(
                "text-larger",
                "Increase text size",
                IconName::AArrowUp,
                size >= TEXT_SIZE.1,
                |cx| {
                    change(cx, |s| s.text_size = step_size(s.text_size, true));
                },
            ),
            size_text(size),
        ))
        .child(
            Button::new("text-scale")
                .outline()
                .label(if size > STANDARD_MAX {
                    "Standard text size"
                } else {
                    "Enlarge text (44 pixels)"
                })
                .min_h(px(target_size()))
                .on_click(move |_, _, cx| {
                    change(cx, |settings| {
                        settings.text_size = if size > STANDARD_MAX {
                            STANDARD_MAX
                        } else {
                            44.0
                        }
                    });
                }),
        )
        .child(section("typeface-heading", "Typeface", foreground))
        .child(face_choices(&settings.typeface, before))
}

fn advanced(settings: &Settings, foreground: Hsla) -> impl IntoElement {
    let height = settings.line_height;
    let column = settings.reading.column;
    let gap = settings.reading.paragraph_gap;
    v_flex()
        .gap_2()
        .child(section("line-heading", "Line height", foreground))
        .child(stepper(
            "line-height",
            Step::new(
                "line-tighter",
                "Decrease line height",
                IconName::Minus,
                height <= LINE_HEIGHT.0,
                |cx| {
                    change(cx, |s| {
                        s.line_height = step_line_height(s.line_height, false);
                    });
                },
            ),
            Step::new(
                "line-looser",
                "Increase line height",
                IconName::Plus,
                height >= LINE_HEIGHT.1,
                |cx| {
                    change(cx, |s| {
                        s.line_height = step_line_height(s.line_height, true);
                    });
                },
            ),
            line_height_text(height),
        ))
        .child(section("column-heading", "Wide-window column", foreground))
        .child(stepper(
            "reading-column",
            Step::new(
                "column-narrower",
                "Decrease column width",
                IconName::Minus,
                column <= 45.0,
                |cx| change(cx, |s| s.reading.column -= 5.0),
            ),
            Step::new(
                "column-wider",
                "Increase column width",
                IconName::Plus,
                column >= 80.0,
                |cx| change(cx, |s| s.reading.column += 5.0),
            ),
            format!("{column:.0} ch"),
        ))
        .child(section(
            "paragraph-heading",
            "Paragraph spacing",
            foreground,
        ))
        .child(stepper(
            "paragraph-spacing",
            Step::new(
                "paragraph-tighter",
                "Decrease paragraph spacing",
                IconName::Minus,
                gap <= 0.0,
                |cx| {
                    change(cx, |s| {
                        s.reading.paragraph_gap =
                            ((s.reading.paragraph_gap - 0.1) * 10.0).round() / 10.0;
                    });
                },
            ),
            Step::new(
                "paragraph-looser",
                "Increase paragraph spacing",
                IconName::Plus,
                gap >= 2.0,
                |cx| {
                    change(cx, |s| {
                        s.reading.paragraph_gap =
                            ((s.reading.paragraph_gap + 0.1) * 10.0).round() / 10.0;
                    });
                },
            ),
            format!("{gap:.1} em"),
        ))
        .child(reading_behavior(settings))
}

fn reading_behavior(settings: &Settings) -> impl IntoElement {
    v_flex()
        .gap_2()
        .child(choice(
            "reading-heavy".to_owned(),
            "Heavier text",
            settings.reading.heavier,
            |cx| change(cx, |s| s.reading.heavier = !s.reading.heavier),
        ))
        .child(choice(
            "reading-paged".to_owned(),
            "Read one page at a time",
            settings.reading.flow.is_paged(),
            |cx| {
                change(cx, |s| {
                    s.reading.flow = if s.reading.flow.is_paged() {
                        ReadingFlow::Scroll
                    } else {
                        ReadingFlow::Pages
                    };
                });
            },
        ))
        .children(settings.reading.flow.is_paged().then(|| {
            choice(
                "reading-page-motion".to_owned(),
                "Animate page turns",
                settings.reading.flow == ReadingFlow::AnimatedPages,
                |cx| {
                    change(cx, |s| {
                        s.reading.flow = if s.reading.flow == ReadingFlow::AnimatedPages {
                            ReadingFlow::Pages
                        } else {
                            ReadingFlow::AnimatedPages
                        };
                    });
                },
            )
        }))
        .child(choice(
            "reading-reduced-motion".to_owned(),
            "Reduce motion",
            settings.reduce_motion,
            |cx| change(cx, |s| s.reduce_motion = !s.reduce_motion),
        ))
        .child(choice(
            "reading-progress".to_owned(),
            "Hide reading progress",
            settings.reading.hide_progress,
            |cx| change(cx, |s| s.reading.hide_progress = !s.reading.hide_progress),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_size_steps_through_the_enlarged_range() {
        assert!((step_size(36.0, true) - 37.0).abs() < f32::EPSILON);
        assert!((step_size(80.0, true) - 80.0).abs() < f32::EPSILON);
        assert!((step_size(16.0, false) - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn line_height_changes_in_twentieths() {
        assert_eq!(line_height_text(step_line_height(1.6, true)), "1.65");
        assert_eq!(line_height_text(step_line_height(1.6, false)), "1.55");
        assert_eq!(line_height_text(step_line_height(1.3, false)), "1.30");
    }

    #[test]
    fn bundled_and_system_fonts_are_offered_alongside_a_saved_font() {
        assert_eq!(
            typefaces("Georgia", Some("Noto Sans")),
            ["Literata", "Noto Sans", "System", "Georgia"]
        );
    }
}
