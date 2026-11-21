//! The tables of a file, checked against RFC 0001 and ready for lookups.
//!
//! [`Index::build`] is where a reader rejects a file whose tables break a
//! rule. One rule cannot be checked here: that a position is at the start of
//! a line needs the text of its chapter, so [`Index::check_positions`] checks
//! it when that chapter is decoded.
//!
//! The anchor table of a chapter is not one of the tables read when a file is
//! opened. [`Index::read_extra_anchors`] checks it when it is decoded.

use crate::error::{Error, MetaError, ResolveError, Result};
use crate::format::{ANCHORS, KIND_SERVICE, META, RESOURCES, SERVICE_COUNT, TOC};
use crate::id::{ChapterId, PictureId};
use crate::link::{self, Parsed};
use crate::meta::Meta;
use crate::path;
use crate::tables::{self, AnchorRow, ResourceRow, Tables, TocRow};
use crate::types::{
    Anchor, ChapterEntry, Destination, PictureEntry, Position, Resource, ResourceKind, Size,
    TocEntry, Version,
};
use mopenzl::TypedOutput;
use std::collections::HashSet;
use std::ops::Range;

/// The checked tables of one file.
#[derive(Debug)]
pub(crate) struct Index {
    version: Version,
    meta: Meta,
    toc: Vec<TocEntry>,
    /// The resource table; the index is the resource number.
    rows: Vec<Row>,
    /// Numbers of text resources, in reading order.
    chapters: Vec<u32>,
    /// Numbers of picture resources, in file order.
    pictures: Vec<u32>,
    /// Numbers of every resource that has a path, sorted by path.
    by_path: Vec<u32>,
    /// Every anchor, grouped by chapter and sorted by offset inside it.
    anchors: Vec<Anchor>,
    /// Which part of `anchors` belongs to a chapter; sorted by chapter.
    anchor_spans: Vec<(u32, Range<usize>)>,
}

/// What a destination leads to, as far as the tables read at opening tell.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Resolved {
    /// Everything is known.
    Known(Destination),
    /// A fragment that is not the anchor of a heading, in a chapter that has
    /// an anchor table of its own.
    Extra {
        /// The chapter the destination names.
        chapter: ChapterId,
        /// The fragment, percent-decoded.
        name: String,
    },
}

/// One row of the resource table, joined with the container directory.
#[derive(Debug)]
struct Row {
    kind: ResourceKind,
    path: String,
    size: Size,
    decoded_len: u64,
}

fn malformed(resource: u32, reason: &'static str) -> Error {
    Error::Malformed { resource, reason }
}

impl Index {
    /// Reads the four tables from their decoded columns and checks them.
    pub(crate) fn from_outputs(
        version: Version,
        [meta, resources, toc, anchors]: [&[TypedOutput]; 4],
        decoded_lens: &[u64],
    ) -> Result<Self> {
        let tables = Tables {
            meta: tables::meta(meta).map_err(|reason| malformed(META, reason))?,
            resources: tables::resources(resources)
                .map_err(|reason| malformed(RESOURCES, reason))?,
            toc: tables::toc(toc).map_err(|reason| malformed(TOC, reason))?,
            anchors: tables::anchors(anchors).map_err(|reason| malformed(ANCHORS, reason))?,
        };
        Self::build(version, tables, decoded_lens)
    }

    /// The text of a chapter from its decoded bytes.
    pub(crate) fn text(&self, chapter: u32, bytes: Vec<u8>) -> Result<String> {
        let text =
            String::from_utf8(bytes).map_err(|_| malformed(chapter, "the text is not UTF-8"))?;
        self.check_positions(chapter, &text)?;
        Ok(text)
    }

    /// Checks the tables of a file and prepares them for lookups.
    ///
    /// `decoded_lens` holds the decoded size of every resource, from the
    /// container directory.
    pub(crate) fn build(version: Version, tables: Tables, decoded_lens: &[u64]) -> Result<Self> {
        let rows = rows(tables.resources, decoded_lens)?;
        let by_path = by_path(&rows)?;
        let numbers_of = |kind| {
            (0u32..)
                .zip(&rows)
                .filter(move |(_, row)| row.kind == kind)
                .map(|(number, _)| number)
                .collect::<Vec<_>>()
        };
        let chapters = numbers_of(ResourceKind::Text);
        let pictures = numbers_of(ResourceKind::Picture);
        let meta = Meta::from_rows(tables.meta).map_err(|error| {
            let reason = match error {
                MetaError::InvalidKey(_) => "a key is not valid",
                MetaError::Required(_) => "a required key is missing",
                MetaError::SingleValued(_) => "a key that holds one value repeats",
                _ => "the title is empty",
            };
            malformed(META, reason)
        })?;

        let mut index = Self {
            version,
            meta,
            toc: Vec::new(),
            rows,
            chapters,
            pictures,
            by_path,
            anchors: Vec::new(),
            anchor_spans: Vec::new(),
        };
        index.take_anchors(tables.anchors)?;
        index.take_toc(tables.toc)?;
        Ok(index)
    }

