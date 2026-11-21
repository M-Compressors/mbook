//! The directories the application keeps its files in.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::model::BookKey;

/// The two directories of the application.
///
/// The *data* root holds what must be kept: the books and the catalogue. The
/// *cache* root holds what can be made again from the data root.
///
/// # Examples
///
/// ```
/// use mbook_ui::Roots;
///
/// let roots = Roots::new("/data/mbook", "/cache/mbook");
/// assert_eq!(roots.data(), std::path::Path::new("/data/mbook"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roots {
    data: PathBuf,
    cache: PathBuf,
}

impl Roots {
    /// Names the data root and the cache root. Nothing is created.
    pub fn new(data: impl Into<PathBuf>, cache: impl Into<PathBuf>) -> Self {
        Self {
            data: data.into(),
            cache: cache.into(),
        }
    }

    /// The root of what must be kept.
    #[must_use]
    pub fn data(&self) -> &Path {
        &self.data
    }

    /// The root of what can be made again.
    #[must_use]
    pub fn cache(&self) -> &Path {
        &self.cache
    }

    /// The file of the catalogue.
    pub(crate) fn catalogue(&self) -> PathBuf {
        self.data.join("catalog.redb")
    }

    /// The folder of the books.
    pub(crate) fn books(&self) -> PathBuf {
        self.data.join("books")
    }

    /// The folder of imports that have not finished. It is on the volume of
    /// [`Self::books`], so that a finished file enters by a rename.
    pub(crate) fn incoming(&self) -> PathBuf {
        self.books().join(".incoming")
    }

    /// The file of one book.
    pub(crate) fn book(&self, key: BookKey) -> PathBuf {
        self.books().join(format!("{key}.mbook"))
    }

    /// The folder of the thumbnails of covers.
    pub(crate) fn covers(&self) -> PathBuf {
        self.cache.join("covers")
    }

    /// The thumbnail of the cover of one book.
    pub(crate) fn cover(&self, key: BookKey) -> PathBuf {
        self.covers().join(format!("{key}.jpg"))
    }

    /// The folder a downloaded update is kept in.
    pub(crate) fn update(&self) -> PathBuf {
        self.cache.join("update")
    }

    /// Creates the folders and removes what unfinished imports left behind.
    ///
    /// Touches the file system: call it on the background executor.
    ///
    /// # Errors
    ///
    /// When a folder cannot be created or emptied.
    pub(crate) fn prepare(&self) -> io::Result<()> {
        let incoming = self.incoming();
        match fs::remove_dir_all(&incoming) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::create_dir_all(&incoming)?;
        fs::create_dir_all(self.covers())?;
        fs::create_dir_all(self.update())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_keep_the_directories_they_were_given() {
        let roots = Roots::new("/data", "/cache");
        assert_eq!(roots.data(), Path::new("/data"));
        assert_eq!(roots.cache(), Path::new("/cache"));
    }

    #[test]
    fn kept_files_are_under_the_data_root() {
        let roots = Roots::new("/data", "/cache");
        let key = BookKey::generate();
        assert_eq!(roots.catalogue(), Path::new("/data/catalog.redb"));
        assert_eq!(roots.books(), Path::new("/data/books"));
        assert_eq!(roots.incoming(), Path::new("/data/books/.incoming"));
        assert_eq!(
            roots.book(key),
            Path::new("/data/books").join(format!("{key}.mbook"))
        );
    }

    #[test]
    fn files_that_can_be_made_again_are_under_the_cache_root() {
        let roots = Roots::new("/data", "/cache");
        let key = BookKey::generate();
        assert_eq!(roots.covers(), Path::new("/cache/covers"));
        assert_eq!(
            roots.cover(key),
            Path::new("/cache/covers").join(format!("{key}.jpg"))
        );
        assert_eq!(roots.update(), Path::new("/cache/update"));
    }

    #[test]
    fn prepare_creates_the_folders() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let roots = Roots::new(dir.path().join("data"), dir.path().join("cache"));
        roots.prepare()?;
        assert!(roots.incoming().is_dir());
        assert!(roots.covers().is_dir());
        assert!(roots.update().is_dir());
        Ok(())
    }

    #[test]
    fn prepare_removes_unfinished_imports_and_keeps_books() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let roots = Roots::new(dir.path().join("data"), dir.path().join("cache"));
        roots.prepare()?;
        let key = BookKey::generate();
        fs::write(roots.incoming().join("half"), b"half a book")?;
        fs::write(roots.book(key), b"a book")?;
        roots.prepare()?;
        assert!(!roots.incoming().join("half").exists());
        assert!(roots.book(key).is_file());
        Ok(())
    }

    #[test]
    fn prepare_fails_when_a_root_cannot_be_a_folder() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("file");
        fs::write(&file, b"")?;
        let roots = Roots::new(file.join("data"), dir.path().join("cache"));
        assert!(roots.prepare().is_err());
        Ok(())
    }
}
