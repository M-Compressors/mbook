//! The entry points of the crate.

use crate::assemble;
use crate::error::{Error, Result};
use crate::format::Format;
use crate::formats;
use crate::options::ConvertOptions;
use crate::report::{Converted, Report};
use std::io::{Read, Write};

/// Converts books in other formats into mbook.
///
/// A converter holds only its options: one value serves any number of
/// conversions, from any number of threads.
///
/// | | To an mbook file | To a [`Book`](mbook_core::Book) |
/// | --- | --- | --- |
/// | From any [`Read`] | [`convert`](Self::convert) | [`load`](Self::load) |
/// | From a byte slice | [`convert_slice`](Self::convert_slice) | [`load_slice`](Self::load_slice) |
///
/// # Examples
///
/// ```no_run
/// use mbook_converter::Converter;
///
/// let source = std::fs::read("book.fb2")?;
/// let mut mbook = Vec::new();
/// let report = Converter::new().convert_slice(&source, &mut mbook)?;
/// println!("{} warning(s)", report.warnings.len());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Converter {
    options: ConvertOptions,
}

impl Converter {
    /// A converter with the default options: the format is detected from
    /// the content of each source.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A converter with the given options.
    #[must_use]
    pub fn with_options(options: ConvertOptions) -> Self {
        Self { options }
    }

    /// Converts a source into an mbook file.
    ///
    /// The source is read to its end before anything is converted, and
    /// nothing is written unless the whole conversion succeeded. Neither the
    /// source nor the sink has to seek.
    ///
    /// # Errors
    ///
    /// As [`load`](Self::load), or the sink fails.
    pub fn convert(&self, source: impl Read, sink: impl Write) -> Result<Report> {
        let converted = self.load(source)?;
        self.write(converted, sink)
    }

    /// Converts a source that is in memory into an mbook file.
    ///
    /// # Errors
    ///
    /// As [`load_slice`](Self::load_slice), or the sink fails.
    pub fn convert_slice(&self, source: &[u8], sink: impl Write) -> Result<Report> {
        let converted = self.load_slice(source)?;
        self.write(converted, sink)
    }

    /// Converts a source into a book that can still be changed.
    ///
    /// # Errors
    ///
    /// The source fails or is larger than the limit; otherwise as
    /// [`load_slice`](Self::load_slice).
    pub fn load(&self, source: impl Read) -> Result<Converted> {
        let limit = self.options.limits.max_source_bytes;
        let mut bytes = Vec::new();
        // One byte more than the limit is enough to know it is exceeded.
        source
            .take(limit.saturating_add(1))
            .read_to_end(&mut bytes)?;
        self.load_slice(&bytes)
    }

    /// Converts a source that is in memory into a book that can still be
    /// changed.
    ///
    /// # Errors
    ///
    /// The format is not recognised, or its feature is not enabled; the
    /// source cannot be parsed as its format, is protected, or holds nothing
    /// that could be converted; a limit of the options is exceeded.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(feature = "txt")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use mbook_converter::{ConvertOptions, Converter, Format};
    ///
    /// let mut options = ConvertOptions::default();
    /// options.format = Some(Format::Text);
    ///
    /// let mut converted = Converter::with_options(options).load_slice(b"Call me Ishmael.")?;
    /// converted.book.meta_mut().set_title("Moby-Dick")?;
    /// assert_eq!(converted.book.chapters().len(), 1);
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "txt"))]
    /// # fn main() {}
    /// ```
    pub fn load_slice(&self, source: &[u8]) -> Result<Converted> {
        let fits = u64::try_from(source.len())
            .is_ok_and(|len| len <= self.options.limits.max_source_bytes);
        if !fits {
            return Err(Error::Limit("the source is larger than the source limit"));
        }
        let format = self
            .options
            .format
            .or_else(|| Format::detect(source))
            .ok_or(Error::UnknownFormat)?;
        let draft = formats::read(format, source, &self.options)?;
        let (book, warnings) = assemble::assemble(draft, source, &self.options)?;
        Ok(Converted {
            book,
            report: Report { format, warnings },
        })
    }

    /// Writes a converted book and hands its report on.
    fn write(&self, converted: Converted, sink: impl Write) -> Result<Report> {
        converted.book.write_with(sink, &self.options.write)?;
        Ok(converted.report)
    }
}
