//! The colours of the application, and the rule they are held to.
//!
//! The values are the tokens of RFC 0004, "Visual direction". Views take
//! colours from here and write no colour of their own.

/// A colour as red, green and blue, each from 0 to 255.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rgb(pub(crate) u8, pub(crate) u8, pub(crate) u8);

impl Rgb {
    /// The colour written as `0xRRGGBB`.
    pub(crate) const fn hex(value: u32) -> Self {
        Self(
            ((value >> 16) & 0xFF) as u8,
            ((value >> 8) & 0xFF) as u8,
            (value & 0xFF) as u8,
        )
    }

    /// The colour as `0xRRGGBB`, the form GPUI's `rgb` takes.
    pub(crate) const fn to_hex(self) -> u32 {
        ((self.0 as u32) << 16) | ((self.1 as u32) << 8) | self.2 as u32
    }

    /// The relative luminance of WCAG 2: 0 for black, 1 for white.
    pub(crate) fn luminance(self) -> f32 {
        fn channel(value: u8) -> f32 {
            let value = f32::from(value) / 255.0;
            if value <= 0.040_45 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.0) + 0.7152 * channel(self.1) + 0.0722 * channel(self.2)
    }

    /// The contrast ratio of WCAG 2 between two colours, from 1 to 21.
    pub(crate) fn contrast(self, other: Self) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        let (lighter, darker) = if a >= b { (a, b) } else { (b, a) };
        (lighter + 0.05) / (darker + 0.05)
    }
}

/// The colours of the interface in one appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Chrome {
    /// The background of the library.
    pub(crate) canvas: Rgb,
    /// Sidebars, sheets, menus.
    pub(crate) surface: Rgb,
    /// Text.
    pub(crate) ink: Rgb,
    /// Authors, captions.
    pub(crate) ink_muted: Rgb,
    /// Separators, which carry no meaning.
    pub(crate) line: Rgb,
    /// The border of a field, the track of a progress line, the edge of a
    /// control: whatever must be seen to use a control.
    pub(crate) outline: Rgb,
    /// Selection, progress, the primary button, the outline of the focus.
    pub(crate) accent: Rgb,
    /// Text on the accent.
    pub(crate) on_accent: Rgb,
}

impl Chrome {
    /// The light appearance.
    pub(crate) const LIGHT: Self = Self {
        canvas: Rgb::hex(0x00F4_F5F7),
        surface: Rgb::hex(0x00FF_FFFF),
        ink: Rgb::hex(0x0016_181D),
        ink_muted: Rgb::hex(0x005F_6673),
        line: Rgb::hex(0x00E3_E5EA),
        outline: Rgb::hex(0x007C_8390),
        accent: Rgb::hex(0x0046_50D8),
        on_accent: Rgb::hex(0x00FF_FFFF),
    };

    /// The dark appearance.
    pub(crate) const DARK: Self = Self {
        canvas: Rgb::hex(0x0015_161A),
        surface: Rgb::hex(0x001E_2026),
        ink: Rgb::hex(0x00EC_EDEF),
        ink_muted: Rgb::hex(0x009A_A0AB),
        line: Rgb::hex(0x002C_2F37),
        outline: Rgb::hex(0x0074_7B89),
        accent: Rgb::hex(0x008C_93FF),
        on_accent: Rgb::hex(0x0015_161A),
    };
}

/// The colours of the page of a book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PageTone {
    /// Black on white.
    Paper,
    /// Brown on cream.
    Sepia,
    /// Light grey on dark grey.
    Dusk,
    /// Grey on black.
    Night,
}

impl PageTone {
    /// Every tone, in the order the settings offer them.
    pub(crate) const ALL: [Self; 4] = [Self::Paper, Self::Sepia, Self::Dusk, Self::Night];

    /// The colour of the page.
    pub(crate) const fn page(self) -> Rgb {
        match self {
            Self::Paper => Rgb::hex(0x00FF_FFFF),
            Self::Sepia => Rgb::hex(0x00F4_ECD8),
            Self::Dusk => Rgb::hex(0x002A_2C31),
            Self::Night => Rgb::hex(0x0000_0000),
        }
    }