    /// Checks the `anchors` table and groups it by chapter.
    fn take_anchors(&mut self, rows: Vec<AnchorRow>) -> Result<()> {
        let mut previous = (0, 0);
        let mut names = HashSet::new();
        self.anchors.reserve(rows.len());
        for row in rows {
            let place = (row.resource, row.offset);
            self.check_position(ANCHORS, row.resource, row.offset)?;
            if row.name.is_empty() {
                return Err(malformed(ANCHORS, "a name is empty"));
            }
            if place < previous {
                return Err(malformed(ANCHORS, "the rows are not sorted"));
            }
            let at = self.anchors.len();
            match self.anchor_spans.last_mut() {
                Some((chapter, span)) if *chapter == row.resource => span.end = at + 1,
                _ => {
                    names.clear();
                    self.anchor_spans.push((row.resource, at..at + 1));
                }
            }
            self.anchors.push(Anchor {
                offset: row.offset,
                name: row.name,
            });
            previous = place;
        }
        // Names are compared after the rows are stored, so that no name has
        // to be cloned: a name must be unique inside its chapter.
        for (_, span) in &self.anchor_spans {
            names.clear();
            let anchors = self.anchors.get(span.clone()).unwrap_or_default();
            if !anchors
                .iter()
                .all(|anchor| names.insert(anchor.name.as_str()))
            {
                return Err(malformed(ANCHORS, "a name repeats in a chapter"));
            }
        }
        Ok(())
    }

    /// Checks the `toc` table. The anchors must have been taken already.
    fn take_toc(&mut self, rows: Vec<TocRow>) -> Result<()> {
        let mut previous_level = 0;
        let mut previous = (0, 0);
        self.toc.reserve(rows.len());
        for row in rows {
            let place = (row.resource, row.offset);
            if row.level == 0 || row.level > previous_level + 1 {
                return Err(malformed(TOC, "the levels do not form a tree"));
            }
            if row.title.is_empty() {
                return Err(malformed(TOC, "a title is empty"));
            }
            self.check_position(TOC, row.resource, row.offset)?;
            if place < previous {
                return Err(malformed(TOC, "the rows are not in reading order"));
            }
            let anchored = self
                .anchors(row.resource)
                .binary_search_by_key(&row.offset, |anchor| anchor.offset)
                .is_ok();
            if row.offset != 0 && !anchored {
                return Err(malformed(TOC, "a row inside a chapter has no anchor"));
            }
            self.toc.push(TocEntry {
                level: row.level,
                title: row.title,
                position: Position {
                    chapter: ChapterId::new(row.resource),
                    offset: row.offset,
                },
            });
            previous_level = row.level;
            previous = place;
        }
        Ok(())
    }

    /// Reads the anchor table of a chapter from its decoded columns and
    /// checks it. `table` is the number of the resource that holds it.
    pub(crate) fn read_extra_anchors(
        &self,
        chapter: u32,
        table: u32,
        outputs: &[TypedOutput],
    ) -> Result<Vec<Anchor>> {
        let anchors =
            tables::chapter_anchors(outputs).map_err(|reason| malformed(table, reason))?;
        self.extra_anchors(chapter, table, anchors)
    }

    /// Checks the rows of the anchor table of a chapter. That an offset is at
    /// the start of a line is checked by [`Self::check_extra_positions`].
    fn extra_anchors(&self, chapter: u32, table: u32, anchors: Vec<Anchor>) -> Result<Vec<Anchor>> {
        let len = self.row(chapter).map_or(0, |row| row.decoded_len);
        // A fragment names one place, whichever table holds it.
        let mut names: HashSet<&str> = self
            .anchors(chapter)
            .iter()
            .map(|anchor| anchor.name.as_str())
            .collect();
        let mut previous = 0;
        for anchor in &anchors {
            if anchor.name.is_empty() {
                return Err(malformed(table, "a name is empty"));
            }
            if anchor.offset != 0 && u64::from(anchor.offset) >= len {
                return Err(malformed(table, "an offset is past the end of its chapter"));
            }
            if anchor.offset < previous {
                return Err(malformed(table, "the rows are not sorted"));
            }
            if !names.insert(anchor.name.as_str()) {
                return Err(malformed(table, "a name repeats in a chapter"));
            }
            previous = anchor.offset;
        }
        Ok(anchors)
    }

    /// Checks, against the decoded text of a chapter, that every anchor of
    /// its own table is at the start of a line.
    pub(crate) fn check_extra_positions(table: u32, anchors: &[Anchor], text: &str) -> Result<()> {
        if anchors
            .iter()
            .all(|anchor| is_line_start(text.as_bytes(), anchor.offset))
        {
            Ok(())
        } else {
            Err(malformed(table, "a position is not at the start of a line"))
        }
    }

    /// The number of the resource that holds the anchor table of a chapter:
    /// the one before the chapter, when it is of that kind.
    pub(crate) fn anchor_table(&self, chapter: u32) -> Option<u32> {
        let table = chapter.checked_sub(1)?;
        (self.row(table)?.kind == ResourceKind::ChapterAnchors).then_some(table)
    }

