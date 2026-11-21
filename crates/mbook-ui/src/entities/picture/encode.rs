//! Encoding the pictures that are added to a book as AVIF.
//!
//! The rules are those of the converter of this repository: the pixel count is
//! compared with the limit before decoding, the orientation tag is applied,
//! the first frame of an animation is kept, and a picture has transparency in
//! the result only if it had it.

use super::decode::decode_avif;
use super::error::{PictureError, check_pixels};
use image::codecs::avif::AvifEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use std::io::Cursor;

/// The image formats that are decoded and converted to AVIF.
const DECODE_FORMATS: [ImageFormat; 5] = [
    ImageFormat::Jpeg,
    ImageFormat::Png,
    ImageFormat::Gif,
    ImageFormat::WebP,
    ImageFormat::Bmp,
];

/// File extensions offered by the picture picker, including AVIF passthrough.
pub(crate) fn picture_file_extensions() -> Vec<&'static str> {
    std::iter::once("avif")
        .chain(
            DECODE_FORMATS
                .into_iter()
                .flat_map(|format| format.extensions_str().iter().copied()),
        )
        .collect()
}

/// How a picture is encoded, and how large a picture may be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EncodeOptions {
    /// Quality, 1 to 100; a value outside is brought into the range.
    pub(crate) quality: u8,
    /// Speed of the encoder, 1 (slowest, smallest) to 10; a value outside is
    /// brought into the range.
    pub(crate) speed: u8,
    /// The most pixels a source picture may declare.
    pub(crate) max_pixels: u64,
    /// Maximum output width and height, if supplied.
    pub(crate) resolution: (Option<u32>, Option<u32>),
    /// Apply quality and resolution to AVIF input as well.
    pub(crate) recompress_avif: bool,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            quality: 80,
            speed: 6,
            max_pixels: 100_000_000,
            resolution: (None, None),
            recompress_avif: false,
        }
    }
}

/// Tells whether a file declares itself as AVIF: an image or a sequence.
fn is_avif(bytes: &[u8]) -> bool {
    bytes.get(4..8) == Some(b"ftyp") && matches!(bytes.get(8..12), Some(b"avif" | b"avis"))
}

/// The bytes of a picture in AVIF, JPEG, PNG, GIF, WebP or BMP, as AVIF.
///
/// An AVIF file can pass through if recompression was not requested. Other
/// pictures are decoded, turned upright and encoded. Animations use their
/// first frame. The declared pixel count is compared with
/// `options.max_pixels` before its pixels are decoded.
///
/// # Errors
///
/// [`PictureError::UnsupportedFormat`] when the bytes are in no format that is
/// read, [`PictureError::TooManyPixels`] above the limit,
/// [`PictureError::Undecodable`] when the picture is damaged and
/// [`PictureError::Encoding`] when the AVIF cannot be written.
pub(crate) fn encode_avif(source: &[u8], options: EncodeOptions) -> Result<Vec<u8>, PictureError> {
    let picture = if is_avif(source) {
        if !options.recompress_avif && options.resolution == (None, None) {
            return Ok(source.to_vec());
        }
        let bitmap = decode_avif(source, options.max_pixels)?;
        let image = image::RgbaImage::from_raw(bitmap.width(), bitmap.height(), bitmap.into_rgba())
            .ok_or_else(|| {
                PictureError::undecodable(std::io::Error::other("invalid bitmap size"))
            })?;
        DynamicImage::ImageRgba8(image)
    } else {
        decode(source, options.max_pixels)?
    };
    let width = options
        .resolution
        .0
        .unwrap_or(picture.width())
        .max(1)
        .min(picture.width());
    let height = options
        .resolution
        .1
        .unwrap_or(picture.height())
        .max(1)
        .min(picture.height());
    let picture = if width < picture.width() || height < picture.height() {
        picture.resize(width, height, FilterType::Lanczos3)
    } else {
        picture
    };
    encode(&picture, options)
}

