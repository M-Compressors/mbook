//! Colours of the application as GPUI colours.

use gpui_kit::{Hsla, rgb};

use crate::shared::theme::{Chrome, PageTone, Rgb};

impl From<Rgb> for Hsla {
    /// The colour as GPUI draws it.
    fn from(value: Rgb) -> Self {
        rgb(value.to_hex()).into()
    }
}

/// The colours of the interface in one appearance, as GPUI colours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ChromeColors {
    /// The background of the library.
    pub(crate) canvas: Hsla,
    /// Sidebars, sheets, menus.
    pub(crate) surface: Hsla,
    /// Text.
    pub(crate) ink: Hsla,
    /// Authors, captions.
    pub(crate) ink_muted: Hsla,
    /// Separators, which carry no meaning.
    pub(crate) line: Hsla,
    /// The edge of a control: whatever must be seen to use it.
    pub(crate) outline: Hsla,
    /// Selection, progress, the primary button, the focus ring.
    pub(crate) accent: Hsla,
    /// Text on the accent.
    pub(crate) on_accent: Hsla,
}

impl From<&Chrome> for ChromeColors {
    fn from(chrome: &Chrome) -> Self {
        Self {
            canvas: chrome.canvas.into(),
            surface: chrome.surface.into(),
            ink: chrome.ink.into(),
            ink_muted: chrome.ink_muted.into(),
            line: chrome.line.into(),
            outline: chrome.outline.into(),
            accent: chrome.accent.into(),
            on_accent: chrome.on_accent.into(),
        }
    }
}

/// The colour of the page of a book and of its text, for a page tone.
pub(crate) fn page_colors(tone: PageTone) -> (Hsla, Hsla) {
    (tone.page().into(), tone.text().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_keeps_its_value_in_gpui() {
        let colour = Hsla::from(Rgb::hex(0x0046_50D8));
        assert_eq!(colour, Hsla::from(rgb(0x0046_50D8)));
    }

    #[test]
    fn the_colours_of_a_chrome_are_its_tokens() {
        let colours = ChromeColors::from(&Chrome::DARK);
        assert_eq!(colours.canvas, Hsla::from(Chrome::DARK.canvas));
        assert_eq!(colours.accent, Hsla::from(Chrome::DARK.accent));
        assert_ne!(colours, ChromeColors::from(&Chrome::LIGHT));
    }

    #[test]
    fn a_page_tone_gives_its_page_and_its_text() {
        let (page, text) = page_colors(PageTone::Sepia);
        assert_eq!(page, Hsla::from(PageTone::Sepia.page()));
        assert_eq!(text, Hsla::from(PageTone::Sepia.text()));
    }
}
