//! The errors of the catalogue.

use std::error::Error;
use std::io;
use std::path::PathBuf;

/// What can go wrong when the catalogue is opened, read or written.
///
/// The error of the database is kept as the [`source`](Error::source), so
/// that no type of the database appears in a signature of the application.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum CatalogueError {
    /// The database failed: it cannot be opened, is damaged, or a
    /// transaction did not complete.
    #[error("the catalogue database failed")]
    Storage(#[source] Box<dyn Error + Send + Sync>),
    /// The database was written by a newer version of the application.
    #[error(
        "the catalogue has schema version {found}, and this application \
         knows versions up to {supported}"
    )]
    NewerSchema {
        /// The version found in the database.
        found: u32,
        /// The newest version this application reads and writes.
        supported: u32,
    },
    /// The folder of books cannot be listed.
    #[error("cannot list the folder of books {}", path.display())]
    Folder {
        /// The folder.
        path: PathBuf,
        /// What the system said.
        #[source]
        source: io::Error,
    },
}

impl CatalogueError {
    /// Wraps an error of the database.
    pub(crate) fn storage(error: impl Error + Send + Sync + 'static) -> Self {
        Self::Storage(Box::new(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_storage_error_keeps_its_cause_out_of_its_message() {
        let cause = io::Error::other("disk gone");
        let error = CatalogueError::storage(cause);
        assert_eq!(error.to_string(), "the catalogue database failed");
        let source = error.source().map(ToString::to_string);
        assert_eq!(source.as_deref(), Some("disk gone"));
    }

    #[test]
    fn a_newer_schema_names_both_versions() {
        let error = CatalogueError::NewerSchema {
            found: 9,
            supported: 1,
        };
        assert_eq!(
            error.to_string(),
            "the catalogue has schema version 9, and this application knows versions up to 1"
        );
        assert!(error.source().is_none());
    }

    #[test]
    fn a_folder_error_names_the_folder_and_keeps_the_io_error() {
        let error = CatalogueError::Folder {
            path: PathBuf::from("books"),
            source: io::Error::other("denied"),
        };
        assert_eq!(error.to_string(), "cannot list the folder of books books");
        let source = error.source().map(ToString::to_string);
        assert_eq!(source.as_deref(), Some("denied"));
    }
}
