//! The example book of RFC 0001, built with the public API.

use crate::texts::{CH01, CH01_NAMED, CH02, CH03, ID, PLAIN, ROTATED};
use mbook_core::{Book, Landmark, Result};

/// The example book: a cover, two chapters and one figure.
pub(crate) fn book() -> Result<Book> {
    let mut book = Book::new("The Example", "en")?;
    book.meta_mut().set_id(ID)?;
    book.meta_mut().add_author("A. Writer")?;
    book.set_cover("img/cover.avif", PLAIN)?;
    book.add_chapter("ch01.md", CH01)?;
    book.add_chapter("ch02.md", CH02)?;
    book.add_picture("img/fig-1.avif", ROTATED)?;
    Ok(book)
}

/// The example book with names for two lines of its first chapter that are
/// not headings, and a third chapter that links to them.
pub(crate) fn annotated() -> Result<Book> {
    let mut book = book()?;
    if let Some(chapter) = book.chapter_mut("ch01.md") {
        chapter.set_text(CH01_NAMED)?;
    }
    book.add_chapter("ch03.md", CH03)?;
    Ok(book)
}

/// The annotated example with landmarks: a start, a place that is a
/// heading, a place that only an anchor definition names, and one more with
/// a title of its own.
pub(crate) fn with_landmarks() -> Result<Book> {
    let mut book = annotated()?;
    book.set_landmarks(vec![
        Landmark::new("start", "", "ch01.md"),
        Landmark::new("preface", "", "ch01.md#the-station"),
        Landmark::new("list-of-figures", "Figures", "ch01.md#fig-1"),
        Landmark::new("x-letter", "The letter", "ch02.md"),
    ])?;
    Ok(book)
}

/// The example book as the bytes of a file.
#[cfg(feature = "read")]
pub(crate) fn bytes() -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    book()?.write(&mut bytes)?;
    Ok(bytes)
}
