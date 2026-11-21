//! Lazy reading of an mbook file.

use crate::book::Book;
use crate::error::{ResolveError, Result};
use crate::id::{ChapterId, PictureId};
use crate::meta::Meta;
use crate::options::ReadOptions;
use crate::types::{
    Anchor, ChapterEntry, Destination, PictureEntry, Resource, ResourceKind, Size, TocEntry,
    Version,
};
use std::fs::File;
use std::io::{Cursor, Read, Seek};
use std::path::Path;

/// Reads a book lazily from anything that can seek.
///
/// Opening a reader reads the directory of the file and its five small
/// tables: enough for the title, the cover and the contents. Each chapter and
/// each picture is read and decoded when it is asked for.
///
/// Methods that only look into the tables take `&self` and do no I/O. Methods
/// that touch the source start with `read_` and take `&mut self`.
///
/// A `BookReader` is `Send` when its source is. To decode on several threads,
/// open one reader per thread.
///
/// # Examples
///
/// ```no_run
/// use mbook_core::BookReader;
///
/// let mut book = BookReader::open("example.mbook")?;
/// println!("{}", book.meta().title());
/// for entry in book.toc() {
///     println!("{}{}", "  ".repeat(usize::from(entry.level) - 1), entry.title);
/// }
///
/// let first = book.chapters().next().map(|chapter| chapter.id);
/// if let Some(id) = first {
///     let text = book.read_chapter(id)?;
///     println!("{text}");
/// }
/// # Ok::<(), mbook_core::Error>(())
/// ```
#[derive(Debug)]
pub struct BookReader<R> {
    source: R,
    version: Version,
    meta: Meta,
    toc: Vec<TocEntry>,
    /// The resource table; the index is the resource number.
    rows: Vec<Row>,
    /// Numbers of text resources, in reading order.
    chapters: Vec<u32>,
    /// Numbers of picture resources, in file order.
    pictures: Vec<u32>,
    /// Numbers of text and picture resources, sorted by path.
    by_path: Vec<u32>,
}

/// One row of the resource table.
#[derive(Debug)]
struct Row {
    kind: ResourceKind,
    path: String,
    size: Size,
    decoded_len: u64,
}

impl BookReader<File> {
    /// Opens the book stored in a file.
    ///
    /// # Errors
    ///
    /// The file cannot be opened, or as [`BookReader::new`].
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::new(File::open(path)?)
    }
}

impl BookReader<Cursor<Vec<u8>>> {
    /// Reads a whole stream into memory and opens the book in that buffer.
    ///
    /// Use it for a source that cannot seek. The buffer holds the compressed
    /// file; chapters and pictures are still decoded on demand. Nothing can
    /// be shown before the stream ends, because the directory is at the end
    /// of a file.
    ///
    /// # Errors
    ///
    /// The stream fails, or as [`BookReader::new`].
    pub fn buffered(mut reader: impl Read) -> Result<Self> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Self::new(Cursor::new(bytes))
    }
}

impl<R: Read + Seek> BookReader<R> {
    /// Opens the book that runs from the current position of `source` to its
    /// end, with the default options.
    ///
    /// # Errors
    ///
    /// The source fails; the data is not an mbook file, declares a newer
    /// major version, or has a damaged or malformed table; a default limit is
    /// exceeded.
    pub fn new(source: R) -> Result<Self> {
        Self::with_options(source, &ReadOptions::default())
    }

    /// Opens a book with the given options.
    ///
    /// # Errors
    ///
    /// As [`BookReader::new`], with the limits of `options`.
    pub fn with_options(_source: R, _options: &ReadOptions) -> Result<Self> {
        todo!("load the container index and decode resources 0 to 4")
    }

    /// The format version the file declares.
    #[must_use]
    pub fn version(&self) -> Version {
        self.version
    }

    /// The metadata.
    #[must_use]
    pub fn meta(&self) -> &Meta {
        &self.meta
    }