/// Decodes a picture and turns it upright.
fn decode(bytes: &[u8], max_pixels: u64) -> Result<DynamicImage, PictureError> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(PictureError::undecodable)?;
    if !reader
        .format()
        .is_some_and(|format| DECODE_FORMATS.contains(&format))
    {
        return Err(PictureError::UnsupportedFormat);
    }
    let mut decoder = reader.into_decoder().map_err(PictureError::undecodable)?;
    let (width, height) = decoder.dimensions();
    check_pixels(width, height, max_pixels)?;
    let orientation = decoder.orientation().map_err(PictureError::undecodable)?;
    let mut picture = DynamicImage::from_decoder(decoder).map_err(PictureError::undecodable)?;
    picture.apply_orientation(orientation);
    Ok(picture)
}

/// Encodes a picture as AVIF, with its transparency when it has any.
fn encode(picture: &DynamicImage, options: EncodeOptions) -> Result<Vec<u8>, PictureError> {
    let mut avif = Vec::new();
    let encoder = AvifEncoder::new_with_speed_quality(
        &mut avif,
        options.speed.clamp(1, 10),
        options.quality.clamp(1, 100),
    );
    if picture.color().has_alpha() {
        picture.to_rgba8().write_with_encoder(encoder)
    } else {
        picture.to_rgb8().write_with_encoder(encoder)
    }
    .map_err(PictureError::encoding)?;
    Ok(avif)
}

#[cfg(test)]
mod tests {
    use super::{EncodeOptions, encode_avif, picture_file_extensions};
    use crate::entities::picture::decode::decode_avif;
    use crate::entities::picture::error::PictureError;
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
    use mbook::Book;
    use std::error::Error as _;
    use std::io::Cursor;
    use std::path::PathBuf;

    /// Fast, and small enough for a test.
    fn quick() -> EncodeOptions {
        EncodeOptions {
            quality: 60,
            speed: 10,
            ..EncodeOptions::default()
        }
    }

    fn file(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
        let picture = RgbImage::from_pixel(width, height, Rgb([200, 30, 30]));
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(picture)
            .write_to(&mut bytes, format)
            .unwrap();
        bytes.into_inner()
    }

    fn stored_size(avif: Vec<u8>) -> (u32, u32) {
        let mut book = Book::new("T", "en").unwrap();
        let size = book.add_picture("p.avif", avif).unwrap().size();
        (size.width, size.height)
    }

    #[test]
    fn the_picture_picker_offers_only_the_supported_picture_extensions() {
        assert_eq!(
            picture_file_extensions(),
            ["avif", "jpg", "jpeg", "png", "gif", "webp", "bmp"]
        );
    }

    #[test]
    fn default_options_are_the_documented_ones() {
        let options = EncodeOptions::default();
        assert_eq!(
            (options.quality, options.speed, options.max_pixels),
            (80, 6, 100_000_000)
        );
    }

    #[test]
    fn png_becomes_an_avif_that_mbook_accepts() {
        let avif = encode_avif(&file(24, 16, ImageFormat::Png), quick()).unwrap();
        assert_eq!(stored_size(avif), (24, 16));
    }

    #[test]
    fn jpeg_becomes_an_avif_that_mbook_accepts() {
        let avif = encode_avif(&file(16, 24, ImageFormat::Jpeg), quick()).unwrap();
        assert_eq!(stored_size(avif), (16, 24));
    }

    #[test]
    fn gif_bmp_and_webp_become_avif() {
        for format in [ImageFormat::Gif, ImageFormat::Bmp, ImageFormat::WebP] {
            let avif = encode_avif(&file(8, 8, format), quick()).unwrap();
            assert_eq!(stored_size(avif), (8, 8), "{format:?}");
        }
    }

    #[test]
    fn encoded_picture_decodes_to_its_size_and_colour() {
        let avif = encode_avif(&file(8, 8, ImageFormat::Png), quick()).unwrap();
        let bitmap = decode_avif(&avif, 1000).unwrap();
        assert_eq!((bitmap.width(), bitmap.height()), (8, 8));
        let pixel = &bitmap.rgba()[..4];
        assert!(pixel[0] > 150 && pixel[1] < 90 && pixel[2] < 90);
    }

