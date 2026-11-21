//! The tokens of RFC 0004 projected onto the theme of GPUI Kit.
//!
//! The Kit has one global `Theme` with about two hundred colours; the design
//! has eight tokens. Every Kit colour is derived from a token, so that no
//! control keeps a colour the design did not choose. The danger, success,
//! warning and info colours, and the code highlighting, are not part of the
//! design and stay as the Kit has them.

use gpui_kit::component::{Colorize as _, Theme, ThemeColor, ThemeMode};
use gpui_kit::{App, Hsla};

use crate::shared::theme::Chrome;
use crate::shared::ui::ChromeColors;

/// How much of the accent a hovered or selected surface takes.
const HOVER: f32 = 0.10;
/// How much of the accent a pressed or current surface takes.
const ACTIVE: f32 = 0.20;
/// How much ink a hovered primary control takes.
const PRIMARY_HOVER: f32 = 0.15;
/// How much ink a pressed primary control takes.
const PRIMARY_ACTIVE: f32 = 0.30;

/// Makes the Kit draw with the colours of `chrome`.
///
/// `dark` picks the mode of the Kit, which decides what its components do
/// with the colours it has no token for (shadows, the strength of overlays).
/// Loading a mode replaces every colour, so the tokens are applied in a second
/// step.
pub(super) fn project(chrome: &Chrome, dark: bool, cx: &mut App) {
    let mode = if dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    Theme::change(mode, None, cx);
    let colours = ChromeColors::from(chrome);
    Theme::update(cx, |theme| {
        assign(&mut theme.colors, &colours);
        theme.focus_ring = true;
    });
}

/// `base` with `amount` of `with` mixed in.
fn mix(base: Hsla, with: Hsla, amount: f32) -> Hsla {
    base.mix_oklab(with, 1.0 - amount)
}

fn assign(colors: &mut ThemeColor, c: &ChromeColors) {
    let hover = mix(c.surface, c.accent, HOVER);
    let active = mix(c.surface, c.accent, ACTIVE);
    let primary_hover = mix(c.accent, c.ink, PRIMARY_HOVER);
    let primary_active = mix(c.accent, c.ink, PRIMARY_ACTIVE);
    let secondary_hover = mix(c.line, c.accent, HOVER);
    let secondary_active = mix(c.line, c.accent, ACTIVE);

    colors.background = c.canvas;
    colors.foreground = c.ink;
    // Separators are decoration; the edge of a field or a control is `input`.
    colors.border = c.line;
    colors.input = c.outline;
    colors.ring = c.accent;
    colors.caret = c.ink;
    colors.muted = c.line;
    colors.muted_foreground = c.ink_muted;
    colors.accent = hover;
    colors.accent_foreground = c.ink;
    colors.selection = c.accent.opacity(0.3);
    colors.popover = c.surface;
    colors.popover_foreground = c.ink;
    colors.overlay = c.ink.opacity(0.4);

    colors.primary = c.accent;
    colors.primary_foreground = c.on_accent;
    colors.primary_hover = primary_hover;
    colors.primary_active = primary_active;
    colors.secondary = c.line;
    colors.secondary_foreground = c.ink;
    colors.secondary_hover = secondary_hover;
    colors.secondary_active = secondary_active;

    colors.button = c.surface;
    colors.button_foreground = c.ink;
    colors.button_hover = hover;
    colors.button_active = active;
    colors.button_primary = c.accent;
    colors.button_primary_foreground = c.on_accent;
    colors.button_primary_hover = primary_hover;
    colors.button_primary_active = primary_active;
    colors.button_secondary = c.line;
    colors.button_secondary_foreground = c.ink;
    colors.button_secondary_hover = secondary_hover;
    colors.button_secondary_active = secondary_active;

    colors.link = c.accent;
    colors.link_hover = primary_hover;
    colors.link_active = primary_active;

    colors.list = c.surface;
    colors.list_even = c.surface;
    colors.list_head = c.canvas;
    colors.list_hover = hover;
    colors.list_active = active;
    colors.list_active_border = c.accent;

    colors.sidebar = c.surface;
    colors.sidebar_foreground = c.ink;
    colors.sidebar_border = c.line;
    colors.sidebar_accent = hover;
    colors.sidebar_accent_foreground = c.ink;
    colors.sidebar_primary = c.accent;
    colors.sidebar_primary_foreground = c.on_accent;

    colors.table = c.surface;
    colors.table_even = c.surface;
    colors.table_head = c.canvas;
    colors.table_head_foreground = c.ink_muted;
    colors.table_hover = hover;
    colors.table_active = active;
    colors.table_active_border = c.accent;
    colors.table_row_border = c.line;

    colors.title_bar = c.canvas;
    colors.title_bar_border = c.line;

    colors.progress_bar = c.accent;
    colors.slider_bar = c.accent;
    colors.slider_thumb = c.accent;
    colors.switch = c.outline;
    colors.switch_thumb = c.surface;
    colors.skeleton = c.line;
    colors.scrollbar_thumb = c.outline.opacity(0.6);
    colors.scrollbar_thumb_hover = c.outline;
    colors.drag_border = c.accent;
    colors.drop_target = c.accent.opacity(0.1);
}
