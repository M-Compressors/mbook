//! The settings of the catalogue, applied to the interface.
//!
//! [`Appearance`] is the one place that changes the settings. It keeps them in
//! memory, writes them to the catalogue on the background, and applies them:
//! the theme of GPUI Kit, the size of the interface, reduced motion, the
//! language. It also answers the questions every view asks: which page tone,
//! which colours, which size class, which typeface for books, which language.

mod model;
mod theme;

pub(crate) use model::{Appearance, SystemLanguage};
