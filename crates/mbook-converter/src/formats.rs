//! The front ends: one module per source format, each behind the feature of
//! its format. A front end reads the bytes of a source into a
//! [`Draft`](crate::draft::Draft) and decides nothing about the mbook file.

use crate::draft::Draft;
use crate::error::Result;
use crate::format::Format;
use crate::options::ConvertOptions;

/// Declares the module of every front end and the function that picks one.
macro_rules! front_ends {
    ($($feature:literal => $format:ident in $module:ident,)*) => {
        $(
            #[cfg(feature = $feature)]
            mod $module;
        )*

        /// Reads a source of a known format into a draft.
        ///
        /// # Errors
        ///
        /// The feature of the format is not enabled, or as the front end of
        /// the format says.
        pub(crate) fn read(
            format: Format,
            source: &[u8],
            options: &ConvertOptions,
        ) -> Result<Draft> {
            // An export-only build recognises formats without opening them.
            #[cfg(not(any($(feature = $feature),*)))]
            let _ = (source, options);
            match format {
                $(
                    #[cfg(feature = $feature)]
                    Format::$format => $module::read(source, options),
                    #[cfg(not(feature = $feature))]
                    Format::$format => Err(crate::error::Error::FormatDisabled(format)),
                )*
            }
        }
    };
}

front_ends! {
    "epub" => Epub in epub,
    "fb2" => Fb2 in fb2,
    "pdf" => Pdf in pdf,
    "cbz" => Cbz in cbz,
    "docx" => Docx in docx,
    "html" => Html in html,
    "md" => Markdown in md,
    "txt" => Text in txt,
}
