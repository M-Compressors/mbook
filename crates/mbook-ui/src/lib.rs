//! The mbook application: a library of books, a reader and an editor.
//!
//! The design is RFC 0004 of the repository. This library holds every model
//! and view and names no platform; a shell, such as the binary of this
//! package, supplies the platform and the directories and calls [`run`].
//!
//! The modules are the layers of Feature-Sliced Design. A layer uses only the
//! layers below it: `app`, `pages`, `widgets`, `features`, `entities`,
//! `shared`.

mod app;
mod entities;
mod error;
mod features;
mod pages;
mod shared;
#[cfg(test)]
mod testing;
#[cfg(feature = "visual")]
pub mod visual;
mod widgets;

pub use app::run;
pub use error::{Error, Result};
pub use shared::roots::Roots;
