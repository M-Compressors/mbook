//! Keeping the place a book was left at.
//!
//! [`reading_state`] turns a position in a chapter into what the catalogue
//! stores, and [`PositionKeeper`] writes it: at most once a second while
//! reading, and at once when it is told to or when it is dropped (RFC 0004,
//! "Reading").

mod keeper;
mod state;

pub(crate) use keeper::PositionKeeper;
pub(crate) use state::{now_ms, reading_state};

#[cfg(test)]
mod tests;
