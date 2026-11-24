//! The languages of the interface, and the words of a count.
//!
//! The strings are in `locales/*.yml`, one file per part of the application,
//! every key with an English and a Russian text; [`rust_i18n::t!`] looks a
//! key up in the current language and falls back to English. The current
//! language is one for the process: GPUI Component translates its own
//! controls (the buttons of a dialog, the placeholder of a select) in the same
//! one. [`Language::make_current`] changes it; the application does that when
//! the settings are read and when the language setting changes (mbook-ui
//! docs, "Languages").

use gpui_kit::Global;
use icu_locale_core::locale;
use icu_plurals::{PluralCategory, PluralRules};
use rust_i18n::t;

/// A language the interface is translated into.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) enum Language {
    /// English, the language every other falls back to.
    #[default]
    English,
    /// Russian.
    Russian,
}

impl Language {
    /// Every language, in the order the settings offer them.
    pub(crate) const ALL: [Self; 2] = [Self::English, Self::Russian];

    /// The code of the language: what the settings store and what the
    /// files of strings are keyed by.
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Russian => "ru",
        }
    }

    /// The language a stored code stands for.
    pub(crate) fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|language| language.code() == code)
    }

    /// The name of the language in that language, which is how a person who
    /// cannot read the current one finds theirs.
    pub(crate) const fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Russian => "Русский",
        }
    }

    /// The language of a tag such as `ru-RU`, `ru_RU.UTF-8` or `en`: its
    /// primary subtag, in any case. `None` when the interface is not
    /// translated into it.
    pub(crate) fn from_tag(tag: &str) -> Option<Self> {
        let primary = tag
            .split(['-', '_', '.', '@'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        Self::from_code(&primary)
    }

    /// The first of `tags`, in the order of preference, that the interface is
    /// translated into; English when there is none.
    pub(crate) fn first_supported<I>(tags: I) -> Self
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        tags.into_iter()
            .find_map(|tag| Self::from_tag(tag.as_ref()))
            .unwrap_or_default()
    }

    /// The language the system prefers, from the languages it lists in
    /// order; English when it lists none the interface is translated into.
    pub(crate) fn of_system() -> Self {
        Self::first_supported(sys_locale::get_locales())
    }

    /// The language the interface is drawn in now.
    pub(crate) fn current() -> Self {
        Self::from_code(&rust_i18n::locale()).unwrap_or_default()
    }

    /// Draws the interface in this language from now on: the strings of the
    /// application and those of GPUI Component. Views show it when they are
    /// next drawn.
    pub(crate) fn make_current(self) {
        rust_i18n::set_locale(self.code());
    }

    /// The plural category of `count` in this language, as the files of
    /// strings name it: `one`, `few`, `many` or `other`.
    fn category(self, count: usize) -> &'static str {
        let category = RULES.with(|rules| {
            rules
                .iter()
                .find(|(language, _)| *language == self)
                .and_then(|(_, rules)| rules.as_ref())
                .map(|rules| rules.category_for(count))
        });
        match category {
            Some(PluralCategory::One) => "one",
            Some(PluralCategory::Few) => "few",
            Some(PluralCategory::Many) => "many",
            // English has no `few` and `many`, Russian no `zero` and `two`,
            // and both use `other` for fractions.
            _ => "other",
        }
    }
}

/// The language the interface is drawn in, as a global of the application.
/// The appearance sets it whenever the language changes; a view that keeps
/// text it made, such as the placeholder of a field, observes it to make that
/// text again ([`crate::shared::ui::follow_language`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InterfaceLanguage(pub(crate) Language);

impl Global for InterfaceLanguage {}

thread_local! {
    /// The plural rules of each language, made once per thread: they are not
    /// `Sync`, and counts are drawn on the main thread.
    static RULES: [(Language, Option<PluralRules>); 2] = [
        (
            Language::English,
            PluralRules::try_new_cardinal(locale!("en").into()).ok(),
        ),
        (
            Language::Russian,
            PluralRules::try_new_cardinal(locale!("ru").into()).ok(),
        ),
    ];
}

/// The words of a count in the current language: the text of
/// `<key>.<category>`, with `%{count}` replaced by the number. A category the
/// file does not have is looked up as `<key>.other`.
pub(crate) fn count_text(key: &str, count: usize) -> String {
    count_text_in(Language::current(), key, count)
}

