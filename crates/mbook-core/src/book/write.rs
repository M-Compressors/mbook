//! Checking a book and writing it out.

use super::{Book, Chapter, Picture, Slot, Toc, TocItem};
use crate::analysis::{self, Heading, Reference, ReferenceKind};
use crate::container::{self, Writer};
use crate::error::{Error, Problem, ProblemKind, Problems, ResolveError, Result};
use crate::format::{KIND_ANCHORS, KIND_PICTURE, KIND_SERVICE, KIND_TEXT, SERVICE_COUNT};
use crate::link::{self, Parsed};
use crate::options::{Limits, WriteOptions};
use crate::tables::{self, AnchorRow, ResourceRow, Tables, TocRow};
use crate::text;
use std::io::Write;
use std::path::Path;

/// One resource after the five fixed ones.
enum Content<'a> {
    /// The anchor table of a chapter, which is the next resource.
    Anchors(&'a Chapter),
    Chapter(&'a Chapter),
    Picture(&'a Picture),
}

/// What a check works out and writing then uses: the four tables and the
/// resources in file order.
struct Layout<'a> {
    tables: Tables,
    contents: Vec<Content<'a>>,
}

/// A resource number or an offset; both fit once the limits are checked.
fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// A problem that belongs to the book as a whole.
fn whole_book(kind: ProblemKind) -> Problem {
    Problem {
        path: None,
        offset: None,
        kind,
    }
}

/// Checking and saving a book.
impl Book {
    /// Checks the rules that connect the parts of a book, without writing it.
    ///
    /// # Errors
    ///
    /// Every problem found, not only the first: links and pictures that lead
    /// nowhere, raw HTML, a cover that is not in the book, contents items
    /// that do not resolve.
    pub fn check(&self) -> Result<(), Problems> {
        self.layout().map(drop)
    }

    /// Writes the book with the default options.
    ///
    /// # Errors
    ///
    /// As [`write_with`](Self::write_with).
    pub fn write(&self, writer: impl Write) -> Result<()> {
        self.write_with(writer, &WriteOptions::default())
    }

    /// Writes the book as one mbook file.
    ///
    /// The book is checked first, and nothing is written if the check fails.
    /// Writing the same book twice with the same options gives the same
    /// bytes. The cover comes first, then the chapters in reading order,
    /// each after its anchor table when it has extra anchors, then the other
    /// pictures.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] with every problem [`check`](Self::check) finds;
    /// the sink fails; a limit of `options` is exceeded.
    pub fn write_with(&self, writer: impl Write, options: &WriteOptions) -> Result<()> {
        let layout = self.layout()?;
        layout.check_limits(&options.limits)?;
        let mut compressor = container::compressor(options)?;
        let mut out = Writer::new(&mut compressor, writer)?;
        out.header()?;
        out.table(&layout.tables.meta_columns()?)?;
        out.table(&layout.tables.resource_columns()?)?;
        out.table(&layout.tables.toc_columns()?)?;
        out.table(&layout.tables.anchor_columns()?)?;
        for content in &layout.contents {
            match content {
                Content::Anchors(chapter) => {
                    out.table(&tables::chapter_anchor_columns(&chapter.extra_anchors)?)?;
                }
                Content::Chapter(chapter) => out.text(&chapter.text)?,
                Content::Picture(picture) => out.stored(&picture.data)?,
            }
        }
        out.finish()
    }

    /// Writes the book to a file.
    ///
    /// The book is written to a temporary file next to `path`, which is then
    /// renamed into place, so an interrupted save leaves the previous file
    /// intact. `path` may be the file the book was loaded from.
    ///
    /// # Errors
    ///
    /// As [`write`](Self::write), or the file cannot be created or renamed.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let folder = path
            .parent()
            .filter(|folder| !folder.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut builder = tempfile::Builder::new();
        builder.prefix(".mbook-").suffix(".tmp");
        // A temporary file is private by default; a book is an ordinary
        // file, left to the umask like any other the program creates.
        #[cfg(unix)]
        builder.permissions(std::os::unix::fs::PermissionsExt::from_mode(0o666));
        // The temporary file removes itself unless it is renamed into place.
        let mut file = builder.tempfile_in(folder)?;
        self.write(file.as_file_mut())?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|failure| failure.error)?;
        Ok(())
    }

    /// Checks the book and lays it out as it will be written.
    fn layout(&self) -> Result<Layout<'_>, Problems> {
        let mut problems = Vec::new();
        let cover = self.meta.cover().map(|path| match self.slots.get(path) {
            Some(Slot::Picture(index)) => Some(*index),
            _ => None,
        });
        if cover == Some(None) {
            problems.push(whole_book(ProblemKind::MissingCover));
        }
        let cover = cover.flatten();
        // The cover, when there is one, takes the first number after the
        // fixed resources; chapters follow in reading order, each after its
        // anchor table when it has one.
        let mut next = SERVICE_COUNT + usize::from(cover.is_some());
        let numbers: Vec<u32> = self
            .chapters
            .iter()
            .map(|chapter| {
                next += usize::from(!chapter.extra_anchors.is_empty());
                next += 1;
                to_u32(next - 1)
            })
            .collect();
        let number_of = |chapter: usize| numbers.get(chapter).copied().unwrap_or(u32::MAX);

        let headings: Vec<Vec<Heading>> = self
            .chapters
            .iter()
            .map(|chapter| self.check_chapter(chapter, &mut problems))
            .collect();
        let toc = match &self.toc {
            Toc::FromHeadings { max_level } => {
                self.toc_from_headings(*max_level, &headings, number_of)
            }
            Toc::Explicit(items) => self.toc_from_items(items, number_of, &mut problems),
        };
        Problems::none_or(problems)?;

        let pictures = || (0..).zip(&self.pictures);
        let contents: Vec<Content<'_>> = pictures()
            .filter(|(index, _)| Some(*index) == cover)
            .map(|(_, picture)| Content::Picture(picture))
            .chain(self.chapters.iter().flat_map(|chapter| {
                let table = (!chapter.extra_anchors.is_empty()).then_some(chapter);
                table
                    .map(Content::Anchors)
                    .into_iter()
                    .chain([Content::Chapter(chapter)])
            }))
            .chain(
                pictures()
                    .filter(|(index, _)| Some(*index) != cover)
                    .map(|(_, picture)| Content::Picture(picture)),
            )
            .collect();
        let service = (0..SERVICE_COUNT).map(|_| ResourceRow {
            kind: KIND_SERVICE,
            path: String::new(),
            width: 0,
            height: 0,
        });
        let tables = Tables {
            meta: self
                .meta
                .iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
            resources: service.chain(contents.iter().map(Content::row)).collect(),
            toc,
            anchors: (0..)
                .zip(&self.chapters)
                .flat_map(|(index, chapter)| {
                    chapter.anchors.iter().map(move |anchor| AnchorRow {
                        resource: number_of(index),
                        offset: anchor.offset,
                        name: anchor.name.clone(),
                    })
                })
                .collect(),
        };
        Ok(Layout { tables, contents })
    }

    /// Checks the text of one chapter and what it refers to. Returns its
    /// headings, which the table of contents may be built from.
    fn check_chapter(&self, chapter: &Chapter, problems: &mut Vec<Problem>) -> Vec<Heading> {
        let mut report = |offset, kind| {
            problems.push(Problem {
                path: Some(chapter.path.clone()),
                offset: Some(offset),
                kind,
            });
        };
        // Text given through the API is stored clean; a loaded chapter is
        // kept as the file had it.
        if let Some(found) = text::violation(&chapter.text) {
            report(
                to_u32(found.offset),
                ProblemKind::ForbiddenCharacter(found.character),
            );
        }
        let analysis = analysis::analyse(&chapter.text);
        for &offset in &analysis.html {
            report(offset, ProblemKind::RawHtml);
        }
        for reference in &analysis.references {
            if let Some(kind) = self.reference_problem(chapter, reference) {
                report(reference.offset, kind);
            }
        }
        analysis.headings
    }

    /// What is wrong with a link or a picture of a chapter, if anything.
    fn reference_problem(&self, chapter: &Chapter, reference: &Reference) -> Option<ProblemKind> {
        let destination = || reference.destination.clone();
        let parsed = link::parse(&chapter.path, &reference.destination);
        match (reference.kind, parsed) {
            (ReferenceKind::Link, Ok(Parsed::External)) => None,
            (ReferenceKind::Link, Ok(Parsed::Internal { path, fragment })) => {
                let resolves = self.chapter(&path).is_some_and(|target| {
                    fragment
                        .as_deref()
                        .is_none_or(|name| target.has_anchor(name))
                });
                (!resolves).then(|| ProblemKind::UnresolvedLink(destination()))
            }
            (
                ReferenceKind::Picture,
                Ok(Parsed::Internal {
                    path,
                    fragment: None,
                }),
            ) if self.picture(&path).is_some() => None,
            // A picture may not name a network location at all.
            (ReferenceKind::Picture, Ok(Parsed::External))
            | (_, Err(ResolveError::ForbiddenScheme)) => {
                Some(ProblemKind::ForbiddenScheme(destination()))
            }
            (ReferenceKind::Link, Err(_)) => Some(ProblemKind::UnresolvedLink(destination())),
            (ReferenceKind::Picture, _) => Some(ProblemKind::UnresolvedPicture(destination())),
        }
    }

    /// The contents as every heading down to `max_level`.
    ///
    /// Heading levels of the source may skip, so the level of a row is its
    /// depth among the headings that enclose it. A heading inside a chapter
    /// is listed only when it has an anchor, as the format requires.
    fn toc_from_headings(
        &self,
        max_level: u8,
        headings: &[Vec<Heading>],
        number_of: impl Fn(usize) -> u32,
    ) -> Vec<TocRow> {
        let mut rows = Vec::new();
        let mut enclosing: Vec<u8> = Vec::new();
        for (index, (chapter, headings)) in self.chapters.iter().zip(headings).enumerate() {
            for heading in headings {
                let anchored = chapter
                    .anchors
                    .binary_search_by_key(&heading.offset, |anchor| anchor.offset)
                    .is_ok();
                let listed = heading.depth <= max_level
                    && !heading.title.is_empty()
                    && (heading.offset == 0 || anchored);
                if !listed {
                    continue;
                }
                while enclosing
                    .last()
                    .is_some_and(|&depth| depth >= heading.depth)
                {
                    enclosing.pop();
                }
                enclosing.push(heading.depth);
                rows.push(TocRow {
                    level: u8::try_from(enclosing.len()).unwrap_or(u8::MAX),
                    title: heading.title.clone(),
                    resource: number_of(index),
                    offset: heading.offset,
                });
            }
        }
        rows
    }

    /// The contents as given, with every item resolved to a position.
    fn toc_from_items(
        &self,
        items: &[TocItem],
        number_of: impl Fn(usize) -> u32,
        problems: &mut Vec<Problem>,
    ) -> Vec<TocRow> {
        let mut rows = Vec::with_capacity(items.len());
        let mut previous_level = 0u8;
        let mut previous = (0, 0);
        for item in items {
            let mut report = |kind| problems.push(whole_book(kind));
            if item.title.is_empty() {
                report(ProblemKind::EmptyTocTitle);
            }
            if item.level == 0 || item.level > previous_level.saturating_add(1) {
                report(ProblemKind::TocLevel(item.title.clone()));
            }
            previous_level = item.level;
            let Some(position) = self.toc_position(item) else {
                report(ProblemKind::DanglingTocItem(item.title.clone()));
                continue;
            };
            let place = (number_of(position.0), position.1);
            if place < previous {
                report(ProblemKind::TocOrder(item.title.clone()));
            }
            previous = place;
            rows.push(TocRow {
                level: item.level,
                title: item.title.clone(),
                resource: place.0,
                offset: place.1,
            });
        }
        rows
    }

    /// Where a contents item leads: the index of its chapter and an offset.
    ///
    /// Inside a chapter an item leads to a heading: the format wants a
    /// contents row to match a row of the `anchors` table, which holds the
    /// anchors of headings only.
    fn toc_position(&self, item: &TocItem) -> Option<(usize, u32)> {
        let Some(Slot::Chapter(index)) = self.slots.get(&item.chapter).copied() else {
            return None;
        };
        let offset = match &item.anchor {
            None => 0,
            Some(name) => {
                let anchors = &self.chapters.get(index)?.anchors;
                anchors.iter().find(|anchor| anchor.name == *name)?.offset
            }
        };
        Some((index, offset))
    }
}

