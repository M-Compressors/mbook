//! The file of an open book, read on the background.

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use mbook::{BookReader, ChapterId, Destination, ResolveError};

use crate::entities::figure::{FigureKind, FigureSource, FigureSources};

use super::chapter::{PictureRef, PreparedChapter, Target, analyse};
use super::error::BookError;
use super::snapshot::{BookSnapshot, ChapterInfo, ContentsEntry};

/// The file of an open book.
///
/// `Send + Sync`. Only background tasks call it, because every call may wait
/// for the file; the main thread works with the [`BookSnapshot`].
#[derive(Debug)]
pub(crate) struct BookSource {
    /// The reader; locked for a read and for nothing else.
    reader: Mutex<BookReader<File>>,
    /// The id of every chapter, in reading order.
    chapters: Vec<ChapterId>,
    /// The index of every chapter, by id.
    indices: HashMap<ChapterId, usize>,
}

/// Opens a book: reads its tables and builds the snapshot.
///
/// # Errors
///
/// [`BookError::Open`] when the file cannot be opened or is not a book.
pub(crate) fn open(path: &Path) -> Result<(BookSnapshot, BookSource), BookError> {
    let reader = BookReader::open(path).map_err(BookError::Open)?;
    let chapters: Vec<ChapterId> = reader.chapters().map(|chapter| chapter.id).collect();
    let indices: HashMap<ChapterId, usize> = chapters
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    let meta = reader.meta();
    let snapshot = BookSnapshot {
        title: meta.title().to_owned(),
        authors: meta.authors().map(str::to_owned).collect(),
        language: meta.language().to_owned(),
        chapters: reader
            .chapters()
            .map(|chapter| ChapterInfo {
                path: chapter.path.to_owned(),
                decoded_len: chapter.decoded_len,
            })
            .collect(),
        contents: reader
            .toc()
            .iter()
            .filter_map(|entry| {
                Some(ContentsEntry {
                    level: entry.level,
                    title: entry.title.clone(),
                    chapter: *indices.get(&entry.position.chapter)?,
                    offset: entry.position.offset,
                })
            })
            .collect(),
        cover: reader
            .cover()
            .map(|cover| (cover.size.width, cover.size.height)),
    };
    let source = BookSource {
        reader: Mutex::new(reader),
        chapters,
        indices,
    };
    Ok((snapshot, source))
}

impl BookSource {
    /// Reads the Markdown of one chapter.
    ///
    /// # Errors
    ///
    /// Returns an error if the index is absent, the file cannot be read, or
    /// its lock is poisoned.
    pub(crate) fn read_chapter(&self, index: usize) -> Result<String, BookError> {
        let id = self.id(index)?;
        self.lock()?.read_chapter(id).map_err(BookError::Read)
    }

    /// Reads one chapter and everything the view needs to show it without
    /// touching the file again.
    ///
    /// The file is locked to read the text and again to resolve the
    /// destinations and to read the diagrams and formulas the text shows; the
    /// Markdown is parsed between the two. A diagram or a formula that cannot
    /// be read does not fail the chapter: it has no source, and the view
    /// shows its alternative text.
    ///
    /// # Errors
    ///
    /// [`BookError::NoSuchChapter`] for an index past the last chapter;
    /// [`BookError::Read`] when the chapter cannot be read;
    /// [`BookError::Unusable`] when the file was poisoned.
    pub(crate) fn prepare_chapter(&self, index: usize) -> Result<PreparedChapter, BookError> {
        let id = self.id(index)?;
        let text = self.read_chapter(index)?;
        let analysis = analyse(&text);
        let mut reader = self.lock()?;
        let links = analysis
            .links
            .into_iter()
            .map(|destination| {
                let resolved = reader.resolve(id, &destination);
                let target = self.target(&reader, &destination, resolved);
                (destination, target)
            })
            .collect();
        let mut pictures = HashMap::new();
        let mut figures = FigureSources::default();
        for destination in analysis.pictures {
            let (kind, source) = match reader.resolve(id, &destination) {
                Ok(Destination::Picture(picture)) => {
                    if let Some(entry) = reader.picture(picture) {
                        pictures.insert(destination, picture_ref(&entry));
                    }
                    continue;
                }
                Ok(Destination::Diagram(diagram)) => {
                    (FigureKind::Diagram, reader.read_diagram(diagram))
                }
                Ok(Destination::Formula(formula)) => {
                    (FigureKind::Formula, reader.read_formula(formula))
                }
                _ => continue,
            };
            let source = source.unwrap_or_default().into();
            figures
                .by_destination
                .insert(destination, FigureSource { kind, source });
        }
        drop(reader);
        figures.by_reference = analysis
            .references
            .into_iter()
            .filter(|(_, reference)| figures.by_destination.contains_key(&reference.destination))
            .collect();
        Ok(PreparedChapter {
            index,
            text,
            blocks: analysis.blocks,
            links,
            pictures,
            figures,
        })
    }

