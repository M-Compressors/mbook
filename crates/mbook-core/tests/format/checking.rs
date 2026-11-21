//! What `Book::check` reports, and that nothing invalid is written.

use crate::sample;
use mbook_core::{Book, Error, Problem, ProblemKind, Result, Toc, TocItem};

/// The problems of a book as (chapter, kind) pairs.
fn problems(book: &Book) -> Vec<(Option<String>, ProblemKind)> {
    match book.check() {
        Ok(()) => Vec::new(),
        Err(problems) => problems
            .iter()
            .map(|Problem { path, kind, .. }| (path.clone(), kind.clone()))
            .collect(),
    }
}

/// A book with one chapter of the given text, next to the example's parts.
fn with_chapter(text: &str) -> Result<Book> {
    let mut book = sample::book()?;
    book.add_chapter("extra.md", text)?;
    Ok(book)
}

fn in_extra(kind: ProblemKind) -> Vec<(Option<String>, ProblemKind)> {
    vec![(Some("extra.md".to_owned()), kind)]
}

#[test]
fn the_example_book_has_no_problems() -> Result<()> {
    assert!(sample::book()?.check().is_ok());
    Ok(())
}

#[test]
fn a_link_to_a_missing_chapter_is_reported() -> Result<()> {
    let book = with_chapter("See [nine](ch09.md).\n")?;
    assert_eq!(
        problems(&book),
        in_extra(ProblemKind::UnresolvedLink("ch09.md".into()))
    );
    Ok(())
}

#[test]
fn a_link_to_a_missing_anchor_is_reported() -> Result<()> {
    let book = with_chapter("See [it](ch01.md#no-such-heading).\n")?;
    assert_eq!(
        problems(&book),
        in_extra(ProblemKind::UnresolvedLink(
            "ch01.md#no-such-heading".into()
        ))
    );
    Ok(())
}

#[test]
fn a_link_to_a_picture_is_reported() -> Result<()> {
    let book = with_chapter("Open [full size](img/fig-1.avif).\n")?;
    assert_eq!(
        problems(&book),
        in_extra(ProblemKind::UnresolvedLink("img/fig-1.avif".into()))
    );
    Ok(())
}

#[test]
fn a_link_into_the_same_chapter_resolves() -> Result<()> {
    let book = with_chapter("# Top\n\nBack to [top](#top).\n")?;
    assert_eq!(problems(&book), []);
    Ok(())
}

#[test]
fn external_links_are_allowed() -> Result<()> {
    let book = with_chapter("[web](https://example.org) and [mail](mailto:a@example.org).\n")?;
    assert_eq!(problems(&book), []);
    Ok(())
}

#[test]
fn a_link_with_another_scheme_is_reported() -> Result<()> {
    let book = with_chapter("[x](ftp://example.org/file)\n")?;
    assert_eq!(
        problems(&book),
        in_extra(ProblemKind::ForbiddenScheme(
            "ftp://example.org/file".into()
        ))
    );
    Ok(())
}

#[test]
fn a_picture_that_is_not_in_the_book_is_reported() -> Result<()> {
    let book = with_chapter("![missing](img/none.avif)\n")?;
    assert_eq!(
        problems(&book),
        in_extra(ProblemKind::UnresolvedPicture("img/none.avif".into()))
    );
    Ok(())
}

#[test]
fn a_picture_that_names_a_chapter_is_reported() -> Result<()> {
    let book = with_chapter("![odd](ch01.md)\n")?;
    assert_eq!(
        problems(&book),
        in_extra(ProblemKind::UnresolvedPicture("ch01.md".into()))
    );
    Ok(())
}

#[test]
fn a_picture_with_a_fragment_is_reported() -> Result<()> {
    let book = with_chapter("![odd](img/fig-1.avif#x)\n")?;
    assert_eq!(
        problems(&book),
        in_extra(ProblemKind::UnresolvedPicture("img/fig-1.avif#x".into()))
    );
    Ok(())
}

#[test]
fn a_picture_from_the_network_is_reported() -> Result<()> {
    let book = with_chapter("![tracker](https://example.org/pixel.avif)\n")?;
    assert_eq!(
        problems(&book),
        in_extra(ProblemKind::ForbiddenScheme(
            "https://example.org/pixel.avif".into()
        ))
    );
    Ok(())
}

