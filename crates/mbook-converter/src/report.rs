//! What a conversion reports besides the book.

use crate::format::Format;
use mbook_core::Book;
use std::fmt;

/// A converted book that can still be changed, and what its conversion
/// reported.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Converted {
    /// The book. Every call of `mbook-core` works on it.
    pub book: Book,
    /// The format of the source and what was lost.
    pub report: Report,
}

/// What a conversion found out.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Report {
    /// The format the source was read as.
    pub format: Format,
    /// Everything that could not be kept, in the order it was met.
    pub warnings: Vec<Warning>,
}

/// One thing a conversion could not keep.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Warning {
    /// What was lost.
    pub kind: WarningKind,
    /// The source's name for the place, when it is known: a file of an
    /// EPUB, a section of an FB2 document.
    pub location: Option<String>,
}

impl Warning {
    /// A warning about the place a front end or the assembler is working on.
    pub(crate) fn new(kind: WarningKind, location: &str) -> Self {
        Self {
            kind,
            location: (!location.is_empty()).then(|| location.to_owned()),
        }
    }

    /// A warning about the book as a whole.
    pub(crate) fn whole_book(kind: WarningKind) -> Self {
        Self {
            kind,
            location: None,
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.location {
            Some(location) => write!(f, "{location}: {}", self.kind),
            None => self.kind.fmt(f),
        }
    }
}

/// What a conversion lost.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WarningKind {
    /// Markup that mbook cannot express was removed and its text kept: the
    /// name of an HTML element, or a description such as
    /// `table with merged cells`.
    MarkupDropped(String),
    /// A picture could not be stored and was replaced by its alternative
    /// text. Holds the source's name of the picture.
    PictureDropped(String),
    /// An entry of the contents leads to the heading above its target, or
    /// to the start of its chapter. Holds its destination in the source.
    LinkRetargeted(String),
    /// A link, or an entry of the contents, leads to nothing that was
    /// converted. The link was replaced by its text. Holds its destination
    /// in the source.
    LinkDropped(String),
    /// A value of the metadata was refused and left out. Holds the key.
    MetadataDropped(String),
    /// The source lacks a value mbook requires, and a stand-in was used.
    /// Holds the key.
    MetadataMissing(String),
}

impl fmt::Display for WarningKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MarkupDropped(what) => write!(f, "{what} markup was dropped"),
            Self::PictureDropped(name) => write!(f, "the picture {name:?} was dropped"),
            Self::LinkRetargeted(target) => {
                write!(
                    f,
                    "the contents entry to {target:?} leads to a heading above it"
                )
            }
            Self::LinkDropped(target) => write!(f, "the link to {target:?} was dropped"),
            Self::MetadataDropped(key) => write!(f, "a value of {key:?} was dropped"),
            Self::MetadataMissing(key) => write!(f, "the source has no {key:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Warning, WarningKind};

    #[test]
    fn a_warning_with_a_place_names_it_first() {
        let warning = Warning::new(WarningKind::MarkupDropped("sub".into()), "ch1.xhtml");
        assert_eq!(warning.to_string(), "ch1.xhtml: sub markup was dropped");
    }

    #[test]
    fn a_warning_without_a_place_has_no_prefix() {
        let warning = Warning::new(WarningKind::MetadataMissing("title".into()), "");
        assert_eq!(warning.location, None);
        assert_eq!(warning.to_string(), r#"the source has no "title""#);
    }

    #[test]
    fn a_warning_about_the_whole_book_has_no_place() {
        let warning = Warning::whole_book(WarningKind::MetadataDropped("date".into()));
        assert_eq!(warning.to_string(), r#"a value of "date" was dropped"#);
    }

    #[test]
    fn every_kind_of_warning_has_a_message() {
        let messages = [
            WarningKind::PictureDropped("a.svg".into()),
            WarningKind::LinkRetargeted("ch1#p".into()),
            WarningKind::LinkDropped("ch9".into()),
        ]
        .map(|kind| kind.to_string());
        assert_eq!(
            messages,
            [
                r#"the picture "a.svg" was dropped"#,
                r#"the contents entry to "ch1#p" leads to a heading above it"#,
                r#"the link to "ch9" was dropped"#
            ]
        );
    }
}