    /// The AVIF bytes of a picture a chapter named.
    ///
    /// # Errors
    ///
    /// [`BookError::NoSuchPicture`] for a reference that is not a picture of
    /// this book; [`BookError::Read`] when the picture cannot be read;
    /// [`BookError::Unusable`] when the file was poisoned.
    pub(crate) fn read_picture(&self, picture: PictureRef) -> Result<Vec<u8>, BookError> {
        let mut reader = self.lock()?;
        let id = reader
            .picture_id(picture.number)
            .ok_or(BookError::NoSuchPicture)?;
        reader.read_picture(id).map_err(BookError::Read)
    }

    /// The AVIF bytes of the cover, when the book has one.
    ///
    /// # Errors
    ///
    /// [`BookError::Read`] when the cover cannot be read;
    /// [`BookError::Unusable`] when the file was poisoned.
    #[cfg(test)]
    pub(crate) fn read_cover(&self) -> Result<Option<Vec<u8>>, BookError> {
        self.lock()?.read_cover().map_err(BookError::Read)
    }

    /// Completes a [`Target::Deferred`] link when it is followed.
    ///
    /// A destination that leads nowhere is [`Target::Unresolved`], not an
    /// error. The result is never `Deferred`.
    ///
    /// # Errors
    ///
    /// [`BookError::NoSuchChapter`] for an index past the last chapter;
    /// [`BookError::Read`] when the anchors of the target chapter cannot be
    /// read; [`BookError::Unusable`] when the file was poisoned.
    pub(crate) fn follow(
        &self,
        from_chapter: usize,
        destination: &str,
    ) -> Result<Target, BookError> {
        let id = self.id(from_chapter)?;
        let mut reader = self.lock()?;
        match reader.read_resolve(id, destination) {
            Ok(resolved) => Ok(self.target(&reader, destination, Ok(resolved))),
            Err(mbook::Error::Unresolved(_)) => Ok(Target::Unresolved),
            Err(error) => Err(BookError::Read(error)),
        }
    }

    /// The id of the chapter with this index.
    fn id(&self, index: usize) -> Result<ChapterId, BookError> {
        self.chapters
            .get(index)
            .copied()
            .ok_or(BookError::NoSuchChapter(index))
    }

    /// The reader, for a read.
    pub(super) fn lock(&self) -> Result<MutexGuard<'_, BookReader<File>>, BookError> {
        self.reader.lock().map_err(|_| BookError::Unusable)
    }

    /// What a resolved destination is for the view.
    pub(super) fn target(
        &self,
        reader: &BookReader<File>,
        destination: &str,
        resolved: Result<Destination, ResolveError>,
    ) -> Target {
        match resolved {
            Ok(Destination::External) => Target::External(destination.to_owned()),
            Ok(Destination::Chapter(position)) => {
                self.indices
                    .get(&position.chapter)
                    .map_or(Target::Unresolved, |&chapter| Target::Place {
                        chapter,
                        offset: position.offset,
                    })
            }
            Ok(Destination::Picture(picture)) => {
                reader.picture(picture).map_or(Target::Unresolved, |entry| {
                    Target::Picture(picture_ref(&entry))
                })
            }
            Ok(Destination::ExtraAnchor(_)) => Target::Deferred,
            // A link leads to a chapter; a diagram and a formula are shown,
            // not led to.
            Ok(Destination::Diagram(_) | Destination::Formula(_)) | Err(_) => Target::Unresolved,
        }
    }
}

/// The reference to a picture of the book.
fn picture_ref(entry: &mbook::PictureEntry<'_>) -> PictureRef {
    PictureRef {
        number: entry.id.number(),
        width: entry.size.width,
        height: entry.size.height,
    }
}

#[cfg(test)]
impl BookSource {
    /// Poisons the lock, as a panic in another task would.
    pub(super) fn poison(&self) {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = self.reader.lock();
            panic!("poisoning the lock on purpose");
        }));
        assert!(outcome.is_err());
    }
}
