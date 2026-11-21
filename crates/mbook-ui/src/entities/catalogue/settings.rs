//! The settings of the application, and how they are stored.
//!
//! The `settings` table is keyed by the name of a setting and holds its
//! value as text: numbers in their shortest decimal form, booleans as `true`
//! and `false`, a page tone by its name. A setting that is not stored has its
//! default; a page tone that follows the system has no row. A stored value
//! that cannot be read, or is out of range, never fails a read: it is
//! replaced by the default or brought into range (RFC 0004, "Settings").

use std::collections::HashMap;

use crate::shared::theme::PageTone;

/// The names of the rows.
pub(super) mod name {
    /// The typeface of books.
    pub(in crate::entities::catalogue) const TYPEFACE: &str = "typeface";
    /// The size of the text.
    pub(in crate::entities::catalogue) const TEXT_SIZE: &str = "text_size";
    /// The line height of the text.
    pub(in crate::entities::catalogue) const LINE_HEIGHT: &str = "line_height";
    /// The page tone.
    pub(in crate::entities::catalogue) const PAGE_TONE: &str = "page_tone";
    /// The size of the interface.
    pub(in crate::entities::catalogue) const INTERFACE_SCALE: &str = "interface_scale";
    /// Whether the controls of the reader stay shown.
    pub(in crate::entities::catalogue) const KEEP_CONTROLS: &str = "keep_controls";
    /// Whether motion is reduced.
    pub(in crate::entities::catalogue) const REDUCE_MOTION: &str = "reduce_motion";
    /// Whether releases are checked.
    pub(in crate::entities::catalogue) const CHECK_UPDATES: &str = "check_updates";
}

/// The range of the size of the text, in pixels.
const TEXT_SIZE: (f32, f32) = (12.0, 32.0);
/// The range of the line height, as a multiple of the size.
const LINE_HEIGHT: (f32, f32) = (1.2, 2.0);
/// The range of the size of the interface, as a multiple of 100%.
const INTERFACE_SCALE: (f32, f32) = (1.0, 2.0);

/// What the reader chose for the application and for the page of a book.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Settings {
    /// The font family of books.
    pub(crate) typeface: String,
    /// The size of the text in pixels, from 12 to 32.
    pub(crate) text_size: f32,
    /// The line height as a multiple of the size, from 1.2 to 2.0.
    pub(crate) line_height: f32,
    /// The colours of the page; `None` follows the system.
    pub(crate) page_tone: Option<PageTone>,
    /// The size of the interface as a multiple of 100%, from 1.0 to 2.0.
    pub(crate) interface_scale: f32,
    /// Whether the controls of the reader are always shown.
    pub(crate) keep_controls: bool,
    /// Whether motion is reduced whatever the system says.
    pub(crate) reduce_motion: bool,
    /// Whether the releases of the repository are checked for an update.
    pub(crate) check_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            typeface: "Literata".to_owned(),
            text_size: 20.0,
            line_height: 1.6,
            page_tone: None,
            interface_scale: 1.0,
            keep_controls: false,
            reduce_motion: false,
            check_updates: true,
        }
    }
}

impl Settings {
    /// The rows that stand for these settings, as name and text. A page tone
    /// that follows the system has none.
    pub(super) fn encode(&self) -> Vec<(&'static str, String)> {
        let mut rows = vec![
            (name::TYPEFACE, self.typeface.clone()),
            (name::TEXT_SIZE, self.text_size.to_string()),
            (name::LINE_HEIGHT, self.line_height.to_string()),
            (name::INTERFACE_SCALE, self.interface_scale.to_string()),
            (name::KEEP_CONTROLS, self.keep_controls.to_string()),
            (name::REDUCE_MOTION, self.reduce_motion.to_string()),
            (name::CHECK_UPDATES, self.check_updates.to_string()),
        ];
        if let Some(tone) = self.page_tone {
            rows.push((name::PAGE_TONE, tone.name().to_owned()));
        }
        rows
    }

    /// The settings the stored rows stand for.
    pub(super) fn decode(rows: &HashMap<String, String>) -> Self {
        let default = Self::default();
        Self {
            typeface: rows
                .get(name::TYPEFACE)
                .filter(|face| !face.trim().is_empty())
                .cloned()
                .unwrap_or(default.typeface),
            text_size: number(rows, name::TEXT_SIZE, TEXT_SIZE, default.text_size),
            line_height: number(rows, name::LINE_HEIGHT, LINE_HEIGHT, default.line_height),
            page_tone: rows
                .get(name::PAGE_TONE)
                .and_then(|tone| PageTone::from_name(tone)),
            interface_scale: number(
                rows,
                name::INTERFACE_SCALE,
                INTERFACE_SCALE,
                default.interface_scale,
            ),
            keep_controls: flag(rows, name::KEEP_CONTROLS, default.keep_controls),
            reduce_motion: flag(rows, name::REDUCE_MOTION, default.reduce_motion),
            check_updates: flag(rows, name::CHECK_UPDATES, default.check_updates),
        }
    }
}

