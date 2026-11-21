//! Reading the pixel size of a picture from its AVIF container.

use crate::types::Size;

/// The size of the primary image item as displayed, after its crop, rotation
/// and mirror properties. No pixel data is decoded.
///
/// The error is the reason the bytes are not a usable AVIF file.
pub(crate) fn displayed_size(_avif: &[u8]) -> Result<Size, &'static str> {
    todo!("parse the AVIF boxes: ispe, clap, irot and imir of the primary item")
}
