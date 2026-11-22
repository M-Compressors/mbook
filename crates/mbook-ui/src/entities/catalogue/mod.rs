//! The catalogue: the store of the library.
//!
//! One `redb` database with three tables, `books`, `state` and `settings`,
//! and a schema version (mbook-ui docs, "The catalogue"). The store is
//! synchronous and knows nothing of the interface; it is shared as an
//! `Arc<Catalogue>` and called from background tasks.

mod error;
mod model;
mod rows;
mod settings;
mod store;

pub(crate) use error::CatalogueError;
pub(crate) use model::{Library, LibraryError, ShelfBook, Store};
pub(crate) use rows::{BookRow, ReadingState};
pub(crate) use settings::{LINE_HEIGHT, READING_FACES, ReadingFlow, Settings, TEXT_SIZE};
pub(crate) use store::Catalogue;