#[test]
fn raw_html_is_reported_with_its_place() -> Result<()> {
    let book = with_chapter("Text.\n\n<div>block</div>\n")?;
    let Err(found) = book.check() else {
        panic!("raw HTML must be reported");
    };
    let places: Vec<_> = found.iter().map(|p| (p.offset, p.kind.clone())).collect();
    assert_eq!(places, [(Some(7), ProblemKind::RawHtml)]);
    Ok(())
}

#[test]
fn a_cover_that_is_not_a_picture_of_the_book_is_reported() -> Result<()> {
    let mut book = sample::book()?;
    book.meta_mut().set_cover("img/gone.avif")?;
    assert_eq!(problems(&book), [(None, ProblemKind::MissingCover)]);
    Ok(())
}

#[test]
fn a_removed_picture_leaves_a_reported_reference() -> Result<()> {
    let mut book = sample::book()?;
    book.remove_picture("img/fig-1.avif");
    assert_eq!(
        problems(&book),
        [(
            Some("ch01.md".to_owned()),
            ProblemKind::UnresolvedPicture("img/fig-1.avif".into())
        )]
    );
    Ok(())
}

#[test]
fn a_removed_chapter_leaves_a_reported_link() -> Result<()> {
    let mut book = sample::book()?;
    book.remove_chapter("ch01.md");
    assert_eq!(
        problems(&book),
        [(
            Some("ch02.md".to_owned()),
            ProblemKind::UnresolvedLink("ch01.md#the-station".into())
        )]
    );
    Ok(())
}

#[test]
fn every_problem_is_reported_not_only_the_first() -> Result<()> {
    let mut book = with_chapter("[a](none.md) <b>x</b> ![c](none.avif)\n")?;
    book.meta_mut().set_cover("nope.avif")?;
    let kinds: Vec<_> = problems(&book).into_iter().map(|(_, kind)| kind).collect();
    assert_eq!(kinds.len(), 5, "{kinds:?}");
    Ok(())
}

fn contents_problems(items: Vec<TocItem>) -> Result<Vec<ProblemKind>> {
    let mut book = sample::book()?;
    book.set_toc(Toc::Explicit(items));
    Ok(problems(&book).into_iter().map(|(_, kind)| kind).collect())
}

#[test]
fn a_contents_item_to_a_missing_chapter_is_reported() -> Result<()> {
    let found = contents_problems(vec![TocItem::new(1, "Nine", "ch09.md")])?;
    assert_eq!(found, [ProblemKind::DanglingTocItem("Nine".into())]);
    Ok(())
}

#[test]
fn a_contents_item_to_a_missing_anchor_is_reported() -> Result<()> {
    let found = contents_problems(vec![TocItem::new(1, "Lost", "ch01.md#lost")])?;
    assert_eq!(found, [ProblemKind::DanglingTocItem("Lost".into())]);
    Ok(())
}

#[test]
fn contents_that_start_below_the_top_level_are_reported() -> Result<()> {
    let found = contents_problems(vec![TocItem::new(2, "Deep", "ch01.md")])?;
    assert_eq!(found, [ProblemKind::TocLevel("Deep".into())]);
    Ok(())
}

#[test]
fn contents_that_skip_a_level_are_reported() -> Result<()> {
    let found = contents_problems(vec![
        TocItem::new(1, "One", "ch01.md"),
        TocItem::new(3, "Three", "ch01.md#the-station"),
    ])?;
    assert_eq!(found, [ProblemKind::TocLevel("Three".into())]);
    Ok(())
}

#[test]
fn contents_out_of_reading_order_are_reported() -> Result<()> {
    let found = contents_problems(vec![
        TocItem::new(1, "Two", "ch02.md"),
        TocItem::new(1, "One", "ch01.md"),
    ])?;
    assert_eq!(found, [ProblemKind::TocOrder("One".into())]);
    Ok(())
}

#[test]
fn a_contents_item_without_a_title_is_reported() -> Result<()> {
    let found = contents_problems(vec![TocItem::new(1, "", "ch01.md")])?;
    assert_eq!(found, [ProblemKind::EmptyTocTitle]);
    Ok(())
}

#[test]
fn an_invalid_book_is_not_written() -> Result<()> {
    let book = with_chapter("[a](none.md)\n")?;
    let mut bytes = Vec::new();
    let result = book.write(&mut bytes);
    assert!(matches!(result, Err(Error::Invalid(found)) if found.len() == 1));
    assert_eq!(bytes.len(), 0);
    Ok(())
}
