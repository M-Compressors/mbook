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

use crate::entities::catalogue::Settings;
use crate::features::appearance::Appearance;
use crate::shared::actions::OpenSettings;
use crate::shared::theme::PageTone;
use crate::shared::ui::{icon_button, page_colors, target_size};

/// The smallest and the largest text, in pixels.
const TEXT_SIZE: (f32, f32) = (12.0, 32.0);
/// How much "smaller" and "larger" change the text, in pixels.
const TEXT_STEP: f32 = 2.0;
/// The tightest and the loosest lines.
const LINE_HEIGHT: (f32, f32) = (1.2, 2.0);
/// How much "tighter" and "looser" change the line height.
const LINE_STEP: f32 = 0.1;
/// The typeface the application is built around.
pub(super) const DEFAULT_TYPEFACE: &str = "Literata";

/// The choices of the page tone, in the order they are offered. `None` follows
/// the system.
pub(super) const TONES: [Option<PageTone>; 5] = [
    None,
    Some(PageTone::Paper),
    Some(PageTone::Sepia),
    Some(PageTone::Dusk),
    Some(PageTone::Night),
];

/// The name of a page tone choice.
pub(super) fn tone_name(tone: Option<PageTone>) -> &'static str {
    match tone {
        None => "Follow the system",
        Some(PageTone::Paper) => "Paper",
        Some(PageTone::Sepia) => "Sepia",
        Some(PageTone::Dusk) => "Dusk",
        Some(PageTone::Night) => "Night",
    }
}

/// `size` one step larger or smaller, kept in its range.
pub(super) fn step_size(size: f32, larger: bool) -> f32 {
    let step = if larger { TEXT_STEP } else { -TEXT_STEP };
    (size + step).clamp(TEXT_SIZE.0, TEXT_SIZE.1)
}

/// `height` one step looser or tighter, kept in its range and rounded to a
/// tenth.
pub(super) fn step_line_height(height: f32, looser: bool) -> f32 {
    let step = if looser { LINE_STEP } else { -LINE_STEP };
    (((height + step) * 10.0).round() / 10.0).clamp(LINE_HEIGHT.0, LINE_HEIGHT.1)
}

/// The size as it is read out: "18 pixels".
pub(super) fn size_text(size: f32) -> String {
    format!("{size:.0} pixels")
}

/// The line height as it is read out: "1.6".
pub(super) fn line_height_text(height: f32) -> String {
    format!("{height:.1}")
}

/// The typefaces on offer: the default and the stored one when they differ,
/// and the one used before it, so that a reader can go back and forth.
pub(super) fn typefaces(current: &str, before: Option<&str>) -> Vec<String> {
    let mut faces = vec![DEFAULT_TYPEFACE.to_owned()];
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
    let name = tone_name(tone);
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
            theme.border
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let settings = Appearance::global(cx).read(cx).settings().clone();
        let muted = cx.theme().muted_foreground;
        let size = settings.text_size;
        let height = settings.line_height;
        let tones = h_flex()
            .id("tones")
            .role(Role::Group)
            .aria_label("Page colour")
            .test_support()
            .flex_wrap()
            .gap_2()
            .children(
                TONES
                    .into_iter()
                    .map(|tone| tone_swatch(tone, settings.page_tone == tone, cx)),
            );
        v_flex()
            .id("appearance-panel")
            .role(Role::Group)
            .aria_label("Text and page appearance")
            .test_support()
            .w_full()
            .gap_3()
            .child(section("tone-heading", "Page colour", muted))
            .child(tones)
            .child(section("size-heading", "Text size", muted))
            .child(stepper(
                "text-size",
                Step::new(
                    "text-smaller",
                    "Smaller",
                    IconName::AArrowDown,
                    size <= TEXT_SIZE.0,
                    |cx| {
                        change(cx, |s| s.text_size = step_size(s.text_size, false));
                    },
                ),
                Step::new(
                    "text-larger",
                    "Larger",
                    IconName::AArrowUp,
                    size >= TEXT_SIZE.1,
                    |cx| {
                        change(cx, |s| s.text_size = step_size(s.text_size, true));
                    },
                ),
                size_text(size),
            ))
            .child(section("line-heading", "Line height", muted))
            .child(stepper(
                "line-height",
                Step::new(
                    "line-tighter",
                    "Tighter",
                    IconName::FoldVertical,
                    height <= LINE_HEIGHT.0,
                    |cx| {
                        change(cx, |s| {
                            s.line_height = step_line_height(s.line_height, false);
                        });
                    },
                ),
                Step::new(
                    "line-looser",
                    "Looser",
                    IconName::UnfoldVertical,
                    height >= LINE_HEIGHT.1,
                    |cx| {
                        change(cx, |s| {
                            s.line_height = step_line_height(s.line_height, true);
                        });
                    },
                ),
                line_height_text(height),
            ))
            .child(section("typeface-heading", "Typeface", muted))
            .child(face_choices(&settings.typeface, self.before.as_deref()))
            .child(
                Button::new("more-typefaces")
                    .ghost()
                    .label("All typefaces in Settings")
                    .min_h(px(target_size()))
                    .cursor_pointer()
                    .on_click(|_, window, cx| {
                        use gpui_kit::Action as _;
                        window.dispatch_action(OpenSettings.boxed_clone(), cx);
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_tone_has_a_name_that_is_not_only_a_swatch() {
        let names: Vec<_> = TONES.into_iter().map(tone_name).collect();
        assert_eq!(
            names,
            ["Follow the system", "Paper", "Sepia", "Dusk", "Night"]
        );
    }

    #[test]
    fn the_text_grows_and_shrinks_by_a_step() {
        assert!((step_size(18.0, true) - 20.0).abs() < f32::EPSILON);
        assert!((step_size(18.0, false) - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_text_stays_between_12_and_32_pixels() {
        assert!((step_size(32.0, true) - 32.0).abs() < f32::EPSILON);
        assert!((step_size(12.0, false) - 12.0).abs() < f32::EPSILON);
        assert!((step_size(31.0, true) - 32.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_lines_loosen_and_tighten_by_a_tenth() {
        assert!((step_line_height(1.6, true) - 1.7).abs() < 1e-4);
        assert!((step_line_height(1.6, false) - 1.5).abs() < 1e-4);
    }

    #[test]
    fn the_line_height_stays_between_1_2_and_2() {
        assert!((step_line_height(2.0, true) - 2.0).abs() < 1e-4);
        assert!((step_line_height(1.2, false) - 1.2).abs() < 1e-4);
    }

    #[test]
    fn the_values_are_read_out_in_words() {
        assert_eq!(size_text(18.0), "18 pixels");
        assert_eq!(line_height_text(1.6), "1.6");
    }

    #[test]
    fn the_default_typeface_is_always_offered() {
        assert_eq!(typefaces("Literata", None), ["Literata"]);
    }

    #[test]
    fn the_stored_typeface_and_the_one_before_are_offered_after_the_default() {
        assert_eq!(
            typefaces("Georgia", Some("Palatino")),
            ["Literata", "Georgia", "Palatino"]
        );
    }

    #[test]
    fn a_typeface_is_not_offered_twice() {
        assert_eq!(
            typefaces("Georgia", Some("Georgia")),
            ["Literata", "Georgia"]
        );
        assert_eq!(
            typefaces("Georgia", Some("Literata")),
            ["Literata", "Georgia"]
        );
    }

    #[test]
    fn a_blank_typeface_is_not_offered() {
        assert_eq!(typefaces("  ", Some("")), ["Literata"]);
    }
}
