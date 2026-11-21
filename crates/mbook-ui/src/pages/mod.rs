//! The screens of the application: library, reader, editor and settings.
//!
//! A page is an entity that renders, emits an event for the navigation it asks
//! for, and keeps a focus handle, so that the root can move the keyboard focus
//! into it. A page never opens another page: it emits an event and the root,
//! in `app`, decides (RFC 0004, "Guide-level explanation").

pub(crate) mod editor;
pub(crate) mod library;
pub(crate) mod reader;
pub(crate) mod settings;
