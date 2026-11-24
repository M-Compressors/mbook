//! Presets over the components of GPUI Kit that every page reuses.
//!
//! They are small `RenderOnce` structs and functions, never hand-made
//! clickable `div`s where the Kit has a control: focus, roles, names and
//! tooltips then come from the Kit. Views take their colours from the Kit
//! theme (`cx.theme()`) or from [`ChromeColors`], and write no hex of their
//! own.

mod colors;
mod empty_state;
mod focus;
mod icon_button;
mod language;
mod page_header;
pub(crate) mod search_bar;
mod status_line;
mod swipe;

pub(crate) use colors::{ChromeColors, page_colors};
pub(crate) use empty_state::empty_state;
pub(crate) use focus::focus_ring;
pub(crate) use icon_button::{back_button, icon_button, target_size};
pub(crate) use language::follow_language;
pub(crate) use page_header::{BAR_HEIGHT, page_header};
pub(crate) use status_line::{announce, fit_notifications, status_line};
pub(crate) use swipe::{Side, Sideways};
