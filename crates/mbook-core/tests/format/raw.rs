//! Containers built by hand with mopenzl, to give the reader files that the
//! writer of this crate never produces.

use crate::texts::{CH01, CH02, ID, PLAIN, ROTATED, station_offset};
use mopenzl::framing::{FramedCompressor, FramedResource, ResourceInput};
use mopenzl::graph::{CompressorGraph, StandardGraph};
use mopenzl::input::{Input, Numbers, Strings};

/// The result of building a container.
pub(crate) type Built = Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>;

/// One column of a table.
#[derive(Clone, Debug)]
pub(crate) enum Column {
    Bytes(Vec<u8>),
    Words(Vec<u32>),
    Text(Vec<String>),
}

impl Column {
    pub(crate) fn text(values: &[&str]) -> Self {
        Self::Text(values.iter().map(|value| (*value).to_owned()).collect())
    }

    /// The values of a `u8` column, to change them in place.
    pub(crate) fn bytes_mut(&mut self) -> &mut Vec<u8> {
        match self {
            Self::Bytes(values) => values,
            _ => panic!("not a u8 column"),
        }
    }

    /// The values of a `u32` column, to change them in place.
    pub(crate) fn words_mut(&mut self) -> &mut Vec<u32> {
        match self {
            Self::Words(values) => values,
            _ => panic!("not a u32 column"),
        }
    }

    /// The values of a string column, to change them in place.
    pub(crate) fn text_mut(&mut self) -> &mut Vec<String> {
        match self {
            Self::Text(values) => values,
            _ => panic!("not a string column"),
        }
    }
}

/// The decoded content of one resource after the tables.
#[derive(Clone, Debug)]
pub(crate) enum Content {
    /// A chapter or a picture.
    Serial(Vec<u8>),
    /// The anchor table of a chapter.
    Table(Vec<Column>),
}

impl From<Vec<u8>> for Content {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Serial(bytes)
    }
}

/// The parts of a file, each open to tampering.
#[derive(Clone, Debug)]
pub(crate) struct Raw {
    /// The decoded content of resource 0.
    pub(crate) header: Vec<u8>,
    /// The columns of resources 1 to 4: meta, resources, toc, anchors.
    pub(crate) tables: Vec<Vec<Column>>,
    /// The decoded content of every resource after the tables.
    pub(crate) contents: Vec<Content>,
}

pub(crate) const META: usize = 0;
pub(crate) const RESOURCES: usize = 1;
pub(crate) const TOC: usize = 2;
pub(crate) const ANCHORS: usize = 3;

impl Raw {
    /// The example book of RFC 0001.
    pub(crate) fn example() -> Self {
        let station = station_offset();
        Self {
            header: b"MBOOK\0\x01\x00".to_vec(),
            tables: vec![
                vec![
                    Column::text(&["title", "lang", "id", "author", "cover"]),
                    Column::text(&["The Example", "en", ID, "A. Writer", "img/cover.avif"]),
                ],
                vec![
                    Column::Bytes(vec![0, 0, 0, 0, 0, 2, 1, 1, 2]),
                    Column::text(&[
                        "",
                        "",
                        "",
                        "",
                        "",
                        "img/cover.avif",
                        "ch01.md",
                        "ch02.md",
                        "img/fig-1.avif",
                    ]),
                    Column::Words(vec![0, 0, 0, 0, 0, 24, 0, 0, 16]),
                    Column::Words(vec![0, 0, 0, 0, 0, 16, 0, 0, 24]),
                ],
                vec![
                    Column::Bytes(vec![1, 2, 1]),
                    Column::text(&["Chapter 1. Arrival", "The station", "Chapter 2. The letter"]),
                    Column::Words(vec![6, 6, 7]),
                    Column::Words(vec![0, station, 0]),
                ],
                vec![
                    Column::Words(vec![6, 6, 7]),
                    Column::Words(vec![0, station, 0]),
                    Column::text(&["chapter-1-arrival", "the-station", "chapter-2-the-letter"]),
                ],
            ],
            contents: vec![
                PLAIN.to_vec().into(),
                CH01.as_bytes().to_vec().into(),
                CH02.as_bytes().to_vec().into(),
                ROTATED.to_vec().into(),
            ],
        }
    }

