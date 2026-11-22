//! The settings: typeface, text, page tone, interface, motion and updates.
//!
//! One screen. Every control has a visible label that is also its name, and
//! every change goes through [`Appearance::update`], so it shows at once: in
//! the sample at the end of the reading settings, in an open book, in the
//! interface itself. On a compact window the form is one column of the full
//! width; on a larger one it is a column of at most 640 px (RFC 0004,
//! "Settings").

mod choices;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::searchable_list::SearchableVec;
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, IndexPath, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Div, Entity, EventEmitter, FocusHandle, Focusable,
    FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _,
    Window, div, px, relative,
};

use self::choices::{
    INTERFACE_PERCENT, INTERFACE_STEP, LINE_HEIGHT, TEXT_SIZE, TONES, Typeface, line_height_text,
    same, scale_text, size_text, snap_line_height, snap_scale, snap_size, tone_index,
    typeface_index, typeface_items,
};
use crate::entities::catalogue::{CatalogueError, Settings};
use crate::features::appearance::Appearance;
#[cfg(feature = "self-update")]
use crate::features::auto_update::UpdateBanner;
use crate::features::update_banner;
use crate::shared::actions::GoBack;
use crate::shared::layout::SizeClass;
use crate::shared::theme::PageTone;
use crate::shared::ui::{back_button, icon_button, page_colors, page_header, status_line};

/// The widest the form grows, in pixels.
const COLUMN_WIDTH: f32 = 640.0;
/// A sentence in Latin letters, for the sample.
const SAMPLE_LATIN: &str =
    "The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs.";
/// A sentence in Cyrillic letters, for the sample.
const SAMPLE_CYRILLIC: &str = "Съешь же ещё этих мягких французских булок, да выпей чаю.";
/// Why the updates are not offered.
const NO_UPDATES: &str = "This build does not update itself";

/// What the settings ask the root to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsEvent {
    /// Go back to the previous screen.
    Close,
}

type Typefaces = SearchableVec<Typeface>;

/// The page of the settings.
pub(crate) struct SettingsPage {
    focus_handle: FocusHandle,
    typeface: Entity<SelectState<Typefaces>>,
    /// The typeface and the number of families the list was last made for.
    listed: (String, usize),
    text_size: Entity<SliderState>,
    line_height: Entity<SliderState>,
    interface: Entity<SliderState>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SettingsEvent> for SettingsPage {}

/// Changes the settings of the application.
fn change(cx: &mut App, change: impl FnOnce(&mut Settings)) {
    Appearance::global(cx).update(cx, |appearance, cx| appearance.update(change, cx));
}

/// A slider between two bounds, set at `value`.
fn slider(
    range: &std::ops::RangeInclusive<f32>,
    step: f32,
    value: f32,
    cx: &mut App,
) -> Entity<SliderState> {
    cx.new(|_| {
        SliderState::new()
            .min(*range.start())
            .max(*range.end())
            .step(step)
            .default_value(value)
    })
}

/// The single value of a slider event.
fn value_of(event: &SliderEvent) -> f32 {
    match event {
        SliderEvent::Change(value) | SliderEvent::Release(value) => value.end(),
    }
}

impl SettingsPage {
    /// The settings page.
    pub(crate) fn new(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let appearance = Appearance::global(cx);
        let (settings, families) = {
            let appearance = appearance.read(cx);
            (
                appearance.settings().clone(),
                appearance.families().to_vec(),
            )
        };
        let items = typeface_items(&settings.typeface, &families);
        let selected = typeface_index(&items, &settings.typeface).map(IndexPath::new);
        let listed = (settings.typeface.clone(), families.len());
        let typeface = cx.new(|cx| {
            SelectState::new(SearchableVec::new(items), selected, window, cx).searchable(true)
        });
        let text_size = slider(&TEXT_SIZE, 1.0, settings.text_size, cx);
        let line_height = slider(&LINE_HEIGHT, 0.1, settings.line_height, cx);
        let interface = slider(
            &INTERFACE_PERCENT,
            INTERFACE_STEP,
            settings.interface_scale * 100.0,
            cx,
        );

        let subscriptions = vec![
            cx.observe_in(&appearance, window, |this, _, window, cx| {
                this.sync(window, cx);
            }),
            cx.subscribe_in(&typeface, window, |_, _, event, _, cx| {
                let SelectEvent::Confirm(Some(family)) = event else {
                    return;
                };
                let family = family.clone();
                change(cx, |settings| settings.typeface = family);
            }),
            cx.subscribe(&text_size, |_, _, event: &SliderEvent, cx| {
                let size = snap_size(value_of(event));
                change(cx, |settings| settings.text_size = size);
            }),
            cx.subscribe(&line_height, |_, _, event: &SliderEvent, cx| {
                let height = snap_line_height(value_of(event));
                change(cx, |settings| settings.line_height = height);
            }),
            // The size of the interface changes the layout under the slider,
            // so it applies when the pointer is released or a key stepped it.
            cx.subscribe(&interface, |_, _, event: &SliderEvent, cx| {
                if let SliderEvent::Release(value) = event {
                    let scale = snap_scale(value.end());
                    change(cx, |settings| settings.interface_scale = scale);
                }
            }),
        ];
        // What the banner finds shows under "Check now".
        #[cfg(feature = "self-update")]
        let updates =
            UpdateBanner::try_global(cx).map(|banner| cx.observe(&banner, |_, _, cx| cx.notify()));
        #[cfg(not(feature = "self-update"))]
        let updates: Option<Subscription> = None;
        Self {
            focus_handle: cx.focus_handle(),
            typeface,
            listed,
            text_size,
            line_height,
            interface,
            _subscriptions: subscriptions.into_iter().chain(updates).collect(),
        }
    }

