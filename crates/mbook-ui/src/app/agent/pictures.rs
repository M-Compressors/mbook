//! Pictures as an agent gives and asks for them: a file, base64 or SVG in,
//! PNG out.

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use image::{ImageFormat, RgbaImage};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{self, fontdb};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::entities::picture::{EncodeOptions, PictureError, decode_avif, encode_avif};
use crate::features::edit_book::agent::error::AgentError;

/// The largest file that is read.
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// The largest side of an SVG drawing, in pixels.
const MAX_SVG_SIDE: f32 = 8192.0;
/// The largest side `view_picture` gives.
pub(crate) const MAX_VIEW: u32 = 2048;

/// Where a picture comes from.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum Source {
    /// A file on this computer, by its absolute path.
    File {
        /// The absolute path.
        file: PathBuf,
    },
    /// The bytes of a picture in base64.
    Base64 {
        /// The bytes in base64 with padding.
        base64: String,
        /// `image/png`, `image/jpeg` and the like.
        #[serde(default)]
        media_type: Option<String>,
    },
    /// An SVG document, drawn at its own size.
    Svg {
        /// The document.
        svg: String,
    },
}

impl Source {
    /// The name of the file it came from, for the path of the picture.
    pub(crate) fn name(&self) -> Option<String> {
        match self {
            Self::File { file } => file
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            Self::Base64 { .. } | Self::Svg { .. } => None,
        }
    }
}

/// How a picture is encoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Encoding {
    pub(crate) quality: Option<u8>,
    pub(crate) max_width: Option<u32>,
    pub(crate) max_height: Option<u32>,
}

/// The picture as AVIF. Runs on the pool: it reads, decodes and encodes.
pub(crate) fn encode(source: &Source, encoding: Encoding) -> Result<Vec<u8>, AgentError> {
    let bytes = match source {
        Source::File { file } => {
            if !file.is_absolute() {
                return Err(AgentError::InvalidArguments(format!(
                    "{} is not an absolute path",
                    file.display()
                )));
            }
            let length = std::fs::metadata(file)
                .map_err(|error| AgentError::Io(format!("{}: {error}", file.display())))?
                .len();
            if length > MAX_FILE_BYTES {
                return Err(AgentError::Limit(format!(
                    "a picture file of at most {MAX_FILE_BYTES} bytes"
                )));
            }
            std::fs::read(file)
                .map_err(|error| AgentError::Io(format!("{}: {error}", file.display())))?
        }
        Source::Base64 { base64, .. } => STANDARD.decode(base64.trim()).map_err(|error| {
            AgentError::InvalidPicture(format!("the base64 is not valid: {error}"))
        })?,
        Source::Svg { svg } => svg_png(svg)?,
    };
    let mut options = EncodeOptions::default();
    if let Some(quality) = encoding.quality {
        options.quality = quality;
    }
    options.resolution = (encoding.max_width, encoding.max_height);
    encode_avif(&bytes, options).map_err(|error| picture_error(&error))
}

/// An error of a picture as the agent is told it.
fn picture_error(error: &PictureError) -> AgentError {
    AgentError::InvalidPicture(crate::features::edit_book::agent::error::chain(error))
}

/// The fonts SVG text is drawn with: those of the system.
fn fonts() -> Arc<fontdb::Database> {
    static FONTS: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    Arc::clone(FONTS.get_or_init(|| {
        let mut fonts = fontdb::Database::new();
        fonts.load_system_fonts();
        Arc::new(fonts)
    }))
}