    /// Changes the file and gives it back.
    pub(crate) fn with(mut self, change: impl FnOnce(&mut Self)) -> Self {
        change(&mut self);
        self
    }

    /// Appends a resource and its row of the resource table.
    pub(crate) fn push_resource(&mut self, kind: u8, path: &str, content: &[u8]) {
        let table = &mut self.tables[RESOURCES];
        table[0].bytes_mut().push(kind);
        table[1].text_mut().push(path.to_owned());
        table[2].words_mut().push(0);
        table[3].words_mut().push(0);
        self.contents.push(content.to_vec().into());
    }

    /// Puts an anchor table in front of resource `before`, which makes that
    /// resource and every later one move up by one. The numbers stored in
    /// the contents and in the anchors of headings follow.
    pub(crate) fn insert_anchor_table(&mut self, before: usize, offsets: &[u32], names: &[&str]) {
        let table = &mut self.tables[RESOURCES];
        table[0].bytes_mut().insert(before, 3);
        table[1].text_mut().insert(before, String::new());
        table[2].words_mut().insert(before, 0);
        table[3].words_mut().insert(before, 0);
        let moved = u32::try_from(before).unwrap_or(u32::MAX);
        for (table, column) in [(TOC, 2), (ANCHORS, 0)] {
            for number in self.tables[table][column].words_mut() {
                *number += u32::from(*number >= moved);
            }
        }
        let columns = vec![Column::Words(offsets.to_vec()), Column::text(names)];
        self.contents.insert(before - 5, Content::Table(columns));
    }

    /// The columns of the anchor table that is resource `number`.
    pub(crate) fn anchor_table_mut(&mut self, number: usize) -> &mut Vec<Column> {
        match &mut self.contents[number - 5] {
            Content::Table(columns) => columns,
            Content::Serial(_) => panic!("not a table"),
        }
    }

    /// Encodes the parts as one container.
    pub(crate) fn build(&self) -> Built {
        let store = CompressorGraph::standard(StandardGraph::Store);
        let mut compressor =
            FramedCompressor::new(mopenzl::framing::FramedEncodeConfig::default())?;
        let mut writer = compressor.framed_writer(Vec::new())?;
        writer.resource(FramedResource {
            input: ResourceInput::Serial(&self.header),
            graph: Some(&store),
            metadata: &[],
        })?;
        let tables = self
            .tables
            .iter()
            .map(|table| Content::Table(table.clone()));
        for content in tables.chain(self.contents.iter().cloned()) {
            match content {
                Content::Serial(bytes) => {
                    writer.resource(FramedResource::from(bytes.as_slice()))?;
                }
                Content::Table(columns) => {
                    let encoded: Vec<Encoded> = columns.iter().map(Encoded::from).collect();
                    let inputs = encoded
                        .iter()
                        .map(Encoded::input)
                        .collect::<Result<Vec<_>, _>>()?;
                    writer.resource(FramedResource::from(inputs.as_slice()))?;
                }
            }
        }
        Ok(writer.finish().map_err(|failure| failure.error)?)
    }
}

/// A column as the bytes mopenzl takes.
struct Encoded {
    content: Vec<u8>,
    lens: Option<Vec<u32>>,
    width: usize,
}

impl From<&Column> for Encoded {
    fn from(column: &Column) -> Self {
        match column {
            Column::Bytes(values) => Self {
                content: values.clone(),
                lens: None,
                width: 1,
            },
            Column::Words(values) => Self {
                content: values
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .collect(),
                lens: None,
                width: 4,
            },
            Column::Text(values) => Self {
                content: values.concat().into_bytes(),
                lens: Some(
                    values
                        .iter()
                        .map(|value| u32::try_from(value.len()).unwrap_or(u32::MAX))
                        .collect(),
                ),
                width: 1,
            },
        }
    }
}

impl Encoded {
    fn input(&self) -> Result<Input<'_>, mopenzl::input::InputError> {
        Ok(match &self.lens {
            Some(lens) => Strings::try_from((self.content.as_slice(), lens.as_slice()))?.into(),
            None => Numbers::try_from((self.content.as_slice(), self.width))?.into(),
        })
    }
}