    /// Brings the controls to the settings: a change made elsewhere, or by a
    /// button, moves the sliders; a font list that arrived fills the select.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let appearance = Appearance::global(cx);
        let (settings, families) = {
            let appearance = appearance.read(cx);
            (
                appearance.settings().clone(),
                appearance.families().to_vec(),
            )
        };
        for (state, value) in [
            (&self.text_size, settings.text_size),
            (&self.line_height, settings.line_height),
            (&self.interface, settings.interface_scale * 100.0),
        ] {
            if !same(state.read(cx).value().end(), value) {
                state.update(cx, |state, cx| state.set_value(value, window, cx));
            }
        }
        let listed = (settings.typeface.clone(), families.len());
        if listed != self.listed {
            self.listed = listed;
            let items = typeface_items(&settings.typeface, &families);
            let stored = settings.typeface;
            self.typeface.update(cx, |select, cx| {
                select.set_items(SearchableVec::new(items), window, cx);
                select.set_selected_value(&stored, window, cx);
            });
        }
        cx.notify();
    }

    /// Whether the build updates itself, and what its state says.
    #[cfg(feature = "self-update")]
    fn updates(cx: &App) -> Updates {
        match UpdateBanner::try_global(cx) {
            Some(banner) => {
                let banner = banner.read(cx);
                Updates {
                    enabled: banner.is_enabled(),
                    checking: banner.state().is_working(),
                    status: if banner.is_enabled() {
                        banner.state().summary()
                    } else {
                        NO_UPDATES.to_owned()
                    },
                }
            }
            None => Updates::unavailable(),
        }
    }

    #[cfg(not(feature = "self-update"))]
    fn updates(_: &App) -> Updates {
        Updates::unavailable()
    }
}

/// What the settings show about updates.
struct Updates {
    /// Whether the build updates itself.
    enabled: bool,
    /// Whether a check or an installation is under way.
    checking: bool,
    /// The sentence under the button.
    status: String,
}

impl Updates {
    fn unavailable() -> Self {
        Self {
            enabled: false,
            checking: false,
            status: NO_UPDATES.to_owned(),
        }
    }
}

/// Asks the releases now.
#[cfg(feature = "self-update")]
fn check_now(cx: &mut App) {
    if let Some(banner) = UpdateBanner::try_global(cx) {
        banner.update(cx, UpdateBanner::check_now);
    }
}

#[cfg(not(feature = "self-update"))]
fn check_now(_: &mut App) {}

impl Focusable for SettingsPage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// The title of a group of settings.
fn heading(id: &'static str, text: &'static str) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Heading)
        .aria_level(2)
        .aria_label(text)
        .test_support()
        .text_lg()
        .font_weight(FontWeight::SEMIBOLD)
        .child(text)
}

/// Text that stands for a value or a state, reported with its text as name.
fn value_text(id: &'static str, text: impl Into<SharedString>) -> impl IntoElement {
    let text = text.into();
    div()
        .id(id)
        .role(Role::Label)
        .aria_label(text.clone())
        .test_support()
        .child(text)
}

/// The visible label of a field.
fn label(text: &'static str) -> Div {
    div().font_weight(FontWeight::MEDIUM).child(text)
}

