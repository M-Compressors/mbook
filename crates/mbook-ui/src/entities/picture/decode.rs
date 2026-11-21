//! Decoding AVIF files, and the thumbnail of a cover.

use super::bitmap::Bitmap;
use super::error::PictureError;
use super::transform::transforms;
use avif_decode::{Decoder, Image};
use image::codecs::jpeg::JpegEncoder;
use mbook::Book;

/// Quality of the JPEG of a thumbnail.
const THUMBNAIL_QUALITY: u8 = 85;

/// The pixels an AVIF file declares, read from its container without decoding.
///
/// The size is the one `mbook` reads, so what the reader decides on the size of
/// a picture is what the decoder is held to.
fn declared_pixels(avif: &[u8]) -> Result<u64, PictureError> {
    let mut book = Book::new("-", "en").map_err(PictureError::undecodable)?;
    let size = book
        .add_picture("p.avif", avif)
        .map_err(PictureError::undecodable)?
        .size();
    Ok(u64::from(size.width) * u64::from(size.height))
}

/// Decodes an AVIF file.
///
/// The size the container declares is compared with `max_pixels` before a
/// pixel is decoded. The crop, rotation and mirror the file lists for its
/// picture are applied, so the size is the one `mbook` reports. 16-bit
/// pictures are reduced to 8 bits, gray ones become RGBA, and one without
/// alpha is opaque.
///
/// # Errors
///
/// [`PictureError::TooManyPixels`] when the picture has more than `max_pixels`
/// pixels, [`PictureError::Undecodable`] when it is not a valid AVIF file.
pub(crate) fn decode_avif(avif: &[u8], max_pixels: u64) -> Result<Bitmap, PictureError> {
    let declared = declared_pixels(avif)?;
    if declared > max_pixels {
        return Err(PictureError::TooManyPixels {
            pixels: declared,
            limit: max_pixels,
        });
    }
    let image = Decoder::from_avif(avif)
        .and_then(Decoder::to_image)
        .map_err(PictureError::undecodable)?;
    let bitmap = to_bitmap(&image)?;
    Ok(transforms(avif)
        .into_iter()
        .fold(bitmap, Bitmap::transformed))
}

/// The 8-bit RGBA pixels of a decoded image.
fn to_bitmap(image: &Image) -> Result<Bitmap, PictureError> {
    let (width, height, rgba) = match image {
        Image::Rgb8(img) => (
            img.width(),
            img.height(),
            collect(img.pixels().map(|p| [p.r, p.g, p.b, u8::MAX])),
        ),
        Image::Rgba8(img) => (
            img.width(),
            img.height(),
            collect(img.pixels().map(|p| [p.r, p.g, p.b, p.a])),
        ),
        Image::Gray8(img) => (
            img.width(),
            img.height(),
            collect(img.pixels().map(|p| {
                let gray = p.value();
                [gray, gray, gray, u8::MAX]
            })),
        ),
        Image::Rgb16(img) => (
            img.width(),
            img.height(),
            collect(
                img.pixels()
                    .map(|p| [to8(p.r), to8(p.g), to8(p.b), u8::MAX]),
            ),
        ),
        Image::Rgba16(img) => (
            img.width(),
            img.height(),
            collect(
                img.pixels()
                    .map(|p| [to8(p.r), to8(p.g), to8(p.b), to8(p.a)]),
            ),
        ),
        Image::Gray16(img) => (
            img.width(),
            img.height(),
            collect(img.pixels().map(|p| {
                let gray = to8(p.value());
                [gray, gray, gray, u8::MAX]
            })),
        ),
    };
    let width = u32::try_from(width).map_err(PictureError::undecodable)?;
    let height = u32::try_from(height).map_err(PictureError::undecodable)?;
    Bitmap::from_rgba(width, height, rgba).ok_or_else(|| {
        PictureError::undecodable(std::io::Error::other(
            "the decoder returned pixels that do not match the size",
        ))
    })
}

/// The bytes of pixels laid one after another.
fn collect(pixels: impl Iterator<Item = [u8; 4]>) -> Vec<u8> {
    let (lower, _) = pixels.size_hint();
    let mut bytes = Vec::with_capacity(lower * 4);
    for pixel in pixels {
        bytes.extend_from_slice(&pixel);
    }
    bytes
}

