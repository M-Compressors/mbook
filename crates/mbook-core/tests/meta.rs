//! The metadata of a book, through the public API.
#![cfg(feature = "write")]

use mbook_core::{Book, Error, MetaError, Result};
use std::error::Error as _;

fn book() -> Result<Book> {
    Book::new("The Example", "en")
}

#[test]
fn a_new_book_has_a_title_a_language_and_an_identifier() -> Result<()> {
    let book = book()?;
    let meta = book.meta();
    assert_eq!((meta.title(), meta.language()), ("The Example", "en"));
    assert!(meta.id().starts_with("urn:uuid:"));
    Ok(())
}

#[test]
fn two_new_books_get_different_identifiers() -> Result<()> {
    assert_ne!(book()?.meta().id(), book()?.meta().id());
    Ok(())
}

#[test]
fn the_identifier_can_be_replaced_with_a_stable_one() -> Result<()> {
    let mut book = book()?;
    book.meta_mut().set_id("urn:isbn:9780000000002")?;
    assert_eq!(book.meta().id(), "urn:isbn:9780000000002");
    Ok(())
}

#[test]
fn people_are_listed_in_the_order_they_were_added() -> Result<()> {
    let mut book = book()?;
    let meta = book.meta_mut();
    meta.add_author("A. Writer")?;
    meta.add_author("C. Second")?;
    meta.add_translator("B. Translator")?;
    assert_eq!(
        meta.authors().collect::<Vec<_>>(),
        ["A. Writer", "C. Second"]
    );
    assert_eq!(meta.translators().collect::<Vec<_>>(), ["B. Translator"]);
    Ok(())
}

#[test]
fn optional_keys_are_absent_until_set() -> Result<()> {
    let book = book()?;
    let meta = book.meta();
    assert_eq!(
        (
            meta.publisher(),
            meta.date(),
            meta.annotation(),
            meta.cover()
        ),
        (None, None, None, None)
    );
    assert!(meta.series().is_none());
    Ok(())
}

#[test]
fn optional_keys_hold_what_was_set() -> Result<()> {
    let mut book = book()?;
    let meta = book.meta_mut();
    meta.set_publisher("Example Press")?;
    meta.set_date("2026-11")?;
    meta.set_annotation("One.\n\nTwo.")?;
    meta.set_cover("img/cover.avif")?;
    assert_eq!(
        (
            meta.publisher(),
            meta.date(),
            meta.annotation(),
            meta.cover()
        ),
        (
            Some("Example Press"),
            Some("2026-11"),
            Some("One.\n\nTwo."),
            Some("img/cover.avif")
        )
    );
    Ok(())
}

#[test]
fn a_series_carries_its_index() -> Result<()> {
    let mut book = book()?;
    book.meta_mut().set_series("The Saga", Some("2.5"))?;
    let series = book
        .meta()
        .series()
        .map(|series| (series.name, series.index));
    assert_eq!(series, Some(("The Saga", Some("2.5"))));
    Ok(())
}

#[test]
fn repeated_keys_keep_every_value() -> Result<()> {
    let mut book = book()?;
    let meta = book.meta_mut();
    meta.add_isbn("9780000000002")?;
    meta.add_isbn("0000000019")?;
    meta.add_genre("fiction")?;
    meta.add("lang", "fr")?;
    assert_eq!(meta.isbns().count(), 2);
    assert_eq!(meta.genres().collect::<Vec<_>>(), ["fiction"]);
    assert_eq!(meta.languages().collect::<Vec<_>>(), ["en", "fr"]);
    Ok(())
}

#[test]
fn rows_are_listed_in_the_order_they_are_stored() -> Result<()> {
    let mut book = book()?;
    book.meta_mut().set("x-shelf", "favourites")?;
    let keys: Vec<_> = book.meta().iter().map(|(key, _)| key).collect();
    assert_eq!(keys, ["title", "lang", "id", "x-shelf"]);
    Ok(())
}

#[test]
fn removing_an_optional_key_makes_it_absent() -> Result<()> {
    let mut book = book()?;
    let meta = book.meta_mut();
    meta.set_publisher("Example Press")?;
    meta.remove("publisher")?;
    assert_eq!(meta.publisher(), None);
    Ok(())
}

#[test]
fn an_empty_title_is_refused_and_the_old_one_kept() -> Result<()> {
    let mut book = book()?;
    let result = book.meta_mut().set_title("");
    assert!(matches!(
        result,
        Err(Error::InvalidMeta(MetaError::EmptyTitle))
    ));
    assert_eq!(book.meta().title(), "The Example");
    Ok(())
}

#[test]
fn a_required_key_cannot_be_removed() -> Result<()> {
    let mut book = book()?;
    let result = book.meta_mut().remove("lang");
    assert!(matches!(result, Err(Error::InvalidMeta(MetaError::Required(key))) if key == "lang"));
    Ok(())
}

#[test]
fn a_refused_change_explains_itself_through_the_source_chain() -> Result<()> {
    let mut book = book()?;
    let Err(error) = book.meta_mut().add("title", "Second") else {
        panic!("a second title must be refused");
    };
    let source = error.source().map(ToString::to_string);
    assert_eq!(
        source.as_deref(),
        Some(r#"the key "title" holds one value"#)
    );
    Ok(())
}

#[test]
fn a_key_with_a_space_is_refused() -> Result<()> {
    let mut book = book()?;
    let result = book.meta_mut().set("my key", "x");
    assert!(
        matches!(result, Err(Error::InvalidMeta(MetaError::InvalidKey(key))) if key == "my key")
    );
    Ok(())
}