/// A number setting: the default when it is missing or not a finite number,
/// else the stored value brought into the range.
fn number(rows: &HashMap<String, String>, key: &str, (low, high): (f32, f32), default: f32) -> f32 {
    rows.get(key)
        .and_then(|text| text.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite())
        .map_or(default, |value| value.clamp(low, high))
}

/// A switch: the default when it is missing or neither `true` nor `false`.
fn flag(rows: &HashMap<String, String>, key: &str, default: bool) -> bool {
    rows.get(key)
        .and_then(|text| text.parse::<bool>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn the_defaults_are_those_of_the_rfc() {
        let settings = Settings::default();
        assert_eq!(settings.typeface, "Literata");
        assert!((settings.text_size - 20.0).abs() < f32::EPSILON);
        assert!((settings.line_height - 1.6).abs() < f32::EPSILON);
        assert_eq!(settings.page_tone, None);
        assert!((settings.interface_scale - 1.0).abs() < f32::EPSILON);
        assert!(!settings.keep_controls);
        assert!(!settings.reduce_motion);
        assert!(settings.check_updates);
    }

    #[test]
    fn no_rows_give_the_defaults() {
        assert_eq!(Settings::decode(&HashMap::new()), Settings::default());
    }

    #[test]
    fn settings_survive_their_rows() {
        let settings = Settings {
            typeface: "Inter".to_owned(),
            text_size: 21.5,
            line_height: 1.75,
            page_tone: Some(PageTone::Sepia),
            interface_scale: 1.25,
            keep_controls: true,
            reduce_motion: true,
            check_updates: false,
        };
        let rows: HashMap<String, String> = settings
            .encode()
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        assert_eq!(Settings::decode(&rows), settings);
    }

    #[test]
    fn a_page_tone_that_follows_the_system_has_no_row() {
        let rows = Settings::default().encode();
        assert!(rows.iter().all(|(key, _)| *key != name::PAGE_TONE));
    }

    #[test]
    fn a_number_above_its_range_is_brought_down() {
        let settings = Settings::decode(&stored(&[
            (name::TEXT_SIZE, "99"),
            (name::LINE_HEIGHT, "5"),
            (name::INTERFACE_SCALE, "3"),
        ]));
        assert!((settings.text_size - 32.0).abs() < f32::EPSILON);
        assert!((settings.line_height - 2.0).abs() < f32::EPSILON);
        assert!((settings.interface_scale - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_number_below_its_range_is_brought_up() {
        let settings = Settings::decode(&stored(&[
            (name::TEXT_SIZE, "1"),
            (name::LINE_HEIGHT, "0"),
            (name::INTERFACE_SCALE, "-4"),
        ]));
        assert!((settings.text_size - 12.0).abs() < f32::EPSILON);
        assert!((settings.line_height - 1.2).abs() < f32::EPSILON);
        assert!((settings.interface_scale - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_ends_of_a_range_are_kept() {
        let settings = Settings::decode(&stored(&[(name::TEXT_SIZE, "12")]));
        assert!((settings.text_size - 12.0).abs() < f32::EPSILON);
        let settings = Settings::decode(&stored(&[(name::TEXT_SIZE, "32")]));
        assert!((settings.text_size - 32.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_stored_text_size_keeps_the_previous_default() {
        let settings = Settings::decode(&stored(&[(name::TEXT_SIZE, "18")]));
        assert!((settings.text_size - 18.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_number_that_is_not_finite_gives_the_default() {
        for text in ["NaN", "inf", "-inf", "large", ""] {
            let settings = Settings::decode(&stored(&[(name::TEXT_SIZE, text)]));
            assert!((settings.text_size - 20.0).abs() < f32::EPSILON, "{text}");
        }
    }

    #[test]
    fn an_unknown_page_tone_follows_the_system() {
        let settings = Settings::decode(&stored(&[(name::PAGE_TONE, "neon")]));
        assert_eq!(settings.page_tone, None);
    }

    #[test]
    fn a_switch_that_is_not_a_boolean_gives_the_default() {
        let settings = Settings::decode(&stored(&[
            (name::CHECK_UPDATES, "maybe"),
            (name::KEEP_CONTROLS, "yes"),
        ]));
        assert!(settings.check_updates);
        assert!(!settings.keep_controls);
    }

    #[test]
    fn a_blank_typeface_gives_the_default() {
        let settings = Settings::decode(&stored(&[(name::TYPEFACE, "  ")]));
        assert_eq!(settings.typeface, "Literata");
    }
}
