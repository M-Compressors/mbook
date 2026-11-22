//! Diagrams and formulas of a book, drawn as SVG.
//!
//! A book stores a diagram as Mermaid notation and a formula as the notation
//! of `KaTeX` (RFC 0001, "Diagram and formula resources"). The drawing is
//! that of `mbook_converter::figure`, the one export uses. Everything here is
//! synchronous and meant to run on a background task: [`render`] turns a
//! source into a [`Figure`], an SVG document with its size, in the colours of
//! the page it will stand on. What was drawn is kept in a bounded cache, so
//! asking again is cheap.
//!
//! GPUI rasterises an SVG image itself; [`Figure::image`] gives it the
//! document at the density of the screen.

mod cache;
mod svg;
mod table;

use std::sync::{Arc, Mutex};

use gpui_kit::{Hsla, Image, Rgba};
use mbook_converter::figure;

use crate::shared::theme::PageTone;

pub(crate) use figure::{FigureError, Setting};
pub(crate) use table::{FigureSource, FigureSources, Figures, ImageReference};

/// What a figure is drawn from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum FigureKind {
    /// A diagram in Mermaid notation.
    Diagram,
    /// A formula in the notation of `KaTeX`.
    Formula,
}

impl FigureKind {
    /// What the interface calls one figure of this kind.
    pub(crate) const fn noun(self) -> &'static str {
        match self {
            Self::Diagram => "diagram",
            Self::Formula => "formula",
        }
    }

    /// The folder and the extension the editor proposes for a new figure.
    pub(crate) const fn home(self) -> (&'static str, &'static str) {
        match self {
            Self::Diagram => ("dia", "mmd"),
            Self::Formula => ("math", "tex"),
        }
    }
}

/// The colours a figure is drawn in, as `0xRRGGBB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Palette {
    /// The colour of the text of the page.
    pub(crate) ink: u32,
    /// The colour of the page.
    pub(crate) page: u32,
}

impl From<PageTone> for Palette {
    /// The colours of a page tone.
    fn from(tone: PageTone) -> Self {
        Self {
            ink: tone.text().to_hex(),
            page: tone.page().to_hex(),
        }
    }
}

impl Palette {
    /// The colours of a page and of its text as GPUI has them.
    pub(crate) fn of(page: Hsla, ink: Hsla) -> Self {
        let packed = |colour: Hsla| {
            let colour = Rgba::from(colour);
            let byte = |channel: f32| {
                let scaled = (channel * 255.0).round().clamp(0.0, 255.0);
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "the value was clamped to the range of a byte"
                )]
                let byte = scaled as u32;
                byte
            };
            byte(colour.r) << 16 | byte(colour.g) << 8 | byte(colour.b)
        };
        Self {
            ink: packed(ink),
            page: packed(page),
        }
    }

    /// Dark text on a white page.
    #[cfg(test)]
    pub(crate) const PAPER: Self = Self {
        ink: 0x001b_1b1b,
        page: 0x00ff_ffff,
    };
}

impl From<Palette> for figure::Palette {
    /// The colours the renderer is given.
    fn from(palette: Palette) -> Self {
        Self {
            ink: palette.ink,
            page: palette.page,
        }
    }
}

/// A diagram or a formula that was drawn: an SVG document and its measures.
#[derive(Debug)]
pub(crate) struct Figure {
    /// The document, sized for text of [`Figure::em`] pixels.
    svg: String,
    /// The width in pixels, for text of [`Figure::em`] pixels.
    pub(crate) width: f32,
    /// The height in pixels, for text of [`Figure::em`] pixels.
    pub(crate) height: f32,
    /// From the top of the figure to the baseline of its text; the height
    /// for a diagram, which has none.
    pub(crate) baseline: f32,
    /// The size of the text the figure was drawn for.
    pub(crate) em: f32,
    /// The last image made by [`Figure::image`] and the density it was made
    /// for, as the bits of the number.
    sized: Mutex<Option<(u32, Arc<Image>)>>,
}

impl From<figure::Figure> for Figure {
    /// A drawing of the renderer, ready to be shown.
    fn from(drawn: figure::Figure) -> Self {
        let (width, height, baseline, em) =
            (drawn.width(), drawn.height(), drawn.baseline(), drawn.em());
        Self {
            svg: drawn.into_svg(),
            width,
            height,
            baseline,
            em,
            sized: Mutex::new(None),
        }
    }
}

impl Figure {
    /// The document as it was drawn.
    #[cfg(test)]
    pub(crate) fn svg(&self) -> &str {
        &self.svg
    }

    /// The bytes the figure takes in the cache.
    fn byte_len(&self) -> usize {
        self.svg.len()
    }

    /// The image for a screen on which one unit of the document takes
    /// `density` device pixels.
    ///
    /// GPUI rasterises an SVG at the size the document declares and then
    /// stretches the raster over the element. At a ratio that is not whole
    /// the strokes of letters blur, so the declared size is set to the size
    /// on the screen; the `viewBox` keeps the drawing.
    pub(crate) fn image(&self, density: f32) -> Arc<Image> {
        let key = density.to_bits();
        let mut sized = self.sized.lock().ok();
        if let Some((made_for, image)) = sized.as_deref().and_then(Option::as_ref)
            && *made_for == key
        {
            return Arc::clone(image);
        }
        let image = self.draw(density);
        if let Some(sized) = sized.as_deref_mut() {
            *sized = Some((key, Arc::clone(&image)));
        }
        image
    }

