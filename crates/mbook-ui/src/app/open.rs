//! Opening the library: the folders and the catalogue.
//!
//! Everything here touches files. It runs on the background executor.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::entities::catalogue::{Catalogue, CatalogueError};
use crate::shared::roots::Roots;

/// The extension of the file of a book.
const BOOK_EXTENSION: &str = "mbook";

/// Why the library could not be opened.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub(super) enum OpenError {
    /// A folder of the library could not be made or emptied.
    #[error("the folders of the library could not be prepared")]
    Folders(#[source] io::Error),
    /// The catalogue could not be opened, or rebuilt from the books.
    #[error("the catalogue could not be opened")]
    Catalogue(#[from] CatalogueError),
    /// The folder of the books could not be read.
    #[error("the folder of the books could not be read")]
    Books(#[source] io::Error),
    /// The catalogue could not be opened and the unreadable file could not be
    /// moved aside to make a new one.
    #[error("the damaged catalogue could not be moved aside")]
    Damaged(#[source] io::Error),
}

/// A catalogue that is open.
#[derive(Debug)]
pub(super) struct Opened {
    /// The catalogue.
    pub(super) catalogue: Arc<Catalogue>,
    /// Whether the file could not be read, was moved aside and was made again
    /// from the books: the reading places and the settings are lost.
    pub(super) damaged: bool,
}

/// The name the file of an unreadable catalogue is moved to.
const DAMAGED_EXTENSION: &str = "damaged";

/// Prepares the folders and opens the catalogue.
///
/// When the catalogue holds no book but the folder of the books holds files of
/// books, the catalogue was lost: the rows are made again from the files
/// ([`Catalogue::rebuild`]), and the reading places and settings are lost with
/// it (mbook-ui docs, "The catalogue").
///
/// When the file cannot be opened, it is moved aside to `catalog.redb.damaged`
/// (replacing an older one), a new catalogue is made and rebuilt from the
/// books, and the result says so. Two failures are left as they are, because
/// moving the file would lose data that is not lost: a file that a newer
/// version wrote, which this version must not touch, and a file that another
/// running copy of the application holds open.
pub(super) fn open_catalogue(roots: &Roots) -> Result<Opened, OpenError> {
    roots.prepare().map_err(OpenError::Folders)?;
    let path = roots.catalogue();
    let (catalogue, damaged) = match Catalogue::open(&path) {
        Ok(catalogue) => (catalogue, false),
        Err(error) if keeps_its_file(&error) => return Err(error.into()),
        Err(_) => {
            move_aside(&path).map_err(OpenError::Damaged)?;
            (Catalogue::open(&path)?, true)
        }
    };
    if catalogue.books()?.is_empty() && holds_books(&roots.books()).map_err(OpenError::Books)? {
        catalogue.rebuild(&roots.books())?;
    }
    Ok(Opened {
        catalogue: Arc::new(catalogue),
        damaged,
    })
}

/// Whether a failure to open the catalogue is one where the file must stay
/// where it is.
fn keeps_its_file(error: &CatalogueError) -> bool {
    match error {
        CatalogueError::NewerSchema { .. } => true,
        CatalogueError::Storage(source) => matches!(
            source.downcast_ref::<redb::DatabaseError>(),
            Some(
                redb::DatabaseError::DatabaseAlreadyOpen
                    | redb::DatabaseError::LockProtocolMismatch
            )
        ),
        _ => false,
    }
}

/// Moves the file of an unreadable catalogue to the same name with
/// `.damaged` added, replacing what an earlier failure left there.
fn move_aside(path: &Path) -> io::Result<()> {
    let mut aside = path.as_os_str().to_owned();
    aside.push(".");
    aside.push(DAMAGED_EXTENSION);
    let aside = PathBuf::from(aside);
    match fs::remove_file(&aside) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::rename(path, aside)
}

/// Whether the folder directly holds a file that is named like a book: with
/// the extension `mbook`. Sub-folders, such as the one of unfinished imports,
/// are not looked into.
fn holds_books(dir: &Path) -> io::Result<bool> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_file() && path.extension().is_some_and(|ext| ext == BOOK_EXTENSION)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::shared::model::BookKey;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn roots(dir: &tempfile::TempDir) -> Roots {
        Roots::new(dir.path().join("data"), dir.path().join("cache"))
    }

    fn save_book(roots: &Roots, title: &str) -> Result<BookKey, mbook::Error> {
        let key = BookKey::generate();
        let mut book = mbook::Book::new(title, "en")?;
        book.add_chapter("ch01.md", "# One\n\nText.\n")?;
        book.save(roots.book(key))?;
        Ok(key)
    }

    #[test]
    fn a_first_start_makes_the_folders_and_an_empty_catalogue() -> TestResult {
        let dir = tempfile::tempdir()?;
        let roots = roots(&dir);
        let opened = open_catalogue(&roots)?;
        let catalogue = opened.catalogue;
        assert!(!opened.damaged);
        assert!(roots.books().is_dir());
        assert!(roots.catalogue().is_file());
        assert_eq!(catalogue.books()?.len(), 0);
        Ok(())
    }

    #[test]
    fn a_lost_catalogue_is_rebuilt_from_the_books() -> TestResult {
        let dir = tempfile::tempdir()?;
        let roots = roots(&dir);
        roots.prepare()?;
        let key = save_book(&roots, "Dune")?;
        let catalogue = open_catalogue(&roots)?.catalogue;
        let rows = catalogue.books()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, key);
        assert_eq!(rows[0].title, "Dune");
        Ok(())
    }

    #[test]
    fn a_catalogue_that_has_books_is_not_rebuilt() -> TestResult {
        let dir = tempfile::tempdir()?;
        let roots = roots(&dir);
        roots.prepare()?;
        save_book(&roots, "Dune")?;
        open_catalogue(&roots)?;
        // A file of a book the catalogue does not list: a rebuild would add it.
        save_book(&roots, "Emma")?;
        let catalogue = open_catalogue(&roots)?.catalogue;
        assert_eq!(catalogue.books()?.len(), 1);
        Ok(())
    }

    #[test]
    fn unfinished_imports_do_not_count_as_books() -> TestResult {
        let dir = tempfile::tempdir()?;
        let roots = roots(&dir);
        roots.prepare()?;
        fs::write(roots.incoming().join("half.mbook"), b"half")?;
        assert!(!holds_books(&roots.books())?);
        Ok(())
    }

    #[test]
    fn a_folder_with_other_files_holds_no_books() -> TestResult {
        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join("notes.txt"), b"notes")?;
        assert!(!holds_books(dir.path())?);
        Ok(())
    }

    #[test]
    fn a_root_that_cannot_be_a_folder_is_reported_with_its_cause() -> TestResult {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("file");
        fs::write(&file, b"")?;
        let roots = Roots::new(file.join("data"), dir.path().join("cache"));
        let error = open_catalogue(&roots).unwrap_err();
        assert!(matches!(error, OpenError::Folders(_)));
        assert_eq!(
            error.to_string(),
            "the folders of the library could not be prepared"
        );
        assert!(error.source().is_some());
        Ok(())
    }

    /// Writes what is no database where the file of the catalogue goes.
    fn damage(roots: &Roots) -> TestResult {
        roots.prepare()?;
        fs::write(roots.catalogue(), vec![0xAB; 8192])?;
        Ok(())
    }

    fn damaged_file(roots: &Roots) -> PathBuf {
        roots.data().join("catalog.redb.damaged")
    }

    #[test]
    fn a_catalogue_that_cannot_be_opened_is_moved_aside_and_made_again() -> TestResult {
        let dir = tempfile::tempdir()?;
        let roots = roots(&dir);
        damage(&roots)?;
        let opened = open_catalogue(&roots)?;
        assert!(opened.damaged);
        assert_eq!(fs::read(damaged_file(&roots))?, vec![0xAB; 8192]);
        assert_eq!(opened.catalogue.books()?, Vec::new());
        assert!(roots.catalogue().is_file());
        Ok(())
    }

    #[test]
    fn a_damaged_catalogue_is_rebuilt_from_the_books() -> TestResult {
        let dir = tempfile::tempdir()?;
        let roots = roots(&dir);
        damage(&roots)?;
        let key = save_book(&roots, "Dune")?;
        let opened = open_catalogue(&roots)?;
        let rows = opened.catalogue.books()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, key);
        assert_eq!(rows[0].title, "Dune");
        Ok(())
    }

    #[test]
    fn a_second_damaged_catalogue_replaces_the_first_one_moved_aside() -> TestResult {
        let dir = tempfile::tempdir()?;
        let roots = roots(&dir);
        damage(&roots)?;
        fs::write(damaged_file(&roots), b"an older failure")?;
        open_catalogue(&roots)?;
        assert_eq!(fs::read(damaged_file(&roots))?, vec![0xAB; 8192]);
        Ok(())
    }

    #[test]
    fn a_catalogue_that_opens_is_not_reported_as_damaged_the_next_time() -> TestResult {
        let dir = tempfile::tempdir()?;
        let roots = roots(&dir);
        damage(&roots)?;
        assert!(open_catalogue(&roots)?.damaged);
        assert!(!open_catalogue(&roots)?.damaged);
        Ok(())
    }

    /// The version of the schema a database holds.
    fn schema_version(roots: &Roots) -> Result<Option<u32>, Box<dyn std::error::Error>> {
        use redb::{ReadableDatabase as _, ReadableTable as _};
        let db = redb::Database::create(roots.catalogue())?;
        let txn = db.begin_read()?;
        let table = txn.open_table(redb::TableDefinition::<&str, u32>::new("schema"))?;
        let version = table.get("version")?.map(|version| version.value());
        Ok(version)
    }

    #[test]
    fn a_catalogue_of_a_newer_schema_is_left_where_it_is() -> TestResult {
        let dir = tempfile::tempdir()?;
        let roots = roots(&dir);
        roots.prepare()?;
        {
            let db = redb::Database::create(roots.catalogue())?;
            let txn = db.begin_write()?;
            let table = redb::TableDefinition::<&str, u32>::new("schema");
            txn.open_table(table)?.insert("version", 99)?;
            txn.commit()?;
        }
        let Err(error) = open_catalogue(&roots) else {
            return Err("a newer schema was opened".into());
        };
        assert!(matches!(
            error,
            OpenError::Catalogue(CatalogueError::NewerSchema { found: 99, .. })
        ));
        assert_eq!(schema_version(&roots)?, Some(99), "the file is not changed");
        assert!(!damaged_file(&roots).exists());
        Ok(())
    }

    #[test]
    fn a_catalogue_that_another_copy_holds_open_is_left_where_it_is() -> TestResult {
        let dir = tempfile::tempdir()?;
        let roots = roots(&dir);
        roots.prepare()?;
        let held = Catalogue::open(&roots.catalogue())?;
        let Err(error) = open_catalogue(&roots) else {
            return Err("a catalogue that is held was opened".into());
        };
        assert!(matches!(
            error,
            OpenError::Catalogue(CatalogueError::Storage(_))
        ));
        assert!(roots.catalogue().is_file());
        assert!(!damaged_file(&roots).exists());
        assert_eq!(held.books()?, Vec::new());
        Ok(())
    }

    #[test]
    fn a_damaged_catalogue_that_cannot_be_moved_aside_is_reported_with_its_cause() -> TestResult {
        let dir = tempfile::tempdir()?;
        let roots = roots(&dir);
        damage(&roots)?;
        // A folder with something in it where the old file would be replaced.
        fs::create_dir_all(damaged_file(&roots).join("inside"))?;
        let Err(error) = open_catalogue(&roots) else {
            return Err("a damaged catalogue was opened".into());
        };
        assert!(matches!(error, OpenError::Damaged(_)));
        assert_eq!(
            error.to_string(),
            "the damaged catalogue could not be moved aside"
        );
        assert!(error.source().is_some());
        assert!(roots.catalogue().is_file(), "the file stays where it was");
        Ok(())
    }

    #[test]
    fn a_folder_in_the_place_of_the_catalogue_is_moved_aside_too() -> TestResult {
        let dir = tempfile::tempdir()?;
        let roots = roots(&dir);
        roots.prepare()?;
        // A folder where the file of the catalogue should be.
        fs::create_dir(roots.catalogue())?;
        let opened = open_catalogue(&roots)?;
        assert!(opened.damaged);
        assert!(damaged_file(&roots).is_dir());
        Ok(())
    }
}
