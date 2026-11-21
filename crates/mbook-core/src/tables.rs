//! The four tables as plain rows, and their encoding as typed columns.
//!
//! A table is one frame with several typed inputs; each input is a column and
//! each element a row. This module only moves rows in and out of columns. The
//! rules that connect rows are checked by `index` (when reading) and by
//! `Book::check` (when writing).

/// One row of the `resources` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResourceRow {
    pub(crate) kind: u8,
    pub(crate) path: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// One row of the `toc` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TocRow {
    pub(crate) level: u8,
    pub(crate) title: String,
    pub(crate) resource: u32,
    pub(crate) offset: u32,
}

/// One row of the `anchors` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnchorRow {
    pub(crate) resource: u32,
    pub(crate) offset: u32,
    pub(crate) name: String,
}

/// The rows of resources 1 to 4.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tables {
    pub(crate) meta: Vec<(String, String)>,
    pub(crate) resources: Vec<ResourceRow>,
    pub(crate) toc: Vec<TocRow>,
    pub(crate) anchors: Vec<AnchorRow>,
}

#[cfg(feature = "read")]
pub(crate) use decode::{anchors, meta, resources, toc};
#[cfg(feature = "write")]
pub(crate) use encode::Column;

/// Rows out of decoded columns.
#[cfg(feature = "read")]
mod decode {
    use super::{AnchorRow, ResourceRow, TocRow};
    use mopenzl::{DataType, TypedOutput};

    /// Why a table cannot be read; the caller adds the resource number.
    type Reason = &'static str;

    /// The columns of one table, checked to have one length.
    struct Columns<'a> {
        outputs: &'a [TypedOutput],
        rows: usize,
    }

    impl<'a> Columns<'a> {
        /// Takes the first `defined` outputs as the columns of a table. Later
        /// outputs are columns of a newer minor version and are ignored.
        fn new(outputs: &'a [TypedOutput], defined: usize) -> Result<Self, Reason> {
            let outputs = outputs.get(..defined).ok_or("a column is missing")?;
            let rows = outputs.first().map_or(0, TypedOutput::len);
            if outputs.iter().any(|column| column.len() != rows) {
                return Err("the columns differ in length");
            }
            Ok(Self { outputs, rows })
        }

        fn numeric(&self, index: usize, width: usize) -> Result<&'a [u8], Reason> {
            let column = self.outputs.get(index).ok_or("a column is missing")?;
            // A frame does not record the width of an empty numeric output.
            let width_matches = column.width() == width || column.is_empty();
            if column.data_type() != DataType::Numeric || !width_matches {
                return Err("a numeric column has another type or width");
            }
            Ok(column.content())
        }

        fn bytes(&self, index: usize) -> Result<&'a [u8], Reason> {
            self.numeric(index, 1)
        }

        fn words(&self, index: usize) -> Result<Vec<u32>, Reason> {
            let content = self.numeric(index, 4)?;
            let (words, _) = content.as_chunks::<4>();
            Ok(words.iter().map(|word| u32::from_le_bytes(*word)).collect())
        }

        fn strings(&self, index: usize) -> Result<Vec<&'a str>, Reason> {
            let column = self.outputs.get(index).ok_or("a column is missing")?;
            if column.data_type() != DataType::String {
                return Err("a string column has another type");
            }
            let mut rest = column.content();
            let mut strings = Vec::with_capacity(self.rows);
            for &len in column.string_lens() {
                let len = usize::try_from(len).map_err(|_| "a string is too long")?;
                let (bytes, tail) = rest
                    .split_at_checked(len)
                    .ok_or("the string lengths exceed the content")?;
                strings.push(std::str::from_utf8(bytes).map_err(|_| "a string is not UTF-8")?);
                rest = tail;
            }
            if !rest.is_empty() {
                return Err("the string lengths do not cover the content");
            }
            Ok(strings)
        }
    }

    /// The rows of the `meta` table: key, value.
    pub(crate) fn meta(outputs: &[TypedOutput]) -> Result<Vec<(String, String)>, Reason> {
        let columns = Columns::new(outputs, 2)?;
        let keys = columns.strings(0)?;
        let values = columns.strings(1)?;
        Ok(keys
            .into_iter()
            .zip(values)
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect())
    }

    /// The rows of the `resources` table: kind, path, width, height.
    pub(crate) fn resources(outputs: &[TypedOutput]) -> Result<Vec<ResourceRow>, Reason> {
        let columns = Columns::new(outputs, 4)?;
        let kinds = columns.bytes(0)?;
        let paths = columns.strings(1)?;
        let widths = columns.words(2)?;
        let heights = columns.words(3)?;
        Ok(kinds
            .iter()
            .zip(paths)
            .zip(widths.into_iter().zip(heights))
            .map(|((&kind, path), (width, height))| ResourceRow {
                kind,
                path: path.to_owned(),
                width,
                height,
            })
            .collect())
    }

    /// The rows of the `toc` table: level, title, resource, offset.
    pub(crate) fn toc(outputs: &[TypedOutput]) -> Result<Vec<TocRow>, Reason> {
        let columns = Columns::new(outputs, 4)?;
        let levels = columns.bytes(0)?;
        let titles = columns.strings(1)?;
        let resources = columns.words(2)?;
        let offsets = columns.words(3)?;
        Ok(levels
            .iter()
            .zip(titles)
            .zip(resources.into_iter().zip(offsets))
            .map(|((&level, title), (resource, offset))| TocRow {
                level,
                title: title.to_owned(),
                resource,
                offset,
            })
            .collect())
    }

    /// The rows of the `anchors` table: resource, offset, name.
    pub(crate) fn anchors(outputs: &[TypedOutput]) -> Result<Vec<AnchorRow>, Reason> {
        let columns = Columns::new(outputs, 3)?;
        let resources = columns.words(0)?;
        let offsets = columns.words(1)?;
        let names = columns.strings(2)?;
        Ok(resources
            .into_iter()
            .zip(offsets)
            .zip(names)
            .map(|((resource, offset), name)| AnchorRow {
                resource,
                offset,
                name: name.to_owned(),
            })
            .collect())
    }
}