    /// The image at `density`, as [`Figure::image`] makes it, without
    /// keeping it. For a second place that shows the figure at another size,
    /// which would otherwise make the two take turns at the one image kept.
    pub(crate) fn draw(&self, density: f32) -> Arc<Image> {
        let document = svg::resized(&self.svg, self.width * density, self.height * density)
            .unwrap_or_else(|| self.svg.clone());
        svg::image(document)
    }
}

/// Draws a diagram or a formula in the colours of a page, or takes what was
/// drawn before from the cache.
///
/// A diagram is the same in a line and alone, so `setting` matters for a
/// formula only. Leading and trailing white space of the source is not a
/// part of it.
///
/// # Errors
///
/// The source is empty, is not in the notation of its kind, or made the
/// renderer fail. The failure is remembered like a figure.
pub(crate) fn render(
    kind: FigureKind,
    source: &str,
    setting: Setting,
    palette: Palette,
) -> Result<Arc<Figure>, FigureError> {
    let source = source.trim();
    let setting = match kind {
        FigureKind::Diagram => Setting::Block,
        FigureKind::Formula => setting,
    };
    cache::get_or_draw((kind, setting, palette), source, || {
        // The renderer refuses an empty source and catches its own panics.
        let drawn = match kind {
            FigureKind::Diagram => figure::diagram(source, palette.into()),
            FigureKind::Formula => figure::formula(source, setting, palette.into()),
        };
        drawn.map(Figure::from)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NIGHT: Palette = Palette {
        ink: 0x00b8_b8b8,
        page: 0,
    };

    #[test]
    fn a_palette_is_made_of_a_page_tone_and_of_gpui_colours() {
        for tone in PageTone::ALL {
            let (page, ink) = crate::shared::ui::page_colors(tone);
            assert_eq!(Palette::of(page, ink), Palette::from(tone));
        }
    }

    #[test]
    fn the_renderer_is_given_the_colours_of_the_palette() {
        let colours = figure::Palette::from(NIGHT);
        assert_eq!((colours.ink, colours.page), (NIGHT.ink, NIGHT.page));
    }

    #[test]
    fn a_kind_names_itself_and_its_folder() {
        assert_eq!(FigureKind::Diagram.noun(), "diagram");
        assert_eq!(FigureKind::Formula.noun(), "formula");
        assert_eq!(FigureKind::Diagram.home(), ("dia", "mmd"));
        assert_eq!(FigureKind::Formula.home(), ("math", "tex"));
    }

    #[test]
    fn an_empty_source_draws_nothing() {
        for kind in [FigureKind::Diagram, FigureKind::Formula] {
            let drawn = render(kind, " \n", Setting::Block, Palette::PAPER);
            assert_eq!(drawn.err(), Some(FigureError::Empty));
        }
    }

    #[test]
    fn a_figure_drawn_twice_is_drawn_once() {
        let first = render(FigureKind::Formula, "a+b", Setting::Line, Palette::PAPER).unwrap();
        let again = render(FigureKind::Formula, " a+b ", Setting::Line, Palette::PAPER).unwrap();
        assert!(Arc::ptr_eq(&first, &again));
    }

    #[test]
    fn a_formula_is_drawn_apart_for_a_line_and_for_a_block() {
        let source = r"\sum_{i=1}^{n} i";
        let line = render(FigureKind::Formula, source, Setting::Line, Palette::PAPER).unwrap();
        let block = render(FigureKind::Formula, source, Setting::Block, Palette::PAPER).unwrap();
        assert!(!Arc::ptr_eq(&line, &block));
        assert!(block.height > line.height, "limits go over and under");
    }

    #[test]
    fn a_diagram_is_the_same_in_a_line_and_alone() {
        let source = "flowchart LR\n  A --> B";
        let line = render(FigureKind::Diagram, source, Setting::Line, Palette::PAPER).unwrap();
        let block = render(FigureKind::Diagram, source, Setting::Block, Palette::PAPER).unwrap();
        assert!(Arc::ptr_eq(&line, &block));
    }

    #[test]
    fn a_figure_is_drawn_again_for_another_page_tone() {
        let paper = render(FigureKind::Formula, "x", Setting::Line, Palette::PAPER).unwrap();
        let night = render(FigureKind::Formula, "x", Setting::Line, NIGHT).unwrap();
        assert!(paper.svg().contains("rgba(27,27,27,1)"));
        assert_ne!(paper.svg(), night.svg());
    }

    #[test]
    fn a_failure_is_remembered_like_a_figure() {
        let first = render(
            FigureKind::Formula,
            r"\frac{",
            Setting::Line,
            Palette::PAPER,
        );
        let again = render(
            FigureKind::Formula,
            r"\frac{",
            Setting::Line,
            Palette::PAPER,
        );
        assert!(matches!(first, Err(FigureError::Formula(_))));
        assert_eq!(first.err(), again.err());
    }

    #[test]
    fn the_image_of_a_figure_is_sized_for_the_screen_and_kept() {
        let figure = render(
            FigureKind::Formula,
            r"\frac{a}{b}",
            Setting::Line,
            Palette::PAPER,
        )
        .unwrap();
        let image = figure.image(2.0);
        let document = std::str::from_utf8(image.bytes()).unwrap();
        let (width, height) = svg::size(document).unwrap();
        assert!((width - figure.width * 2.0).abs() < 0.01);
        assert!((height - figure.height * 2.0).abs() < 0.01);
        assert!(Arc::ptr_eq(&image, &figure.image(2.0)));
        assert!(!Arc::ptr_eq(&image, &figure.image(3.0)));
    }
}
