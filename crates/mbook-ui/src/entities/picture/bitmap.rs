//! Decoded pixels.

use super::transform::Transform;
use gpui_kit::RenderImage;
use image::imageops::{self, FilterType};
use image::{Frame, RgbaImage};
use std::sync::Arc;

/// Decoded pixels: 8-bit RGBA, rows top to bottom, not premultiplied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Bitmap {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Bitmap {
    /// A bitmap of `width` by `height` pixels; `None` when `rgba` does not
    /// have exactly four bytes for each pixel.
    pub(crate) fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Option<Self> {
        let expected = u64::from(width) * u64::from(height) * 4;
        (u64::try_from(rgba.len()).ok()? == expected).then_some(Self {
            width,
            height,
            rgba,
        })
    }

    /// The width in pixels.
    pub(crate) fn width(&self) -> u32 {
        self.width
    }

    /// The height in pixels.
    pub(crate) fn height(&self) -> u32 {
        self.height
    }

    /// The pixels as red, green, blue, alpha, row after row.
    pub(crate) fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// Takes the RGBA pixels without copying them.
    pub(crate) fn into_rgba(self) -> Vec<u8> {
        self.rgba
    }

    /// The bytes the pixels take; what the cache counts.
    pub(crate) fn byte_len(&self) -> usize {
        self.rgba.len()
    }

    /// Scaled down so that it is at most `max_width` wide, keeping its
    /// proportions. A bitmap that is not wider is returned as it is: nothing
    /// is scaled up. Neither side becomes zero, so a `max_width` of 0 is taken
    /// as 1.
    pub(crate) fn fit_width(self, max_width: u32) -> Self {
        let width = max_width.max(1);
        if self.width <= width || self.height == 0 {
            return self;
        }
        let height = u64::from(self.height) * u64::from(width) / u64::from(self.width);
        let height = u32::try_from(height).unwrap_or(u32::MAX).max(1);
        self.resize(width, height)
    }

    /// Resizes to the already fitted dimensions of the destination.
    pub(crate) fn resize(self, width: u32, height: u32) -> Self {
        if (self.width, self.height) == (width, height) {
            return self;
        }
        let Self {
            width: source_width,
            height: source_height,
            rgba,
        } = self;
        let Some(source) = RgbaImage::from_raw(source_width, source_height, rgba) else {
            // The length is checked on construction, so this cannot happen.
            return Self {
                width: 0,
                height: 0,
                rgba: Vec::new(),
            };
        };
        let scaled = imageops::resize(&source, width, height, FilterType::CatmullRom);
        Self {
            width,
            height,
            rgba: scaled.into_raw(),
        }
    }

    /// The bitmap after a crop, a rotation or a mirror of an AVIF file.
    ///
    /// A crop that reaches outside the bitmap is cut to what is inside.
    pub(super) fn transformed(self, transform: Transform) -> Self {
        let Self {
            width,
            height,
            rgba,
        } = self;
        let Some(image) = RgbaImage::from_raw(width, height, rgba) else {
            return Self {
                width: 0,
                height: 0,
                rgba: Vec::new(),
            };
        };
        let image = match transform {
            Transform::Crop {
                width: crop_width,
                height: crop_height,
                x_offset,
                y_offset,
            } => {
                let crop_width = crop_width.min(width);
                let crop_height = crop_height.min(height);
                let left = centred(width, crop_width, x_offset);
                let top = centred(height, crop_height, y_offset);
                imageops::crop_imm(&image, left, top, crop_width, crop_height).to_image()
            }
            // `rotate90` and `rotate270` turn clockwise.
            Transform::Rotate(1) => imageops::rotate270(&image),
            Transform::Rotate(2) => imageops::rotate180(&image),
            Transform::Rotate(3) => imageops::rotate90(&image),
            Transform::Rotate(_) => image,
            Transform::Mirror { left_right: true } => imageops::flip_horizontal(&image),
            Transform::Mirror { left_right: false } => imageops::flip_vertical(&image),
        };
        Self {
            width: image.width(),
            height: image.height(),
            rgba: image.into_raw(),
        }
    }

    /// What GPUI draws: one frame in the byte order it wants, blue first.
    pub(crate) fn into_render_image(self) -> Arc<RenderImage> {
        let Self {
            width,
            height,
            mut rgba,
        } = self;
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let frames = RgbaImage::from_raw(width, height, rgba)
            .map(Frame::new)
            .into_iter()
            .collect::<Vec<_>>();
        Arc::new(RenderImage::new(frames))
    }
}

