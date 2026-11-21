//! Decoding, scaling and encoding the pictures of a book.
//!
//! Everything here is synchronous and meant to run on a background task:
//! decoding an AVIF file ([`decode_avif`]), scaling it ([`Bitmap::fit_width`]),
//! making the thumbnail of a cover ([`cover_thumbnail`]), encoding a picture of
//! another format as AVIF ([`encode_avif`]) and keeping what was decoded in a
//! bounded cache ([`PictureCache`]).

mod bitmap;
mod cache;
mod decode;
mod encode;
mod error;
mod transform;

pub(crate) use bitmap::Bitmap;
pub(crate) use cache::PictureCache;
pub(crate) use decode::{cover_thumbnail, decode_avif};
pub(crate) use encode::{EncodeOptions, encode_avif, picture_file_extensions};
pub(crate) use error::PictureError;