/// A field: its visible label over its control.
fn field(text: &'static str, control: impl IntoElement) -> Div {
    v_flex().w_full().gap_2().child(label(text)).child(control)
}

/// A field that is a group of choices, named by its label.
fn choices(id: &'static str, text: &'static str, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .id(id)
        .role(Role::Group)
        .aria_label(text)
        .test_support()
        .w_full()
        .gap_2()
        .child(label(text))
        .child(control)
}

/// A page tone as a swatch: the page and the text colour on it.
fn swatch(tone: PageTone, border: gpui_kit::Hsla) -> impl IntoElement {
    let (page, text) = page_colors(tone);
    div()
        .w(px(56.0))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .border_1()
        .border_color(border)
        .bg(page)
        .text_color(text)
        .text_sm()
        .child("Aa")
}

impl SettingsPage {
    fn typeface_field(&self) -> AnyElement {
        field(
            "Typeface of books",
            Select::new(&self.typeface)
                .id("typeface")
                .accessibility_label("Typeface of books")
                .search_placeholder("Search typefaces")
                .w_full(),
        )
        .into_any_element()
    }

    fn text_size_field(&self, settings: &Settings, cx: &mut Context<'_, Self>) -> AnyElement {
        let size = settings.text_size;
        let shown = self.text_size.read(cx).value().end();
        v_flex()
            .w_full()
            .gap_2()
            .child(
                h_flex()
                    .justify_between()
                    .child(label("Size of the text"))
                    .child(value_text("text-size-value", size_text(shown))),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(
                        icon_button("text-smaller", IconName::Minus, "Smaller text")
                            .disabled(size <= *TEXT_SIZE.start())
                            .on_click(|_, _, cx| {
                                change(cx, |settings| settings.text_size -= 1.0);
                            }),
                    )
                    .child(div().flex_1().px_2().child(
                        Slider::new(&self.text_size).accessibility_label("Size of the text"),
                    ))
                    .child(
                        icon_button("text-larger", IconName::Plus, "Larger text")
                            .disabled(size >= *TEXT_SIZE.end())
                            .on_click(|_, _, cx| {
                                change(cx, |settings| settings.text_size += 1.0);
                            }),
                    ),
            )
            .into_any_element()
    }

    fn line_height_field(&self, cx: &mut Context<'_, Self>) -> AnyElement {
        let shown = self.line_height.read(cx).value().end();
        v_flex()
            .w_full()
            .gap_2()
            .child(
                h_flex()
                    .justify_between()
                    .child(label("Line height"))
                    .child(value_text("line-height-value", line_height_text(shown))),
            )
            .child(
                div()
                    .w_full()
                    .px_2()
                    .child(Slider::new(&self.line_height).accessibility_label("Line height")),
            )
            .into_any_element()
    }

    fn page_tone_field(settings: &Settings, border: gpui_kit::Hsla) -> AnyElement {
        choices(
            "page-tone",
            "Page tone",
            RadioGroup::vertical("page-tone-options")
                .selected_index(Some(tone_index(settings.page_tone)))
                .on_click(|index, _, cx| {
                    let tone = TONES.get(*index).and_then(|(tone, _)| *tone);
                    change(cx, |settings| settings.page_tone = tone);
                })
                .children(TONES.iter().map(|(tone, name)| {
                    let radio = Radio::new(*name).label(*name);
                    match tone {
                        Some(tone) => radio.child(swatch(*tone, border)),
                        None => radio,
                    }
                })),
        )
        .into_any_element()
    }

    fn sample(settings: &Settings, tone: PageTone, font: SharedString, cx: &App) -> AnyElement {
        let (page, text) = page_colors(tone);
        v_flex()
            .id("settings-sample")
            .role(Role::Group)
            .aria_label("Sample of the text")
            .test_support()
            .w_full()
            .gap_2()
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(page)
            .text_color(text)
            .font_family(font)
            .text_size(px(settings.text_size))
            .line_height(relative(settings.line_height))
            .child(SAMPLE_LATIN)
            .child(SAMPLE_CYRILLIC)
            .into_any_element()
    }

    fn interface_field(&self, cx: &mut Context<'_, Self>) -> AnyElement {
        let percent = self.interface.read(cx).value().end();
        v_flex()
            .w_full()
            .gap_2()
            .child(
                h_flex()
                    .justify_between()
                    .child(label("Size of the interface"))
                    .child(value_text(
                        "interface-size-value",
                        scale_text(snap_scale(percent)),
                    )),
            )
            .child(
                div().w_full().px_2().child(
                    Slider::new(&self.interface).accessibility_label("Size of the interface"),
                ),
            )
            .into_any_element()
    }

