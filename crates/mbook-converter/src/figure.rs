//! Diagrams and formulas of a book, drawn as SVG.
//!
//! A book stores a diagram as Mermaid notation and a formula as the notation
//! of `KaTeX` (RFC 0001, "Diagram and formula resources"), not as pictures.
//! [`diagram`] and [`formula`] draw one source as a [`Figure`]: an SVG
//! document with its size and baseline, in the colours of a [`Palette`].
//! The writers of [`Converter::export`](crate::Converter::export) use them to
//! show figures as pictures; a reading application can use them to show
//! figures on its pages.
//!
//! Both calls are synchronous and take milliseconds for a formula and up to
//! tens of them for a diagram. Nothing is fetched or run on behalf of a
//! source: a `click` of Mermaid and `\href` of `KaTeX` draw nothing that
//! could be followed.
//!
//! ```
//! use mbook_converter::figure::{self, Palette, Setting};
//!
//! let formula = figure::formula(r"v = \frac{s}{t}", Setting::Block, Palette::default())?;
//! assert!(formula.svg().starts_with("<svg"));
//! assert!(formula.baseline() < formula.height());
//! # Ok::<(), mbook_converter::figure::FigureError>(())
//! ```

mod diagram;
mod formula;
mod glyphs;

use std::panic::{AssertUnwindSafe, catch_unwind};

/// How a figure stands in the text.
///
/// A formula alone in its paragraph is a display formula; one inside a line
/// is set in the style of running text. A diagram is the same in both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Setting {
    /// Alone in a paragraph: a formula is set as a display formula.
    Block,
    /// Inside a line: a formula is set in the style of running text.
    Line,
}

/// The colours a figure is drawn in, each as `0xRRGGBB`.
///
/// The default is dark text on a white page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Palette {
    /// The colour of the text of the page.
    pub ink: u32,
    /// The colour of the page.
    pub page: u32,
}

impl Default for Palette {
    /// Dark text on a white page.
    fn default() -> Self {
        Self {
            ink: 0x001b_1b1b,
            page: 0x00ff_ffff,
        }
    }
}

impl Palette {
    /// A colour between the page and the ink: `part` of 0 is the page and
    /// 1 the ink.
    fn mix(self, part: f32) -> u32 {
        let channel = |shift: u32| {
            let page = f32::from(u8::try_from((self.page >> shift) & 0xff).unwrap_or(0));
            let ink = f32::from(u8::try_from((self.ink >> shift) & 0xff).unwrap_or(0));
            let mixed = (page + (ink - page) * part).round().clamp(0.0, 255.0);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the value was clamped to the range of a byte"
            )]
            let byte = mixed as u32;
            byte << shift
        };
        channel(16) | channel(8) | channel(0)
    }

    /// Whether the page is darker than its text.
    fn is_dark(self) -> bool {
        let light = |colour: u32| (colour >> 16 & 0xff) + (colour >> 8 & 0xff) + (colour & 0xff);
        light(self.page) < light(self.ink)
    }
}

/// `#rrggbb` for a colour.
fn hex(colour: u32) -> String {
    format!("#{:06x}", colour & 0x00ff_ffff)
}

/// A diagram or a formula that was drawn: an SVG document and its measures.
///
/// The measures are in the units of the document, for text of
/// [`em`](Figure::em) units: a page with text of another size scales the
/// figure by the ratio of the two.
#[derive(Clone, Debug, PartialEq)]
pub struct Figure {
    svg: String,
    width: f32,
    height: f32,
    baseline: f32,
    em: f32,
}

impl Figure {
    /// The SVG document.
    #[must_use]
    pub fn svg(&self) -> &str {
        &self.svg
    }

    /// The SVG document, without copying it.
    #[must_use]
    pub fn into_svg(self) -> String {
        self.svg
    }

    /// The width.
    #[must_use]
    pub fn width(&self) -> f32 {
        self.width
    }

    /// The height.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.height
    }

    /// From the top of the figure to the baseline of its text: where a
    /// formula inside a line sits on the line. A diagram has none, and its
    /// baseline is its height.
    #[must_use]
    pub fn baseline(&self) -> f32 {
        self.baseline
    }

    /// The size of the text the figure was drawn for.
    #[must_use]
    pub fn em(&self) -> f32 {
        self.em
    }
}

#[cfg(test)]
impl Figure {
    /// The figure with another document, for the tests of what draws one.
    pub(crate) fn with_svg(mut self, svg: &str) -> Self {
        svg.clone_into(&mut self.svg);
        self
    }
}

/// Why a diagram or a formula could not be drawn.
///
/// The messages of the renderers are kept as text, so that the error can be
/// cloned and remembered with the figure it stands for.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum FigureError {
    /// The source holds nothing to draw.
    #[error("there is nothing to draw")]
    Empty,
    /// The source is not a diagram in Mermaid notation that can be drawn.
    #[error("the diagram cannot be drawn: {0}")]
    Diagram(String),
    /// The source is not a formula in the notation of `KaTeX` that can be
    /// set.
    #[error("the formula cannot be set: {0}")]
    Formula(String),
    /// The renderer stopped unexpectedly.
    #[error("the renderer stopped unexpectedly")]
    Renderer,
}

