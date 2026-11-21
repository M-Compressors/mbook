//! Reading the example book of RFC 0001 from a checked-in file.

use crate::raw::Raw;
use crate::texts::{CH01, CH02, ID, PLAIN, ROTATED, station_offset};
use mbook::{
    Book, BookReader, Destination, Error, ReadOptions, ResolveError, ResourceKind, Result, Size,
    Version,
};
use std::io::Cursor;

/// The example book as written by this crate.
const EXAMPLE: &[u8] = include_bytes!("../fixtures/example.mbook");

type Reader = BookReader<Cursor<&'static [u8]>>;

fn open() -> Result<Reader> {
    BookReader::new(Cursor::new(EXAMPLE))
}

fn with_limits(change: impl FnOnce(&mut ReadOptions)) -> ReadOptions {
    let mut options = ReadOptions::default();
    change(&mut options);
    options
}

#[test]
fn the_declared_version_is_one_dot_zero() -> Result<()> {
    assert_eq!(open()?.version(), Version { major: 1, minor: 0 });
    Ok(())
}

#[test]
fn the_metadata_is_available_after_opening() -> Result<()> {
    let reader = open()?;
    let meta = reader.meta();
    assert_eq!(
        (meta.title(), meta.language(), meta.id(), meta.cover()),
        ("The Example", "en", ID, Some("img/cover.avif"))
    );
    assert_eq!(meta.authors().collect::<Vec<_>>(), ["A. Writer"]);
    Ok(())
}

#[test]
fn the_contents_list_every_heading_with_its_place() -> Result<()> {
    let reader = open()?;
    let entries: Vec<_> = reader
        .toc()
        .iter()
        .map(|e| {
            (
                e.level,
                e.title.as_str(),
                e.position.chapter.number(),
                e.position.offset,
            )
        })
        .collect();
    assert_eq!(
        entries,
        [
            (1, "Chapter 1. Arrival", 6, 0),
            (2, "The station", 6, station_offset()),
            (1, "Chapter 2. The letter", 7, 0),
        ]
    );
    Ok(())
}

#[test]
fn chapters_are_listed_in_reading_order_with_their_length() -> Result<()> {
    let reader = open()?;
    let chapters: Vec<_> = reader.chapters().map(|c| (c.path, c.decoded_len)).collect();
    assert_eq!(
        chapters,
        [
            ("ch01.md", CH01.len() as u64),
            ("ch02.md", CH02.len() as u64)
        ]
    );
    Ok(())
}

#[test]
fn pictures_are_listed_with_their_displayed_size() -> Result<()> {
    let reader = open()?;
    let size = |width, height| Size { width, height };
    let pictures: Vec<_> = reader.pictures().map(|p| (p.path, p.size)).collect();
    assert_eq!(
        pictures,
        [
            ("img/cover.avif", size(24, 16)),
            ("img/fig-1.avif", size(16, 24))
        ]
    );
    Ok(())
}

#[test]
fn the_cover_is_found_and_read() -> Result<()> {
    let mut reader = open()?;
    assert_eq!(
        reader.cover().map(|cover| cover.path),
        Some("img/cover.avif")
    );
    assert_eq!(reader.read_cover()?.as_deref(), Some(PLAIN));
    Ok(())
}

#[test]
fn a_chapter_is_read_as_the_text_that_was_stored() -> Result<()> {
    let mut reader = open()?;
    let Some(id) = reader.find_chapter("ch01.md").map(|chapter| chapter.id) else {
        panic!("the example has a first chapter");
    };
    assert_eq!(reader.read_chapter(id)?, CH01);
    Ok(())
}

#[test]
fn a_picture_is_read_as_the_bytes_that_were_stored() -> Result<()> {
    let mut reader = open()?;
    let Some(id) = reader
        .find_picture("img/fig-1.avif")
        .map(|picture| picture.id)
    else {
        panic!("the example has a figure");
    };
    assert_eq!(reader.read_picture(id)?, ROTATED);
    Ok(())
}

#[test]
fn chapters_can_be_read_in_any_order_and_more_than_once() -> Result<()> {
    let mut reader = open()?;
    let ids: Vec<_> = reader.chapters().map(|chapter| chapter.id).collect();
    let mut texts = Vec::new();
    for &id in ids.iter().rev().chain(&ids) {
        texts.push(reader.read_chapter(id)?);
    }
    assert_eq!(texts, [CH02, CH01, CH01, CH02]);
    Ok(())
}

