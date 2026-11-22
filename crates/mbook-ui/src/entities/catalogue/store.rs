//! The database: its tables, and every call that reads or writes them.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition, WriteTransaction};

use super::error::CatalogueError;
use super::rows::{BookRow, BookValue, ReadingState, StateValue};
use super::settings::Settings;
use crate::shared::model::BookKey;

/// The version of the schema this code writes. It grows when a table
/// changes in a way an older application cannot read.
const SCHEMA_VERSION: u32 = 1;

/// The one row of the `schema` table.
const SCHEMA_ROW: &str = "version";
/// The version of the schema.
const SCHEMA: TableDefinition<'_, &str, u32> = TableDefinition::new("schema");
/// The books of the library, by key. See [`BookRow`] for the encoding.
const BOOKS: TableDefinition<'_, u128, BookValue<'static>> = TableDefinition::new("books");
/// What was read of each book, by key. See [`ReadingState`].
const STATE: TableDefinition<'_, u128, StateValue<'static>> = TableDefinition::new("state");
/// The settings, by name, as text. See [`Settings`].
const SETTINGS: TableDefinition<'_, &str, &str> = TableDefinition::new("settings");

/// The extension of the file of a book.
const BOOK_EXTENSION: &str = "mbook";

/// What a rebuild found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rebuilt {
    /// The books now in the catalogue.
    pub(crate) added: usize,
    /// The files that were left out: not named by a key, or not a valid book.
    pub(crate) skipped: usize,
}

/// The library's database.
///
/// Every call is one transaction. The type is `Send + Sync`: reads run side
/// by side, writes wait for one another.
#[derive(Debug)]
pub(crate) struct Catalogue {
    db: Database,
}

impl Catalogue {
    /// Opens the database at `path`, creating it when it is missing.
    ///
    /// # Errors
    ///
    /// [`CatalogueError::NewerSchema`] when a newer application wrote the
    /// database, and [`CatalogueError::Storage`] when it cannot be opened or
    /// is damaged.
    pub(crate) fn open(path: &Path) -> Result<Self, CatalogueError> {
        let db = Database::create(path).map_err(CatalogueError::storage)?;
        let txn = db.begin_write().map_err(CatalogueError::storage)?;
        {
            let mut schema = txn.open_table(SCHEMA).map_err(CatalogueError::storage)?;
            let found = schema
                .get(SCHEMA_ROW)
                .map_err(CatalogueError::storage)?
                .map(|version| version.value());
            if let Some(found) = found.filter(|found| *found > SCHEMA_VERSION) {
                return Err(CatalogueError::NewerSchema {
                    found,
                    supported: SCHEMA_VERSION,
                });
            }
            if found != Some(SCHEMA_VERSION) {
                schema
                    .insert(SCHEMA_ROW, SCHEMA_VERSION)
                    .map_err(CatalogueError::storage)?;
            }
            txn.open_table(BOOKS).map_err(CatalogueError::storage)?;
            txn.open_table(STATE).map_err(CatalogueError::storage)?;
            txn.open_table(SETTINGS).map_err(CatalogueError::storage)?;
        }
        txn.commit().map_err(CatalogueError::storage)?;
        Ok(Self { db })
    }

    /// Every book, in the order of the keys, which is the order they were
    /// added in.
    ///
    /// # Errors
    ///
    /// [`CatalogueError::Storage`] when the database cannot be read.
    pub(crate) fn books(&self) -> Result<Vec<BookRow>, CatalogueError> {
        let txn = self.db.begin_read().map_err(CatalogueError::storage)?;
        let table = txn.open_table(BOOKS).map_err(CatalogueError::storage)?;
        let mut rows = Vec::new();
        for entry in table.iter().map_err(CatalogueError::storage)? {
            let (key, value) = entry.map_err(CatalogueError::storage)?;
            rows.push(BookRow::decode(key.value(), value.value()));
        }
        Ok(rows)
    }

