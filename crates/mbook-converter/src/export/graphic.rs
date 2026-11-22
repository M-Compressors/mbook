//! What a chapter shows with the syntax of a picture: a picture, a diagram
//! or a formula, drawn as PNG for the writers.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use mbook::{Diagram, Formula, Picture};
use resvg::tiny_skia::{Color, Pixmap, Transform};
use resvg::usvg::{self, fontdb};

use super::picture::{self, Png};
use super::{FONT, Format, Result};
use crate::figure::{self, Figure, Palette, Setting};

/// The pixels a drawn figure may take; a larger one is drawn less sharply.
const MAX_FIGURE_PIXELS: f32 = 16_777_216.0;

/// The target of a reference written as a picture.
#[derive(Clone, Copy)]
pub(super) enum Graphic<'a> {
    Picture(&'a Picture),
    Diagram(&'a Diagram, Setting),
    Formula(&'a Formula, Setting),
}

impl<'a> Graphic<'a> {
    /// What tells one drawing apart from another: a formula is drawn apart
    /// for a line and for a block.
    pub(super) fn key(self) -> (&'a str, Option<Setting>) {
        match self {
            Self::Picture(picture) => (picture.path(), None),
            Self::Diagram(diagram, setting) => (diagram.path(), Some(setting)),
            Self::Formula(formula, setting) => (formula.path(), Some(setting)),
        }
    }
}

/// A graphic as a PNG, and the room it takes on a page.
pub(super) struct Drawn {
    pub(super) png: Png,
    /// The width on the page, in the unit of the writer. A picture takes a
    /// unit for each of its pixels.
    pub(super) width: f32,
    /// The height on the page, in the same unit.
    pub(super) height: f32,
    /// From the top to the baseline, in the same unit.
    pub(super) baseline: f32,
    /// How a diagram or a formula stands in its text; `None` for a picture.
    pub(super) figure: Option<Setting>,
}

/// The graphics of one export, each drawn once.
pub(super) struct Drawings<'a> {
    format: Format,
    /// The size of the body text, in the unit of the writer.
    text: f32,
    /// The pixels of a figure for each unit of the writer.
    density: f32,
    drawn: HashMap<(&'a str, Option<Setting>), Option<Rc<Drawn>>>,
}

impl<'a> Drawings<'a> {
    /// Drawings for a writer whose body text is `text` units high, with
    /// `density` pixels of a figure for each unit.
    pub(super) fn new(format: Format, text: f32, density: f32) -> Self {
        Self {
            format,
            text,
            density,
            drawn: HashMap::new(),
        }
    }

    /// The graphic as a PNG. `None` for a diagram or a formula that cannot
    /// be drawn: the writer shows its alternative text, as a reader would.
    ///
    /// # Errors
    ///
    /// A picture cannot be decoded or exceeds the pixel limit.
    pub(super) fn draw(&mut self, graphic: Graphic<'a>) -> Result<Option<Rc<Drawn>>> {
        let key = graphic.key();
        if let Some(drawn) = self.drawn.get(&key) {
            return Ok(drawn.clone());
        }
        let palette = Palette::default();
        let drawn = match graphic {
            Graphic::Picture(source) => {
                let png = picture::png(source, self.format)?;
                let (width, height) = (pixels(png.width), pixels(png.height));
                Some(Drawn {
                    png,
                    width,
                    height,
                    baseline: height,
                    figure: None,
                })
            }
            Graphic::Diagram(source, setting) => figure::diagram(source.source(), palette)
                .ok()
                .and_then(|drawn| self.raster(&drawn, setting, Some(palette.page))),
            Graphic::Formula(source, setting) => figure::formula(source.source(), setting, palette)
                .ok()
                .and_then(|drawn| self.raster(&drawn, setting, None)),
        }
        .map(Rc::new);
        self.drawn.insert(key, drawn.clone());
        Ok(drawn)
    }

    /// A figure scaled to the body text, as pixels on `background`, or on
    /// nothing.
    fn raster(&self, figure: &Figure, setting: Setting, background: Option<u32>) -> Option<Drawn> {
        let scale = self.text / figure.em();
        let png = rasterise(figure, scale * self.density, background)?;
        Some(Drawn {
            png,
            width: figure.width() * scale,
            height: figure.height() * scale,
            baseline: figure.baseline() * scale,
            figure: Some(setting),
        })
    }
}

/// A count of pixels as a length.
fn pixels(count: u32) -> f32 {
    // A picture is bounded by the pixel limit of the decoder, far below the
    // range in which `f32` holds every whole number.
    f32::from(u16::try_from(count).unwrap_or(u16::MAX))
}

/// The fonts the labels of a diagram are drawn with: those of the system,
/// and Noto Sans for any family the system does not have.
fn fonts() -> Arc<fontdb::Database> {
    static FONTS: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    Arc::clone(FONTS.get_or_init(|| {
        let mut fonts = fontdb::Database::new();
        fonts.load_system_fonts();
        fonts.load_font_data(FONT.to_vec());
        fonts.set_sans_serif_family("Noto Sans");
        Arc::new(fonts)
    }))
}

/// The SVG of a figure as PNG, `scale` pixels to a unit of the document.
/// `None` for a document that cannot be read or drawn.
fn rasterise(figure: &Figure, scale: f32, background: Option<u32>) -> Option<Png> {
    let options = usvg::Options {
        fontdb: fonts(),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_str(figure.svg(), &options).ok()?;
    let size = tree.size();
    let area = size.width() * size.height();
    let scale = scale.min((MAX_FIGURE_PIXELS / area).sqrt());
    let size = usvg::Size::from_wh(size.width() * scale, size.height() * scale)?.to_int_size();
    let mut pixmap = Pixmap::new(size.width(), size.height())?;
    if let Some(colour) = background {
        let [_, red, green, blue] = colour.to_be_bytes();
        pixmap.fill(Color::from_rgba8(red, green, blue, 255));
    }
    resvg::render(
        &tree,
        Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(Png {
        bytes: pixmap.encode_png().ok()?,
        width: size.width(),
        height: size.height(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formula(source: &str) -> Figure {
        figure::formula(source, Setting::Line, Palette::default()).unwrap()
    }

    #[test]
    fn a_figure_is_drawn_at_the_scale_it_is_given() {
        let drawn = formula("x+y");
        let once = rasterise(&drawn, 1.0, None).unwrap();
        let twice = rasterise(&drawn, 2.0, None).unwrap();
        assert!((pixels(once.width) - drawn.width()).abs() <= 1.0);
        assert!(twice.width.abs_diff(2 * once.width) <= 1);
        assert!(twice.height.abs_diff(2 * once.height) <= 1);
    }

    #[test]
    fn a_large_figure_is_drawn_within_the_pixel_limit() {
        let drawn = formula("x+y");
        let png = rasterise(&drawn, 10_000.0, None).unwrap();
        assert!(pixels(png.width) * pixels(png.height) <= MAX_FIGURE_PIXELS * 1.01);
    }

    #[test]
    fn a_figure_on_a_page_is_drawn_on_its_colour() {
        let png = rasterise(&formula("x"), 1.0, Some(0x00ff_0000)).unwrap();
        let image = image::load_from_memory(&png.bytes).unwrap().into_rgba8();
        assert_eq!(image.get_pixel(0, 0).0, [255, 0, 0, 255]);
        let bare = rasterise(&formula("x"), 1.0, None).unwrap();
        let image = image::load_from_memory(&bare.bytes).unwrap().into_rgba8();
        assert_eq!(image.get_pixel(0, 0).0[3], 0);
    }

    #[test]
    fn a_document_that_is_no_svg_is_not_drawn() {
        let broken = formula("x").with_svg("not svg");
        assert!(rasterise(&broken, 1.0, None).is_none());
    }
}