/// Where a window of `inner` pixels starts in `outer` pixels when it is
/// centred and moved by `offset`, kept inside.
fn centred(outer: u32, inner: u32, offset: i64) -> u32 {
    let start = i64::from(outer - inner) / 2 + offset;
    u32::try_from(start.clamp(0, i64::from(outer - inner))).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::Bitmap;
    use crate::entities::picture::transform::Transform;

    fn plain(width: u32, height: u32) -> Bitmap {
        let rgba = [10, 20, 30, 255].repeat((width * height) as usize);
        Bitmap::from_rgba(width, height, rgba).unwrap()
    }

    #[test]
    fn bitmap_reports_its_size_and_bytes() {
        let bitmap = plain(3, 2);
        assert_eq!((bitmap.width(), bitmap.height()), (3, 2));
        assert_eq!(bitmap.byte_len(), 24);
        assert_eq!(bitmap.rgba().len(), 24);
    }

    #[test]
    fn pixels_of_the_wrong_length_are_refused() {
        assert!(Bitmap::from_rgba(2, 2, vec![0; 15]).is_none());
        assert!(Bitmap::from_rgba(2, 2, vec![0; 17]).is_none());
    }

    #[test]
    fn empty_bitmap_is_accepted() {
        assert!(Bitmap::from_rgba(0, 0, Vec::new()).is_some());
    }

    #[test]
    fn narrower_bitmap_is_not_scaled() {
        let bitmap = plain(10, 4);
        assert_eq!(bitmap.clone().fit_width(20), bitmap);
    }

    #[test]
    fn bitmap_of_the_asked_width_is_not_scaled() {
        let bitmap = plain(10, 4);
        assert_eq!(bitmap.clone().fit_width(10), bitmap);
    }

    #[test]
    fn wider_bitmap_is_scaled_keeping_its_proportions() {
        let scaled = plain(100, 50).fit_width(40);
        assert_eq!((scaled.width(), scaled.height()), (40, 20));
        assert_eq!(scaled.byte_len(), 40 * 20 * 4);
    }

    #[test]
    fn scaling_keeps_a_flat_colour() {
        let scaled = plain(64, 64).fit_width(8);
        assert!(
            scaled
                .rgba()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [10, 20, 30, 255])
        );
    }

    #[test]
    fn width_of_one_leaves_one_pixel_column() {
        let scaled = plain(100, 50).fit_width(1);
        assert_eq!((scaled.width(), scaled.height()), (1, 1));
    }

    #[test]
    fn width_of_zero_is_taken_as_one() {
        let scaled = plain(100, 50).fit_width(0);
        assert_eq!((scaled.width(), scaled.height()), (1, 1));
    }

    #[test]
    fn height_never_becomes_zero() {
        let scaled = plain(1000, 2).fit_width(10);
        assert_eq!((scaled.width(), scaled.height()), (10, 1));
    }

    #[test]
    fn render_image_keeps_the_size_and_puts_blue_first() {
        let bitmap = Bitmap::from_rgba(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let image = bitmap.into_render_image();
        assert_eq!(image.frame_count(), 1);
        let size = image.size(0);
        assert_eq!((size.width.0, size.height.0), (2, 1));
        assert_eq!(image.as_bytes(0), Some(&[3, 2, 1, 4, 7, 6, 5, 8][..]));
    }

    /// 3 by 2 pixels whose red channel counts them: 0 1 2 / 3 4 5.
    fn counted() -> Bitmap {
        let rgba = (0..6).flat_map(|n| [n, 0, 0, 255]).collect();
        Bitmap::from_rgba(3, 2, rgba).unwrap()
    }

    fn reds(bitmap: &Bitmap) -> Vec<u8> {
        bitmap
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[0])
            .collect()
    }

    #[test]
    fn quarter_turn_moves_the_top_right_to_the_top_left() {
        let turned = counted().transformed(Transform::Rotate(1));
        assert_eq!((turned.width(), turned.height()), (2, 3));
        assert_eq!(reds(&turned), [2, 5, 1, 4, 0, 3]);
    }

    #[test]
    fn half_turn_reverses_the_pixels() {
        let turned = counted().transformed(Transform::Rotate(2));
        assert_eq!(reds(&turned), [5, 4, 3, 2, 1, 0]);
    }

    #[test]
    fn three_quarter_turns_move_the_bottom_left_to_the_top_left() {
        let turned = counted().transformed(Transform::Rotate(3));
        assert_eq!((turned.width(), turned.height()), (2, 3));
        assert_eq!(reds(&turned), [3, 0, 4, 1, 5, 2]);
    }

    #[test]
    fn no_turn_leaves_the_pixels() {
        assert_eq!(counted().transformed(Transform::Rotate(0)), counted());
    }

    #[test]
    fn left_right_mirror_reverses_each_row() {
        let mirrored = counted().transformed(Transform::Mirror { left_right: true });
        assert_eq!(reds(&mirrored), [2, 1, 0, 5, 4, 3]);
    }

    #[test]
    fn top_bottom_mirror_swaps_the_rows() {
        let mirrored = counted().transformed(Transform::Mirror { left_right: false });
        assert_eq!(reds(&mirrored), [3, 4, 5, 0, 1, 2]);
    }

    #[test]
    fn crop_is_taken_around_the_centre() {
        let crop = Transform::Crop {
            width: 1,
            height: 2,
            x_offset: 0,
            y_offset: 0,
        };
        assert_eq!(reds(&counted().transformed(crop)), [1, 4]);
    }

    #[test]
    fn crop_offset_moves_the_window() {
        let crop = Transform::Crop {
            width: 1,
            height: 2,
            x_offset: 1,
            y_offset: 0,
        };
        assert_eq!(reds(&counted().transformed(crop)), [2, 5]);
    }

    #[test]
    fn crop_offset_cannot_leave_the_picture() {
        let crop = Transform::Crop {
            width: 1,
            height: 2,
            x_offset: -50,
            y_offset: 50,
        };
        assert_eq!(reds(&counted().transformed(crop)), [0, 3]);
    }

    #[test]
    fn crop_larger_than_the_picture_keeps_the_picture() {
        let crop = Transform::Crop {
            width: 99,
            height: 99,
            x_offset: 0,
            y_offset: 0,
        };
        assert_eq!(counted().transformed(crop), counted());
    }
}
