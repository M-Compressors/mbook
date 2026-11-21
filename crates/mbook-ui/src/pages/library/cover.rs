//! The cover of a book on the shelf: its thumbnail, or a plate drawn from its
//! title.
//!
//! Whether a thumbnail exists is decided by the page on the background
//! executor; this module only draws what it is told.

use std::path::PathBuf;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyElement, App, FontWeight, Hsla, IntoElement, ObjectFit, ParentElement as _, Rgba,
    Styled as _, StyledImage as _, div, img, px,
};

use crate::shared::theme::Rgb;

/// The proportions of a cover that has no picture: two wide, three high.
pub(super) const PLATE_RATIO: f32 = 1.5;
/// The flattest cover that is drawn as it is; a wider picture is shown inside
/// a box of this ratio.
const MIN_RATIO: f32 = 0.8;
/// The tallest cover that is drawn as it is.
const MAX_RATIO: f32 = 1.9;
/// The contrast text has against its plate: WCAG 2 level AA.
const TEXT_CONTRAST: f32 = 4.5;

/// The height of a cover `width` pixels wide, in the proportions of its
/// picture (`size` is the width and the height of the picture, if there is
/// one), kept within what a shelf can hold.
pub(super) fn cover_height(width: f32, size: Option<(u32, u32)>) -> f32 {
    let ratio = match size {
        Some((w, h)) if w > 0 && h > 0 => {
            // Pictures are far smaller than 2^24 pixels on a side.
            #[expect(
                clippy::cast_precision_loss,
                reason = "a picture is far narrower than 2^24 pixels"
            )]
            let ratio = h as f32 / w as f32;
            ratio.clamp(MIN_RATIO, MAX_RATIO)
        }
        _ => PLATE_RATIO,
    };
    (width * ratio).round()
}

/// The colours of a plate: its ground and the ink of the title on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Plate {
    /// The colour of the plate.
    pub(super) ground: Hsla,
    /// The colour of the title.
    pub(super) ink: Hsla,
}

/// A number the same title always gives, on every platform and every run.
fn title_hash(title: &str) -> u32 {
    // FNV-1a: a rule of a few lines, and fixed, unlike `DefaultHasher`.
    title.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
}

fn rgb(colour: Hsla) -> Rgb {
    let Rgba { r, g, b, .. } = Rgba::from(colour);
    // A channel is a number from 0 to 1; rounding it gives a byte.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a channel is clamped to 0..=255 before it is cast"
    )]
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb(byte(r), byte(g), byte(b))
}

/// The plate of a book called `title`: the accent turned to one of twelve
/// hues, dark enough that the best of `inks` reads on it at the contrast of
/// level AA.
///
/// `accent` and `inks` come from the theme, so the plates follow the
/// appearance; the title only picks the hue.
pub(super) fn plate(title: &str, accent: Hsla, inks: &[Hsla]) -> Plate {
    let step = title_hash(title) % 12;
    // `step` is below 12.
    #[expect(
        clippy::cast_precision_loss,
        reason = "`step` is a number below twelve"
    )]
    let hue = (accent.h + step as f32 / 12.0).fract();
    let mut ground = Hsla {
        h: hue,
        s: accent.s.clamp(0.35, 0.7),
        l: 0.42,
        a: 1.0,
    };
    let best = |ground: Hsla| {
        inks.iter()
            .copied()
            .map(|ink| (rgb(ink).contrast(rgb(ground)), ink))
            .fold(
                (0.0_f32, accent),
                |best, next| {
                    if next.0 > best.0 { next } else { best }
                },
            )
    };
    // Darker until some ink reads; at 0.1 lightness every plate is nearly
    // black and any light ink passes.
    while ground.l > 0.1 && best(ground).0 < TEXT_CONTRAST {
        ground.l -= 0.02;
    }
    Plate {
        ground,
        ink: best(ground).1,
    }
}

/// A cover drawn from the title and the first author: a coloured plate with
/// the title set on it.
pub(super) fn drawn_cover(
    title: &str,
    author: Option<&str>,
    width: f32,
    height: f32,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let plate = plate(
        title,
        theme.primary,
        &[theme.foreground, theme.background, theme.primary_foreground],
    );
    div()
        .w(px(width))
        .h(px(height))
        .flex_none()
        .flex()
        .flex_col()
        .justify_between()
        .overflow_hidden()
        .p(px(width * 0.08))
        .rounded(theme.radius)
        .bg(plate.ground)
        .text_color(plate.ink)
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .line_clamp(5)
                .child(title.to_owned()),
        )
        .children(author.map(|author| {
            div()
                .text_xs()
                .truncate()
                .child(author.to_owned())
                .into_any_element()
        }))
        .into_any_element()
}