#[test]
fn an_id_is_restored_from_its_number() -> Result<()> {
    let reader = open()?;
    let restored = reader.chapter_id(7).and_then(|id| reader.chapter(id));
    assert_eq!(restored.map(|chapter| chapter.path), Some("ch02.md"));
    assert!(reader.chapter_id(5).is_none() && reader.picture_id(7).is_none());
    Ok(())
}

#[test]
fn the_anchors_of_a_chapter_are_named_after_its_headings() -> Result<()> {
    let reader = open()?;
    let Some(id) = reader.chapter_id(6) else {
        panic!("resource 6 is the first chapter");
    };
    let anchors: Vec<_> = reader
        .anchors(id)
        .iter()
        .map(|a| (a.offset, a.name.as_str()))
        .collect();
    assert_eq!(
        anchors,
        [(0, "chapter-1-arrival"), (station_offset(), "the-station")]
    );
    Ok(())
}

#[test]
fn the_link_of_the_second_chapter_leads_to_the_station() -> Result<()> {
    let reader = open()?;
    let Some(from) = reader.chapter_id(7) else {
        panic!("resource 7 is the second chapter");
    };
    assert!(matches!(
        reader.resolve(from, "ch01.md#the-station"),
        Ok(Destination::Chapter(position))
            if position.chapter.number() == 6 && position.offset == station_offset()
    ));
    Ok(())
}

#[test]
fn the_figure_of_the_first_chapter_resolves_to_its_picture() -> Result<()> {
    let reader = open()?;
    let Some(from) = reader.chapter_id(6) else {
        panic!("resource 6 is the first chapter");
    };
    let picture = match reader.resolve(from, "img/fig-1.avif") {
        Ok(Destination::Picture(id)) => reader.picture(id).map(|picture| picture.path),
        _ => None,
    };
    assert_eq!(picture, Some("img/fig-1.avif"));
    assert_eq!(
        reader.resolve(from, "https://example.org"),
        Ok(Destination::External)
    );
    assert_eq!(
        reader.resolve(from, "img/none.avif"),
        Err(ResolveError::NotFound)
    );
    Ok(())
}

#[test]
fn every_resource_is_listed_with_its_kind() -> Result<()> {
    let reader = open()?;
    let kinds: Vec<_> = reader.resources().map(|resource| resource.kind).collect();
    let (service, text, picture) = (
        ResourceKind::Service,
        ResourceKind::Text,
        ResourceKind::Picture,
    );
    assert_eq!(
        kinds,
        [
            service, service, service, service, service, picture, text, text, picture
        ]
    );
    Ok(())
}

#[test]
fn a_stream_that_cannot_seek_is_buffered() -> Result<()> {
    // A byte slice is a reader that cannot seek.
    let mut reader = BookReader::buffered(EXAMPLE)?;
    let Some(id) = reader.chapter_id(7) else {
        panic!("resource 7 is the second chapter");
    };
    assert_eq!(reader.read_chapter(id)?, CH02);
    Ok(())
}

#[test]
fn a_book_may_start_anywhere_in_its_source() -> Result<()> {
    let mut bytes = b"something before the book".to_vec();
    let start = bytes.len() as u64;
    bytes.extend_from_slice(EXAMPLE);
    let mut source = Cursor::new(bytes);
    source.set_position(start);
    let mut reader = BookReader::new(source)?;
    assert_eq!(reader.read_cover()?.as_deref(), Some(PLAIN));
    Ok(())
}

#[test]
fn the_source_is_given_back() -> Result<()> {
    assert_eq!(open()?.into_inner().into_inner().len(), EXAMPLE.len());
    Ok(())
}

#[test]
fn the_whole_book_is_read_into_memory() -> Result<()> {
    let book = Book::read(EXAMPLE)?;
    let chapters: Vec<_> = book
        .chapters()
        .iter()
        .map(|c| (c.path(), c.text()))
        .collect();
    assert_eq!(chapters, [("ch01.md", CH01), ("ch02.md", CH02)]);
    assert_eq!(
        book.picture("img/cover.avif").map(mbook::Picture::data),
        Some(PLAIN)
    );
    Ok(())
}