/// Rows into columns ready to be compressed.
#[cfg(feature = "write")]
mod encode {
    use super::{AnchorRow, ResourceRow, Tables, TocRow};
    use crate::error::{Error, Result};
    use bytes::BufMut;
    use mopenzl::input::{Input, InputError, Numbers, Strings};

    /// One column of a table, owning its bytes.
    #[derive(Debug)]
    pub(crate) enum Column {
        /// A `u8` column.
        Bytes(Vec<u8>),
        /// A `u32` column, already little-endian.
        Words(Vec<u8>),
        /// A string column: concatenated contents and one length each.
        Strings { content: Vec<u8>, lens: Vec<u32> },
    }

    impl Column {
        fn bytes<T>(rows: &[T], value: impl Fn(&T) -> u8) -> Self {
            Self::Bytes(rows.iter().map(value).collect())
        }

        fn words<T>(rows: &[T], value: impl Fn(&T) -> u32) -> Self {
            let mut bytes = Vec::with_capacity(rows.len() * 4);
            for row in rows {
                bytes.put_u32_le(value(row));
            }
            Self::Words(bytes)
        }

        /// A string column. A string of 4 GiB or more cannot be stored.
        fn strings<'a, T: 'a>(rows: &'a [T], value: impl Fn(&'a T) -> &'a str) -> Result<Self> {
            let mut content = Vec::new();
            let mut lens = Vec::with_capacity(rows.len());
            for row in rows {
                let text = value(row);
                content.put_slice(text.as_bytes());
                let len = u32::try_from(text.len());
                lens.push(len.map_err(|_| Error::Limit("a string in a table is too long"))?);
            }
            Ok(Self::Strings { content, lens })
        }

        /// The column as an input of a frame.
        pub(crate) fn input(&self) -> std::result::Result<Input<'_>, InputError> {
            Ok(match self {
                Self::Bytes(bytes) => Numbers::try_from((bytes.as_slice(), 1))?.into(),
                Self::Words(bytes) => Numbers::try_from((bytes.as_slice(), 4))?.into(),
                Self::Strings { content, lens } => {
                    Strings::try_from((content.as_slice(), lens.as_slice()))?.into()
                }
            })
        }
    }

    impl Tables {
        /// The columns of the `meta` table.
        pub(crate) fn meta_columns(&self) -> Result<Vec<Column>> {
            Ok(vec![
                Column::strings(&self.meta, |(key, _)| key.as_str())?,
                Column::strings(&self.meta, |(_, value)| value.as_str())?,
            ])
        }

        /// The columns of the `resources` table.
        pub(crate) fn resource_columns(&self) -> Result<Vec<Column>> {
            let rows = &self.resources;
            Ok(vec![
                Column::bytes(rows, |row: &ResourceRow| row.kind),
                Column::strings(rows, |row| row.path.as_str())?,
                Column::words(rows, |row| row.width),
                Column::words(rows, |row| row.height),
            ])
        }

        /// The columns of the `toc` table.
        pub(crate) fn toc_columns(&self) -> Result<Vec<Column>> {
            let rows = &self.toc;
            Ok(vec![
                Column::bytes(rows, |row: &TocRow| row.level),
                Column::strings(rows, |row| row.title.as_str())?,
                Column::words(rows, |row| row.resource),
                Column::words(rows, |row| row.offset),
            ])
        }

        /// The columns of the `anchors` table.
        pub(crate) fn anchor_columns(&self) -> Result<Vec<Column>> {
            let rows = &self.anchors;
            Ok(vec![
                Column::words(rows, |row: &AnchorRow| row.resource),
                Column::words(rows, |row| row.offset),
                Column::strings(rows, |row| row.name.as_str())?,
            ])
        }
    }
}