    #[test]
    fn transparency_survives_encoding() {
        let picture = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 255, 100]));
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(picture)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let avif = encode_avif(png.get_ref(), quick()).unwrap();
        let bitmap = decode_avif(&avif, 1000).unwrap();
        assert!(bitmap.rgba().as_chunks::<4>().0.iter().all(|p| p[3] < 200));
    }

    #[test]
    fn avif_is_returned_as_it_is() {
        let avif = encode_avif(&file(8, 8, ImageFormat::Png), quick()).unwrap();
        let again = encode_avif(&avif, quick()).unwrap();
        assert!(avif == again);
    }

    #[test]
    fn custom_resolution_and_quality_are_applied_to_avif_input() {
        let source = encode_avif(&file(24, 16, ImageFormat::Png), quick()).unwrap();
        let options = EncodeOptions {
            quality: 30,
            resolution: (None, Some(8)),
            recompress_avif: true,
            ..quick()
        };
        let fitted = encode_avif(&source, options).unwrap();
        assert_eq!(stored_size(fitted), (12, 8));
    }

    #[test]
    fn avif_fixture_is_returned_as_it_is() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../mbook/tests/fixtures/plain.avif");
        let avif = std::fs::read(path).unwrap();
        let options = EncodeOptions {
            max_pixels: 1,
            ..quick()
        };
        assert!(encode_avif(&avif, options).unwrap() == avif);
    }

    #[test]
    fn picture_above_the_pixel_limit_is_refused_with_the_count() {
        let options = EncodeOptions {
            max_pixels: 100,
            ..quick()
        };
        let error = encode_avif(&file(24, 16, ImageFormat::Png), options).unwrap_err();
        assert!(matches!(
            error,
            PictureError::TooManyPixels {
                pixels: 384,
                limit: 100
            }
        ));
    }

    #[test]
    fn picture_with_exactly_the_limit_is_encoded() {
        let options = EncodeOptions {
            max_pixels: 64,
            ..quick()
        };
        assert!(encode_avif(&file(8, 8, ImageFormat::Png), options).is_ok());
    }

    #[test]
    fn garbage_is_an_unsupported_format() {
        assert!(matches!(
            encode_avif(b"<svg/>", quick()),
            Err(PictureError::UnsupportedFormat)
        ));
    }

    #[test]
    fn empty_input_is_an_unsupported_format() {
        assert!(matches!(
            encode_avif(&[], quick()),
            Err(PictureError::UnsupportedFormat)
        ));
    }

    #[test]
    fn format_that_is_not_read_is_unsupported() {
        let tiff = b"II*\0\x08\0\0\0\0\0";
        assert!(matches!(
            encode_avif(tiff, quick()),
            Err(PictureError::UnsupportedFormat)
        ));
    }

    #[test]
    fn damaged_picture_is_undecodable_and_keeps_its_source() {
        let mut png = file(16, 16, ImageFormat::Png);
        png.truncate(40);
        let error = encode_avif(&png, quick()).unwrap_err();
        assert!(matches!(error, PictureError::Undecodable(_)));
        assert!(error.source().is_some());
    }

    #[test]
    fn quality_and_speed_outside_their_range_are_brought_into_it() {
        let options = EncodeOptions {
            quality: 0,
            speed: 200,
            ..quick()
        };
        assert!(encode_avif(&file(8, 8, ImageFormat::Png), options).is_ok());
    }

    #[test]
    fn jpeg_orientation_tag_turns_the_picture_upright() {
        // A 24 by 16 JPEG whose Exif orientation says "rotate 90 degrees".
        let jpeg = with_orientation(&file(24, 16, ImageFormat::Jpeg), 6);
        let avif = encode_avif(&jpeg, quick()).unwrap();
        assert_eq!(stored_size(avif), (16, 24));
    }

    /// The JPEG with an Exif block that holds an orientation tag after SOI.
    fn with_orientation(jpeg: &[u8], orientation: u8) -> Vec<u8> {
        let exif: [u8; 36] = [
            0xFF,
            0xE1,
            0x00,
            0x22,
            b'E',
            b'x',
            b'i',
            b'f',
            0,
            0,
            b'M',
            b'M',
            0,
            42,
            0,
            0,
            0,
            8,
            0,
            1,
            0x01,
            0x12,
            0,
            3,
            0,
            0,
            0,
            1,
            0,
            orientation,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        let mut out = jpeg[..2].to_vec();
        out.extend_from_slice(&exif);
        out.extend_from_slice(&jpeg[2..]);
        out
    }
}
