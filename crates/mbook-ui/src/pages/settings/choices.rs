//! What the settings offer, and how a value is shown: plain functions.

use std::ops::RangeInclusive;

use gpui_kit::SharedString;
use gpui_kit::component::searchable_list::SearchableListItem;
use rust_i18n::t;

use crate::entities::catalogue::READING_FACES;
use crate::shared::i18n::Language;
use crate::shared::theme::PageTone;

/// The sizes of the text of a book, in pixels.
pub(super) const TEXT_SIZE: RangeInclusive<f32> = 12.0..=80.0;
/// The line heights, as a multiple of the size.
pub(super) const LINE_HEIGHT: RangeInclusive<f32> = 1.3..=2.0;
/// The sizes of the interface, in percent.
pub(super) const INTERFACE_PERCENT: RangeInclusive<f32> = 100.0..=200.0;
/// The step of the size of the interface, in percent.
pub(super) const INTERFACE_STEP: f32 = 10.0;

/// The page tones the settings offer, in order. `None` follows the system.
pub(super) const TONES: [Option<PageTone>; 6] = [
    None,
    Some(PageTone::Paper),
    Some(PageTone::Neutral),
    Some(PageTone::Sepia),
    Some(PageTone::Night),
    Some(PageTone::HighContrast),
];

/// The position of a page tone among [`TONES`].
pub(super) fn tone_index(tone: Option<PageTone>) -> usize {
    TONES
        .iter()
        .position(|candidate| *candidate == tone)
        .unwrap_or(0)
}

/// The name of a choice of page tone in the current language.
pub(super) fn tone_label(tone: Option<PageTone>) -> SharedString {
    tone.map_or_else(|| t!("settings.tone_system"), PageTone::label)
        .into()
}

/// The languages the settings offer, in order. `None` follows the system.
pub(super) fn language_options() -> Vec<Option<Language>> {
    std::iter::once(None)
        .chain(Language::ALL.into_iter().map(Some))
        .collect()
}

/// The position of a choice of language among [`language_options`].
pub(super) fn language_index(language: Option<Language>) -> usize {
    language_options()
        .iter()
        .position(|candidate| *candidate == language)
        .unwrap_or(0)
}

/// The name of a choice of language. A language is named in itself, so that
/// a person who cannot read the current one finds theirs; the choice that
/// follows the system says, in the current language, which one that is.
pub(super) fn language_label(language: Option<Language>, system: Language) -> SharedString {
    match language {
        Some(language) => language.native_name().into(),
        None => t!("settings.language_system", language = system.native_name()).into(),
    }
}

/// A typeface in the list of the settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Typeface {
    /// The name of the family, which is what is stored.
    pub(super) family: String,
    /// What the list shows: the family, and "not installed" when the device
    /// does not have it.
    pub(super) title: SharedString,
}

impl Typeface {
    fn installed(family: &str) -> Self {
        Self {
            family: family.to_owned(),
            title: family.to_owned().into(),
        }
    }

    fn missing(family: &str) -> Self {
        Self {
            family: family.to_owned(),
            title: t!("settings.not_installed", family = family).into(),
        }
    }
}

impl SearchableListItem for Typeface {
    type Value = String;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &String {
        &self.family
    }
}

/// The list of typefaces: Literata first, then the stored one when it is
/// neither Literata nor on the device, then every family of the device.
///
/// While the families are not read (`families` is empty) the stored typeface
/// is shown as it is, not as missing.
pub(super) fn typeface_items(stored: &str, families: &[String]) -> Vec<Typeface> {
    let mut items: Vec<_> = READING_FACES.into_iter().map(Typeface::installed).collect();
    let known = READING_FACES.contains(&stored) || families.iter().any(|family| family == stored);
    if !known {
        items.push(if families.is_empty() {
            Typeface::installed(stored)
        } else {
            Typeface::missing(stored)
        });
    }
    items.extend(
        families
            .iter()
            .filter(|family| !READING_FACES.contains(&family.as_str()))
            .map(|family| Typeface::installed(family)),
    );
    items
}

/// The position of a typeface in the list, which always holds the stored one.
pub(super) fn typeface_index(items: &[Typeface], stored: &str) -> Option<usize> {
    items.iter().position(|item| item.family == stored)
}

/// The size of the text as shown: `18 px`.
pub(super) fn size_text(size: f32) -> String {
    format!("{} px", size.round())
}

/// The line height as shown: `1.6`.
pub(super) fn line_height_text(height: f32) -> String {
    format!("{height:.2}")
}

/// The size of the interface as shown: `120%`.
pub(super) fn scale_text(scale: f32) -> String {
    format!("{}%", (scale * 100.0).round())
}

/// A size of the text from a slider: whole pixels.
pub(super) fn snap_size(value: f32) -> f32 {
    value.round().clamp(*TEXT_SIZE.start(), *TEXT_SIZE.end())
}

/// A line height from a slider: tenths.
pub(super) fn snap_line_height(value: f32) -> f32 {
    ((value * 20.0).round() / 20.0).clamp(*LINE_HEIGHT.start(), *LINE_HEIGHT.end())
}

