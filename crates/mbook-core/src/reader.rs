//! Lazy reading of an mbook file.

use crate::book::Book;
use crate::container;
use crate::error::{Error, ResolveError, Result};
use crate::format::{ANCHORS, CONTAINER_MAGIC, FORMAT, META, RESOURCES, SERVICE_COUNT, TOC};
use crate::id::{ChapterId, PictureId};
use crate::index::Index;
use crate::meta::Meta;
use crate::options::{Limits, ReadOptions};
use crate::types::{Anchor, ChapterEntry, Destination, PictureEntry, Resource, TocEntry, Version};
use mopenzl::framing::{FramedDecompressor, FramedIndex};
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
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
    decoder: FramedDecompressor,
    /// Where every resource is in the source.
    directory: FramedIndex,
    /// The checked tables.
    index: Index,
    limits: Limits,
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
    pub fn with_options(mut source: R, options: &ReadOptions) -> Result<Self> {
        let limits = options.limits;
        check_signature(&mut source)?;
        // One resource is decoded at a time, so nothing bounds their sum.
        let mut decoder = container::decoder(&limits, u64::MAX)?;
        let directory = decoder
            .framed_index(&mut source)
            .map_err(container::error)?;
        let mut read = |number: u32| {
            let at = usize::try_from(number).unwrap_or(usize::MAX);
            decoder
                .read_indexed_resource(&mut source, &directory, at)
                .map_err(container::error)
        };
        if directory.is_empty() {
            return Err(Error::NotMbook);
        }
        let version = container::header(&read(FORMAT)?.outputs)?;
        if directory.len() < SERVICE_COUNT {
            return Err(Error::Malformed {
                resource: FORMAT,
                reason: "the file has fewer than five resources",
            });
        }
        let meta = read(META)?;
        let resources = read(RESOURCES)?;
        let toc = read(TOC)?;
        let anchors = read(ANCHORS)?;
        let decoded_lens: Vec<u64> = directory.iter().map(|info| info.decoded_size).collect();
        let index = Index::from_outputs(
            version,
            [
                &meta.outputs,
                &resources.outputs,
                &toc.outputs,
                &anchors.outputs,
            ],
            &decoded_lens,
        )?;
        Ok(Self {
            source,
            decoder,
            directory,
            index,
            limits,
        })
    }

    /// The format version the file declares.
    #[must_use]
    pub fn version(&self) -> Version {
        self.index.version()
    }

    /// The metadata.
    #[must_use]
    pub fn meta(&self) -> &Meta {
        self.index.meta()
    }

    /// The table of contents: a tree listed in pre-order.
    #[must_use]
    pub fn toc(&self) -> &[TocEntry] {
        self.index.toc()
    }

    /// The chapters, in reading order.
    pub fn chapters(&self) -> impl ExactSizeIterator<Item = ChapterEntry<'_>> {
        self.index.chapters()
    }

    /// The pictures, in file order.
    pub fn pictures(&self) -> impl ExactSizeIterator<Item = PictureEntry<'_>> {
        self.index.pictures()
    }

    /// What the tables say about a chapter. `None` for an id that came from
    /// another book.
    #[must_use]
    pub fn chapter(&self, id: ChapterId) -> Option<ChapterEntry<'_>> {
        self.index.chapter(id)
    }

    /// What the tables say about a picture. `None` for an id that came from
    /// another book.
    #[must_use]
    pub fn picture(&self, id: PictureId) -> Option<PictureEntry<'_>> {
        self.index.picture(id)
    }

    /// The chapter with this path.
    #[must_use]
    pub fn find_chapter(&self, path: &str) -> Option<ChapterEntry<'_>> {
        self.index.find_chapter(path)
    }

    /// The picture with this path.
    #[must_use]
    pub fn find_picture(&self, path: &str) -> Option<PictureEntry<'_>> {
        self.index.find_picture(path)
    }

    /// The cover named by the metadata, when it is a picture of the book.
    #[must_use]
    pub fn cover(&self) -> Option<PictureEntry<'_>> {
        self.meta().cover().and_then(|path| self.find_picture(path))
    }

    /// The id of the chapter with this resource number, when that resource
    /// is a chapter. Use it to restore an id stored outside the program.
    #[must_use]
    pub fn chapter_id(&self, number: u32) -> Option<ChapterId> {
        self.index.chapter_id(number)
    }

    /// The id of the picture with this resource number, when that resource
    /// is a picture.
    #[must_use]
    pub fn picture_id(&self, number: u32) -> Option<PictureId> {
        self.index.picture_id(number)
    }

    /// The anchors of a chapter, sorted by offset. Empty for an id that came
    /// from another book.
    #[must_use]
    pub fn anchors(&self, chapter: ChapterId) -> &[Anchor] {
        self.index.anchors(chapter.number())
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
    pub fn resolve(&self, from: ChapterId, destination: &str) -> Result<Destination, ResolveError> {
        self.index.resolve(from, destination)
    }

    /// Every row of the resource table, including service rows and kinds
    /// this crate does not know. For tools that list a file.
    pub fn resources(&self) -> impl ExactSizeIterator<Item = Resource<'_>> {
        self.index.resources()
    }

    /// Reads and decodes one chapter.
    ///
    /// An error here does not affect other resources.
    ///
    /// # Errors
    ///
    /// The source fails; the record is damaged or larger than the limit; the
    /// text is not UTF-8, or a stored position in it is not at the start of
    /// a line; the id came from another book.
    pub fn read_chapter(&mut self, id: ChapterId) -> Result<String> {
        let number = id.number();
        if self.index.chapter_id(number).is_none() {
            return Err(Error::NoSuchResource(number));
        }
        let bytes = self.read_serial(number)?;
        self.index.text(number, bytes)
    }

    /// Reads one picture as the bytes of an AVIF file.
    ///
    /// An error here does not affect other resources.
    ///
    /// # Errors
    ///
    /// The source fails, the record is damaged or larger than the limit, or
    /// the id came from another book.
    pub fn read_picture(&mut self, id: PictureId) -> Result<Vec<u8>> {
        let number = id.number();
        if self.index.picture_id(number).is_none() {
            return Err(Error::NoSuchResource(number));
        }
        self.read_serial(number)
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
    /// Resources of a kind this crate does not know are left behind.
    ///
    /// # Errors
    ///
    /// As [`read_chapter`](Self::read_chapter) and
    /// [`read_picture`](Self::read_picture), for any resource; or the decoded
    /// book is larger than the file limit.
    pub fn load(&mut self) -> Result<Book> {
        if self.index.content_len() > self.limits.max_file_bytes {
            return Err(Error::Limit("the decoded book exceeds the file limit"));
        }
        let Self {
            source,
            decoder,
            directory,
            index,
            ..
        } = self;
        Book::assemble(index, |number| {
            read_serial(decoder, source, directory, number)
        })
    }

    /// Gives the source back.
    #[must_use]
    pub fn into_inner(self) -> R {
        self.source
    }

    fn read_serial(&mut self, number: u32) -> Result<Vec<u8>> {
        read_serial(&mut self.decoder, &mut self.source, &self.directory, number)
    }
}

/// Reads one resource that must decode to a single serial output.
fn read_serial<R: Read + Seek>(
    decoder: &mut FramedDecompressor,
    source: &mut R,
    directory: &FramedIndex,
    number: u32,
) -> Result<Vec<u8>> {
    let at = usize::try_from(number).unwrap_or(usize::MAX);
    let resource = decoder
        .read_indexed_resource(source, directory, at)
        .map_err(container::error)?;
    container::serial(resource).ok_or(Error::Malformed {
        resource: number,
        reason: "the resource is not one serial output",
    })
}

/// Tells a file that is not a framed container from a damaged one, leaving
/// the source where it was.
fn check_signature<R: Read + Seek>(source: &mut R) -> Result<()> {
    let start = source.stream_position()?;
    let mut signature = [0; CONTAINER_MAGIC.len()];
    let complete = match source.read_exact(&mut signature) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => false,
        Err(error) => return Err(error.into()),
    };
    source.seek(SeekFrom::Start(start))?;
    if complete && signature == CONTAINER_MAGIC {
        Ok(())
    } else {
        Err(Error::NotMbook)
    }
}