/// [`count_text`] in a given language.
pub(crate) fn count_text_in(language: Language, key: &str, count: usize) -> String {
    let code = language.code();
    let exact = format!("{key}.{}", language.category(count));
    let text = crate::_rust_i18n_try_translate(code, &exact)
        .unwrap_or_else(|| t!(format!("{key}.other"), locale = code));
    text.replace("%{count}", &count.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use super::*;

    #[test]
    fn a_tag_is_known_by_its_primary_subtag_in_any_case() {
        assert_eq!(Language::from_tag("ru"), Some(Language::Russian));
        assert_eq!(Language::from_tag("ru-RU"), Some(Language::Russian));
        assert_eq!(Language::from_tag("RU_ua.UTF-8"), Some(Language::Russian));
        assert_eq!(Language::from_tag("en-GB"), Some(Language::English));
        assert_eq!(Language::from_tag("en_US@posix"), Some(Language::English));
    }

    #[test]
    fn a_language_without_a_translation_has_no_tag() {
        assert_eq!(Language::from_tag("de-DE"), None);
        assert_eq!(Language::from_tag(""), None);
        assert_eq!(Language::from_tag("russian"), None);
    }

    #[test]
    fn the_first_translated_language_of_the_system_is_chosen() {
        assert_eq!(
            Language::first_supported(["de-DE", "ru-RU", "en-US"]),
            Language::Russian
        );
        assert_eq!(
            Language::first_supported(["fr", "en-US", "ru"]),
            Language::English
        );
    }

    #[test]
    fn english_is_the_fallback_when_the_system_has_no_translated_language() {
        assert_eq!(
            Language::first_supported(["de-DE", "ja"]),
            Language::English
        );
        assert_eq!(
            Language::first_supported(Vec::<String>::new()),
            Language::English
        );
    }

    #[test]
    fn a_language_survives_its_code() {
        for language in Language::ALL {
            assert_eq!(Language::from_code(language.code()), Some(language));
        }
        assert_eq!(Language::from_code("ru-RU"), None, "a code is exact");
    }

    #[test]
    fn a_language_is_named_in_itself() {
        assert_eq!(Language::English.native_name(), "English");
        assert_eq!(Language::Russian.native_name(), "Русский");
    }

    #[test]
    fn russian_has_three_forms_of_a_count() {
        let russian = |count| Language::Russian.category(count);
        assert_eq!(russian(1), "one");
        assert_eq!(russian(21), "one");
        assert_eq!(russian(2), "few");
        assert_eq!(russian(24), "few");
        assert_eq!(russian(5), "many");
        assert_eq!(russian(11), "many");
        assert_eq!(russian(0), "many");
    }

    #[test]
    fn english_has_two_forms_of_a_count() {
        assert_eq!(Language::English.category(1), "one");
        assert_eq!(Language::English.category(0), "other");
        assert_eq!(Language::English.category(2), "other");
        assert_eq!(Language::English.category(5), "other");
    }

    #[test]
    fn a_count_takes_the_form_of_its_category() {
        let books = |language, count| count_text_in(language, "library.books", count);
        assert_eq!(books(Language::English, 1), "1 book");
        assert_eq!(books(Language::English, 5), "5 books");
        assert_eq!(books(Language::Russian, 1), "1 книга");
        assert_eq!(books(Language::Russian, 3), "3 книги");
        assert_eq!(books(Language::Russian, 12), "12 книг");
    }

    #[test]
    fn a_count_without_its_form_falls_back_to_other() {
        // A missing key is shown as the key itself, which names the form.
        let key = ["no", "such", "count"].join(".");
        assert_eq!(
            count_text_in(Language::Russian, &key, 3),
            "no.such.count.other"
        );
    }

    /// The keys a language has, with the names of the values each text
    /// takes.
    fn keys(code: &str) -> BTreeSet<(String, BTreeSet<String>)> {
        let placeholder = regex::Regex::new(r"%\{(\w+)\}").unwrap();
        crate::_rust_i18n_backend()
            .messages_for_locale(code)
            .unwrap_or_default()
            .into_iter()
            .map(|(key, text)| {
                let names = placeholder
                    .captures_iter(&text)
                    .map(|capture| capture[1].to_owned())
                    .collect();
                (key.into_owned(), names)
            })
            .collect()
    }

    #[test]
    fn every_key_has_both_languages_with_the_same_values() {
        let english = keys("en");
        let russian = keys("ru");
        assert!(!english.is_empty());
        // A count of Russian has `few` and `many` where English has `other`.
        let plain = |set: &BTreeSet<(String, BTreeSet<String>)>| -> BTreeSet<_> {
            set.iter()
                .filter(|(key, _)| {
                    ![".one", ".few", ".many", ".other"]
                        .iter()
                        .any(|end| key.ends_with(end))
                })
                .cloned()
                .collect()
        };
        let missing: Vec<_> = plain(&english)
            .symmetric_difference(&plain(&russian))
            .cloned()
            .collect();
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn every_count_has_the_forms_of_each_language() {
        let english = keys("en");
        let russian = keys("ru");
        let counts: BTreeSet<_> = english
            .iter()
            .filter_map(|(key, _)| key.strip_suffix(".other"))
            .map(str::to_owned)
            .collect();
        for count in counts {
            for form in ["one", "other"] {
                let key = format!("{count}.{form}");
                assert!(english.iter().any(|(k, _)| *k == key), "en {key}");
            }
            for form in ["one", "few", "many"] {
                let key = format!("{count}.{form}");
                assert!(russian.iter().any(|(k, _)| *k == key), "ru {key}");
            }
        }
    }

    /// Every key the source names in a `t!` or a count exists in English.
    #[test]
    fn every_key_the_source_names_exists() {
        let english: BTreeSet<String> = keys("en").into_iter().map(|(key, _)| key).collect();
        let call =
            regex::Regex::new(r#"(?:\bt!\(\s*|count_text(?:_in)?\([^"\n]*?)"([a-z0-9_.]+)""#)
                .unwrap();
        let mut unknown = Vec::new();
        let mut stack = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(path) = stack.pop() {
            if path.is_dir() {
                stack.extend(
                    std::fs::read_dir(&path)
                        .unwrap()
                        .map(|entry| entry.unwrap().path()),
                );
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            for capture in call.captures_iter(&source) {
                let key = &capture[1];
                let known = english.contains(key) || english.contains(&format!("{key}.other"));
                if !known {
                    unknown.push(format!("{}: {key}", path.display()));
                }
            }
        }
        assert!(unknown.is_empty(), "{unknown:#?}");
    }
}