    /// The colour of the text.
    pub(crate) const fn text(self) -> Rgb {
        match self {
            Self::Paper => Rgb::hex(0x001B_1B1B),
            Self::Sepia => Rgb::hex(0x003A_2E1F),
            Self::Dusk => Rgb::hex(0x00D5_D7DB),
            Self::Night => Rgb::hex(0x00B8_B8B8),
        }
    }

    /// Whether the page is dark, which decides the appearance of the
    /// controls drawn over it.
    pub(crate) const fn is_dark(self) -> bool {
        matches!(self, Self::Dusk | Self::Night)
    }

    /// The colours of the interface that go with the page.
    pub(crate) const fn chrome(self) -> Chrome {
        if self.is_dark() {
            Chrome::DARK
        } else {
            Chrome::LIGHT
        }
    }

    /// The name the settings store.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Paper => "paper",
            Self::Sepia => "sepia",
            Self::Dusk => "dusk",
            Self::Night => "night",
        }
    }

    /// The tone a stored name stands for.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tone| tone.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG 2.2, 1.4.3: text against its background.
    const TEXT: f32 = 4.5;
    /// WCAG 2.2, 1.4.11: what identifies a control.
    const CONTROL: f32 = 3.0;

    #[test]
    fn a_colour_survives_its_hex_form() {
        let colour = Rgb::hex(0x0046_50D8);
        assert_eq!(colour, Rgb(0x46, 0x50, 0xD8));
        assert_eq!(colour.to_hex(), 0x0046_50D8);
    }

    #[test]
    fn black_on_white_is_the_largest_contrast() {
        let (black, white) = (Rgb(0, 0, 0), Rgb(255, 255, 255));
        assert!((black.contrast(white) - 21.0).abs() < 0.01);
        assert!((white.contrast(black) - 21.0).abs() < 0.01);
        assert!((white.contrast(white) - 1.0).abs() < 0.01);
    }

    #[test]
    fn text_of_the_interface_meets_level_aa() {
        for chrome in [Chrome::LIGHT, Chrome::DARK] {
            for background in [chrome.canvas, chrome.surface] {
                assert!(chrome.ink.contrast(background) >= TEXT, "{chrome:?}");
                assert!(chrome.ink_muted.contrast(background) >= TEXT, "{chrome:?}");
                assert!(chrome.accent.contrast(background) >= TEXT, "{chrome:?}");
            }
            assert!(
                chrome.on_accent.contrast(chrome.accent) >= TEXT,
                "{chrome:?}"
            );
        }
    }

    #[test]
    fn what_identifies_a_control_meets_level_aa() {
        for chrome in [Chrome::LIGHT, Chrome::DARK] {
            for background in [chrome.canvas, chrome.surface] {
                assert!(chrome.outline.contrast(background) >= CONTROL, "{chrome:?}");
                assert!(chrome.accent.contrast(background) >= CONTROL, "{chrome:?}");
            }
        }
    }

    #[test]
    fn the_text_of_every_page_tone_meets_level_aa() {
        for tone in PageTone::ALL {
            assert!(tone.text().contrast(tone.page()) >= TEXT, "{tone:?}");
        }
    }

    #[test]
    fn the_accent_is_seen_on_every_page_tone() {
        for tone in PageTone::ALL {
            let accent = tone.chrome().accent;
            assert!(accent.contrast(tone.page()) >= CONTROL, "{tone:?}");
        }
    }

    #[test]
    fn dark_tones_take_the_dark_interface() {
        assert_eq!(PageTone::Paper.chrome(), Chrome::LIGHT);
        assert_eq!(PageTone::Sepia.chrome(), Chrome::LIGHT);
        assert_eq!(PageTone::Dusk.chrome(), Chrome::DARK);
        assert_eq!(PageTone::Night.chrome(), Chrome::DARK);
    }

    #[test]
    fn a_tone_survives_its_stored_name() {
        for tone in PageTone::ALL {
            assert_eq!(PageTone::from_name(tone.name()), Some(tone));
        }
        assert_eq!(PageTone::from_name("neon"), None);
    }
}
