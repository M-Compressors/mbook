//! Letters the fonts of `KaTeX` do not have, such as Cyrillic: `ratex` draws
//! them with a system font, and the layout needs the real size of each.

use ab_glyph::{Font as _, FontRef};
use ratex_font::{FontId, get_char_metrics};
use ratex_layout::{LayoutOptions, layout};
use ratex_parser::parse_node::ParseNode;
use serde_json::{Value, json};

/// The measures of a letter in the system font `ratex` draws it with in
/// place of a font of `KaTeX`: its width, height and depth in em. The fonts
/// are asked in the order `ratex-svg` uses them.
pub(super) fn system_glyph_metrics(ch: char) -> Option<(f64, f64, f64)> {
    use ratex_unicode_font::{
        fallback_font_data_ref, fallback_font_face_index, unicode_font_data_ref,
        unicode_font_face_index,
    };
    let fonts = [
        (unicode_font_data_ref(), unicode_font_face_index()),
        (fallback_font_data_ref(), fallback_font_face_index()),
    ];
    fonts.into_iter().find_map(|(data, index)| {
        let font = FontRef::try_from_slice_and_index(data?.as_slice(), index.unwrap_or(0)).ok()?;
        let glyph = font.glyph_id(ch);
        if glyph.0 == 0 {
            return None;
        }
        let em = f64::from(font.units_per_em()?);
        let bounds = font.outline(glyph)?.bounds;
        let (low, high) = (
            bounds.min.y.min(bounds.max.y),
            bounds.min.y.max(bounds.max.y),
        );
        Some((
            f64::from(font.h_advance_unscaled(glyph)) / em,
            f64::from(high.max(0.0)) / em,
            f64::from((-low).max(0.0)) / em,
        ))
    })
}

/// A letter without metrics of `KaTeX`: `ratex` gives it half an em and the
/// height of a small letter and draws the real glyph, so that neighbours
/// overlap and the top is cut. The answer is the node with a kern up to the
/// real width and an invisible strut of the real height.
fn fit_system_glyph(node: &Value) -> Option<Value> {
    if !matches!(node["type"].as_str(), Some("textord" | "mathord")) {
        return None;
    }
    let mut chars = node["text"].as_str()?.chars();
    let (Some(ch), None) = (chars.next(), chars.next()) else {
        return None;
    };
    let code = u32::from(ch);
    if !ch.is_alphabetic()
        || get_char_metrics(FontId::MainRegular, code).is_some()
        || get_char_metrics(FontId::MathItalic, code).is_some()
    {
        return None;
    }
    let (width, height, depth) = system_glyph_metrics(ch)?;
    // How much room `ratex` gave is asked of `ratex`: for an ideograph it
    // is not half an em.
    let symbol: ParseNode = serde_json::from_value(node.clone()).ok()?;
    let assumed = layout(&[symbol], &LayoutOptions::default());
    let mode = &node["mode"];
    let em = |number: f64| json!({ "number": number, "unit": "em" });
    Some(json!({
        "type": "ordgroup",
        "mode": mode,
        "body": [
            node,
            { "type": "kern", "mode": mode, "dimension": em(width - assumed.width) },
            {
                "type": "vphantom",
                "mode": mode,
                "body": {
                    "type": "rule",
                    "mode": mode,
                    "shift": em(-depth),
                    "width": em(0.0),
                    "height": em(height + depth),
                },
            },
        ],
    }))
}

fn fit_system_glyphs(value: &mut Value) {
    if let Some(fitted) = fit_system_glyph(value) {
        *value = fitted;
        return;
    }
    match value {
        Value::Array(items) => items.iter_mut().for_each(fit_system_glyphs),
        Value::Object(fields) => fields.values_mut().for_each(fit_system_glyphs),
        _ => {}
    }
}

/// The tree to lay out. A `ParseNode` has dozens of variants, so the tree is
/// walked as JSON. A formula of ASCII alone has no such letter and is left
/// as it is: `None`.
pub(super) fn with_system_glyphs(source: &str, ast: &[ParseNode]) -> Option<Vec<ParseNode>> {
    if source.is_ascii() {
        return None;
    }
    let mut tree = serde_json::to_value(ast).ok()?;
    fit_system_glyphs(&mut tree);
    serde_json::from_value(tree).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(source: &str) -> Vec<ParseNode> {
        ratex_parser::parser::parse(source).unwrap()
    }

    #[test]
    fn a_formula_of_ascii_is_left_as_it_is() {
        assert!(with_system_glyphs("a+b", &parsed("a+b")).is_none());
    }

    #[test]
    fn a_letter_of_the_fonts_is_not_fitted() {
        let node = json!({ "type": "mathord", "mode": "math", "text": "a" });
        assert!(fit_system_glyph(&node).is_none());
        let word = json!({ "type": "textord", "mode": "text", "text": "ab" });
        assert!(fit_system_glyph(&word).is_none());
        let other = json!({ "type": "rel", "mode": "math", "text": "б" });
        assert!(fit_system_glyph(&other).is_none());
        let digit = json!({ "type": "textord", "mode": "math", "text": "٣" });
        assert!(fit_system_glyph(&digit).is_none());
    }

    #[test]
    fn a_letter_outside_the_fonts_is_wrapped_with_its_real_measures() {
        // A build machine may have no system font with Cyrillic letters.
        if system_glyph_metrics('б').is_none() {
            return;
        }
        let source = r"\text{б} + x";
        let fitted = with_system_glyphs(source, &parsed(source)).unwrap();
        let tree = serde_json::to_string(&fitted).unwrap();
        assert!(tree.contains("vphantom") && tree.contains("kern"), "{tree}");
    }

    #[test]
    fn a_character_no_font_has_has_no_measures() {
        assert!(system_glyph_metrics('\u{10ffff}').is_none());
    }
}
