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
//! The design is described in the `docs/src/mbook-converter` folder of the
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
//! The opt-in `rayon` feature encodes the pictures of a source on all cores
//! at once instead of one after another. The book is the same.
//!
//! The opt-in `export` feature enables writing all eight formats, including
//! their picture codecs, and [`figure`], which draws the diagrams and
//! formulas of a book. It currently requires Rust 1.98 or later.
//!
// Without `export`, the link to `figure` leads to the table above instead.
// The empty doc line above is what lets the definition start a new block.
#![cfg_attr(not(feature = "export"), doc = "[`figure`]: #features")]

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

// Without a format that carries pictures, `rayon` has nothing to encode.
#[cfg(all(feature = "rayon", not(parallel_pictures)))]
use rayon as _;

mod assemble;
mod converter;
mod draft;
// Text in any encoding, for the formats that are text and nothing else.
#[cfg(any(feature = "md", feature = "txt"))]
mod encoding;
mod error;
#[cfg(feature = "export")]
mod export;
#[cfg(feature = "export")]
pub mod figure;
mod format;
mod formats;
pub mod inline;
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