    /// A position must name a chapter and lie inside its text. That it is at
    /// the start of a line is checked by [`Self::check_positions`].
    fn check_position(&self, table: u32, resource: u32, offset: u32) -> Result<()> {
        let Some(row) = self
            .row(resource)
            .filter(|row| row.kind == ResourceKind::Text)
        else {
            return Err(malformed(table, "a row does not name a chapter"));
        };
        if offset != 0 && u64::from(offset) >= row.decoded_len {
            return Err(malformed(table, "an offset is past the end of its chapter"));
        }
        Ok(())
    }

    /// Checks, against the decoded text of a chapter, that every stored
    /// position in it is at the start of a line.
    pub(crate) fn check_positions(&self, chapter: u32, text: &str) -> Result<()> {
        // Every contents row inside a chapter has an anchor at its offset, so
        // checking the anchors covers the contents as well.
        if self
            .anchors(chapter)
            .iter()
            .all(|anchor| is_line_start(text.as_bytes(), anchor.offset))
        {
            Ok(())
        } else {
            Err(malformed(
                chapter,
                "a position is not at the start of a line",
            ))
        }
    }

    pub(crate) fn version(&self) -> Version {
        self.version
    }

    pub(crate) fn meta(&self) -> &Meta {
        &self.meta
    }

    pub(crate) fn toc(&self) -> &[TocEntry] {
        &self.toc
    }