impl Content<'_> {
    /// The row of the resource table that describes this resource.
    fn row(&self) -> ResourceRow {
        match self {
            Self::Anchors(_) => ResourceRow {
                kind: KIND_ANCHORS,
                path: String::new(),
                width: 0,
                height: 0,
            },
            Self::Chapter(chapter) => ResourceRow {
                kind: KIND_TEXT,
                path: chapter.path.clone(),
                width: 0,
                height: 0,
            },
            Self::Picture(picture) => ResourceRow {
                kind: KIND_PICTURE,
                path: picture.path.clone(),
                width: picture.size.width,
                height: picture.size.height,
            },
        }
    }

    /// The decoded size of the resource.
    fn len(&self) -> usize {
        match self {
            // One 32-bit offset and one name for every row.
            Self::Anchors(chapter) => chapter
                .extra_anchors
                .iter()
                .map(|anchor| anchor.name.len() + 4)
                .sum(),
            Self::Chapter(chapter) => chapter.text.len(),
            Self::Picture(picture) => picture.data.len(),
        }
    }
}

impl Layout<'_> {
    /// A book must fit the limits it is written with; the size of the file
    /// is watched by the container as it grows.
    fn check_limits(&self, limits: &Limits) -> Result<()> {
        if self.tables.resources.len() > limits.max_resources {
            return Err(Error::Limit("the book has more resources than the limit"));
        }
        let fits = |content: &Content<'_>| {
            u64::try_from(content.len()).is_ok_and(|len| len <= limits.max_resource_bytes)
        };
        if self.contents.iter().all(fits) {
            Ok(())
        } else {
            Err(Error::Limit(
                "a chapter, its anchor table or a picture exceeds the resource limit",
            ))
        }
    }
}