/// Draws a diagram in Mermaid notation.
///
/// Leading and trailing white space of the source is not a part of it. The
/// labels are measured for the font "Helvetica Neue" and its look-alikes,
/// at [`Figure::em`] of 16.
///
/// # Errors
///
/// [`FigureError::Empty`] for a source of white space,
/// [`FigureError::Diagram`] for one that is no diagram `merman` can draw,
/// and [`FigureError::Renderer`] if the renderer stopped.
///
/// # Examples
///
/// ```
/// use mbook_converter::figure::{self, Palette};
///
/// let drawn = figure::diagram("flowchart LR\n  A --> B", Palette::default())?;
/// assert!(drawn.width() > 0.0);
/// assert_eq!(drawn.baseline(), drawn.height());
/// # Ok::<(), mbook_converter::figure::FigureError>(())
/// ```
pub fn diagram(source: &str, palette: Palette) -> Result<Figure, FigureError> {
    guarded(source, |source| diagram::draw(source, palette))
}

/// Sets a formula in the notation of `KaTeX`, without the `$` that surround
/// one in running text.
///
/// Leading and trailing white space of the source is not a part of it. The
/// glyphs become outlines, so no font has to be installed to show the
/// document; [`Figure::em`] is 20.
///
/// # Errors
///
/// [`FigureError::Empty`] for a source of white space or one that sets
/// nothing, [`FigureError::Formula`] for one that `ratex` cannot parse, and
/// [`FigureError::Renderer`] if the renderer stopped.
///
/// # Examples
///
/// ```
/// use mbook_converter::figure::{self, FigureError, Palette, Setting};
///
/// let palette = Palette::default();
/// let line = figure::formula(r"\sum_{i=1}^{n} i", Setting::Line, palette)?;
/// let block = figure::formula(r"\sum_{i=1}^{n} i", Setting::Block, palette)?;
/// assert!(block.height() > line.height());
///
/// let broken = figure::formula(r"\frac{", Setting::Line, palette);
/// assert!(matches!(broken, Err(FigureError::Formula(_))));
/// # Ok::<(), FigureError>(())
/// ```
pub fn formula(source: &str, setting: Setting, palette: Palette) -> Result<Figure, FigureError> {
    guarded(source, |source| formula::draw(source, setting, palette))
}

/// Trims the source, refuses an empty one, and turns a panic of a renderer
/// into [`FigureError::Renderer`]: the renderers are given whatever a book
/// holds.
fn guarded(
    source: &str,
    draw: impl FnOnce(&str) -> Result<Figure, FigureError>,
) -> Result<Figure, FigureError> {
    let source = source.trim();
    if source.is_empty() {
        return Err(FigureError::Empty);
    }
    // The renderers keep no state between calls that a panic could leave
    // broken.
    catch_unwind(AssertUnwindSafe(|| draw(source))).unwrap_or(Err(FigureError::Renderer))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NIGHT: Palette = Palette {
        ink: 0x00b8_b8b8,
        page: 0,
    };

    #[test]
    fn a_colour_between_page_and_ink_is_mixed_by_channel() {
        let palette = Palette {
            ink: 0x0000_0000,
            page: 0x00ff_8040,
        };
        assert_eq!(palette.mix(0.0), 0x00ff_8040);
        assert_eq!(palette.mix(1.0), 0);
        assert_eq!(palette.mix(0.5), 0x0080_4020);
    }

    #[test]
    fn a_page_darker_than_its_text_is_dark() {
        assert!(!Palette::default().is_dark());
        assert!(NIGHT.is_dark());
    }

    #[test]
    fn a_colour_is_written_as_six_hex_digits() {
        assert_eq!(hex(0x0000_0a0b), "#000a0b");
    }

    #[test]
    fn an_empty_source_draws_nothing() {
        assert_eq!(diagram(" \n", Palette::default()), Err(FigureError::Empty));
        assert_eq!(
            formula("\t", Setting::Block, Palette::default()),
            Err(FigureError::Empty)
        );
    }

    #[test]
    fn white_space_around_a_source_is_not_a_part_of_it() {
        let palette = Palette::default();
        assert_eq!(
            formula(" a+b \n", Setting::Line, palette),
            formula("a+b", Setting::Line, palette)
        );
    }

    #[test]
    fn a_panicking_renderer_is_reported_as_an_error() {
        let drawn = guarded("x", |_| -> Result<Figure, FigureError> {
            std::panic::panic_any("the renderer broke")
        });
        assert_eq!(drawn, Err(FigureError::Renderer));
    }

    #[test]
    fn a_figure_gives_its_document_and_measures() {
        let figure = Figure {
            svg: "<svg/>".to_owned(),
            width: 3.0,
            height: 2.0,
            baseline: 1.5,
            em: 20.0,
        };
        assert_eq!(
            (
                figure.width(),
                figure.height(),
                figure.baseline(),
                figure.em()
            ),
            (3.0, 2.0, 1.5, 20.0)
        );
        assert_eq!(figure.svg(), "<svg/>");
        assert_eq!(figure.into_svg(), "<svg/>");
    }

    #[test]
    fn a_failure_says_what_the_renderer_said() {
        assert_eq!(
            FigureError::Diagram("no diagram type".into()).to_string(),
            "the diagram cannot be drawn: no diagram type"
        );
        assert_eq!(
            FigureError::Formula("missing brace".into()).to_string(),
            "the formula cannot be set: missing brace"
        );
        assert_eq!(FigureError::Empty.to_string(), "there is nothing to draw");
        assert_eq!(
            FigureError::Renderer.to_string(),
            "the renderer stopped unexpectedly"
        );
    }
}