/// An SVG document drawn at its own size as PNG, smaller when a side is
/// larger than 8192 pixels.
fn svg_png(svg: &str) -> Result<Vec<u8>, AgentError> {
    let options = usvg::Options {
        fontdb: fonts(),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_str(svg, &options)
        .map_err(|error| AgentError::InvalidPicture(format!("the SVG cannot be read: {error}")))?;
    let size = tree.size();
    let scale = (MAX_SVG_SIDE / size.width().max(size.height())).min(1.0);
    let pixels = usvg::Size::from_wh(size.width() * scale, size.height() * scale)
        .map(|size| size.to_int_size())
        .and_then(|size| Pixmap::new(size.width(), size.height()));
    let Some(mut pixmap) = pixels else {
        return Err(AgentError::InvalidPicture("the SVG has no size".into()));
    };
    resvg::render(
        &tree,
        Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|error| AgentError::InvalidPicture(error.to_string()))
}

/// An AVIF picture as PNG, no larger than `max_side` on either side.
pub(crate) fn png(avif: &[u8], max_side: u32) -> Result<Vec<u8>, AgentError> {
    let bitmap = decode_avif(avif, EncodeOptions::default().max_pixels)
        .map_err(|error| picture_error(&error))?;
    let max_side = max_side.clamp(1, MAX_VIEW);
    let (width, height) = (bitmap.width(), bitmap.height());
    let bitmap = if width.max(height) > max_side {
        let scale = f64::from(max_side) / f64::from(width.max(height));
        let fit = |side: u32| {
            // The result is at most `max_side`, which fits `u32`.
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a side scaled down stays between 1 and the side it was"
            )]
            let fitted = (f64::from(side) * scale).round().max(1.0) as u32;
            fitted
        };
        bitmap.resize(fit(width), fit(height))
    } else {
        bitmap
    };
    let image = RgbaImage::from_raw(bitmap.width(), bitmap.height(), bitmap.into_rgba())
        .ok_or_else(|| AgentError::InvalidPicture("the picture has no pixels".into()))?;
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|error| AgentError::InvalidPicture(error.to_string()))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PICTURE: &[u8] = include_bytes!("../../../../mbook/tests/fixtures/plain.avif");
    const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='40' height='20'><rect width='40' height='20' fill='red'/></svg>";

    fn size(png: &[u8]) -> (u32, u32) {
        let image = image::load_from_memory(png).unwrap();
        (image.width(), image.height())
    }

    #[test]
    fn an_svg_is_drawn_at_its_size_and_encoded() {
        let avif = encode(&Source::Svg { svg: SVG.into() }, Encoding::default()).unwrap();
        assert_eq!(size(&png(&avif, 1024).unwrap()), (40, 20));
    }

    #[test]
    fn a_picture_in_base64_is_encoded_within_the_size_asked() {
        let source = Source::Base64 {
            base64: STANDARD.encode(svg_png(SVG).unwrap()),
            media_type: Some("image/png".into()),
        };
        let encoding = Encoding {
            max_width: Some(20),
            ..Encoding::default()
        };
        let avif = encode(&source, encoding).unwrap();
        assert_eq!(size(&png(&avif, 1024).unwrap()), (20, 10));
    }

    #[test]
    fn a_file_is_read_by_its_absolute_path() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("a.avif");
        std::fs::write(&file, PICTURE).unwrap();
        let source = Source::File { file: file.clone() };
        assert_eq!(source.name().as_deref(), Some("a.avif"));
        assert_eq!(encode(&source, Encoding::default()).unwrap(), PICTURE);
        let relative = Source::File {
            file: PathBuf::from("a.avif"),
        };
        assert_eq!(
            encode(&relative, Encoding::default()).map_err(|e| e.code()),
            Err("invalid_arguments")
        );
        let missing = Source::File {
            file: folder.path().join("missing.png"),
        };
        assert_eq!(
            encode(&missing, Encoding::default()).map_err(|e| e.code()),
            Err("io")
        );
    }

    #[test]
    fn what_is_no_picture_is_refused() {
        let broken = Source::Base64 {
            base64: "%%%".into(),
            media_type: None,
        };
        assert_eq!(
            encode(&broken, Encoding::default()).map_err(|e| e.code()),
            Err("invalid_picture")
        );
        let text = Source::Base64 {
            base64: STANDARD.encode("hello"),
            media_type: None,
        };
        assert_eq!(
            encode(&text, Encoding::default()).map_err(|e| e.code()),
            Err("invalid_picture")
        );
        let svg = Source::Svg {
            svg: "<nope".into(),
        };
        assert_eq!(
            encode(&svg, Encoding::default()).map_err(|e| e.code()),
            Err("invalid_picture")
        );
    }

    #[test]
    fn a_large_picture_is_given_no_larger_than_asked() {
        let avif = encode(&Source::Svg { svg: SVG.into() }, Encoding::default()).unwrap();
        assert_eq!(size(&png(&avif, 10).unwrap()), (10, 5));
        assert_eq!(
            png(b"nope", 10).map_err(|e| e.code()),
            Err("invalid_picture")
        );
    }

    #[test]
    fn the_three_sources_are_told_apart_by_their_fields() {
        let file: Source = serde_json::from_str(r#"{"file": "/x/a.png"}"#).unwrap();
        assert!(matches!(file, Source::File { .. }));
        let svg: Source = serde_json::from_str(r#"{"svg": "<svg/>"}"#).unwrap();
        assert!(matches!(svg, Source::Svg { .. }));
        let bytes: Source = serde_json::from_str(r#"{"base64": "AA=="}"#).unwrap();
        assert!(matches!(
            bytes,
            Source::Base64 {
                media_type: None,
                ..
            }
        ));
    }

    #[test]
    fn a_file_larger_than_the_limit_is_not_read() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("huge.png");
        std::fs::File::create(&file)
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        let source = Source::File { file };
        assert_eq!(
            encode(&source, Encoding::default()).map_err(|e| e.code()),
            Err("limit")
        );
    }

    #[test]
    fn a_quality_is_given_to_the_encoder() {
        let low = Encoding {
            quality: Some(10),
            ..Encoding::default()
        };
        let avif = encode(&Source::Svg { svg: SVG.into() }, low).unwrap();
        assert_eq!(avif.get(4..8), Some(&b"ftyp"[..]));
    }

    #[test]
    fn an_svg_without_a_size_is_refused() {
        let svg = Source::Svg {
            svg: "<svg xmlns='http://www.w3.org/2000/svg' width='0' height='0'/>".into(),
        };
        assert_eq!(
            encode(&svg, Encoding::default()).map_err(|e| e.code()),
            Err("invalid_picture")
        );
    }
}
