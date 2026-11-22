//! Convert books between mbook and other formats.
//!
//! One type does the work: [`Converter`]. It takes a source as any
//! [`Read`](std::io::Read) or as a byte slice, and gives an mbook file
//! written to any [`Write`](std::io::Write), or a [`Book`](mbook::Book)
//! that can still be changed.
//! The optional `export` feature writes an existing book in another format.
//!
//! ```no_run
//! use mbook_converter::Converter;
//! use std::fs::File;
//! use std::io::BufWriter;
//!
//! let source = File::open("book.fb2")?;
//! let sink = BufWriter::new(File::create("book.mbook")?);
//! let report = Converter::new().convert(source, sink)?;
//!
//! println!("converted from {}", report.format);
//! for warning in &report.warnings {
//!     eprintln!("{warning}");
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! The design is described by RFC 0003 in the `rfcs` folder of the
//! repository.
//!
//! # Features
//!
//! Each import format has a feature enabled by default.
//!
//! | Feature | Format |
//! | --- | --- |
//! | `epub` | EPUB 2 and 3 |
//! | `fb2` | `FictionBook` 2, plain or inside a ZIP |
//! | `pdf` | PDF |
//! | `cbz` | Comic book archive |
//! | `docx` | Word documents |
//! | `html` | One HTML document |
//! | `md` | One Markdown document |
//! | `txt` | Plain text |
//!
//! A source in a format whose feature is disabled is still recognised and
//! refused with [`Error::FormatDisabled`].
//!
//! The opt-in `export` feature enables writing all eight formats, including
//! their picture codecs. It currently requires Rust 1.98 or later.

#[cfg(not(any(
    feature = "epub",
    feature = "fb2",
    feature = "pdf",
    feature = "cbz",
    feature = "docx",
    feature = "html",
    feature = "md",
    feature = "txt",
    feature = "export"
)))]
compile_error!("mbook-converter needs at least one import format or the export feature");

mod assemble;
mod converter;
mod draft;
// Text in any encoding, for the formats that are text and nothing else.
#[cfg(any(feature = "md", feature = "txt"))]
mod encoding;
mod error;
#[cfg(feature = "export")]
mod export;
mod format;
mod formats;
mod markdown;
mod options;
#[cfg(pictures)]
mod picture;
mod report;
// HTML to Markdown, shared by the formats whose text is HTML.
#[cfg(any(feature = "epub", feature = "html"))]
mod xhtml;

pub use converter::Converter;
pub use error::{Error, Result};
pub use format::Format;
pub use mbook;
pub use options::{ConvertLimits, ConvertOptions, PictureOptions};
pub use report::{Converted, Report, Warning, WarningKind};
