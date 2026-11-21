//! Options and limits of a conversion.

use crate::format::Format;
use mbook_core::{Limits, WriteOptions};

/// How a source is converted. Start from [`ConvertOptions::default`] and
/// change the fields that matter.
///
/// # Examples
///
/// ```
/// use mbook_converter::{ConvertOptions, Format};
///
/// let mut options = ConvertOptions::default();
/// options.format = Some(Format::Fb2);
/// options.pictures.quality = 60;
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConvertOptions {
    /// The format of the source. `None`, the default, detects it from the
    /// content.
    pub format: Option<Format>,
    /// How pictures are encoded.
    pub pictures: PictureOptions,
    /// Bounds on what a source may ask for.
    pub limits: ConvertLimits,
    /// How the mbook file is written by [`Converter::convert`] and
    /// [`Converter::convert_slice`].
    ///
    /// [`Converter::convert`]: crate::Converter::convert
    /// [`Converter::convert_slice`]: crate::Converter::convert_slice
    pub write: WriteOptions,
}

/// How pictures are encoded as AVIF. A value outside its range is brought
/// into the range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PictureOptions {
    /// Quality from 1 to 100. The default is 80.
    pub quality: u8,
    /// Speed from 1, the smallest file, to 10, the fastest. The default is 6.
    pub speed: u8,
}

impl Default for PictureOptions {
    fn default() -> Self {
        Self {
            quality: 80,
            speed: 6,
        }
    }
}

/// Bounds applied before memory is reserved for something a source declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConvertLimits {
    /// The largest source. The default is 2 GiB.
    pub max_source_bytes: u64,
    /// The largest picture, as width times height. The default is
    /// 100,000,000 pixels.
    pub max_picture_pixels: u64,
    /// The limits of the converted book. `max_resource_bytes` also bounds
    /// one unpacked entry of an archive, and `max_file_bytes` everything
    /// unpacked together.
    pub book: Limits,
}

impl Default for ConvertLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 2 << 30,
            max_picture_pixels: 100_000_000,
            book: Limits::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ConvertLimits, ConvertOptions, PictureOptions};

    #[test]
    fn the_defaults_match_the_converter_rfc() {
        let options = ConvertOptions::default();
        assert_eq!(options.format, None);
        assert_eq!((options.pictures.quality, options.pictures.speed), (80, 6));
        assert_eq!(
            (
                options.limits.max_source_bytes,
                options.limits.max_picture_pixels
            ),
            (2 << 30, 100_000_000)
        );
        assert_eq!(options.limits.book, mbook_core::Limits::default());
    }

    #[test]
    fn options_are_changed_by_assigning_fields() {
        let mut options = ConvertOptions::default();
        options.pictures.quality = 60;
        options.limits.max_source_bytes = 1;
        assert_eq!(
            (options.pictures, options.limits.max_source_bytes),
            (
                PictureOptions {
                    quality: 60,
                    speed: 6
                },
                1
            )
        );
        assert_eq!(options.limits.book, ConvertLimits::default().book);
    }
}