    fn controls_field(settings: &Settings) -> AnyElement {
        choices(
            "reader-controls",
            "Controls of the reader",
            RadioGroup::vertical("reader-controls-options")
                .selected_index(Some(usize::from(settings.keep_controls)))
                .on_click(|index, _, cx| {
                    let keep = *index == 1;
                    change(cx, |settings| settings.keep_controls = keep);
                })
                .children(["Hide while reading", "Always shown"]),
        )
        .into_any_element()
    }

    fn motion_field(settings: &Settings) -> AnyElement {
        choices(
            "motion",
            "Motion",
            RadioGroup::vertical("motion-options")
                .selected_index(Some(usize::from(settings.reduce_motion)))
                .on_click(|index, _, cx| {
                    let reduce = *index == 1;
                    change(cx, |settings| settings.reduce_motion = reduce);
                })
                .children(["As the system says", "Reduced"]),
        )
        .into_any_element()
    }

    fn updates_group(settings: &Settings, updates: &Updates) -> AnyElement {
        v_flex()
            .w_full()
            .gap_3()
            .child(
                Switch::new("check-updates")
                    .label("Check for updates")
                    .checked(settings.check_updates)
                    .disabled(!updates.enabled)
                    .on_click(|checked, _, cx| {
                        let checked = *checked;
                        change(cx, |settings| settings.check_updates = checked);
                    }),
            )
            .child(value_text(
                "app-version",
                format!("Version {}", env!("CARGO_PKG_VERSION")),
            ))
            .child(
                div().child(
                    Button::new("check-now")
                        .label("Check now")
                        .min_h(px(crate::shared::ui::target_size()))
                        .disabled(!updates.enabled || updates.checking)
                        .on_click(|_, _, cx| check_now(cx)),
                ),
            )
            .when(!updates.status.is_empty(), |this| {
                this.child(status_line("update-status", updates.status.clone()))
            })
            .into_any_element()
    }
}

/// What went wrong with the catalogue, and what to do about it, in words for
/// a person. The changes stay in effect in the window; any change tries the
/// write again.
fn save_error_words(error: &CatalogueError) -> String {
    format!(
        "Settings could not be saved: {}. They still apply until the application closes; change a setting to try saving again.",
        crate::features::import_book::error_chain(error)
    )
}

impl Render for SettingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let (settings, class, tone, font, border) = {
            let appearance = Appearance::global(cx).read(cx);
            (
                appearance.settings().clone(),
                appearance.size_class(window),
                appearance.page_tone(window),
                appearance.reading_font(cx),
                crate::shared::ui::ChromeColors::from(&appearance.chrome(window)).outline,
            )
        };
        let updates = Self::updates(cx);
        let compact = class == SizeClass::Compact;
        let failure = Appearance::global(cx)
            .read(cx)
            .error()
            .map(save_error_words);

        let mut column =
            v_flex()
                .w_full()
                .mx_auto()
                .gap_6()
                .p(px(if compact { 16.0 } else { 24.0 }));
        if !compact {
            column = column.max_w(px(COLUMN_WIDTH));
        }
        let column = column
            .children(failure.map(|words| {
                status_line("settings-error", words)
                    .text_sm()
                    .text_color(cx.theme().danger)
            }))
            .child(heading("reading-heading", "Reading"))
            .child(self.typeface_field())
            .child(self.text_size_field(&settings, cx))
            .child(self.line_height_field(cx))
            .child(Self::page_tone_field(&settings, border))
            .child(Self::sample(&settings, tone, font, cx))
            .child(heading("interface-heading", "Interface"))
            .child(self.interface_field(cx))
            .child(Self::controls_field(&settings))
            .child(Self::motion_field(&settings))
            .child(heading("updates-heading", "Updates"))
            .child(Self::updates_group(&settings, &updates));

        v_flex()
            .id("settings-page")
            .role(Role::Main)
            .aria_label("Settings")
            .test_support()
            .key_context("Settings")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|_, _: &GoBack, _, cx| cx.emit(SettingsEvent::Close)))
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                page_header("Settings").leading(
                    back_button("back")
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(SettingsEvent::Close))),
                ),
            )
            .children(update_banner(cx))
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .child(column),
            )
    }
}

#[cfg(test)]
mod tests;