    /// The row of one book; `None` when the library has no such book.
    ///
    /// # Errors
    ///
    /// [`CatalogueError::Storage`] when the database cannot be read.
    pub(crate) fn book(&self, key: BookKey) -> Result<Option<BookRow>, CatalogueError> {
        let txn = self.db.begin_read().map_err(CatalogueError::storage)?;
        let table = txn.open_table(BOOKS).map_err(CatalogueError::storage)?;
        let row = table.get(key.as_u128()).map_err(CatalogueError::storage)?;
        Ok(row.map(|value| BookRow::decode(key.as_u128(), value.value())))
    }

    /// The key of the book whose metadata has this `id`; the first one when
    /// two books share it.
    ///
    /// # Errors
    ///
    /// [`CatalogueError::Storage`] when the database cannot be read.
    pub(crate) fn find_by_id(&self, id: &str) -> Result<Option<BookKey>, CatalogueError> {
        let txn = self.db.begin_read().map_err(CatalogueError::storage)?;
        let table = txn.open_table(BOOKS).map_err(CatalogueError::storage)?;
        for entry in table.iter().map_err(CatalogueError::storage)? {
            let (key, value) = entry.map_err(CatalogueError::storage)?;
            if value.value().0 == id {
                return Ok(Some(BookKey::from_u128(key.value())));
            }
        }
        Ok(None)
    }

    /// Adds a book, or replaces the row of the book with the same key.
    ///
    /// # Errors
    ///
    /// [`CatalogueError::Storage`] when the database cannot be written.
    pub(crate) fn put_book(&self, row: &BookRow) -> Result<(), CatalogueError> {
        self.write(|txn| {
            let mut table = txn.open_table(BOOKS).map_err(CatalogueError::storage)?;
            insert_book(&mut table, row)
        })
    }

    /// Removes a book and its reading state. A key that is not in the
    /// library is not an error.
    ///
    /// # Errors
    ///
    /// [`CatalogueError::Storage`] when the database cannot be written.
    pub(crate) fn remove_book(&self, key: BookKey) -> Result<(), CatalogueError> {
        self.write(|txn| {
            let mut books = txn.open_table(BOOKS).map_err(CatalogueError::storage)?;
            books
                .remove(key.as_u128())
                .map_err(CatalogueError::storage)?;
            let mut state = txn.open_table(STATE).map_err(CatalogueError::storage)?;
            state
                .remove(key.as_u128())
                .map_err(CatalogueError::storage)?;
            Ok(())
        })
    }

    /// What was read of a book; the default state when nothing is stored.
    ///
    /// # Errors
    ///
    /// [`CatalogueError::Storage`] when the database cannot be read.
    pub(crate) fn state(&self, key: BookKey) -> Result<ReadingState, CatalogueError> {
        let txn = self.db.begin_read().map_err(CatalogueError::storage)?;
        let table = txn.open_table(STATE).map_err(CatalogueError::storage)?;
        let state = table.get(key.as_u128()).map_err(CatalogueError::storage)?;
        Ok(state.map_or_else(ReadingState::default, |value| {
            ReadingState::decode(value.value())
        }))
    }

    /// Stores what was read of a book, replacing what was stored.
    ///
    /// # Errors
    ///
    /// [`CatalogueError::Storage`] when the database cannot be written.
    pub(crate) fn put_state(
        &self,
        key: BookKey,
        state: &ReadingState,
    ) -> Result<(), CatalogueError> {
        self.write(|txn| {
            let mut table = txn.open_table(STATE).map_err(CatalogueError::storage)?;
            let place = state
                .place
                .as_ref()
                .map(|place| (place.chapter.as_str(), place.offset));
            table
                .insert(
                    key.as_u128(),
                    (
                        place,
                        state.progress_permille,
                        state.opened_ms,
                        state.finished,
                    ),
                )
                .map_err(CatalogueError::storage)?;
            Ok(())
        })
    }

