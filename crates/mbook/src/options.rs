//! Limits and options for reading and writing.

/// Bounds applied before memory is allocated from lengths found in a file.
///
/// The struct is non-exhaustive: start from [`Limits::default`] and change
/// the fields that matter.
///
/// # Examples
///
/// ```
/// let mut limits = mbook::Limits::default();
/// limits.max_file_bytes = 1 << 30;
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// The largest number of resources in one file. The default is 1,000,000.
    pub max_resources: usize,
    /// The largest decoded size of one chapter or picture. The default is
    /// 64 MiB.
    pub max_resource_bytes: u64,
    /// The largest file. The default is 16 GiB.
    pub max_file_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_resources: 1_000_000,
            max_resource_bytes: 64 << 20,
            max_file_bytes: 16 << 30,
        }
    }
}

/// How a book is read. Start from [`ReadOptions::default`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReadOptions {
    /// Bounds on what a file may ask for.
    pub limits: Limits,
}

/// How a book is written. Start from [`WriteOptions::default`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct WriteOptions {
    /// The compression level for tables and chapters; `None` uses the
    /// default of the compressor. Pictures are always stored.
    pub level: Option<i32>,
    /// Bounds the written file must stay within.
    pub limits: Limits,
}
