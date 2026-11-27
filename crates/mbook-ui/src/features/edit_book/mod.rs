//! Editing a book: the task that owns it and the commands it takes.
//!
//! A session ([`actor::session`]) is a future that owns a [`mbook::Book`] and a handle
//! ([`EditHandle`](actor::EditHandle)) that sends it [`Command`](command::Command)s. The future is spawned on a
//! background executor and hands opening the book and each command to the
//! pool of `compute`; commands are applied in the order they were sent and
//! each answers with an [`Outline`](outline::Outline) of the book, which is all the main thread
//! knows of it (mbook-ui docs, "Editing").
//!
//! The future uses only channels and the pool, so it runs on any executor.

pub(crate) mod actor;
#[cfg(not(target_os = "android"))]
pub(crate) mod agent;
pub(crate) mod command;
mod contents;
pub(crate) mod error;
pub(crate) mod outline;
pub(crate) mod revisions;
pub(crate) mod sessions;
#[cfg(test)]
mod tests;
