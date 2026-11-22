//! Formulas in the notation of `KaTeX`, set with `ratex` and written as SVG.

use ratex_layout::{LayoutOptions, layout, to_display_list};
use ratex_svg::{SvgOptions, render_to_svg};
use ratex_types::color::Color;
use ratex_types::math_style::MathStyle;

use super::glyphs::with_system_glyphs;
use super::{Figure, FigureError, Palette, Setting, hex};

/// The size of text a formula is drawn for, in pixels. A page scales the
/// drawing to the size of the text around it.
const EM: f32 = 20.0;
/// The room around a formula, in pixels: strokes at its edge are not cut.
const PADDING: f32 = 2.0;

/// Sets a formula. Its glyphs become outlines, so no font has to be
/// installed to show it.
pub(super) fn draw(
    source: &str,
    setting: Setting,
    palette: Palette,
) -> Result<Figure, FigureError> {
    let ast = ratex_parser::parser::parse(source)
        .map_err(|error| FigureError::Formula(error.to_string()))?;
    let style = match setting {
        Setting::Block => MathStyle::Display,
        Setting::Line => MathStyle::Text,
    };
    let options = LayoutOptions::default()
        .with_style(style)
        .with_color(Color::from_hex(&hex(palette.ink)).unwrap_or(Color::BLACK));
    let fitted = with_system_glyphs(source, &ast);
    let list = to_display_list(&layout(fitted.as_deref().unwrap_or(&ast), &options));
    if list.items.is_empty() || !list.width.is_finite() || list.width <= 0.0 {
        return Err(FigureError::Empty);
    }
    let svg = render_to_svg(
        &list,
        &SvgOptions {
            font_size: f64::from(EM),
            padding: f64::from(PADDING),
            stroke_width: 1.0,
            embed_glyphs: true,
            font_dir: String::new(),
        },
    )
    // `ratex` declares the size in points, which a rasteriser reads as a
    // third more pixels than the drawing has units.
    .replacen("pt\" height=", "\" height=", 1)
    .replacen("pt\">", "\">", 1);
    Ok(Figure {
        svg,
        width: to_pixels(list.width) + 2.0 * PADDING,
        height: to_pixels(list.height + list.depth) + 2.0 * PADDING,
        baseline: to_pixels(list.height) + PADDING,
        em: EM,
    })
}

/// A length in em as pixels at [`EM`].
fn to_pixels(em: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a formula is a few hundred em at most"
    )]
    let em = em as f32;
    em * EM
}

#[cfg(test)]
mod tests {
    use resvg::usvg;

    use super::super::glyphs::system_glyph_metrics;
    use super::*;

    fn drawn(source: &str, setting: Setting) -> Figure {
        draw(source, setting, Palette::default()).unwrap()
    }

    #[test]
    fn a_formula_has_its_baseline_inside_its_box() {
        let formula = drawn(r"\frac{a}{b} + \sqrt{x}", Setting::Block);
        assert!(formula.width > 0.0 && formula.height > 0.0);
        assert!(formula.baseline > 0.0 && formula.baseline < formula.height);
        assert!((formula.em - EM).abs() < f32::EPSILON);
    }

    #[test]
    fn a_formula_link_keeps_its_glyphs_without_a_destination() {
        let formula = drawn(r"\href{https://example.com/formula}{x}", Setting::Line);
        assert!(formula.svg.contains("<path"));
        assert!(!formula.svg.contains("https://example.com/formula"));
    }

    #[test]
    fn the_glyphs_are_outlines_and_the_size_is_in_pixels() {
        let formula = drawn(r"\frac{a}{b}", Setting::Line);
        assert!(formula.svg.contains("<path"));
        assert!(!formula.svg.contains("pt\""));
        let size = usvg::Tree::from_str(&formula.svg, &usvg::Options::default())
            .unwrap()
            .size();
        assert!((size.width() - formula.width).abs() < 0.5);
        assert!((size.height() - formula.height).abs() < 0.5);
    }

    #[test]
    fn a_formula_is_drawn_in_the_ink_of_the_page() {
        let night = Palette {
            ink: 0x00b8_b8b8,
            page: 0,
        };
        let formula = draw("x", Setting::Line, night).unwrap();
        assert!(formula.svg.contains("rgba(184,184,184,1)"));
    }

    #[test]
    fn a_source_that_is_no_formula_says_why() {
        let refused = draw(r"\frac{", Setting::Line, Palette::default()).unwrap_err();
        assert!(matches!(&refused, FigureError::Formula(why) if !why.is_empty()));
    }

    #[test]
    fn a_letter_outside_the_fonts_gets_the_room_of_its_glyph() {
        // A build machine may have no system font with Cyrillic letters.
        let Some((width, height, _)) = system_glyph_metrics('б') else {
            return;
        };
        let formula = drawn(r"\text{б}", Setting::Line);
        let expected = to_pixels(width) + 2.0 * PADDING;
        assert!((formula.width - expected).abs() < 0.5);
        assert!(formula.baseline >= to_pixels(height));
    }
}