    /// The settings. A setting that is missing, or stored with a value that
    /// makes no sense, has its default, or is brought into its range; this
    /// does not fail for a strange value.
    ///
    /// # Errors
    ///
    /// [`CatalogueError::Storage`] when the database cannot be read.
    pub(crate) fn settings(&self) -> Result<Settings, CatalogueError> {
        let txn = self.db.begin_read().map_err(CatalogueError::storage)?;
        let table = txn.open_table(SETTINGS).map_err(CatalogueError::storage)?;
        let mut rows = HashMap::new();
        for entry in table.iter().map_err(CatalogueError::storage)? {
            let (name, value) = entry.map_err(CatalogueError::storage)?;
            rows.insert(name.value().to_owned(), value.value().to_owned());
        }
        Ok(Settings::decode(&rows))
    }

    /// Stores the settings, replacing every one stored.
    ///
    /// # Errors
    ///
    /// [`CatalogueError::Storage`] when the database cannot be written.
    pub(crate) fn put_settings(&self, settings: &Settings) -> Result<(), CatalogueError> {
        self.write(|txn| {
            txn.delete_table(SETTINGS)
                .map_err(CatalogueError::storage)?;
            let mut table = txn.open_table(SETTINGS).map_err(CatalogueError::storage)?;
            for (name, value) in settings.encode() {
                table
                    .insert(name, value.as_str())
                    .map_err(CatalogueError::storage)?;
            }
            Ok(())
        })
    }

    /// Rebuilds `books` from the files `<key>.mbook` of a folder, opening
    /// each with [`mbook::BookReader::open`], which reads the tables only.
    ///
    /// A file that is not a valid book, or whose name is not a key, is
    /// skipped and counted; a sub-folder is passed over without being
    /// counted. A book that was already in the catalogue keeps its time of
    /// adding and the format it came from; a new one is dated by its file,
    /// and has no source format. Rows of books whose file is gone are
    /// dropped. Reading states are not touched.
    ///
    /// # Errors
    ///
    /// [`CatalogueError::Folder`] when the folder cannot be listed, and
    /// [`CatalogueError::Storage`] when the database cannot be written.
    pub(crate) fn rebuild(&self, books_dir: &Path) -> Result<Rebuilt, CatalogueError> {
        let (found, skipped) = scan(books_dir)?;
        let added = found.len();
        self.write(|txn| {
            let mut previous = HashMap::new();
            {
                let old = txn.open_table(BOOKS).map_err(CatalogueError::storage)?;
                for entry in old.iter().map_err(CatalogueError::storage)? {
                    let (key, value) = entry.map_err(CatalogueError::storage)?;
                    let row = BookRow::decode(key.value(), value.value());
                    previous.insert(key.value(), (row.added_ms, row.source_format));
                }
            }
            txn.delete_table(BOOKS).map_err(CatalogueError::storage)?;
            let mut table = txn.open_table(BOOKS).map_err(CatalogueError::storage)?;
            for mut row in found {
                if let Some((added_ms, source_format)) = previous.remove(&row.key.as_u128()) {
                    row.added_ms = added_ms;
                    row.source_format = source_format;
                }
                insert_book(&mut table, &row)?;
            }
            Ok(())
        })?;
        Ok(Rebuilt { added, skipped })
    }

    /// Runs `body` in a write transaction and commits it; an error rolls the
    /// transaction back.
    fn write<T>(
        &self,
        body: impl FnOnce(&WriteTransaction) -> Result<T, CatalogueError>,
    ) -> Result<T, CatalogueError> {
        let txn = self.db.begin_write().map_err(CatalogueError::storage)?;
        let value = body(&txn)?;
        txn.commit().map_err(CatalogueError::storage)?;
        Ok(value)
    }
}

