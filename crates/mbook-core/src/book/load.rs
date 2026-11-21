//! Loading a whole book into memory.

use super::{Book, Chapter, Picture, Toc, TocItem};
use crate::container;
use crate::error::{Error, Result};
use crate::format::{CONTAINER_MAGIC, FORMAT, SERVICE_COUNT};
use crate::index::Index;
use crate::options::ReadOptions;
use std::io::{Cursor, Read};
use std::path::Path;

/// Loading a book.
impl Book {
    /// Loads the book stored in a file.
    ///
    /// # Errors
    ///
    /// The file cannot be opened, or as [`Book::read`].
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::read(std::fs::File::open(path)?)
    }

    /// Loads a book from a reader, decoding everything in one pass. The
    /// reader does not have to seek.
    ///
    /// Loading is lenient where the format tells readers to be: a chapter
    /// with a broken link loads, and the problem is reported when the book is
    /// written. Resources of a kind this crate does not know are left behind.
    ///
    /// # Errors
    ///
    /// The source fails, the data is not a valid mbook file, or a default
    /// limit is exceeded.
    pub fn read(reader: impl Read) -> Result<Self> {
        Self::read_with(reader, &ReadOptions::default())
    }

    /// Loads a book from a reader with the given options.
    ///
    /// The file limit of `options` also bounds the decoded size of the book,
    /// which is what a loaded book takes in memory.
    ///
    /// # Errors
    ///
    /// As [`Book::read`], with the limits of `options`.
    pub fn read_with(mut reader: impl Read, options: &ReadOptions) -> Result<Self> {
        let limits = options.limits;
        // Tell a file that is not a framed container from a damaged one.
        let mut signature = Vec::with_capacity(CONTAINER_MAGIC.len());
        let wanted = u64::try_from(CONTAINER_MAGIC.len()).unwrap_or(u64::MAX);
        reader.by_ref().take(wanted).read_to_end(&mut signature)?;
        if signature != CONTAINER_MAGIC {
            return Err(Error::NotMbook);
        }
        let mut decoder = container::decoder(&limits, limits.max_file_bytes)?;
        let mut resources = decoder.framed_reader(Cursor::new(signature).chain(reader));

        let mut version = None;
        let mut service = Vec::with_capacity(SERVICE_COUNT);
        let mut contents = Vec::new();
        let mut decoded_lens = Vec::new();
        while let Some(resource) = resources.read_resource().map_err(container::error)? {
            decoded_lens.push(resource.info.decoded_size);
            if service.len() < SERVICE_COUNT {
                if service.is_empty() {
                    // Fail before the rest of an unsupported file is decoded.
                    version = Some(container::header(&resource.outputs)?);
                }
                service.push(resource.outputs);
                contents.push(None);
            } else {
                contents.push(container::serial(resource));
            }
        }
        let version = version.ok_or(Error::NotMbook)?;
        let [_, meta, table, toc, anchors] = service.as_slice() else {
            return Err(Error::Malformed {
                resource: FORMAT,
                reason: "the file has fewer than five resources",
            });
        };
        let index = Index::from_outputs(version, [meta, table, toc, anchors], &decoded_lens)?;
        Self::assemble(&index, |number| {
            usize::try_from(number)
                .ok()
                .and_then(|at| contents.get_mut(at))
                .and_then(Option::take)
                .ok_or(Error::Malformed {
                    resource: number,
                    reason: "the resource is not one serial output",
                })
        })
    }

    /// Builds the model from checked tables. `content` gives the decoded
    /// bytes of a chapter or a picture by its resource number.
    pub(crate) fn assemble(
        index: &Index,
        mut content: impl FnMut(u32) -> Result<Vec<u8>>,
    ) -> Result<Self> {
        let mut book = Self::empty(index.meta().clone());
        for entry in index.chapters() {
            let number = entry.id.number();
            let chapter = Chapter {
                path: entry.path.to_owned(),
                text: index.text(number, content(number)?)?,
                // The anchors stored in the file are kept until the text of
                // the chapter is replaced.
                anchors: index.anchors(number).to_vec(),
            };
            book.place_chapter(book.chapters.len(), chapter);
        }
        for entry in index.pictures() {
            book.place_picture(Picture {
                path: entry.path.to_owned(),
                size: entry.size,
                data: content(entry.id.number())?,
            });
        }
        book.toc = Toc::Explicit(
            index
                .toc()
                .iter()
                .map(|entry| toc_item(index, entry))
                .collect(),
        );
        Ok(book)
    }
}

/// A stored contents row, named the way a link names its target.
fn toc_item(index: &Index, entry: &crate::types::TocEntry) -> TocItem {
    let position = entry.position;
    let chapter = index.chapter(position.chapter);
    // A row inside a chapter always has an anchor at its offset.
    let anchor = (position.offset != 0)
        .then(|| index.anchors(position.chapter.number()))
        .and_then(|anchors| {
            anchors
                .iter()
                .find(|anchor| anchor.offset == position.offset)
        })
        .map(|anchor| anchor.name.clone());
    TocItem {
        level: entry.level,
        title: entry.title.clone(),
        chapter: chapter.map_or_else(String::new, |chapter| chapter.path.to_owned()),
        anchor,
    }
}
