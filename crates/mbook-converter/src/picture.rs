//! A picture of a source, turned into an AVIF file.
//!
//! Decoding and encoding are both done by the `image` crate; its AVIF
//! encoder is `ravif`, built on `rav1e`.

use crate::draft::PictureData;
use crate::options::{ConvertLimits, PictureOptions};
use image::codecs::avif::AvifEncoder;
use image::{DynamicImage, ImageDecoder, ImageReader};
use std::io::Cursor;

/// The AVIF file for a picture of a source.
///
/// A picture that already is an AVIF file is returned as it is. `None` when
/// the picture is in a format this crate does not read, is damaged, or has
/// more pixels than the limit allows.
pub(crate) fn to_avif(
    data: &PictureData,
    options: PictureOptions,
    limits: &ConvertLimits,
) -> Option<Vec<u8>> {
    let picture = match data {
        PictureData::Encoded(bytes) if is_avif(bytes) => return Some(bytes.clone()),
        PictureData::Encoded(bytes) => decode(bytes, limits)?,
        #[cfg(feature = "pdf")]
        PictureData::Rgba {
            width,
            height,
            pixels,
        } => {
            if !fits(*width, *height, limits) {
                return None;
            }
            let pixels = image::RgbaImage::from_raw(*width, *height, pixels.clone())?;
            DynamicImage::ImageRgba8(pixels)
        }
    };
    encode(&picture, options)
}

/// Tells whether a file declares itself as AVIF: an image or a sequence.
fn is_avif(bytes: &[u8]) -> bool {
    bytes.get(4..8) == Some(b"ftyp") && matches!(bytes.get(8..12), Some(b"avif" | b"avis"))
}

/// Tells whether a picture of this size is within the limit.
fn fits(width: u32, height: u32, limits: &ConvertLimits) -> bool {
    u64::from(width) * u64::from(height) <= limits.max_picture_pixels
}

/// Decodes a picture and turns it upright. The size the file declares is
/// compared with the limit before its pixels are decoded.
fn decode(bytes: &[u8], limits: &ConvertLimits) -> Option<DynamicImage> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut decoder = reader.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    if !fits(width, height, limits) {
        return None;
    }
    let orientation = decoder.orientation().ok()?;
    let mut picture = DynamicImage::from_decoder(decoder).ok()?;
    picture.apply_orientation(orientation);
    Some(picture)
}

/// Encodes a picture as AVIF, with its transparency when it has any.
fn encode(picture: &DynamicImage, options: PictureOptions) -> Option<Vec<u8>> {
    let mut avif = Vec::new();
    let encoder = AvifEncoder::new_with_speed_quality(
        &mut avif,
        options.speed.clamp(1, 10),
        options.quality.clamp(1, 100),
    );
    let written = if picture.color().has_alpha() {
        picture.to_rgba8().write_with_encoder(encoder)
    } else {
        picture.to_rgb8().write_with_encoder(encoder)
    };
    written.ok().map(|()| avif)
}

#[cfg(test)]
mod tests {
    use super::to_avif;
    use crate::draft::PictureData;
    use crate::options::{ConvertLimits, PictureOptions};
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
    use mbook::{Book, Size};
    use std::io::Cursor;

    /// A picture of one colour, as a file of the given format.
    fn file(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
        let picture = RgbImage::from_pixel(width, height, Rgb([200, 30, 30]));
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(picture)
            .write_to(&mut bytes, format)
            .unwrap();
        bytes.into_inner()
    }

    fn converted(data: &PictureData) -> Option<Vec<u8>> {
        to_avif(data, PictureOptions::default(), &ConvertLimits::default())
    }

    /// The size `mbook` reads from an AVIF file.
    fn stored_size(avif: Vec<u8>) -> Size {
        let mut book = Book::new("T", "en").unwrap();
        book.add_picture("p.avif", avif).unwrap().size()
    }

    #[test]
    fn pictures_of_other_formats_become_avif_of_the_same_size() {
        for format in [
            ImageFormat::Png,
            ImageFormat::Jpeg,
            ImageFormat::Gif,
            ImageFormat::Bmp,
            ImageFormat::WebP,
        ] {
            let avif = converted(&PictureData::Encoded(file(24, 16, format))).unwrap();
            let size = stored_size(avif);
            assert_eq!((size.width, size.height), (24, 16), "{format:?}");
        }
    }

    #[test]
    fn a_picture_with_transparency_is_encoded() {
        let picture = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 255, 128]));
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(picture)
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        let avif = converted(&PictureData::Encoded(bytes.into_inner())).unwrap();
        assert_eq!(stored_size(avif).width, 8);
    }

    #[test]
    fn an_avif_file_is_kept_as_it_is() {
        let avif = converted(&PictureData::Encoded(file(8, 8, ImageFormat::Png))).unwrap();
        let again = converted(&PictureData::Encoded(avif.clone())).unwrap();
        assert!(avif == again);
    }

    #[test]
    fn bytes_that_are_not_a_picture_give_nothing() {
        assert_eq!(converted(&PictureData::Encoded(b"<svg/>".to_vec())), None);
        assert_eq!(converted(&PictureData::Encoded(Vec::new())), None);
    }

    #[test]
    fn a_damaged_picture_gives_nothing() {
        let mut bytes = file(16, 16, ImageFormat::Png);
        bytes.truncate(40);
        assert_eq!(converted(&PictureData::Encoded(bytes)), None);
    }

    #[test]
    fn a_picture_above_the_pixel_limit_gives_nothing() {
        let limits = ConvertLimits {
            max_picture_pixels: 100,
            ..ConvertLimits::default()
        };
        let data = PictureData::Encoded(file(24, 16, ImageFormat::Png));
        assert_eq!(to_avif(&data, PictureOptions::default(), &limits), None);
    }

    #[test]
    fn quality_and_speed_outside_their_range_are_brought_into_it() {
        let options = PictureOptions {
            quality: 0,
            speed: 200,
        };
        let data = PictureData::Encoded(file(8, 8, ImageFormat::Png));
        assert!(to_avif(&data, options, &ConvertLimits::default()).is_some());
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn rendered_pixels_are_encoded() {
        let data = PictureData::Rgba {
            width: 4,
            height: 2,
            pixels: vec![255; 4 * 2 * 4],
        };
        let size = stored_size(converted(&data).unwrap());
        assert_eq!((size.width, size.height), (4, 2));
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn rendered_pixels_that_do_not_match_their_size_give_nothing() {
        let short = PictureData::Rgba {
            width: 4,
            height: 2,
            pixels: vec![255; 5],
        };
        assert_eq!(converted(&short), None);
        let limits = ConvertLimits {
            max_picture_pixels: 4,
            ..ConvertLimits::default()
        };
        let large = PictureData::Rgba {
            width: 4,
            height: 2,
            pixels: vec![255; 32],
        };
        assert_eq!(to_avif(&large, PictureOptions::default(), &limits), None);
    }
}