/// Stores one row of `books`.
fn insert_book(
    table: &mut redb::Table<'_, u128, BookValue<'static>>,
    row: &BookRow,
) -> Result<(), CatalogueError> {
    let authors = row.joined_authors();
    table
        .insert(
            row.key.as_u128(),
            (
                row.id.as_str(),
                row.title.as_str(),
                authors.as_str(),
                row.language.as_str(),
                row.series.as_deref(),
                row.file_len,
                row.cover,
                row.added_ms,
                row.source_format.as_deref(),
            ),
        )
        .map_err(CatalogueError::storage)?;
    Ok(())
}

/// The valid books of a folder, and how many files were left out.
fn scan(books_dir: &Path) -> Result<(Vec<BookRow>, usize), CatalogueError> {
    let folder_error = |source| CatalogueError::Folder {
        path: books_dir.to_owned(),
        source,
    };
    let mut found = Vec::new();
    let mut skipped = 0;
    for entry in fs::read_dir(books_dir).map_err(folder_error)? {
        let path = entry.map_err(folder_error)?.path();
        if !path.is_file() {
            continue;
        }
        let key = (path.extension().is_some_and(|ext| ext == BOOK_EXTENSION))
            .then(|| path.file_stem().and_then(|stem| stem.to_str()))
            .flatten()
            .and_then(|stem| stem.parse::<BookKey>().ok());
        let row = key.and_then(|key| row_of_file(key, &path));
        match row {
            Some(row) => found.push(row),
            None => skipped += 1,
        }
    }
    Ok((found, skipped))
}

