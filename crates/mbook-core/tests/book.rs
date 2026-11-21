//! Building and rearranging a book, as far as it works without the Markdown
//! and AVIF analysis: argument checks happen before either is needed.
#![cfg(feature = "write")]

use mbook_core::{Book, Error, PathError, Result, Toc, TocItem};

fn book() -> Result<Book> {
    Book::new("The Example", "en")
}

#[test]
fn a_new_book_is_empty() -> Result<()> {
    let book = book()?;
    assert!(book.chapters().is_empty() && book.pictures().is_empty());
    Ok(())
}

#[test]
fn a_new_book_builds_its_contents_from_every_heading() -> Result<()> {
    assert_eq!(book()?.toc(), &Toc::default());
    assert_eq!(Toc::default(), Toc::FromHeadings { max_level: 6 });
    Ok(())
}

#[test]
fn a_book_without_a_title_is_refused() {
    assert!(matches!(Book::new("", "en"), Err(Error::InvalidMeta(_))));
}

#[test]
fn explicit_contents_replace_the_default() -> Result<()> {
    let mut book = book()?;
    let items = vec![
        TocItem::new(1, "Arrival", "ch01.md"),
        TocItem::new(2, "The station", "ch01.md#the-station"),
    ];
    book.set_toc(Toc::Explicit(items.clone()));
    assert_eq!(book.toc(), &Toc::Explicit(items));
    Ok(())
}

#[test]
fn a_chapter_path_that_leaves_the_book_is_refused() -> Result<()> {
    let mut book = book()?;
    let result = book.add_chapter("../ch01.md", "# One\n");
    assert!(matches!(
        result,
        Err(Error::InvalidPath { path, reason: PathError::DotSegment }) if path == "../ch01.md"
    ));
    Ok(())
}

#[test]
fn an_absolute_picture_path_is_refused() -> Result<()> {
    let mut book = book()?;
    let result = book.add_picture("/img/a.avif", Vec::new());
    assert!(matches!(
        result,
        Err(Error::InvalidPath {
            reason: PathError::EmptySegment,
            ..
        })
    ));
    Ok(())
}

#[test]
fn an_empty_cover_path_is_refused_and_no_cover_is_named() -> Result<()> {
    let mut book = book()?;
    let result = book.set_cover("", Vec::new());
    assert!(matches!(
        result,
        Err(Error::InvalidPath {
            reason: PathError::Empty,
            ..
        })
    ));
    assert_eq!(book.meta().cover(), None);
    Ok(())
}

#[test]
fn a_chapter_cannot_be_inserted_past_the_end() -> Result<()> {
    let mut book = book()?;
    let result = book.insert_chapter(1, "ch01.md", "# One\n");
    assert!(matches!(
        result,
        Err(Error::IndexOutOfRange { index: 1, len: 0 })
    ));
    Ok(())
}

#[test]
fn moving_an_unknown_chapter_is_an_error() -> Result<()> {
    let mut book = book()?;
    let result = book.move_chapter("ch01.md", 0);
    assert!(matches!(result, Err(Error::NoSuchPath(path)) if path == "ch01.md"));
    Ok(())
}

#[test]
fn removing_something_that_is_not_there_returns_nothing() -> Result<()> {
    let mut book = book()?;
    assert!(book.remove_chapter("ch01.md").is_none());
    assert!(book.remove_picture("img/a.avif").is_none());
    Ok(())
}

#[test]
fn unknown_paths_find_nothing() -> Result<()> {
    let mut book = book()?;
    assert!(book.chapter("ch01.md").is_none());
    assert!(book.picture("img/a.avif").is_none());
    assert!(book.chapter_mut("ch01.md").is_none());
    Ok(())
}

#[test]
fn a_book_can_be_cloned_with_its_metadata() -> Result<()> {
    let mut book = book()?;
    book.meta_mut().add_author("A. Writer")?;
    let copy = book.clone();
    assert_eq!(copy.meta(), book.meta());
    Ok(())
}
