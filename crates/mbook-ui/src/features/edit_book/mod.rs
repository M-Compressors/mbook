//! Editing a book: the task that owns it and the commands it takes.
//!
//! A session ([`actor::session`]) is a future that owns a [`mbook::Book`] and a handle
//! ([`EditHandle`](actor::EditHandle)) that sends it [`Command`](command::Command)s. The future is spawned on a
//! background executor; commands are applied in the order they were sent and
//! each answers with an [`Outline`](outline::Outline) of the book, which is all the main thread
//! knows of it (RFC 0004, "Editing").
//!
//! The future uses only channels, so it runs on any executor.

pub(crate) mod actor;
pub(crate) mod command;
pub(crate) mod error;
pub(crate) mod outline;
#[cfg(test)]
mod tests;