/// The row of the book in a file; `None` when the file is not a valid book.
fn row_of_file(key: BookKey, path: &Path) -> Option<BookRow> {
    let reader = mbook::BookReader::open(path).ok()?;
    let metadata = fs::metadata(path).ok()?;
    let added_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0);
    Some(BookRow::from_reader(
        key,
        &reader,
        metadata.len(),
        added_ms,
        None,
    ))
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use tempfile::TempDir;

    use super::*;
    use crate::shared::model::Place;
    use crate::shared::theme::PageTone;

    const COVER: &[u8] = include_bytes!("../../../../mbook/tests/fixtures/plain.avif");

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn catalogue(dir: &TempDir) -> Result<Catalogue, CatalogueError> {
        Catalogue::open(&dir.path().join("catalog.redb"))
    }

    fn row(id: &str) -> BookRow {
        BookRow {
            key: BookKey::generate(),
            id: id.to_owned(),
            title: format!("Title of {id}"),
            authors: vec!["Ann".to_owned(), "Bo".to_owned()],
            language: "en".to_owned(),
            series: Some("Saga #2".to_owned()),
            file_len: 4096,
            cover: Some((600, 900)),
            added_ms: 1_700_000_000_000,
            source_format: Some("epub".to_owned()),
        }
    }

    fn save_book(dir: &Path, key: BookKey, title: &str) -> Result<(), mbook::Error> {
        let mut book = mbook::Book::new(title, "en")?;
        book.add_chapter("ch01.md", "# One\n\nText.\n")?;
        book.save(dir.join(format!("{key}.mbook")))
    }

    #[test]
    fn the_catalogue_is_send_and_sync() {
        fn check<T: Send + Sync>() {}
        check::<Catalogue>();
    }

    #[test]
    fn a_new_catalogue_is_empty_and_has_default_settings() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        assert_eq!(catalogue.books()?, Vec::new());
        assert_eq!(catalogue.settings()?, Settings::default());
        Ok(())
    }

    #[test]
    fn a_book_row_survives_a_round_trip() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        let stored = row("urn:a");
        catalogue.put_book(&stored)?;
        assert_eq!(catalogue.book(stored.key)?, Some(stored.clone()));
        assert_eq!(catalogue.books()?, vec![stored]);
        Ok(())
    }

    #[test]
    fn a_row_without_optional_fields_survives_a_round_trip() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        let stored = BookRow {
            authors: Vec::new(),
            series: None,
            cover: None,
            source_format: None,
            ..row("urn:bare")
        };
        catalogue.put_book(&stored)?;
        assert_eq!(catalogue.book(stored.key)?, Some(stored));
        Ok(())
    }

    #[test]
    fn a_book_that_is_not_stored_is_none() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        assert_eq!(catalogue.book(BookKey::generate())?, None);
        Ok(())
    }

    #[test]
    fn putting_a_book_again_replaces_its_row() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        let mut stored = row("urn:a");
        catalogue.put_book(&stored)?;
        stored.title = "Renamed".to_owned();
        catalogue.put_book(&stored)?;
        assert_eq!(catalogue.books()?, vec![stored]);
        Ok(())
    }

    #[test]
    fn books_come_in_the_order_they_were_added() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        let first = row("urn:1");
        let second = row("urn:2");
        catalogue.put_book(&second)?;
        catalogue.put_book(&first)?;
        let keys: Vec<BookKey> = catalogue.books()?.iter().map(|book| book.key).collect();
        assert_eq!(keys, vec![first.key, second.key]);
        Ok(())
    }

    #[test]
    fn a_book_is_found_by_the_id_of_its_metadata() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        let stored = row("urn:a");
        catalogue.put_book(&stored)?;
        catalogue.put_book(&row("urn:b"))?;
        assert_eq!(catalogue.find_by_id("urn:a")?, Some(stored.key));
        Ok(())
    }

    #[test]
    fn an_unknown_id_finds_nothing() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        catalogue.put_book(&row("urn:a"))?;
        assert_eq!(catalogue.find_by_id("urn:zzz")?, None);
        Ok(())
    }

    #[test]
    fn a_state_survives_a_round_trip() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        let key = BookKey::generate();
        let state = ReadingState {
            place: Some(Place {
                chapter: "ch02.md".to_owned(),
                offset: 1234,
            }),
            progress_permille: 380,
            opened_ms: Some(1_700_000_000_123),
            finished: true,
        };
        catalogue.put_state(key, &state)?;
        assert_eq!(catalogue.state(key)?, state);
        Ok(())
    }

    #[test]
    fn a_book_without_a_state_has_the_default_one() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        assert_eq!(
            catalogue.state(BookKey::generate())?,
            ReadingState::default()
        );
        Ok(())
    }

    #[test]
    fn a_state_without_a_place_survives_a_round_trip() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        let key = BookKey::generate();
        let state = ReadingState {
            progress_permille: 1000,
            ..ReadingState::default()
        };
        catalogue.put_state(key, &state)?;
        assert_eq!(catalogue.state(key)?, state);
        Ok(())
    }

    #[test]
    fn removing_a_book_removes_its_state() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        let stored = row("urn:a");
        let kept = row("urn:b");
        catalogue.put_book(&stored)?;
        catalogue.put_book(&kept)?;
        let state = ReadingState {
            progress_permille: 500,
            ..ReadingState::default()
        };
        catalogue.put_state(stored.key, &state)?;
        catalogue.put_state(kept.key, &state)?;
        catalogue.remove_book(stored.key)?;
        assert_eq!(catalogue.book(stored.key)?, None);
        assert_eq!(catalogue.state(stored.key)?, ReadingState::default());
        assert_eq!(catalogue.state(kept.key)?, state);
        assert_eq!(catalogue.books()?, vec![kept]);
        Ok(())
    }

    #[test]
    fn removing_a_book_that_is_not_stored_is_not_an_error() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        catalogue.remove_book(BookKey::generate())?;
        Ok(())
    }

    #[test]
    fn settings_survive_a_round_trip() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        let settings = Settings {
            typeface: "Inter".to_owned(),
            text_size: 22.0,
            line_height: 1.4,
            page_tone: Some(PageTone::Night),
            interface_scale: 1.5,
            keep_controls: true,
            reduce_motion: true,
            check_updates: false,
            reading: super::super::settings::ReadingOptions {
                size_set: true,
                ..Default::default()
            },
        };
        catalogue.put_settings(&settings)?;
        assert_eq!(catalogue.settings()?, settings);
        Ok(())
    }

    #[test]
    fn putting_settings_again_forgets_a_page_tone_that_was_set() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        let tone = Settings {
            page_tone: Some(PageTone::Sepia),
            ..Settings::default()
        };
        catalogue.put_settings(&tone)?;
        catalogue.put_settings(&Settings::default())?;
        assert_eq!(catalogue.settings()?.page_tone, None);
        Ok(())
    }

    #[test]
    fn settings_out_of_range_are_brought_into_range_when_read() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        catalogue.put_settings(&Settings {
            text_size: 500.0,
            line_height: 0.1,
            interface_scale: f32::NAN,
            ..Settings::default()
        })?;
        let settings = catalogue.settings()?;
        assert!((settings.text_size - 80.0).abs() < f32::EPSILON);
        assert!((settings.line_height - 1.3).abs() < f32::EPSILON);
        assert!((settings.interface_scale - 1.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn a_stored_page_tone_with_an_unknown_name_follows_the_system() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        let txn = catalogue.db.begin_write()?;
        txn.open_table(SETTINGS)?.insert("page_tone", "neon")?;
        txn.commit()?;
        assert_eq!(catalogue.settings()?.page_tone, None);
        Ok(())
    }

    #[test]
    fn a_reopened_catalogue_keeps_its_data() -> TestResult {
        let dir = tempfile::tempdir()?;
        let stored = row("urn:a");
        let state = ReadingState {
            progress_permille: 42,
            ..ReadingState::default()
        };
        let settings = Settings {
            check_updates: false,
            ..Settings::default()
        };
        {
            let catalogue = catalogue(&dir)?;
            catalogue.put_book(&stored)?;
            catalogue.put_state(stored.key, &state)?;
            catalogue.put_settings(&settings)?;
        }
        let catalogue = catalogue(&dir)?;
        assert_eq!(catalogue.books()?, vec![stored.clone()]);
        assert_eq!(catalogue.state(stored.key)?, state);
        assert_eq!(catalogue.settings()?, settings);
        Ok(())
    }

    #[test]
    fn a_newer_schema_is_refused_with_both_versions() -> TestResult {
        let dir = tempfile::tempdir()?;
        {
            let catalogue = catalogue(&dir)?;
            let txn = catalogue.db.begin_write()?;
            txn.open_table(SCHEMA)?
                .insert(SCHEMA_ROW, SCHEMA_VERSION + 1)?;
            txn.commit()?;
        }
        let error = catalogue(&dir).unwrap_err();
        assert!(matches!(
            error,
            CatalogueError::NewerSchema {
                found: 2,
                supported: 1
            }
        ));
        assert_eq!(
            error.to_string(),
            "the catalogue has schema version 2, and this application knows versions up to 1"
        );
        Ok(())
    }

    #[test]
    fn a_refused_newer_schema_leaves_the_database_as_it_was() -> TestResult {
        let dir = tempfile::tempdir()?;
        {
            let catalogue = catalogue(&dir)?;
            let txn = catalogue.db.begin_write()?;
            txn.open_table(SCHEMA)?.insert(SCHEMA_ROW, 7)?;
            txn.commit()?;
        }
        assert!(catalogue(&dir).is_err());
        let db = Database::create(dir.path().join("catalog.redb"))?;
        let txn = db.begin_read()?;
        let version = txn.open_table(SCHEMA)?.get(SCHEMA_ROW)?.map(|v| v.value());
        assert_eq!(version, Some(7));
        Ok(())
    }

    #[test]
    fn a_file_that_is_no_database_is_a_storage_error_with_its_source() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog.redb");
        fs::write(&path, vec![0xAB; 8192])?;
        let error = Catalogue::open(&path).unwrap_err();
        assert!(matches!(error, CatalogueError::Storage(_)));
        assert_eq!(error.to_string(), "the catalogue database failed");
        assert!(error.source().is_some());
        Ok(())
    }

    #[test]
    fn a_catalogue_in_a_missing_folder_is_a_storage_error() {
        let path = Path::new("/nonexistent-mbook-folder/catalog.redb");
        let error = Catalogue::open(path).unwrap_err();
        assert!(matches!(error, CatalogueError::Storage(_)));
        assert!(error.source().is_some());
    }

    #[test]
    fn rebuilding_reads_a_valid_book() -> TestResult {
        let dir = tempfile::tempdir()?;
        let books = dir.path().join("books");
        fs::create_dir(&books)?;
        let key = BookKey::generate();
        save_book(&books, key, "Rebuilt")?;
        let catalogue = catalogue(&dir)?;
        let rebuilt = catalogue.rebuild(&books)?;
        assert_eq!(
            rebuilt,
            Rebuilt {
                added: 1,
                skipped: 0
            }
        );
        let row = catalogue.book(key)?.ok_or("the book is missing")?;
        assert_eq!(row.title, "Rebuilt");
        assert_eq!(row.language, "en");
        assert!(row.id.starts_with("urn:uuid:"));
        assert_eq!(
            row.file_len,
            fs::metadata(books.join(format!("{key}.mbook")))?.len()
        );
        assert_eq!(row.cover, None);
        assert_eq!(row.source_format, None);
        assert_eq!(catalogue.find_by_id(&row.id)?, Some(key));
        Ok(())
    }

    #[test]
    fn rebuilding_counts_a_file_that_is_not_a_book() -> TestResult {
        let dir = tempfile::tempdir()?;
        let books = dir.path().join("books");
        fs::create_dir(&books)?;
        let key = BookKey::generate();
        fs::write(books.join(format!("{key}.mbook")), b"not a book")?;
        let catalogue = catalogue(&dir)?;
        let rebuilt = catalogue.rebuild(&books)?;
        assert_eq!(
            rebuilt,
            Rebuilt {
                added: 0,
                skipped: 1
            }
        );
        assert_eq!(catalogue.books()?, Vec::new());
        Ok(())
    }

    #[test]
    fn rebuilding_counts_a_file_whose_name_is_no_key() -> TestResult {
        let dir = tempfile::tempdir()?;
        let books = dir.path().join("books");
        fs::create_dir(&books)?;
        let key = BookKey::generate();
        save_book(&books, key, "Misnamed")?;
        fs::rename(
            books.join(format!("{key}.mbook")),
            books.join("my-novel.mbook"),
        )?;
        let catalogue = catalogue(&dir)?;
        let rebuilt = catalogue.rebuild(&books)?;
        assert_eq!(
            rebuilt,
            Rebuilt {
                added: 0,
                skipped: 1
            }
        );
        Ok(())
    }

    #[test]
    fn rebuilding_counts_a_book_with_another_extension() -> TestResult {
        let dir = tempfile::tempdir()?;
        let books = dir.path().join("books");
        fs::create_dir(&books)?;
        let key = BookKey::generate();
        save_book(&books, key, "Wrong extension")?;
        fs::rename(
            books.join(format!("{key}.mbook")),
            books.join(format!("{key}.epub")),
        )?;
        let rebuilt = catalogue(&dir)?.rebuild(&books)?;
        assert_eq!(rebuilt.skipped, 1);
        Ok(())
    }

    #[test]
    fn rebuilding_passes_over_a_folder_without_counting_it() -> TestResult {
        let dir = tempfile::tempdir()?;
        let books = dir.path().join("books");
        fs::create_dir_all(books.join(".incoming"))?;
        let rebuilt = catalogue(&dir)?.rebuild(&books)?;
        assert_eq!(
            rebuilt,
            Rebuilt {
                added: 0,
                skipped: 0
            }
        );
        Ok(())
    }

    #[test]
    fn rebuilding_an_empty_folder_empties_the_books() -> TestResult {
        let dir = tempfile::tempdir()?;
        let books = dir.path().join("books");
        fs::create_dir(&books)?;
        let catalogue = catalogue(&dir)?;
        catalogue.put_book(&row("urn:gone"))?;
        catalogue.rebuild(&books)?;
        assert_eq!(catalogue.books()?, Vec::new());
        Ok(())
    }

    #[test]
    fn rebuilding_keeps_the_time_and_format_of_a_known_book() -> TestResult {
        let dir = tempfile::tempdir()?;
        let books = dir.path().join("books");
        fs::create_dir(&books)?;
        let key = BookKey::generate();
        save_book(&books, key, "Known")?;
        let catalogue = catalogue(&dir)?;
        let known = BookRow {
            key,
            ..row("urn:old")
        };
        catalogue.put_book(&known)?;
        catalogue.rebuild(&books)?;
        let row = catalogue.book(key)?.ok_or("the book is missing")?;
        assert_eq!(row.added_ms, known.added_ms);
        assert_eq!(row.source_format, known.source_format);
        assert_eq!(row.title, "Known");
        Ok(())
    }

    #[test]
    fn rebuilding_leaves_reading_states_alone() -> TestResult {
        let dir = tempfile::tempdir()?;
        let books = dir.path().join("books");
        fs::create_dir(&books)?;
        let key = BookKey::generate();
        save_book(&books, key, "Read")?;
        let catalogue = catalogue(&dir)?;
        let state = ReadingState {
            progress_permille: 700,
            ..ReadingState::default()
        };
        catalogue.put_state(key, &state)?;
        catalogue.rebuild(&books)?;
        assert_eq!(catalogue.state(key)?, state);
        Ok(())
    }

    #[test]
    fn rebuilding_a_missing_folder_names_it() -> TestResult {
        let dir = tempfile::tempdir()?;
        let catalogue = catalogue(&dir)?;
        let missing = dir.path().join("nowhere");
        let error = catalogue.rebuild(&missing).unwrap_err();
        assert!(matches!(error, CatalogueError::Folder { .. }));
        assert!(error.to_string().contains("nowhere"));
        assert!(error.source().is_some());
        Ok(())
    }

    #[test]
    fn a_book_row_takes_series_and_cover_from_a_reader() -> TestResult {
        let mut book = mbook::Book::new("Sagas", "en")?;
        book.meta_mut().add_author("Ann")?;
        book.meta_mut().add_author("Bo")?;
        book.meta_mut().set_series("Saga", Some("2"))?;
        book.set_cover("img/cover.avif", COVER)?;
        book.add_chapter("ch01.md", "# One\n")?;
        let mut bytes = Vec::new();
        book.write(&mut bytes)?;
        let reader = mbook::BookReader::buffered(bytes.as_slice())?;
        let key = BookKey::generate();
        let row = BookRow::from_reader(key, &reader, 99, 5, Some("fb2".to_owned()));
        assert_eq!(row.key, key);
        assert_eq!(row.title, "Sagas");
        assert_eq!(row.authors, vec!["Ann", "Bo"]);
        assert_eq!(row.series.as_deref(), Some("Saga #2"));
        assert_eq!(row.file_len, 99);
        assert_eq!(row.added_ms, 5);
        assert_eq!(row.source_format.as_deref(), Some("fb2"));
        let cover = reader.cover().ok_or("no cover")?;
        assert_eq!(row.cover, Some((cover.size.width, cover.size.height)));
        assert!(row.cover.is_some());
        Ok(())
    }

    #[test]
    fn a_series_without_an_index_is_shown_by_its_name() -> TestResult {
        let mut book = mbook::Book::new("Sagas", "en")?;
        book.meta_mut().set_series("Saga", None)?;
        book.add_chapter("ch01.md", "# One\n")?;
        let mut bytes = Vec::new();
        book.write(&mut bytes)?;
        let reader = mbook::BookReader::buffered(bytes.as_slice())?;
        let row = BookRow::from_reader(BookKey::generate(), &reader, 1, 1, None);
        assert_eq!(row.series.as_deref(), Some("Saga"));
        assert_eq!(row.cover, None);
        Ok(())
    }
}
