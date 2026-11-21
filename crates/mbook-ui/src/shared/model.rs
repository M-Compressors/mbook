//! Values that name a book of the library and a place in it.

use std::fmt;
use std::str::FromStr;

use uuid::Uuid;

/// The name of a book in the library.
///
/// A UUID version 7 made when the book is added. It names the file of the
/// book and its rows in the catalogue. It is not the `id` of the book's
/// metadata, which is any string and may be edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct BookKey(Uuid);

impl BookKey {
    /// A key no book of this library has yet.
    pub(crate) fn generate() -> Self {
        Self(Uuid::now_v7())
    }

    /// The key as the 128-bit number the catalogue stores.
    pub(crate) fn as_u128(self) -> u128 {
        self.0.as_u128()
    }

    /// The key a stored number stands for.
    pub(crate) fn from_u128(value: u128) -> Self {
        Self(Uuid::from_u128(value))
    }
}

impl fmt::Display for BookKey {
    /// The hyphenated form, which is also the stem of the file name.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.as_hyphenated().fmt(f)
    }
}

/// A text that is not a [`BookKey`].
#[derive(Debug, thiserror::Error)]
#[error("not the key of a book")]
pub(crate) struct ParseKeyError(#[source] uuid::Error);

impl FromStr for BookKey {
    type Err = ParseKeyError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Uuid::try_parse(text).map(Self).map_err(ParseKeyError)
    }
}

/// A place in a book that survives the book being saved again.
///
/// The chapter is named by its path, not by its resource number: numbers
/// change when a book is written again, paths do not (RFC 0004, "The
/// catalogue").
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Place {
    /// The path of the chapter.
    pub(crate) chapter: String,
    /// A byte offset into the text of the chapter.
    pub(crate) offset: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[test]
    fn a_key_survives_its_text_form() {
        let key = BookKey::generate();
        assert_eq!(key.to_string().parse::<BookKey>().unwrap(), key);
    }

    #[test]
    fn a_key_survives_its_stored_number() {
        let key = BookKey::generate();
        assert_eq!(BookKey::from_u128(key.as_u128()), key);
    }

    #[test]
    fn generated_keys_differ() {
        assert_ne!(BookKey::generate(), BookKey::generate());
    }

    #[test]
    fn a_text_that_is_no_key_is_refused_with_its_cause() {
        let error = "cover.jpg".parse::<BookKey>().unwrap_err();
        assert_eq!(error.to_string(), "not the key of a book");
        assert!(error.source().is_some());
    }
}
