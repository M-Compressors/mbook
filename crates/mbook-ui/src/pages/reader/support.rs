//! Books written to a temporary folder, for the tests of the reader.

use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;

use mbook::Book;

/// The type the tests of the reader return.
pub(super) type TestResult<T = ()> = Result<T, Box<dyn Error>>;

/// The picture the books carry, at `img/fig.avif`.
pub(super) const FIGURE: &str = "img/fig.avif";

/// The bytes of a fixture of the `mbook` crate.
pub(super) fn fixture(name: &str) -> TestResult<Vec<u8>> {
    let path = format!(
        "{}/../mbook/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    Ok(std::fs::read(path)?)
}

/// Writes a book of `chapters` (path and text) to `path`. With `figure` it
/// also holds a picture at [`FIGURE`], whose bytes are the fixture of that
/// name, and with `language` it is in that language.
pub(super) fn write_book(
    path: &Path,
    title: &str,
    language: &str,
    chapters: &[(&str, &str)],
    figure: Option<&str>,
) -> TestResult {
    let mut book = Book::new(title, language)?;
    for (chapter, text) in chapters {
        book.add_chapter(*chapter, *text)?;
    }
    if let Some(figure) = figure {
        book.add_picture(FIGURE, fixture(figure)?)?;
    }
    book.save(path)?;
    Ok(())
}

/// A chapter of `paragraphs` paragraphs under a heading, long enough to
/// scroll in a window.
pub(super) fn long_chapter(heading: &str, paragraphs: usize) -> String {
    let mut text = format!("# {heading}\n\n");
    for n in 1..=paragraphs {
        writeln!(
            text,
            "Paragraph {n} of {heading}. It has a few words so that it takes room on a page, and a few more.\n"
        )
        .unwrap();
    }
    text
}
