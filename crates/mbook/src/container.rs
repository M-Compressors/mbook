//! The one place that talks to mopenzl: configuration from [`Limits`], error
//! mapping, and the shapes a resource may decode to.

use crate::error::Error;
use crate::options::Limits;
use mopenzl::framing::{FramedError, FramingConfig};

/// Room for the record and frame headers around a stored resource.
const FRAME_OVERHEAD: u64 = 1 << 20;

/// Turns a container failure into an error of this crate: transport and limit
/// failures keep their own variants, everything else is damage.
pub(crate) fn error(error: FramedError) -> Error {
    match error {
        FramedError::Io(error) => Error::Io(error),
        FramedError::Limit(limit) => Error::Limit(limit),
        #[cfg(feature = "read")]
        FramedError::Decode(
            mopenzl::DecodeError::OutputLimitExceeded { .. }
            | mopenzl::DecodeError::InputLimitExceeded { .. },
        ) => Error::Limit("a resource exceeds the resource limit"),
        other => Error::Container(Box::new(other)),
    }
}

/// The container limits that follow from [`Limits`].
///
/// `total` bounds the decoded size of all resources together: unbounded for a
/// reader that decodes one resource at a time, and the file limit for a book
/// that is held in memory whole.
fn framing(limits: &Limits, total: u64) -> FramingConfig {
    let frame = limits.max_resource_bytes.saturating_add(FRAME_OVERHEAD);
    FramingConfig {
        max_resources: limits.max_resources,
        max_frame_bytes: usize::try_from(frame).unwrap_or(usize::MAX),
        max_output_bytes: total,
        max_container_bytes: limits.max_file_bytes,
        ..FramingConfig::default()
    }
}

#[cfg(feature = "read")]
pub(crate) use read::{decoder, header, serial};
#[cfg(feature = "write")]
pub(crate) use write::{Writer, compressor};

#[cfg(feature = "read")]
mod read {
    use super::{error, framing};
    use crate::error::{Error, Result};
    use crate::format::{self, FORMAT};
    use crate::options::Limits;
    use crate::types::Version;
    use mopenzl::framing::{FramedDecodeConfig, FramedDecompressor};
    use mopenzl::{DataType, DecodeLimits, DecoderConfig, TypedOutput};

    /// A decoder bounded by `limits`; see [`framing`] for `total`.
    pub(crate) fn decoder(limits: &Limits, total: u64) -> Result<FramedDecompressor> {
        let one_resource =
            DecodeLimits::DEFAULT.with_max_output_bytes(Some(limits.max_resource_bytes));
        FramedDecompressor::new(FramedDecodeConfig {
            decoder: DecoderConfig::DEFAULT.with_limits(one_resource),
            framing: framing(limits, total),
        })
        .map_err(error)
    }

    /// The content of resource `number`, which must have decoded to one
    /// serial output, as every chapter and picture does.
    pub(crate) fn serial(number: u32, outputs: Vec<TypedOutput>) -> Result<Vec<u8>> {
        let mut outputs = outputs.into_iter();
        outputs
            .next()
            .filter(|only| outputs.next().is_none() && only.data_type() == DataType::Serial)
            .map(TypedOutput::into_content)
            .ok_or(Error::Malformed {
                resource: number,
                reason: "the resource is not one serial output",
            })
    }

    /// The version declared by resource 0, when it is a format header of a
    /// version this crate reads.
    pub(crate) fn header(outputs: &[TypedOutput]) -> Result<Version> {
        let version = match outputs {
            [only] if only.data_type() == DataType::Serial => format::parse_header(only.content()),
            _ => None,
        }
        .ok_or(Error::NotMbook)?;
        if version.major == 0 {
            return Err(Error::Malformed {
                resource: FORMAT,
                reason: "major version 0 is not defined",
            });
        }
        if !format::is_supported(version) {
            return Err(Error::UnsupportedVersion(version));
        }
        Ok(version)
    }
}

#[cfg(feature = "write")]
mod write {
    use super::{error, framing};
    use crate::error::{Error, Result};
    use crate::format;
    use crate::options::WriteOptions;
    use crate::tables::Column;
    use mopenzl::framing::{
        FramedCompressor, FramedEncodeConfig, FramedResource, FramedWriter, ResourceInput,
    };
    use mopenzl::graph::{CompressorGraph, StandardGraph};
    use mopenzl::{EncoderConfig, FormatVersion, Level};
    use std::io::Write;

    /// The `OpenZL` format version RFC 0001 fixes for every frame.
    const FRAME_FORMAT: u32 = 27;

    /// A compressor configured as RFC 0001 requires.
    ///
    /// Both frame checksums stay enabled, as the defaults have them. A level
    /// outside the range of the compressor is brought into it.
    pub(crate) fn compressor(options: &WriteOptions) -> Result<FramedCompressor> {
        let level = options.level.map_or(Level::DEFAULT, |level| {
            let clamped = level.clamp(Level::MIN.get(), Level::MAX.get());
            Level::try_from(clamped).unwrap_or(Level::DEFAULT)
        });
        let version = FormatVersion::try_from(FRAME_FORMAT)
            .map_err(|unsupported| Error::Container(Box::new(unsupported)))?;
        FramedCompressor::new(FramedEncodeConfig {
            encoder: EncoderConfig::DEFAULT
                .with_level(level)
                .with_format_version(version),
            framing: framing(&options.limits, u64::MAX),
        })
        .map_err(error)
    }

    /// Writes the resources of one book in order.
    pub(crate) struct Writer<'a, W> {
        inner: FramedWriter<'a, W>,
        store: CompressorGraph,
    }

    impl<'a, W: Write> Writer<'a, W> {
        /// Starts a container on `sink`.
        pub(crate) fn new(compressor: &'a mut FramedCompressor, sink: W) -> Result<Self> {
            Ok(Self {
                inner: compressor.framed_writer(sink).map_err(error)?,
                store: CompressorGraph::standard(StandardGraph::Store),
            })
        }

        /// Resource 0, encoded so that its bytes land at fixed file offsets.
        pub(crate) fn header(&mut self) -> Result<()> {
            self.stored(&format::HEADER)
        }

        /// A table: one frame with a typed input per column.
        pub(crate) fn table(&mut self, columns: &[Column]) -> Result<()> {
            let inputs = columns
                .iter()
                .map(Column::input)
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|invalid| Error::Container(Box::new(invalid)))?;
            self.inner
                .resource(FramedResource::from(inputs.as_slice()))
                .map_err(error)
        }

        /// A chapter, a diagram or a formula, compressed with the default
        /// graph.
        pub(crate) fn text(&mut self, text: &str) -> Result<()> {
            self.inner
                .resource(FramedResource::from(text.as_bytes()))
                .map_err(error)
        }

        /// Bytes that are stored as they are: the header and pictures, which
        /// are already entropy-coded.
        pub(crate) fn stored(&mut self, bytes: &[u8]) -> Result<()> {
            self.inner
                .resource(FramedResource {
                    input: ResourceInput::Serial(bytes),
                    graph: Some(&self.store),
                    metadata: &[],
                })
                .map_err(error)
        }

        /// Writes the directory and the footer, and flushes the sink.
        pub(crate) fn finish(self) -> Result<()> {
            self.inner
                .finish()
                .map(drop)
                .map_err(|failure| error(failure.error))
        }
    }
}