#[test]
fn a_loaded_chapter_keeps_the_anchors_of_the_file() -> Result<()> {
    let book = BookReader::new(Cursor::new(EXAMPLE))?.load()?;
    let names: Vec<_> = book
        .chapter("ch01.md")
        .map(|chapter| chapter.anchors().iter().map(|a| a.name.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(names, ["chapter-1-arrival", "the-station"]);
    Ok(())
}

#[test]
fn a_file_of_another_format_is_not_an_mbook() {
    let zip = b"PK\x03\x04 this is certainly not a framed container";
    assert!(matches!(
        BookReader::new(Cursor::new(&zip[..])),
        Err(Error::NotMbook)
    ));
    assert!(matches!(Book::read(&zip[..]), Err(Error::NotMbook)));
}

#[test]
fn an_empty_source_is_not_an_mbook() {
    assert!(matches!(
        BookReader::new(Cursor::new(&[][..])),
        Err(Error::NotMbook)
    ));
    assert!(matches!(Book::read(&[][..]), Err(Error::NotMbook)));
}

#[test]
fn a_truncated_file_is_damaged() {
    let cut = &EXAMPLE[..EXAMPLE.len() - 10];
    assert!(matches!(
        BookReader::new(Cursor::new(cut)),
        Err(Error::Container(_))
    ));
    assert!(matches!(Book::read(cut), Err(Error::Container(_))));
}

#[test]
fn damage_in_one_picture_leaves_the_rest_readable() -> Result<()> {
    // Pictures are stored, so the bytes of the figure are found in the file.
    let Some(at) = EXAMPLE
        .windows(ROTATED.len())
        .position(|window| window == ROTATED)
    else {
        panic!("the figure is stored as it is");
    };
    let mut damaged = EXAMPLE.to_vec();
    damaged[at + ROTATED.len() / 2] ^= 0xff;
    let mut reader = BookReader::new(Cursor::new(damaged))?;
    let (Some(figure), Some(chapter)) = (reader.picture_id(8), reader.chapter_id(6)) else {
        panic!("the example has a figure and a first chapter");
    };
    assert!(matches!(
        reader.read_picture(figure),
        Err(Error::Container(_))
    ));
    assert_eq!(reader.read_chapter(chapter)?, CH01);
    Ok(())
}

#[test]
fn an_id_from_another_book_reads_nothing() -> Result<()> {
    // In this hand-built book resource 9 is a third chapter.
    let other = Raw::example()
        .with(|raw| raw.push_resource(1, "ch03.md", b"# Three\n"))
        .build()
        .map_err(Error::Container)?;
    let other = BookReader::new(Cursor::new(other))?;
    let Some(foreign) = other.chapter_id(9) else {
        panic!("resource 9 is a chapter of the other book");
    };
    let mut reader = open()?;
    assert!(reader.chapter(foreign).is_none() && reader.anchors(foreign).is_empty());
    assert!(matches!(
        reader.read_chapter(foreign),
        Err(Error::NoSuchResource(9))
    ));
    Ok(())
}

#[test]
fn a_file_with_more_resources_than_the_limit_is_refused() {
    let options = with_limits(|options| options.limits.max_resources = 4);
    let result = BookReader::with_options(Cursor::new(EXAMPLE), &options);
    assert!(matches!(result, Err(Error::Limit(_))));
}

#[test]
fn a_file_larger_than_the_limit_is_refused() {
    let options = with_limits(|options| options.limits.max_file_bytes = 512);
    let result = BookReader::with_options(Cursor::new(EXAMPLE), &options);
    assert!(matches!(result, Err(Error::Limit(_))));
    assert!(matches!(
        Book::read_with(EXAMPLE, &options),
        Err(Error::Limit(_))
    ));
}

#[test]
fn a_resource_above_the_limit_fails_only_when_it_is_read() -> Result<()> {
    // The tables and the chapters are smaller than 300 bytes; pictures are not.
    let options = with_limits(|options| options.limits.max_resource_bytes = 300);
    let mut reader = BookReader::with_options(Cursor::new(EXAMPLE), &options)?;
    let (Some(cover), Some(chapter)) = (reader.picture_id(5), reader.chapter_id(7)) else {
        panic!("the example has a cover and a second chapter");
    };
    assert!(matches!(reader.read_picture(cover), Err(Error::Limit(_))));
    assert_eq!(reader.read_chapter(chapter)?, CH02);
    Ok(())
}

#[test]
fn a_book_that_decodes_to_more_than_the_file_limit_is_not_loaded() -> Result<()> {
    // A chapter that compresses a thousandfold: the file is small, the book
    // in memory is not.
    let big = "all work and no play makes a dull book.\n".repeat(5_000);
    let bytes = Raw::example()
        .with(|raw| raw.push_resource(1, "big.md", big.as_bytes()))
        .build()
        .map_err(Error::Container)?;
    assert!(bytes.len() < 20_000 && big.len() > 20_000);
    let options = with_limits(|options| options.limits.max_file_bytes = 20_000);
    let mut reader = BookReader::with_options(Cursor::new(&bytes), &options)?;
    assert!(matches!(reader.load(), Err(Error::Limit(_))));
    assert!(matches!(
        Book::read_with(bytes.as_slice(), &options),
        Err(Error::Limit(_))
    ));
    Ok(())
}
