//! A book that is open for reading.
//!
//! The main thread holds a [`snapshot::BookSnapshot`], which is immutable. Background
//! tasks call the [`source::BookSource`], which owns the file: it prepares chapters,
//! reads pictures and completes links (mbook-ui docs, "Reading" and "Threads").
//! Nothing here depends on GPUI.

pub(crate) mod chapter;
pub(crate) mod error;
pub(crate) mod snapshot;
pub(crate) mod source;
#[cfg(test)]
mod tests;