/// A 16-bit sample as 8 bits, rounded: 65535 is 255.
fn to8(sample: u16) -> u8 {
    let rounded = (u32::from(sample) * 255 + 32767) / 65535;
    u8::try_from(rounded).unwrap_or(u8::MAX)
}

/// The JPEG thumbnail of a cover: decoded, scaled to `width` and encoded.
///
/// A cover narrower than `width` is not scaled up. Transparency is laid over
/// white, since JPEG has none.
///
/// # Errors
///
/// The errors of [`decode_avif`], and [`PictureError::Encoding`] when the JPEG
/// cannot be written.
pub(crate) fn cover_thumbnail(
    avif: &[u8],
    width: u32,
    max_pixels: u64,
) -> Result<Vec<u8>, PictureError> {
    let bitmap = decode_avif(avif, max_pixels)?.fit_width(width);
    let mut rgb = Vec::with_capacity(bitmap.byte_len() / 4 * 3);
    for pixel in bitmap.rgba().as_chunks::<4>().0 {
        let alpha = u32::from(pixel[3]);
        for &channel in &pixel[..3] {
            let over_white = (u32::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255;
            rgb.push(u8::try_from(over_white).unwrap_or(u8::MAX));
        }
    }
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, THUMBNAIL_QUALITY)
        .encode(
            &rgb,
            bitmap.width(),
            bitmap.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(PictureError::encoding)?;
    Ok(jpeg)
}

#[cfg(test)]
mod tests {
    use super::{cover_thumbnail, decode_avif, to8};
    use crate::entities::picture::error::PictureError;
    use image::{ImageFormat, ImageReader};
    use mbook::Book;
    use std::error::Error as _;
    use std::io::Cursor;
    use std::path::PathBuf;

    const LIMIT: u64 = 100_000_000;

    fn fixture(name: &str) -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../mbook/tests/fixtures")
            .join(name);
        std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
    }

    /// The size `mbook` reads from an AVIF file.
    fn stored_size(avif: Vec<u8>) -> (u32, u32) {
        let mut book = Book::new("T", "en").unwrap();
        let size = book.add_picture("p.avif", avif).unwrap().size();
        (size.width, size.height)
    }

    fn decoded_size(name: &str) -> (u32, u32) {
        let bitmap = decode_avif(&fixture(name), LIMIT).unwrap();
        (bitmap.width(), bitmap.height())
    }

    #[test]
    fn plain_fixture_has_the_size_mbook_reports() {
        assert_eq!(
            decoded_size("plain.avif"),
            stored_size(fixture("plain.avif"))
        );
    }

    #[test]
    fn rotated_fixture_has_the_size_mbook_reports() {
        assert_eq!(
            decoded_size("rotated.avif"),
            stored_size(fixture("rotated.avif"))
        );
    }

    #[test]
    fn mirrored_fixture_has_the_size_mbook_reports() {
        assert_eq!(
            decoded_size("mirrored.avif"),
            stored_size(fixture("mirrored.avif"))
        );
    }

    #[test]
    fn cropped_fixture_has_the_size_mbook_reports() {
        assert_eq!(
            decoded_size("cropped.avif"),
            stored_size(fixture("cropped.avif"))
        );
    }

    /// The colour at the top left of a bitmap, as red and blue.
    fn corner(name: &str, right: bool) -> (u8, u8) {
        let bitmap = decode_avif(&fixture(name), LIMIT).unwrap();
        let at = if right {
            bitmap.width() as usize - 1
        } else {
            0
        };
        let pixel = bitmap.rgba().as_chunks::<4>().0[at];
        (pixel[0], pixel[2])
    }

    fn is_blue((red, blue): (u8, u8)) -> bool {
        blue > 100 && red < 60
    }

    fn is_orange((red, blue): (u8, u8)) -> bool {
        red > 200 && blue < 60
    }

    #[test]
    fn plain_fixture_is_blue_above_and_orange_below() {
        let bitmap = decode_avif(&fixture("plain.avif"), LIMIT).unwrap();
        let pixels = bitmap.rgba().as_chunks::<4>().0;
        assert!(is_blue((pixels[0][0], pixels[0][2])));
        let last = pixels[pixels.len() - 1];
        assert!(is_orange((last[0], last[2])));
    }

    #[test]
    fn rotated_fixture_is_turned_as_its_file_says() {
        // A quarter turn anticlockwise: the top right, blue, comes to the top
        // left and the bottom right, orange, to the top right.
        assert!(is_blue(corner("rotated.avif", false)));
        assert!(is_orange(corner("rotated.avif", true)));
    }

    #[test]
    fn mirrored_fixture_is_flipped_as_its_file_says() {
        assert!(is_orange(corner("mirrored.avif", false)));
    }

    #[test]
    fn decoded_pixels_fill_the_bitmap() {
        let bitmap = decode_avif(&fixture("plain.avif"), LIMIT).unwrap();
        let bytes = u64::from(bitmap.width()) * u64::from(bitmap.height()) * 4;
        assert_eq!(u64::try_from(bitmap.byte_len()), Ok(bytes));
    }

    #[test]
    fn picture_without_alpha_is_opaque() {
        let bitmap = decode_avif(&fixture("plain.avif"), LIMIT).unwrap();
        assert!(bitmap.rgba().as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }

    #[test]
    fn picture_with_more_pixels_than_the_limit_is_refused_with_the_count() {
        let (width, height) = stored_size(fixture("plain.avif"));
        let pixels = u64::from(width) * u64::from(height);
        let error = decode_avif(&fixture("plain.avif"), pixels - 1).unwrap_err();
        assert!(matches!(
            error,
            PictureError::TooManyPixels { pixels: p, limit } if p == pixels && limit == pixels - 1
        ));
    }

    #[test]
    fn picture_with_exactly_the_limit_is_decoded() {
        let (width, height) = stored_size(fixture("plain.avif"));
        let pixels = u64::from(width) * u64::from(height);
        assert!(decode_avif(&fixture("plain.avif"), pixels).is_ok());
    }

    #[test]
    fn garbage_is_not_decodable_and_keeps_its_source() {
        let error = decode_avif(b"definitely not a picture", LIMIT).unwrap_err();
        assert!(matches!(error, PictureError::Undecodable(_)));
        assert!(error.source().is_some());
    }

    #[test]
    fn empty_input_is_not_decodable() {
        assert!(matches!(
            decode_avif(&[], LIMIT),
            Err(PictureError::Undecodable(_))
        ));
    }

    #[test]
    fn truncated_picture_is_not_decodable() {
        let avif = fixture("plain.avif");
        assert!(decode_avif(&avif[..avif.len() / 2], LIMIT).is_err());
    }

    #[test]
    fn sixteen_bit_samples_are_reduced_to_eight() {
        assert_eq!((to8(0), to8(128), to8(32768), to8(65535)), (0, 0, 128, 255));
    }

    #[test]
    fn thumbnail_is_a_jpeg_of_the_asked_width() {
        let jpeg = cover_thumbnail(&fixture("plain.avif"), 8, LIMIT).unwrap();
        let reader = ImageReader::new(Cursor::new(&jpeg))
            .with_guessed_format()
            .unwrap();
        assert_eq!(reader.format(), Some(ImageFormat::Jpeg));
        let image = reader.decode().unwrap();
        assert_eq!(image.width(), 8);
    }

    #[test]
    fn thumbnail_keeps_the_proportions() {
        let (width, height) = decoded_size("plain.avif");
        let jpeg = cover_thumbnail(&fixture("plain.avif"), 8, LIMIT).unwrap();
        let image = image::load_from_memory(&jpeg).unwrap();
        let expected = (u64::from(height) * 8 / u64::from(width)).max(1);
        assert_eq!(u64::from(image.height()), expected);
    }

    #[test]
    fn thumbnail_is_not_scaled_up() {
        let (width, _) = decoded_size("plain.avif");
        let jpeg = cover_thumbnail(&fixture("plain.avif"), width + 100, LIMIT).unwrap();
        assert_eq!(image::load_from_memory(&jpeg).unwrap().width(), width);
    }

    #[test]
    fn thumbnail_of_garbage_fails() {
        assert!(matches!(
            cover_thumbnail(b"nope", 400, LIMIT),
            Err(PictureError::Undecodable(_))
        ));
    }

    #[test]
    fn thumbnail_above_the_pixel_limit_fails() {
        assert!(matches!(
            cover_thumbnail(&fixture("plain.avif"), 400, 1),
            Err(PictureError::TooManyPixels { .. })
        ));
    }
}
