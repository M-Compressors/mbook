//! The example book of RFC 0001, built with the public API.

use crate::texts::{CH01, CH02, ID, PLAIN, ROTATED};
use mbook_core::{Book, Result};

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

/// The example book as the bytes of a file.
#[cfg(feature = "read")]
pub(crate) fn bytes() -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    book()?.write(&mut bytes)?;
    Ok(bytes)
}
