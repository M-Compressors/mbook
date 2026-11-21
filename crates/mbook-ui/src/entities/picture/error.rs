//! The error of this slice.

use std::error::Error;
use thiserror::Error;

/// Why a picture could not be decoded, scaled or encoded.
///
/// A message does not repeat its source; the source is reached through
/// [`Error::source`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub(crate) enum PictureError {
    /// The bytes are not a picture that can be decoded: damaged, truncated, or
    /// not a picture at all.
    #[error("the bytes are not a picture that can be decoded")]
    Undecodable(#[source] Box<dyn Error + Send + Sync>),
    /// The picture has more pixels than the limit allows.
    #[error("the picture has {pixels} pixels, more than the {limit} allowed")]
    TooManyPixels {
        /// The pixels the picture declares or has.
        pixels: u64,
        /// The limit.
        limit: u64,
    },
    /// The picture is in a format that is not read: AVIF, JPEG, PNG, GIF,
    /// WebP and BMP are.
    #[error("the picture is in a format that is not supported")]
    UnsupportedFormat,
    /// The picture was decoded but could not be written in the target format.
    #[error("the picture could not be encoded")]
    Encoding(#[source] Box<dyn Error + Send + Sync>),
}

impl PictureError {
    /// An undecodable picture, with the error of the decoder as its source.
    pub(super) fn undecodable(source: impl Error + Send + Sync + 'static) -> Self {
        Self::Undecodable(Box::new(source))
    }

    /// A failed encoding, with the error of the encoder as its source.
    pub(super) fn encoding(source: impl Error + Send + Sync + 'static) -> Self {
        Self::Encoding(Box::new(source))
    }
}

/// Fails with [`PictureError::TooManyPixels`] when `width` by `height` is more
/// than `limit`.
pub(super) fn check_pixels(width: u32, height: u32, limit: u64) -> Result<(), PictureError> {
    let pixels = u64::from(width) * u64::from(height);
    if pixels > limit {
        Err(PictureError::TooManyPixels { pixels, limit })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{PictureError, check_pixels};
    use std::error::Error as _;
    use std::io;

    #[test]
    fn undecodable_message_leaves_the_cause_to_its_source() {
        let error = PictureError::undecodable(io::Error::other("bad box"));
        assert_eq!(
            error.to_string(),
            "the bytes are not a picture that can be decoded"
        );
        assert_eq!(
            error.source().map(ToString::to_string).as_deref(),
            Some("bad box")
        );
    }

    #[test]
    fn encoding_message_leaves_the_cause_to_its_source() {
        let error = PictureError::encoding(io::Error::other("no memory"));
        assert_eq!(error.to_string(), "the picture could not be encoded");
        assert_eq!(
            error.source().map(ToString::to_string).as_deref(),
            Some("no memory")
        );
    }

    #[test]
    fn too_many_pixels_message_has_the_count_and_the_limit() {
        let error = PictureError::TooManyPixels {
            pixels: 12,
            limit: 10,
        };
        assert_eq!(
            error.to_string(),
            "the picture has 12 pixels, more than the 10 allowed"
        );
        assert!(error.source().is_none());
    }

    #[test]
    fn unsupported_format_has_no_source() {
        let error = PictureError::UnsupportedFormat;
        assert_eq!(
            error.to_string(),
            "the picture is in a format that is not supported"
        );
        assert!(error.source().is_none());
    }

    #[test]
    fn pixel_count_equal_to_the_limit_is_accepted() {
        assert!(check_pixels(4, 5, 20).is_ok());
    }

    #[test]
    fn pixel_count_above_the_limit_is_refused_with_both_numbers() {
        let error = check_pixels(4, 5, 19).unwrap_err();
        assert!(matches!(
            error,
            PictureError::TooManyPixels {
                pixels: 20,
                limit: 19
            }
        ));
    }

    #[test]
    fn pixel_count_does_not_overflow_for_the_largest_sides() {
        assert!(check_pixels(u32::MAX, u32::MAX, u64::MAX).is_ok());
    }
}
