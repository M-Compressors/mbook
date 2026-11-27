//! Convert a stored picture to a format supported by the output writers.

use avif_decode::Image;
use image::{ExtendedColorType, ImageEncoder as _, codecs::png::PngEncoder};
use mbook::Picture;

use super::export_error;
use super::{Format, Result};

const MAX_PIXELS: u64 = 100_000_000;

pub(crate) struct Png {
    pub(crate) bytes: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

pub(super) fn png(picture: &Picture, format: Format) -> Result<Png> {
    let mut bytes = picture.data();
    let avif = avif_parse::read_avif(&mut bytes).map_err(|error| export_error(format, error))?;
    let primary = avif
        .primary_item_metadata()
        .map_err(|error| export_error(format, error))?;
    let alpha = avif
        .alpha_item_metadata()
        .map_err(|error| export_error(format, error))?;
    for plane in std::iter::once(primary).chain(alpha) {
        let pixels =
            u64::from(plane.max_frame_width.get()) * u64::from(plane.max_frame_height.get());
        if pixels > MAX_PIXELS {
            return Err(crate::Error::Limit(
                "a picture is larger than the export pixel limit",
            ));
        }
    }
    drop(avif);
    // rav1d needs more stack space than the UI's native background queue.
    let image = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("mbook-export-avif".to_owned())
            .stack_size(8 * 1024 * 1024)
            .spawn_scoped(scope, || {
                avif_decode::Decoder::from_avif(picture.data())
                    .and_then(avif_decode::Decoder::to_image)
                    .map_err(|error| export_error(format, error))
            })
            .map_err(|error| export_error(format, error))?
            .join()
            .map_err(|_| export_error(format, "the AVIF decoder thread stopped"))?
    })?;

    let (width, height, pixels, color) = match image {
        Image::Rgb8(p) => (
            p.width(),
            p.height(),
            p.pixels().flat_map(|x| [x.r, x.g, x.b]).collect(),
            ExtendedColorType::Rgb8,
        ),
        Image::Rgba8(p) => (
            p.width(),
            p.height(),
            p.pixels().flat_map(|x| [x.r, x.g, x.b, x.a]).collect(),
            ExtendedColorType::Rgba8,
        ),
        Image::Gray8(p) => {
            let mut gray = Vec::with_capacity(p.width() * p.height());
            for sample in p.pixels() {
                gray.push(sample.value());
            }
            (p.width(), p.height(), gray, ExtendedColorType::L8)
        }
        Image::Rgb16(p) => (
            p.width(),
            p.height(),
            p.pixels()
                .flat_map(|x| [high_byte(x.r), high_byte(x.g), high_byte(x.b)])
                .collect(),
            ExtendedColorType::Rgb8,
        ),
        Image::Rgba16(p) => (
            p.width(),
            p.height(),
            p.pixels()
                .flat_map(|x| {
                    [
                        high_byte(x.r),
                        high_byte(x.g),
                        high_byte(x.b),
                        high_byte(x.a),
                    ]
                })
                .collect(),
            ExtendedColorType::Rgba8,
        ),
        Image::Gray16(p) => (
            p.width(),
            p.height(),
            p.pixels().map(|x| high_byte(x.value())).collect(),
            ExtendedColorType::L8,
        ),
    };
    let (pixels, width, height) = (
        pixels,
        u32::try_from(width).map_err(|error| export_error(format, error))?,
        u32::try_from(height).map_err(|error| export_error(format, error))?,
    );
    let pixels: Vec<u8> = pixels;
    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes)
        .write_image(&pixels, width, height, color)
        .map_err(|error| export_error(format, error))?;
    Ok(Png {
        bytes,
        width,
        height,
    })
}

fn high_byte(sample: u16) -> u8 {
    sample.to_be_bytes()[0]
}

#[cfg(test)]
mod tests {
    use super::high_byte;

    #[test]
    fn reducing_a_sample_to_eight_bits_keeps_its_most_significant_bits() {
        assert_eq!(
            [0, 255, 256, 32_768, u16::MAX].map(high_byte),
            [0, 0, 1, 128, 255]
        );
    }
}