    /// The table of contents: a tree listed in pre-order.
    #[must_use]
    pub fn toc(&self) -> &[TocEntry] {
        &self.toc
    }

    /// The chapters, in reading order.
    pub fn chapters(&self) -> impl ExactSizeIterator<Item = ChapterEntry<'_>> {
        self.chapters
            .iter()
            .map(|&number| self.chapter_entry(number))
    }

    /// The pictures, in file order.
    pub fn pictures(&self) -> impl ExactSizeIterator<Item = PictureEntry<'_>> {
        self.pictures
            .iter()
            .map(|&number| self.picture_entry(number))
    }

    /// What the tables say about a chapter. `None` for an id that came from
    /// another book.
    #[must_use]
    pub fn chapter(&self, id: ChapterId) -> Option<ChapterEntry<'_>> {
        self.chapter_id(id.number())
            .map(|id| self.chapter_entry(id.number()))
    }

    /// What the tables say about a picture. `None` for an id that came from
    /// another book.
    #[must_use]
    pub fn picture(&self, id: PictureId) -> Option<PictureEntry<'_>> {
        self.picture_id(id.number())
            .map(|id| self.picture_entry(id.number()))
    }

    /// The chapter with this path.
    #[must_use]
    pub fn find_chapter(&self, path: &str) -> Option<ChapterEntry<'_>> {
        self.find(path)
            .and_then(|number| self.chapter_id(number))
            .map(|id| self.chapter_entry(id.number()))
    }

    /// The picture with this path.
    #[must_use]
    pub fn find_picture(&self, path: &str) -> Option<PictureEntry<'_>> {
        self.find(path)
            .and_then(|number| self.picture_id(number))
            .map(|id| self.picture_entry(id.number()))
    }

    /// The cover named by the metadata, when it is a picture of the book.
    #[must_use]
    pub fn cover(&self) -> Option<PictureEntry<'_>> {
        self.meta.cover().and_then(|path| self.find_picture(path))
    }

    /// The id of the chapter with this resource number, when that resource
    /// is a chapter. Use it to restore an id stored outside the program.
    #[must_use]
    pub fn chapter_id(&self, number: u32) -> Option<ChapterId> {
        (self.kind_of(number)? == ResourceKind::Text).then(|| ChapterId::new(number))
    }

    /// The id of the picture with this resource number, when that resource
    /// is a picture.
    #[must_use]
    pub fn picture_id(&self, number: u32) -> Option<PictureId> {
        (self.kind_of(number)? == ResourceKind::Picture).then(|| PictureId::new(number))
    }

    /// The anchors of a chapter, sorted by offset. Empty for an id that came
    /// from another book.
    #[must_use]
    pub fn anchors(&self, _chapter: ChapterId) -> &[Anchor] {
        todo!("keep the anchor table grouped by chapter")
    }

    /// Tells what a link or picture destination found in a chapter points
    /// to.
    ///
    /// `destination` is taken after the Markdown parser has processed escapes
    /// and entities. The call does not know whether it came from a link or
    /// from a picture: a link that resolves to [`Destination::Picture`], or a
    /// picture that resolves to anything else, is unresolved.
    ///
    /// # Errors
    ///
    /// The destination leads nowhere; the caller shows the link text or the
    /// alternative text instead.
    pub fn resolve(
        &self,
        _from: ChapterId,
        _destination: &str,
    ) -> Result<Destination, ResolveError> {
        todo!("apply the destination rules of RFC 0001")
    }

    /// Every row of the resource table, including service rows and kinds
    /// this crate does not know. For tools that list a file.
    pub fn resources(&self) -> impl ExactSizeIterator<Item = Resource<'_>> {
        // A container holds at most `u32::MAX` resources.
        let count = u32::try_from(self.rows.len()).unwrap_or(u32::MAX);
        (0..count).zip(&self.rows).map(|(number, row)| Resource {
            number,
            kind: row.kind,
            path: &row.path,
            size: (row.kind == ResourceKind::Picture).then_some(row.size),
            decoded_len: row.decoded_len,
        })
    }

    /// Reads and decodes one chapter.
    ///
    /// An error here does not affect other resources.
    ///
    /// # Errors
    ///
    /// The source fails, the record is damaged, the text is not UTF-8, or the
    /// id came from another book.
    pub fn read_chapter(&mut self, _id: ChapterId) -> Result<String> {
        todo!("read and decode one text resource")
    }

    /// Reads one picture as the bytes of an AVIF file.
    ///
    /// An error here does not affect other resources.
    ///
    /// # Errors
    ///
    /// The source fails, the record is damaged, or the id came from another
    /// book.
    pub fn read_picture(&mut self, _id: PictureId) -> Result<Vec<u8>> {
        todo!("read and decode one picture resource")
    }

    /// Reads the cover as the bytes of an AVIF file; `None` when the book
    /// has no cover.
    ///
    /// # Errors
    ///
    /// As [`read_picture`](Self::read_picture).
    pub fn read_cover(&mut self) -> Result<Option<Vec<u8>>> {
        match self.cover().map(|cover| cover.id) {
            Some(id) => self.read_picture(id).map(Some),
            None => Ok(None),
        }
    }

    /// Decodes every chapter and picture into a [`Book`] that can be changed.
    ///
    /// # Errors
    ///
    /// As [`read_chapter`](Self::read_chapter) and
    /// [`read_picture`](Self::read_picture), for any resource.
    pub fn load(&mut self) -> Result<Book> {
        todo!("decode every resource and build the model")
    }

    /// Gives the source back.
    #[must_use]
    pub fn into_inner(self) -> R {
        self.source
    }

    fn row(&self, number: u32) -> Option<&Row> {
        self.rows.get(usize::try_from(number).ok()?)
    }

    fn kind_of(&self, number: u32) -> Option<ResourceKind> {
        self.row(number).map(|row| row.kind)
    }

    /// The number of the text or picture resource with this path.
    fn find(&self, path: &str) -> Option<u32> {
        self.by_path
            .binary_search_by(|&number| self.path_of(number).cmp(path))
            .ok()
            .map(|at| self.by_path[at])
    }

    fn path_of(&self, number: u32) -> &str {
        self.row(number).map_or("", |row| row.path.as_str())
    }

    /// The entry of a resource known to be a chapter.
    fn chapter_entry(&self, number: u32) -> ChapterEntry<'_> {
        ChapterEntry {
            id: ChapterId::new(number),
            path: self.path_of(number),
            decoded_len: self.row(number).map_or(0, |row| row.decoded_len),
        }
    }

    /// The entry of a resource known to be a picture.
    fn picture_entry(&self, number: u32) -> PictureEntry<'_> {
        let row = self.row(number);
        PictureEntry {
            id: PictureId::new(number),
            path: self.path_of(number),
            size: row.map_or(
                Size {
                    width: 0,
                    height: 0,
                },
                |row| row.size,
            ),
            decoded_len: row.map_or(0, |row| row.decoded_len),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BookReader, Row};
    use crate::id::{ChapterId, PictureId};
    use crate::meta::Meta;
    use crate::types::{ResourceKind, Size, Version};
    use std::io::Cursor;

    fn row(kind: ResourceKind, path: &str, size: (u32, u32), decoded_len: u64) -> Row {
        Row {
            kind,
            path: path.to_owned(),
            size: Size {
                width: size.0,
                height: size.1,
            },
            decoded_len,
        }
    }

    /// The tables of the example book of RFC 0001, with a resource of an
    /// unknown kind appended.
    fn reader() -> BookReader<Cursor<Vec<u8>>> {
        let mut rows: Vec<Row> = (0..5)
            .map(|_| row(ResourceKind::Service, "", (0, 0), 8))
            .collect();
        rows.push(row(
            ResourceKind::Picture,
            "img/cover.avif",
            (1200, 1800),
            18_000,
        ));
        rows.push(row(ResourceKind::Text, "ch01.md", (0, 0), 279));
        rows.push(row(ResourceKind::Text, "ch02.md", (0, 0), 101));
        rows.push(row(
            ResourceKind::Picture,
            "img/fig-1.avif",
            (1600, 900),
            42_000,
        ));
        rows.push(row(ResourceKind::Unknown(9), "data/x.bin", (0, 0), 1));
        let mut meta = Meta::new("The Example".into(), "en".into(), "urn:uuid:0".into()).unwrap();
        meta.set_cover("img/cover.avif").unwrap();
        BookReader {
            source: Cursor::new(Vec::new()),
            version: Version { major: 1, minor: 0 },
            meta,
            toc: Vec::new(),
            rows,
            chapters: vec![6, 7],
            pictures: vec![5, 8],
            by_path: vec![6, 7, 5, 8],
        }
    }

    #[test]
    fn chapters_are_listed_in_reading_order() {
        let reader = reader();
        let paths: Vec<_> = reader.chapters().map(|chapter| chapter.path).collect();
        assert_eq!(paths, ["ch01.md", "ch02.md"]);
    }

    #[test]
    fn pictures_carry_their_size() {
        let reader = reader();
        let size = reader.pictures().next().map(|picture| picture.size);
        assert_eq!(
            size,
            Some(Size {
                width: 1200,
                height: 1800
            })
        );
    }

    #[test]
    fn chapter_id_accepts_the_number_of_a_chapter() {
        assert_eq!(reader().chapter_id(6).map(ChapterId::number), Some(6));
    }

    #[test]
    fn chapter_id_rejects_the_number_of_a_picture() {
        assert_eq!(reader().chapter_id(5), None);
    }

    #[test]
    fn chapter_id_rejects_a_service_resource() {
        assert_eq!(reader().chapter_id(2), None);
    }

    #[test]
    fn chapter_id_rejects_a_number_past_the_table() {
        assert_eq!(reader().chapter_id(99), None);
    }

    #[test]
    fn picture_id_rejects_an_unknown_kind() {
        assert_eq!(reader().picture_id(9), None);
    }

    #[test]
    fn find_picture_returns_the_picture_with_that_path() {
        let reader = reader();
        let found = reader
            .find_picture("img/fig-1.avif")
            .map(|picture| picture.id);
        assert_eq!(found.map(PictureId::number), Some(8));
    }

    #[test]
    fn find_picture_ignores_the_path_of_a_chapter() {
        assert!(reader().find_picture("ch01.md").is_none());
    }

    #[test]
    fn find_chapter_returns_none_for_an_unknown_path() {
        assert!(reader().find_chapter("ch99.md").is_none());
    }

    #[test]
    fn cover_is_the_picture_named_by_the_metadata() {
        let reader = reader();
        assert_eq!(
            reader.cover().map(|cover| cover.path),
            Some("img/cover.avif")
        );
    }

    #[test]
    fn chapter_returns_none_for_an_id_from_another_book() {
        assert!(reader().chapter(ChapterId::new(5)).is_none());
    }

    #[test]
    fn resources_lists_every_row() {
        assert_eq!(reader().resources().len(), 10);
    }

    #[test]
    fn resources_gives_a_size_to_pictures_only() {
        let reader = reader();
        let sized = reader.resources().filter(|r| r.size.is_some()).count();
        assert_eq!(sized, 2);
    }

    #[test]
    fn resources_keeps_an_unknown_kind() {
        let reader = reader();
        let last = reader.resources().last().map(|resource| resource.kind);
        assert_eq!(last, Some(ResourceKind::Unknown(9)));
    }
}