/// The thumbnail at `path`, in a box `width` by `height` pixels, with the
/// drawn cover standing in while it loads and when it cannot be read.
pub(super) fn thumbnail(
    path: PathBuf,
    title: &str,
    author: Option<&str>,
    width: f32,
    height: f32,
    cx: &App,
) -> AnyElement {
    let (title_for_fallback, author_for_fallback) = (title.to_owned(), author.map(str::to_owned));
    let fallback = {
        let theme = cx.theme();
        let (primary, inks) = (
            theme.primary,
            [theme.foreground, theme.background, theme.primary_foreground],
        );
        move || {
            let plate = plate(&title_for_fallback, primary, &inks);
            div()
                .w(px(width))
                .h(px(height))
                .flex()
                .flex_col()
                .justify_end()
                .p(px(width * 0.08))
                .bg(plate.ground)
                .text_color(plate.ink)
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .line_clamp(5)
                .child(
                    author_for_fallback
                        .clone()
                        .unwrap_or_else(|| title_for_fallback.clone()),
                )
                .into_any_element()
        }
    };
    let loading = fallback.clone();
    img(path)
        .w(px(width))
        .h(px(height))
        .flex_none()
        .rounded(cx.theme().radius)
        .object_fit(ObjectFit::Contain)
        .with_loading(loading)
        .with_fallback(fallback)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::theme::Chrome;
    use crate::shared::ui::ChromeColors;

    fn colours(chrome: &Chrome) -> (Hsla, [Hsla; 3]) {
        let c = ChromeColors::from(chrome);
        (c.accent, [c.ink, c.canvas, c.on_accent])
    }

    #[test]
    fn the_same_title_always_gets_the_same_plate() {
        let (accent, inks) = colours(&Chrome::LIGHT);
        assert_eq!(
            plate("The Example", accent, &inks),
            plate("The Example", accent, &inks)
        );
    }

    #[test]
    fn titles_are_spread_over_the_hues() {
        let (accent, inks) = colours(&Chrome::LIGHT);
        let hues: std::collections::HashSet<u32> = (0..60)
            .map(|n| {
                let ground = plate(&format!("Book number {n}"), accent, &inks).ground;
                // A hue as a whole number of degrees.
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a hue is between 0 and 1"
                )]
                let degrees = (ground.h * 360.0).round() as u32;
                degrees
            })
            .collect();
        assert!(hues.len() >= 6, "{hues:?}");
    }

    #[test]
    fn the_title_on_every_plate_reads_at_the_contrast_of_level_aa() {
        for chrome in [Chrome::LIGHT, Chrome::DARK] {
            let (accent, inks) = colours(&chrome);
            for n in 0..24 {
                let plate = plate(&format!("title {n}"), accent, &inks);
                let ratio = rgb(plate.ink).contrast(rgb(plate.ground));
                assert!(ratio >= TEXT_CONTRAST, "title {n}: {ratio}");
            }
        }
    }

    #[test]
    fn an_empty_title_still_gets_a_plate() {
        let (accent, inks) = colours(&Chrome::LIGHT);
        let plate = plate("", accent, &inks);
        assert!(rgb(plate.ink).contrast(rgb(plate.ground)) >= TEXT_CONTRAST);
    }

    #[test]
    fn a_cover_without_a_picture_has_the_proportions_of_a_plate() {
        assert!((cover_height(100.0, None) - 150.0).abs() < f32::EPSILON);
        assert!((cover_height(100.0, Some((0, 10))) - 150.0).abs() < f32::EPSILON);
        assert!((cover_height(100.0, Some((10, 0))) - 150.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_cover_keeps_the_proportions_of_its_picture() {
        assert!((cover_height(100.0, Some((400, 600))) - 150.0).abs() < f32::EPSILON);
        assert!((cover_height(100.0, Some((400, 500))) - 125.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_very_tall_or_flat_picture_is_held_to_what_a_shelf_can_hold() {
        assert!((cover_height(100.0, Some((100, 1000))) - 190.0).abs() < f32::EPSILON);
        assert!((cover_height(100.0, Some((1000, 100))) - 80.0).abs() < f32::EPSILON);
    }
}