/// A scale of the interface from a slider in percent: steps of ten percent,
/// as a multiple of 1.
pub(super) fn snap_scale(percent: f32) -> f32 {
    let steps = (percent / INTERFACE_STEP).round() * INTERFACE_STEP;
    steps.clamp(*INTERFACE_PERCENT.start(), *INTERFACE_PERCENT.end()) / 100.0
}

/// Whether two sizes are the same to what a slider can show.
pub(super) fn same(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.001
}

#[cfg(test)]
mod tests {
    use super::*;

    fn families(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    fn names(items: &[Typeface]) -> Vec<&str> {
        items.iter().map(|item| item.family.as_str()).collect()
    }

    #[test]
    fn literata_comes_first_then_the_families_of_the_device() {
        let items = typeface_items("Literata", &families(&["Arial", "Georgia"]));
        assert_eq!(
            names(&items),
            ["Literata", "Noto Sans", "System", "Arial", "Georgia"]
        );
    }

    #[test]
    fn literata_is_not_listed_twice() {
        let items = typeface_items("Literata", &families(&["Arial", "Literata"]));
        assert_eq!(names(&items), ["Literata", "Noto Sans", "System", "Arial"]);
    }

    #[test]
    fn a_stored_typeface_the_device_lacks_is_listed_and_marked() {
        let items = typeface_items("Palatino", &families(&["Arial"]));
        assert_eq!(
            names(&items),
            ["Literata", "Noto Sans", "System", "Palatino", "Arial"]
        );
        assert_eq!(items[3].title, "Palatino (not installed)");
        assert_eq!(items[3].family, "Palatino", "the stored name is kept");
    }

    #[test]
    fn a_stored_typeface_the_device_has_is_not_marked() {
        let items = typeface_items("Arial", &families(&["Arial", "Georgia"]));
        assert_eq!(
            names(&items),
            ["Literata", "Noto Sans", "System", "Arial", "Georgia"]
        );
        assert_eq!(items[3].title, "Arial");
    }

    #[test]
    fn a_stored_typeface_is_not_marked_before_the_families_are_read() {
        let items = typeface_items("Palatino", &[]);
        assert_eq!(
            names(&items),
            ["Literata", "Noto Sans", "System", "Palatino"]
        );
        assert_eq!(items[3].title, "Palatino");
    }

    #[test]
    fn the_stored_typeface_is_found_in_the_list() {
        let items = typeface_items("Palatino", &families(&["Arial"]));
        assert_eq!(typeface_index(&items, "Palatino"), Some(3));
        assert_eq!(typeface_index(&items, "Arial"), Some(4));
        assert_eq!(typeface_index(&items, "Nothing"), None);
    }

    #[test]
    fn a_typeface_is_found_by_its_title_in_a_search() {
        let item = Typeface::missing("Palatino");
        assert!(item.matches("palat"));
        assert!(!item.matches("georgia"));
    }

    #[test]
    fn values_are_shown_in_the_unit_the_user_knows() {
        assert_eq!(size_text(18.0), "18 px");
        assert_eq!(size_text(17.6), "18 px");
        assert_eq!(line_height_text(1.6), "1.60");
        assert_eq!(line_height_text(2.0), "2.00");
        assert_eq!(scale_text(1.0), "100%");
        assert_eq!(scale_text(1.2), "120%");
    }

    #[test]
    fn a_slider_value_snaps_to_what_the_setting_accepts() {
        assert!(same(snap_size(17.6), 18.0));
        assert!(same(snap_size(3.0), 12.0));
        assert!(same(snap_size(99.0), 80.0));
        assert!(same(snap_line_height(1.500_000_1), 1.5));
        assert!(same(snap_line_height(0.5), 1.3));
        assert!(same(snap_line_height(5.0), 2.0));
        assert!(same(snap_scale(123.0), 1.2));
        assert!(same(snap_scale(100.0), 1.0));
        assert!(same(snap_scale(250.0), 2.0));
        assert!(same(snap_scale(10.0), 1.0));
    }

    #[test]
    fn every_page_tone_has_its_place_and_the_system_comes_first() {
        assert_eq!(tone_index(None), 0);
        for tone in PageTone::ALL {
            let index = tone_index(Some(tone));
            assert_eq!(TONES[index], Some(tone));
        }
    }

    #[test]
    fn a_page_tone_is_named_in_the_current_language() {
        assert_eq!(tone_label(None), "Follow the system");
        assert_eq!(tone_label(Some(PageTone::HighContrast)), "High contrast");
    }

    #[test]
    fn the_system_comes_first_then_every_language() {
        assert_eq!(
            language_options(),
            [None, Some(Language::English), Some(Language::Russian)]
        );
        assert_eq!(language_index(None), 0);
        for language in Language::ALL {
            let index = language_index(Some(language));
            assert_eq!(language_options()[index], Some(language));
        }
    }

    #[test]
    fn a_language_is_named_in_itself_and_the_system_says_which_it_is() {
        assert_eq!(
            language_label(Some(Language::Russian), Language::English),
            "Русский"
        );
        assert_eq!(
            language_label(None, Language::Russian),
            "System language (Русский)"
        );
    }

    #[test]
    fn two_sizes_that_differ_by_less_than_a_slider_can_show_are_the_same() {
        assert!(same(1.6, 1.600_000_1));
        assert!(!same(1.6, 1.7));
    }
}
