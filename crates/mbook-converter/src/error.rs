//! Error types.

use crate::format::Format;

/// The result type of this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Why a conversion failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The source or the sink failed.
    #[error("I/O failed")]
    Io(#[from] std::io::Error),
    /// The content of the source matches no format this crate knows.
    #[error("the format of the source is not recognised")]
    UnknownFormat,
    /// The format was recognised or named, and its feature is not enabled.
    #[error("this build has no support for {0}")]
    FormatDisabled(Format),
    /// The source could not be parsed as its format.
    #[error("the {format} source cannot be read")]
    Source {
        /// The format the source was read as.
        format: Format,
        /// What the parser of that format reported.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The source is encrypted or under rights management.
    #[error("the {0} source is protected")]
    Protected(Format),
    /// Nothing in the source could be converted.
    #[error("the source has no text and no picture")]
    Empty,
    /// A limit of the options was exceeded.
    #[error("limit exceeded: {0}")]
    Limit(&'static str),
    /// The converted book was refused by `mbook`.
    #[error("the converted book cannot be built")]
    Book(#[source] mbook::Error),
}

impl From<mbook::Error> for Error {
    /// A failure of the sink stays an I/O error; anything else is about the
    /// book.
    fn from(error: mbook::Error) -> Self {
        match error {
            mbook::Error::Io(error) => Self::Io(error),
            other => Self::Book(other),
        }
    }
}

impl Error {
    /// Wraps what the parser of a format reported.
    // Plain text has no parser that could fail.
    #[cfg(pictures)]
    pub(crate) fn source(
        format: Format,
        source: impl Into<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        Self::Source {
            format,
            source: source.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Error;
    use crate::format::Format;
    use std::error::Error as _;

    #[test]
    fn an_io_error_keeps_its_source() {
        let error = Error::from(std::io::Error::other("disk on fire"));
        let source = error.source().map(ToString::to_string);
        assert_eq!(source.as_deref(), Some("disk on fire"));
    }

    #[test]
    fn a_disabled_format_is_named() {
        let error = Error::FormatDisabled(Format::Pdf);
        assert_eq!(error.to_string(), "this build has no support for pdf");
    }

    #[test]
    fn a_protected_source_names_its_format() {
        let error = Error::Protected(Format::Epub);
        assert_eq!(error.to_string(), "the epub source is protected");
    }

    #[cfg(pictures)]
    #[test]
    fn a_parser_error_is_the_source_of_a_source_error() {
        let error = Error::source(Format::Fb2, "unexpected end of file");
        let source = error.source().map(ToString::to_string);
        assert_eq!(
            (error.to_string().as_str(), source.as_deref()),
            (
                "the fb2 source cannot be read",
                Some("unexpected end of file")
            )
        );
    }

    #[test]
    fn a_failing_sink_stays_an_io_error() {
        let sink = mbook::Error::from(std::io::Error::other("pipe closed"));
        assert!(matches!(Error::from(sink), Error::Io(_)));
    }

    #[test]
    fn a_refused_book_keeps_the_error_of_mbook_as_its_source() {
        let error = Error::from(mbook::Error::NotMbook);
        let source = error.source().map(ToString::to_string);
        assert_eq!(
            (error.to_string().as_str(), source.as_deref()),
            (
                "the converted book cannot be built",
                Some("not an mbook file")
            )
        );
    }

    #[test]
    fn limits_and_empty_sources_have_messages() {
        assert_eq!(
            Error::Limit("the source is too large").to_string(),
            "limit exceeded: the source is too large"
        );
        assert_eq!(
            Error::Empty.to_string(),
            "the source has no text and no picture"
        );
        assert_eq!(
            Error::UnknownFormat.to_string(),
            "the format of the source is not recognised"
        );
    }
}