    /// The chapters, in reading order.
    pub(crate) fn chapters(&self) -> impl ExactSizeIterator<Item = ChapterEntry<'_>> {
        self.chapters
            .iter()
            .map(|&number| self.chapter_entry(number))
    }

    /// The pictures, in file order.
    pub(crate) fn pictures(&self) -> impl ExactSizeIterator<Item = PictureEntry<'_>> {
        self.pictures
            .iter()
            .map(|&number| self.picture_entry(number))
    }

    /// Every row of the resource table.
    pub(crate) fn resources(&self) -> impl ExactSizeIterator<Item = Resource<'_>> {
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

    pub(crate) fn chapter_id(&self, number: u32) -> Option<ChapterId> {
        (self.row(number)?.kind == ResourceKind::Text).then(|| ChapterId::new(number))
    }

    pub(crate) fn picture_id(&self, number: u32) -> Option<PictureId> {
        (self.row(number)?.kind == ResourceKind::Picture).then(|| PictureId::new(number))
    }

    pub(crate) fn chapter(&self, id: ChapterId) -> Option<ChapterEntry<'_>> {
        self.chapter_id(id.number())
            .map(|id| self.chapter_entry(id.number()))
    }

    pub(crate) fn picture(&self, id: PictureId) -> Option<PictureEntry<'_>> {
        self.picture_id(id.number())
            .map(|id| self.picture_entry(id.number()))
    }

    pub(crate) fn find_chapter(&self, path: &str) -> Option<ChapterEntry<'_>> {
        self.find(path)
            .and_then(|number| self.chapter_id(number))
            .map(|id| self.chapter_entry(id.number()))
    }

    pub(crate) fn find_picture(&self, path: &str) -> Option<PictureEntry<'_>> {
        self.find(path)
            .and_then(|number| self.picture_id(number))
            .map(|id| self.picture_entry(id.number()))
    }

    /// The anchors of the headings of a chapter, sorted by offset.
    pub(crate) fn anchors(&self, chapter: u32) -> &[Anchor] {
        self.anchor_spans
            .binary_search_by_key(&chapter, |(number, _)| *number)
            .ok()
            .and_then(|at| self.anchor_spans.get(at))
            .and_then(|(_, span)| self.anchors.get(span.clone()))
            .unwrap_or_default()
    }

    /// The decoded size of everything a loaded book is built from: every
    /// chapter, picture and anchor table of a chapter.
    pub(crate) fn content_len(&self) -> u64 {
        let loaded = |row: &&Row| {
            matches!(
                row.kind,
                ResourceKind::Text | ResourceKind::Picture | ResourceKind::ChapterAnchors
            )
        };
        self.rows
            .iter()
            .filter(loaded)
            .fold(0, |total, row| total.saturating_add(row.decoded_len))
    }

    /// Applies "Resolving destinations" of RFC 0001 without reading
    /// anything.
    pub(crate) fn resolve(
        &self,
        from: ChapterId,
        destination: &str,
    ) -> Result<Destination, ResolveError> {
        Ok(match self.resolve_named(from, destination)? {
            Resolved::Known(destination) => destination,
            Resolved::Extra { chapter, .. } => Destination::ExtraAnchor(chapter),
        })
    }

    /// Applies "Resolving destinations" of RFC 0001 as far as the tables read
    /// at opening allow, and keeps the fragment when more is needed.
    pub(crate) fn resolve_named(
        &self,
        from: ChapterId,
        destination: &str,
    ) -> Result<Resolved, ResolveError> {
        let from = self.chapter(from).ok_or(ResolveError::NotFound)?;
        let (path, fragment) = match link::parse(from.path, destination)? {
            Parsed::External => return Ok(Resolved::Known(Destination::External)),
            Parsed::Internal { path, fragment } => (path, fragment),
        };
        let number = self.find(&path).ok_or(ResolveError::NotFound)?;
        if let Some(picture) = self.picture_id(number) {
            return match fragment {
                Some(_) => Err(ResolveError::FragmentOnPicture),
                None => Ok(Resolved::Known(Destination::Picture(picture))),
            };
        }
        // A resource of a kind this crate does not know is unresolved.
        let chapter = self.chapter_id(number).ok_or(ResolveError::NotFound)?;
        let at = |offset| Resolved::Known(Destination::Chapter(Position { chapter, offset }));
        let Some(name) = fragment else {
            return Ok(at(0));
        };
        let heading = self
            .anchors(number)
            .iter()
            .find(|anchor| anchor.name == name);
        match heading {
            Some(anchor) => Ok(at(anchor.offset)),
            None if self.anchor_table(number).is_some() => Ok(Resolved::Extra { chapter, name }),
            None => Err(ResolveError::UnknownAnchor),
        }
    }

    fn row(&self, number: u32) -> Option<&Row> {
        self.rows.get(usize::try_from(number).ok()?)
    }

    fn path_of(&self, number: u32) -> &str {
        self.row(number).map_or("", |row| row.path.as_str())
    }

    /// The number of the resource with this path.
    fn find(&self, path: &str) -> Option<u32> {
        self.by_path
            .binary_search_by(|&number| self.path_of(number).cmp(path))
            .ok()
            .and_then(|at| self.by_path.get(at).copied())
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

/// Tells whether an offset is a valid position in a chapter with this text:
/// 0, or the first byte of a line.
fn is_line_start(text: &[u8], offset: u32) -> bool {
    usize::try_from(offset).is_ok_and(|offset| {
        offset == 0 || (offset < text.len() && text.get(offset - 1) == Some(&b'\n'))
    })
}

/// Checks the `resources` table and joins it with the container directory.
fn rows(table: Vec<ResourceRow>, decoded_lens: &[u64]) -> Result<Vec<Row>> {
    if table.len() != decoded_lens.len() {
        return Err(malformed(
            RESOURCES,
            "the table does not have one row per resource",
        ));
    }
    let mut rows = Vec::with_capacity(table.len());
    for (number, (row, &decoded_len)) in table.into_iter().zip(decoded_lens).enumerate() {
        let is_service = row.kind == KIND_SERVICE;
        if is_service != (number < SERVICE_COUNT) {
            return Err(malformed(
                RESOURCES,
                "kind 0 is not exactly the first five rows",
            ));
        }
        let kind = ResourceKind::from(row.kind);
        // Service resources and anchor tables are found by position.
        let has_path = !is_service && kind != ResourceKind::ChapterAnchors;
        if has_path == row.path.is_empty() {
            return Err(malformed(
                RESOURCES,
                "a path is missing, or a row that takes none has one",
            ));
        }
        if has_path && path::validate(&row.path).is_err() {
            return Err(malformed(RESOURCES, "a path is not valid"));
        }
        if kind != ResourceKind::Picture && (row.width, row.height) != (0, 0) {
            return Err(malformed(
                RESOURCES,
                "a row that is not a picture has a size",
            ));
        }
        rows.push(Row {
            kind,
            path: row.path,
            size: Size {
                width: row.width,
                height: row.height,
            },
            decoded_len,
        });
    }
    let followed_by_chapter = |at: usize| {
        rows.get(at + 1)
            .is_some_and(|next| next.kind == ResourceKind::Text)
    };
    if (0..rows.len())
        .any(|at| rows[at].kind == ResourceKind::ChapterAnchors && !followed_by_chapter(at))
    {
        return Err(malformed(
            RESOURCES,
            "an anchor table is not followed by its chapter",
        ));
    }
    Ok(rows)
}

/// The numbers of every resource that has a path, sorted by path. Fails when
/// two resources share one.
fn by_path(rows: &[Row]) -> Result<Vec<u32>> {
    let path_of = |number: &u32| {
        usize::try_from(*number)
            .ok()
            .and_then(|at| rows.get(at))
            .map_or("", |row| row.path.as_str())
    };
    let mut numbers: Vec<u32> = (0u32..)
        .zip(rows)
        .filter(|(_, row)| !row.path.is_empty())
        .map(|(number, _)| number)
        .collect();
    numbers.sort_unstable_by(|a, b| path_of(a).cmp(path_of(b)));
    if numbers
        .windows(2)
        .any(|pair| path_of(&pair[0]) == path_of(&pair[1]))
    {
        return Err(malformed(RESOURCES, "two resources share a path"));
    }
    Ok(numbers)
}

#[cfg(test)]
mod tests {
    use super::{Index, Resolved};
    use crate::error::{Error, ResolveError};
    use crate::format::{ANCHORS, META, RESOURCES, TOC};
    use crate::id::ChapterId;
    use crate::tables::{AnchorRow, ResourceRow, Tables, TocRow};
    use crate::types::{Anchor, Destination, ResourceKind, Size, Version};

    const VERSION: Version = Version { major: 1, minor: 0 };

    fn resource(kind: u8, path: &str, width: u32, height: u32) -> ResourceRow {
        ResourceRow {
            kind,
            path: path.to_owned(),
            width,
            height,
        }
    }

    fn toc(level: u8, title: &str, resource: u32, offset: u32) -> TocRow {
        TocRow {
            level,
            title: title.to_owned(),
            resource,
            offset,
        }
    }

    fn anchor(resource: u32, offset: u32, name: &str) -> AnchorRow {
        AnchorRow {
            resource,
            offset,
            name: name.to_owned(),
        }
    }

    /// The example book of RFC 0001.
    fn tables() -> Tables {
        let meta = [
            ("title", "The Example"),
            ("lang", "en"),
            ("id", "urn:uuid:3f1c2a7e-8a54-4d0b-9c1e-6b2f0d9a7e11"),
            ("author", "A. Writer"),
            ("cover", "img/cover.avif"),
        ];
        let mut resources: Vec<_> = (0..5).map(|_| resource(0, "", 0, 0)).collect();
        resources.extend([
            resource(2, "img/cover.avif", 1200, 1800),
            resource(1, "ch01.md", 0, 0),
            resource(1, "ch02.md", 0, 0),
            resource(2, "img/fig-1.avif", 1600, 900),
        ]);
        Tables {
            meta: meta
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            resources,
            toc: vec![
                toc(1, "Chapter 1. Arrival", 6, 0),
                toc(2, "The station", 6, 130),
                toc(1, "Chapter 2. The letter", 7, 0),
            ],
            anchors: vec![
                anchor(6, 0, "chapter-1-arrival"),
                anchor(6, 130, "the-station"),
                anchor(7, 0, "chapter-2-the-letter"),
            ],
        }
    }

    const LENS: [u64; 9] = [8, 103, 123, 77, 72, 18_000, 279, 101, 42_000];

    fn index() -> Index {
        Index::build(VERSION, tables(), &LENS).unwrap()
    }

    /// The table and the rule a changed example is rejected for.
    fn rejection(change: impl FnOnce(&mut Tables)) -> (u32, &'static str) {
        let mut tables = tables();
        change(&mut tables);
        match Index::build(VERSION, tables, &LENS) {
            Err(Error::Malformed { resource, reason }) => (resource, reason),
            other => panic!("expected a malformed table, got {other:?}"),
        }
    }

    #[test]
    fn the_example_of_the_format_rfc_is_accepted() {
        let index = index();
        assert_eq!(
            (index.version(), index.meta().title(), index.toc().len()),
            (VERSION, "The Example", 3)
        );
    }

    #[test]
    fn chapters_are_listed_in_reading_order() {
        let index = index();
        let chapters: Vec<_> = index.chapters().map(|c| (c.path, c.decoded_len)).collect();
        assert_eq!(chapters, [("ch01.md", 279), ("ch02.md", 101)]);
    }

    #[test]
    fn pictures_carry_their_size() {
        let index = index();
        let sizes: Vec<_> = index.pictures().map(|p| (p.path, p.size)).collect();
        let size = |width, height| Size { width, height };
        assert_eq!(
            sizes,
            [
                ("img/cover.avif", size(1200, 1800)),
                ("img/fig-1.avif", size(1600, 900))
            ]
        );
    }

    #[test]
    fn ids_are_given_only_to_resources_of_the_right_kind() {
        let index = index();
        let chapters = [2, 5, 6, 99].map(|number| index.chapter_id(number).is_some());
        let pictures = [2, 5, 6, 99].map(|number| index.picture_id(number).is_some());
        assert_eq!(
            (chapters, pictures),
            ([false, false, true, false], [false, true, false, false])
        );
    }

    #[test]
    fn resources_are_found_by_path() {
        let index = index();
        assert_eq!(
            index.find_chapter("ch02.md").map(|c| c.id.number()),
            Some(7)
        );
        assert_eq!(
            index.find_picture("img/fig-1.avif").map(|p| p.id.number()),
            Some(8)
        );
    }

    #[test]
    fn a_path_of_the_other_kind_finds_nothing() {
        let index = index();
        assert!(index.find_chapter("img/cover.avif").is_none());
        assert!(index.find_picture("ch01.md").is_none());
        assert!(index.find_chapter("ch09.md").is_none());
    }

    #[test]
    fn an_id_from_another_book_describes_nothing() {
        let index = index();
        assert!(index.chapter(ChapterId::new(5)).is_none());
    }

    #[test]
    fn anchors_are_grouped_by_chapter() {
        let index = index();
        let names: Vec<_> = index.anchors(6).iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["chapter-1-arrival", "the-station"]);
        assert_eq!(index.anchors(5), []);
    }

    #[test]
    fn the_content_length_counts_chapters_and_pictures() {
        assert_eq!(index().content_len(), 18_000 + 279 + 101 + 42_000);
    }

    #[test]
    fn every_row_is_listed_as_a_resource() {
        let index = index();
        let kinds: Vec<_> = index.resources().skip(4).map(|r| r.kind).collect();
        assert_eq!(
            kinds,
            [
                ResourceKind::Service,
                ResourceKind::Picture,
                ResourceKind::Text,
                ResourceKind::Text,
                ResourceKind::Picture
            ]
        );
    }

    #[test]
    fn a_link_with_a_fragment_resolves_to_the_anchor() {
        let index = index();
        let from = index.chapter_id(7).unwrap();
        let target = index.resolve(from, "ch01.md#the-station");
        assert!(matches!(
            target,
            Ok(Destination::Chapter(position))
                if position.chapter.number() == 6 && position.offset == 130
        ));
    }

    #[test]
    fn a_link_without_a_fragment_leads_to_the_chapter_start() {
        let index = index();
        let from = index.chapter_id(6).unwrap();
        assert!(matches!(
            index.resolve(from, "ch02.md"),
            Ok(Destination::Chapter(position)) if position.offset == 0
        ));
    }

    #[test]
    fn a_picture_destination_resolves_to_the_picture() {
        let index = index();
        let from = index.chapter_id(6).unwrap();
        assert!(matches!(
            index.resolve(from, "img/fig-1.avif"),
            Ok(Destination::Picture(picture)) if picture.number() == 8
        ));
    }

    #[test]
    fn destinations_that_lead_nowhere_say_why() {
        let index = index();
        let from = index.chapter_id(6).unwrap();
        let errors = [
            "ch09.md",
            "ch02.md#nope",
            "img/fig-1.avif#x",
            "ftp://example.org/",
        ]
        .map(|destination| index.resolve(from, destination).unwrap_err());
        assert_eq!(
            errors,
            [
                ResolveError::NotFound,
                ResolveError::UnknownAnchor,
                ResolveError::FragmentOnPicture,
                ResolveError::ForbiddenScheme
            ]
        );
    }

    #[test]
    fn an_external_link_is_external() {
        let index = index();
        let from = index.chapter_id(6).unwrap();
        assert_eq!(
            index.resolve(from, "https://example.org"),
            Ok(Destination::External)
        );
    }

    #[test]
    fn a_resource_of_an_unknown_kind_is_listed_but_unresolved() {
        let mut tables = tables();
        tables.resources.push(resource(9, "data/x.bin", 0, 0));
        let mut lens = LENS.to_vec();
        lens.push(1);
        let index = Index::build(VERSION, tables, &lens).unwrap();
        let from = index.chapter_id(6).unwrap();
        assert_eq!(
            index.resources().last().map(|r| r.kind),
            Some(ResourceKind::Unknown(9))
        );
        assert_eq!(
            index.resolve(from, "data/x.bin"),
            Err(ResolveError::NotFound)
        );
    }

    #[test]
    fn positions_at_line_starts_pass_the_text_check() {
        let text = format!(
            "# Chapter 1. Arrival\n{}\n## The station\n",
            "x".repeat(108)
        );
        assert_eq!(text.find("## The").unwrap(), 130);
        assert!(index().check_positions(6, &text).is_ok());
    }

    #[test]
    fn a_position_inside_a_line_fails_the_text_check() {
        let text = "x".repeat(279);
        assert!(matches!(
            index().check_positions(6, &text),
            Err(Error::Malformed { resource: 6, .. })
        ));
    }

    #[test]
    fn a_row_count_that_differs_from_the_directory_is_rejected() {
        let result = Index::build(VERSION, tables(), &LENS[..8]);
        assert!(matches!(
            result,
            Err(Error::Malformed {
                resource: RESOURCES,
                ..
            })
        ));
    }

    #[test]
    fn a_fixed_row_of_another_kind_is_rejected() {
        let (table, reason) = rejection(|t| t.resources[3].kind = 1);
        assert_eq!(
            (table, reason),
            (RESOURCES, "kind 0 is not exactly the first five rows")
        );
    }

    #[test]
    fn a_later_row_of_kind_zero_is_rejected() {
        let (table, _) = rejection(|t| t.resources[6] = resource(0, "", 0, 0));
        assert_eq!(table, RESOURCES);
    }

    #[test]
    fn a_resource_without_a_path_is_rejected() {
        let (_, reason) = rejection(|t| t.resources[6].path.clear());
        assert_eq!(
            reason,
            "a path is missing, or a row that takes none has one"
        );
    }

    #[test]
    fn a_path_that_leaves_the_book_is_rejected() {
        let (_, reason) = rejection(|t| t.resources[6].path = "../ch01.md".into());
        assert_eq!(reason, "a path is not valid");
    }

    #[test]
    fn a_repeated_path_is_rejected() {
        let (_, reason) = rejection(|t| t.resources[7].path = "ch01.md".into());
        assert_eq!(reason, "two resources share a path");
    }

    #[test]
    fn a_chapter_with_a_size_is_rejected() {
        let (_, reason) = rejection(|t| t.resources[6].width = 1);
        assert_eq!(reason, "a row that is not a picture has a size");
    }

    #[test]
    fn metadata_without_an_identifier_is_rejected() {
        let (table, reason) = rejection(|t| t.meta.retain(|(key, _)| key != "id"));
        assert_eq!((table, reason), (META, "a required key is missing"));
    }

    #[test]
    fn metadata_with_two_titles_is_rejected() {
        let (_, reason) = rejection(|t| t.meta.push(("title".into(), "Again".into())));
        assert_eq!(reason, "a key that holds one value repeats");
    }

    #[test]
    fn metadata_with_an_invalid_key_is_rejected() {
        let (_, reason) = rejection(|t| t.meta.push(("My Key".into(), "x".into())));
        assert_eq!(reason, "a key is not valid");
    }

    #[test]
    fn metadata_with_an_empty_title_is_rejected() {
        let (_, reason) = rejection(|t| t.meta[0].1.clear());
        assert_eq!(reason, "the title is empty");
    }

    #[test]
    fn contents_that_start_below_level_one_are_rejected() {
        let (table, reason) = rejection(|t| t.toc[0].level = 2);
        assert_eq!((table, reason), (TOC, "the levels do not form a tree"));
    }

    #[test]
    fn contents_that_skip_a_level_are_rejected() {
        let (_, reason) = rejection(|t| t.toc[1].level = 3);
        assert_eq!(reason, "the levels do not form a tree");
    }

    #[test]
    fn contents_with_level_zero_are_rejected() {
        let (_, reason) = rejection(|t| t.toc[2].level = 0);
        assert_eq!(reason, "the levels do not form a tree");
    }

    #[test]
    fn a_contents_row_without_a_title_is_rejected() {
        let (_, reason) = rejection(|t| t.toc[0].title.clear());
        assert_eq!(reason, "a title is empty");
    }

    #[test]
    fn a_contents_row_that_names_a_picture_is_rejected() {
        let (_, reason) = rejection(|t| t.toc[2].resource = 8);
        assert_eq!(reason, "a row does not name a chapter");
    }

    #[test]
    fn a_contents_row_past_the_end_of_its_chapter_is_rejected() {
        let (_, reason) = rejection(|t| t.toc[1].offset = 279);
        assert_eq!(reason, "an offset is past the end of its chapter");
    }

    #[test]
    fn contents_out_of_reading_order_are_rejected() {
        let (_, reason) = rejection(|t| t.toc.swap(0, 2));
        assert_eq!(reason, "the rows are not in reading order");
    }

    #[test]
    fn a_contents_row_inside_a_chapter_needs_an_anchor() {
        let (table, reason) = rejection(|t| t.toc[1].offset = 131);
        assert_eq!(
            (table, reason),
            (TOC, "a row inside a chapter has no anchor")
        );
    }

    #[test]
    fn an_anchor_in_a_picture_is_rejected() {
        let (table, reason) = rejection(|t| t.anchors[2].resource = 8);
        assert_eq!((table, reason), (ANCHORS, "a row does not name a chapter"));
    }

    #[test]
    fn an_anchor_without_a_name_is_rejected() {
        let (_, reason) = rejection(|t| t.anchors[0].name.clear());
        assert_eq!(reason, "a name is empty");
    }

    #[test]
    fn unsorted_anchors_are_rejected() {
        let (_, reason) = rejection(|t| t.anchors.swap(0, 1));
        assert_eq!(reason, "the rows are not sorted");
    }

    #[test]
    fn a_name_that_repeats_in_a_chapter_is_rejected() {
        let (_, reason) = rejection(|t| t.anchors[1].name = "chapter-1-arrival".into());
        assert_eq!(reason, "a name repeats in a chapter");
    }

    #[test]
    fn the_same_name_in_two_chapters_is_accepted() {
        let mut tables = tables();
        tables.anchors[2].name = "chapter-1-arrival".into();
        assert!(Index::build(VERSION, tables, &LENS).is_ok());
    }

    /// The example with an anchor table in front of the first chapter: the
    /// table is resource 6, the chapters are 7 and 8.
    fn with_anchor_table() -> Index {
        let mut tables = tables();
        tables.resources.insert(6, resource(3, "", 0, 0));
        for row in &mut tables.toc {
            row.resource += 1;
        }
        for row in &mut tables.anchors {
            row.resource += 1;
        }
        let mut lens = LENS.to_vec();
        lens.insert(6, 40);
        Index::build(VERSION, tables, &lens).unwrap()
    }

    fn extra(offset: u32, name: &str) -> Anchor {
        Anchor {
            offset,
            name: name.to_owned(),
        }
    }

    /// The rule the anchor table of the first chapter is rejected for.
    fn extra_rejection(anchors: Vec<Anchor>) -> &'static str {
        match with_anchor_table().extra_anchors(7, 6, anchors) {
            Err(Error::Malformed {
                resource: 6,
                reason,
            }) => reason,
            other => panic!("expected a malformed anchor table, got {other:?}"),
        }
    }

    #[test]
    fn an_anchor_table_belongs_to_the_chapter_after_it() {
        let index = with_anchor_table();
        assert_eq!(
            (index.anchor_table(7), index.anchor_table(8)),
            (Some(6), None)
        );
        assert_eq!(index.anchor_table(0), None);
    }

    #[test]
    fn an_anchor_table_is_listed_but_is_neither_chapter_nor_picture() {
        let index = with_anchor_table();
        let listed = index.resources().nth(6).map(|r| (r.kind, r.path, r.size));
        assert_eq!(listed, Some((ResourceKind::ChapterAnchors, "", None)));
        assert!(index.chapter_id(6).is_none() && index.picture_id(6).is_none());
        assert_eq!(index.chapters().len(), 2);
    }

    #[test]
    fn the_content_length_counts_anchor_tables() {
        let plain = index().content_len();
        assert_eq!(with_anchor_table().content_len(), plain + 40);
    }

    #[test]
    fn an_anchor_table_before_a_picture_is_rejected() {
        let (table, reason) = rejection(|t| t.resources[7] = resource(3, "", 0, 0));
        assert_eq!(
            (table, reason),
            (RESOURCES, "an anchor table is not followed by its chapter")
        );
    }

    #[test]
    fn an_anchor_table_at_the_end_of_a_file_is_rejected() {
        let (_, reason) = rejection(|t| t.resources[8] = resource(3, "", 0, 0));
        assert_eq!(reason, "an anchor table is not followed by its chapter");
    }

    #[test]
    fn an_anchor_table_with_a_path_is_rejected() {
        let (_, reason) = rejection(|t| t.resources[5] = resource(3, "anchors", 0, 0));
        assert_eq!(
            reason,
            "a path is missing, or a row that takes none has one"
        );
    }

    #[test]
    fn an_anchor_table_with_a_size_is_rejected() {
        let (_, reason) = rejection(|t| t.resources[5] = resource(3, "", 1, 1));
        assert_eq!(reason, "a row that is not a picture has a size");
    }

    #[test]
    fn a_fragment_that_is_not_a_heading_is_left_to_the_anchor_table() {
        let index = with_anchor_table();
        let from = index.chapter_id(8).unwrap();
        let chapter = index.chapter_id(7).unwrap();
        assert_eq!(
            index.resolve(from, "ch01.md#fig%2D1"),
            Ok(Destination::ExtraAnchor(chapter))
        );
        assert_eq!(
            index.resolve_named(from, "ch01.md#fig%2D1"),
            Ok(Resolved::Extra {
                chapter,
                name: "fig-1".to_owned()
            })
        );
    }

    #[test]
    fn the_anchor_of_a_heading_needs_no_anchor_table() {
        let index = with_anchor_table();
        let from = index.chapter_id(8).unwrap();
        assert!(matches!(
            index.resolve(from, "ch01.md#the-station"),
            Ok(Destination::Chapter(position))
                if position.chapter.number() == 7 && position.offset == 130
        ));
    }

    #[test]
    fn a_fragment_into_a_chapter_without_an_anchor_table_is_unknown() {
        let index = with_anchor_table();
        let from = index.chapter_id(7).unwrap();
        assert_eq!(
            index.resolve(from, "ch02.md#fig-1"),
            Err(ResolveError::UnknownAnchor)
        );
    }

    #[test]
    fn extra_anchors_that_follow_the_rules_are_accepted() {
        let anchors = vec![extra(0, "top"), extra(22, "fig-1"), extra(22, "figure")];
        let checked = with_anchor_table().extra_anchors(7, 6, anchors.clone());
        assert_eq!(checked.unwrap(), anchors);
    }

    #[test]
    fn an_empty_anchor_table_is_accepted() {
        let checked = with_anchor_table().extra_anchors(7, 6, Vec::new());
        assert_eq!(checked.unwrap(), []);
    }

    #[test]
    fn an_extra_anchor_without_a_name_is_rejected() {
        assert_eq!(extra_rejection(vec![extra(0, "")]), "a name is empty");
    }

    #[test]
    fn an_extra_anchor_past_the_end_of_its_chapter_is_rejected() {
        assert_eq!(
            extra_rejection(vec![extra(279, "end")]),
            "an offset is past the end of its chapter"
        );
    }

    #[test]
    fn unsorted_extra_anchors_are_rejected() {
        assert_eq!(
            extra_rejection(vec![extra(22, "b"), extra(0, "a")]),
            "the rows are not sorted"
        );
    }

    #[test]
    fn an_extra_anchor_name_that_repeats_is_rejected() {
        assert_eq!(
            extra_rejection(vec![extra(0, "a"), extra(22, "a")]),
            "a name repeats in a chapter"
        );
    }

    #[test]
    fn an_extra_anchor_named_like_a_heading_is_rejected() {
        assert_eq!(
            extra_rejection(vec![extra(22, "the-station")]),
            "a name repeats in a chapter"
        );
    }

    #[test]
    fn extra_anchors_at_line_starts_pass_the_text_check() {
        let anchors = [extra(0, "top"), extra(4, "second")];
        assert!(Index::check_extra_positions(6, &anchors, "one\ntwo\n").is_ok());
    }

    #[test]
    fn an_extra_anchor_inside_a_line_fails_the_text_check() {
        let anchors = [extra(5, "inside")];
        assert!(matches!(
            Index::check_extra_positions(6, &anchors, "one\ntwo\n"),
            Err(Error::Malformed {
                resource: 6,
                reason: "a position is not at the start of a line"
            })
        ));
    }

    #[test]
    fn an_extra_anchor_at_the_end_of_the_text_fails_the_text_check() {
        let anchors = [extra(8, "end")];
        assert!(Index::check_extra_positions(6, &anchors, "one\ntwo\n").is_err());
    }

    #[test]
    fn empty_contents_and_anchors_are_accepted() {
        let mut tables = tables();
        tables.toc.clear();
        tables.anchors.clear();
        let index = Index::build(VERSION, tables, &LENS).unwrap();
        assert!(index.toc().is_empty() && index.anchors(6).is_empty());
    }
}
