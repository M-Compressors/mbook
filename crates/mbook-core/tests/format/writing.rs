//! Writing a book and reading back what was written.

use crate::raw::Raw;
use crate::sample;
use crate::texts::{CH01, CH01_NAMED, CH02, CROPPED, MIRRORED, PLAIN, ROTATED, named_offset};
use mbook_core::{
    Book, BookReader, ChapterId, Destination, Error, Limits, Position, ResolveError, ResourceKind,
    Result, Size, Toc, TocItem, WriteOptions, is_mbook,
};
use std::io::Cursor;
use std::path::Path;
use tempfile::tempdir;

/// The example book as a checked-in file, for the tests that only read.
const FIXTURE: &[u8] = include_bytes!("../fixtures/example.mbook");

fn written(book: &Book) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    book.write(&mut bytes)?;
    Ok(bytes)
}

/// The names of the files in a folder, sorted.
fn names(folder: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

/// Run with `cargo test --test format -- --ignored regenerate` after a
/// deliberate change of what the writer produces.
#[test]
#[ignore = "rewrites the checked-in fixture"]
fn regenerate_the_fixture() -> Result<()> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/example.mbook");
    std::fs::write(path, sample::bytes()?)?;
    Ok(())
}

#[test]
fn the_writer_still_produces_the_checked_in_file() -> Result<()> {
    assert!(
        sample::bytes()? == FIXTURE,
        "run the regenerate test if this is intended"
    );
    Ok(())
}

#[test]
fn a_file_starts_with_the_fixed_prefix_of_the_format_rfc() -> Result<()> {
    /// The first 53 bytes of every mbook file, from RFC 0001.
    const PREFIX: [u8; 53] = [
        0x4d, 0x4f, 0x5a, 0x46, 0x52, 0x4d, 0x01, 0x00, 0x52, 0x53, 0x52, 0x43, 0x00, 0x00, 0x00,
        0x00, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x8e, 0x00, 0x8c, 0xba, 0xdb, 0xa5, 0xb1, 0xd7, 0x03, 0x01, 0x09, 0xf8, 0x01,
        0x01, 0x08, 0x4d, 0x42, 0x4f, 0x4f, 0x4b, 0x00,
    ];
    let bytes = sample::bytes()?;
    assert_eq!(bytes.get(..53), Some(&PREFIX[..]));
    assert_eq!(bytes.get(53..55), Some(&[1, 0][..]));
    Ok(())
}

#[test]
fn a_written_file_is_recognised_by_its_first_bytes() -> Result<()> {
    assert!(is_mbook(&sample::bytes()?));
    Ok(())
}

#[test]
fn writing_the_same_book_twice_gives_the_same_bytes() -> Result<()> {
    let book = sample::book()?;
    assert_eq!(written(&book)?, written(&book)?);
    Ok(())
}

#[test]
fn a_book_survives_being_written_and_read() -> Result<()> {
    let book = sample::book()?;
    let read = Book::read(written(&book)?.as_slice())?;
    assert_eq!(read.meta(), book.meta());
    assert_eq!(read.chapters(), book.chapters());
    assert_eq!(read.pictures().len(), 2);
    Ok(())
}

#[test]
fn a_loaded_book_writes_back_to_the_same_bytes() -> Result<()> {
    let first = sample::bytes()?;
    let second = written(&Book::read(first.as_slice())?)?;
    assert_eq!(first, second);
    Ok(())
}

#[test]
fn loading_through_a_reader_equals_reading_the_stream() -> Result<()> {
    let bytes = sample::bytes()?;
    let loaded = BookReader::new(Cursor::new(&bytes))?.load()?;
    assert_eq!(loaded, Book::read(bytes.as_slice())?);
    Ok(())
}

#[test]
fn the_cover_is_written_first_and_the_other_pictures_last() -> Result<()> {
    let bytes = sample::bytes()?;
    let reader = BookReader::new(Cursor::new(&bytes))?;
    let paths: Vec<_> = reader.resources().skip(5).map(|r| r.path).collect();
    assert_eq!(
        paths,
        ["img/cover.avif", "ch01.md", "ch02.md", "img/fig-1.avif"]
    );
    Ok(())
}

#[test]
fn a_loaded_book_names_its_contents_like_links() -> Result<()> {
    let book = Book::read(sample::bytes()?.as_slice())?;
    let Toc::Explicit(items) = book.toc() else {
        panic!("a loaded book keeps its contents as written");
    };
    assert_eq!(
        items,
        &[
            TocItem::new(1, "Chapter 1. Arrival", "ch01.md"),
            TocItem::new(2, "The station", "ch01.md#the-station"),
            TocItem::new(1, "Chapter 2. The letter", "ch02.md"),
        ]
    );
    Ok(())
}

#[test]
fn picture_sizes_are_read_from_real_avif_files() -> Result<()> {
    let mut book = Book::new("Pictures", "en")?;
    let size = |width, height| Size { width, height };
    let sizes = [
        book.add_picture("plain.avif", PLAIN)?.size(),
        book.add_picture("rotated.avif", ROTATED)?.size(),
        book.add_picture("cropped.avif", CROPPED)?.size(),
        book.add_picture("mirrored.avif", MIRRORED)?.size(),
    ];
    assert_eq!(
        sizes,
        [size(24, 16), size(16, 24), size(16, 8), size(24, 16)]
    );
    Ok(())
}

#[test]
fn bytes_that_are_not_a_picture_are_refused() -> Result<()> {
    let mut book = Book::new("Pictures", "en")?;
    let result = book.add_picture("a.avif", b"GIF89a, definitely not AVIF".to_vec());
    assert!(matches!(result, Err(Error::InvalidPicture(_))));
    assert_eq!(book.pictures().len(), 0);
    Ok(())
}

#[test]
fn line_endings_and_a_byte_order_mark_are_normalised() -> Result<()> {
    let mut book = Book::new("Text", "en")?;
    let chapter = book.add_chapter("ch01.md", "\u{feff}# One\r\n\r\ntext\r")?;
    assert_eq!(chapter.text(), "# One\n\ntext\n");
    Ok(())
}

#[test]
fn a_control_character_is_refused_with_its_place() -> Result<()> {
    let mut book = Book::new("Text", "en")?;
    let result = book.add_chapter("ch01.md", "ab\u{7}c");
    assert!(matches!(result, Err(Error::InvalidText(error)) if error.offset == 2));
    Ok(())
}

#[test]
fn replacing_the_text_rebuilds_the_anchors() -> Result<()> {
    let mut book = sample::book()?;
    let Some(chapter) = book.chapter_mut("ch01.md") else {
        panic!("the example has a first chapter");
    };
    chapter.set_text("# Arrival\n\n## The platform\n")?;
    let names: Vec<_> = chapter.anchors().iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["arrival", "the-platform"]);
    Ok(())
}

#[test]
fn a_fixed_typo_keeps_links_and_contents_working() -> Result<()> {
    let mut book = Book::read(sample::bytes()?.as_slice())?;
    let Some(chapter) = book.chapter_mut("ch01.md") else {
        panic!("the example has a first chapter");
    };
    // The heading keeps its name while every offset after the edit moves.
    chapter.set_text(CH01.replace("The train was late.", "The train was very late indeed."))?;
    let reread = BookReader::new(Cursor::new(written(&book)?))?;
    let station = reread.toc().get(1).map(|entry| entry.position.offset);
    let moved = u32::try_from(CH01.find("## The station").unwrap_or(0) + 12).ok();
    assert_eq!(station, moved);
    Ok(())
}

#[test]
fn a_chapter_moves_to_another_path_by_remove_and_insert() -> Result<()> {
    let mut book = sample::book()?;
    let Some(old) = book.remove_chapter("ch02.md") else {
        panic!("the example has a second chapter");
    };
    // The moved chapter now links one folder up.
    book.insert_chapter(
        1,
        "part2/ch02.md",
        old.text().replace("ch01.md", "../ch01.md"),
    )?;
    let paths: Vec<_> = book
        .chapters()
        .iter()
        .map(|c| c.path().to_owned())
        .collect();
    assert_eq!(paths, ["ch01.md", "part2/ch02.md"]);
    assert!(book.check().is_ok());
    Ok(())
}

#[test]
fn explicit_contents_are_written_as_given() -> Result<()> {
    let mut book = sample::book()?;
    book.set_toc(Toc::Explicit(vec![
        TocItem::new(1, "Part one", "ch01.md"),
        TocItem::new(2, "At the station", "ch01.md#the-station"),
    ]));
    let reader = BookReader::new(Cursor::new(written(&book)?))?;
    let titles: Vec<_> = reader
        .toc()
        .iter()
        .map(|e| (e.level, e.title.as_str()))
        .collect();
    assert_eq!(titles, [(1, "Part one"), (2, "At the station")]);
    Ok(())
}

#[test]
fn contents_from_headings_respect_the_deepest_level() -> Result<()> {
    let mut book = sample::book()?;
    book.set_toc(Toc::FromHeadings { max_level: 1 });
    let reader = BookReader::new(Cursor::new(written(&book)?))?;
    let titles: Vec<_> = reader.toc().iter().map(|e| e.title.as_str()).collect();
    assert_eq!(titles, ["Chapter 1. Arrival", "Chapter 2. The letter"]);
    Ok(())
}

#[test]
fn heading_levels_that_skip_still_form_a_tree() -> Result<()> {
    let mut book = Book::new("Levels", "en")?;
    book.add_chapter(
        "a.md",
        "### Deep start\n\n# Top\n\n### Skipped\n\n## Back\n",
    )?;
    let reader = BookReader::new(Cursor::new(written(&book)?))?;
    let levels: Vec<_> = reader
        .toc()
        .iter()
        .map(|e| (e.level, e.title.as_str()))
        .collect();
    assert_eq!(
        levels,
        [(1, "Deep start"), (1, "Top"), (2, "Skipped"), (2, "Back")]
    );
    Ok(())
}

#[test]
fn a_book_without_chapters_or_pictures_can_be_written() -> Result<()> {
    let book = Book::new("Empty", "en")?;
    let reader = BookReader::new(Cursor::new(written(&book)?))?;
    assert_eq!(
        (
            reader.chapters().len(),
            reader.pictures().len(),
            reader.toc().len()
        ),
        (0, 0, 0)
    );
    Ok(())
}

#[test]
fn private_metadata_survives_a_round_trip() -> Result<()> {
    let mut book = sample::book()?;
    book.meta_mut().add("x-shelf", "favourites")?;
    book.meta_mut().add("x-shelf", "to read")?;
    let read = Book::read(written(&book)?.as_slice())?;
    assert_eq!(
        read.meta().get_all("x-shelf").collect::<Vec<_>>(),
        ["favourites", "to read"]
    );
    Ok(())
}

#[test]
fn text_outside_ascii_survives_a_round_trip() -> Result<()> {
    let mut book = Book::new("Мастер и Маргарита", "ru")?;
    book.add_chapter(
        "главы/первая.md",
        "# Глава 1. «Никогда»\n\nТекст — с тире…\n",
    )?;
    let mut reader = BookReader::new(Cursor::new(written(&book)?))?;
    let Some(chapter) = reader.find_chapter("главы/первая.md").map(|c| c.id) else {
        panic!("the chapter is found by its path");
    };
    assert_eq!(
        reader.read_chapter(chapter)?,
        "# Глава 1. «Никогда»\n\nТекст — с тире…\n"
    );
    assert_eq!(
        reader.anchors(chapter).first().map(|a| a.name.as_str()),
        Some("глава-1-никогда")
    );
    Ok(())
}

#[test]
fn the_compression_level_changes_the_bytes_but_not_the_book() -> Result<()> {
    let mut book = Book::new("Levels", "en")?;
    book.add_chapter(
        "a.md",
        format!(
            "# One\n\n{}",
            "the same words again and again. ".repeat(400)
        ),
    )?;
    let write = |level| -> Result<Vec<u8>> {
        let mut options = WriteOptions::default();
        options.level = Some(level);
        let mut bytes = Vec::new();
        book.write_with(&mut bytes, &options)?;
        Ok(bytes)
    };
    // Levels far outside the range of the compressor are brought into it.
    let (fast, strong) = (write(i32::MIN)?, write(i32::MAX)?);
    assert_eq!(Book::read(fast.as_slice())?, Book::read(strong.as_slice())?);
    assert!(strong.len() <= fast.len());
    Ok(())
}

#[test]
fn a_book_with_too_many_resources_is_not_written() -> Result<()> {
    let book = sample::book()?;
    let mut options = WriteOptions::default();
    options.limits.max_resources = 8;
    let mut bytes = Vec::new();
    assert!(matches!(
        book.write_with(&mut bytes, &options),
        Err(Error::Limit(_))
    ));
    assert_eq!(bytes.len(), 0);
    Ok(())
}

#[test]
fn a_chapter_above_the_resource_limit_is_not_written() -> Result<()> {
    let book = sample::book()?;
    let mut options = WriteOptions::default();
    options.limits = Limits::default();
    options.limits.max_resource_bytes = 100;
    let result = book.write_with(Vec::new(), &options);
    assert!(matches!(result, Err(Error::Limit(_))));
    Ok(())
}

#[test]
fn a_failing_sink_is_reported_as_an_io_error() -> Result<()> {
    struct Full;
    impl std::io::Write for Full {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("the disk is full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(matches!(sample::book()?.write(Full), Err(Error::Io(_))));
    Ok(())
}

#[test]
fn a_saved_book_opens_from_its_file() -> Result<()> {
    let folder = tempdir()?;
    let path = folder.path().join("example.mbook");
    let book = sample::book()?;
    book.save(&path)?;
    assert_eq!(Book::open(&path)?.chapters(), book.chapters());
    assert_eq!(BookReader::open(&path)?.meta().title(), "The Example");
    Ok(())
}

#[test]
fn saving_leaves_only_the_book_in_its_folder() -> Result<()> {
    let folder = tempdir()?;
    sample::book()?.save(folder.path().join("example.mbook"))?;
    assert_eq!(names(folder.path())?, ["example.mbook"]);
    Ok(())
}

#[test]
fn a_book_can_be_saved_over_the_file_it_was_loaded_from() -> Result<()> {
    let folder = tempdir()?;
    let path = folder.path().join("example.mbook");
    sample::book()?.save(&path)?;
    let mut book = Book::open(&path)?;
    book.meta_mut().set_title("The Example, 2nd edition")?;
    book.save(&path)?;
    assert_eq!(
        Book::open(&path)?.meta().title(),
        "The Example, 2nd edition"
    );
    Ok(())
}

#[test]
fn a_refused_save_leaves_the_previous_file_untouched() -> Result<()> {
    let folder = tempdir()?;
    let path = folder.path().join("example.mbook");
    let mut book = sample::book()?;
    book.save(&path)?;
    let before = std::fs::read(&path)?;
    book.remove_picture("img/fig-1.avif");
    assert!(matches!(book.save(&path), Err(Error::Invalid(_))));
    assert_eq!(std::fs::read(&path)?, before);
    assert_eq!(names(folder.path())?, ["example.mbook"]);
    Ok(())
}

#[test]
fn saving_into_a_missing_folder_is_an_io_error() -> Result<()> {
    let folder = tempdir()?;
    let result = sample::book()?.save(folder.path().join("no/such/folder/example.mbook"));
    assert!(matches!(result, Err(Error::Io(_))));
    Ok(())
}

#[test]
fn the_second_chapter_of_the_example_is_what_was_given() -> Result<()> {
    let book = Book::read(sample::bytes()?.as_slice())?;
    assert_eq!(
        book.chapter("ch02.md").map(mbook_core::Chapter::text),
        Some(CH02)
    );
    Ok(())
}

/// The annotated example as a file: the anchor table of the first chapter is
/// resource 6, and the chapters are 7, 8 and 9.
fn annotated() -> Result<Vec<u8>> {
    written(&sample::annotated()?)
}

fn open(bytes: Vec<u8>) -> Result<BookReader<Cursor<Vec<u8>>>> {
    BookReader::new(Cursor::new(bytes))
}

/// The ids of the chapters of a reader, in reading order.
fn chapter_ids<R: std::io::Read + std::io::Seek>(reader: &BookReader<R>) -> Vec<ChapterId> {
    reader.chapters().map(|chapter| chapter.id).collect()
}

#[test]
fn an_anchor_table_is_written_in_front_of_its_chapter() -> Result<()> {
    let reader = open(annotated()?)?;
    let kinds: Vec<_> = reader.resources().skip(5).map(|r| r.kind).collect();
    assert_eq!(
        kinds,
        [
            ResourceKind::Picture,
            ResourceKind::ChapterAnchors,
            ResourceKind::Text,
            ResourceKind::Text,
            ResourceKind::Text,
            ResourceKind::Picture,
        ]
    );
    Ok(())
}

#[test]
fn contents_and_heading_anchors_count_the_anchor_table() -> Result<()> {
    let reader = open(annotated()?)?;
    let numbers: Vec<_> = chapter_ids(&reader).iter().map(|id| id.number()).collect();
    let listed: Vec<_> = reader
        .toc()
        .iter()
        .map(|entry| entry.position.chapter.number())
        .collect();
    assert_eq!((numbers, listed), (vec![7, 8, 9], vec![7, 7, 8, 9]));
    let first = chapter_ids(&reader)[0];
    assert_eq!(reader.anchors(first).len(), 2);
    Ok(())
}

#[test]
fn extra_anchors_are_read_sorted_by_offset() -> Result<()> {
    let mut reader = open(annotated()?)?;
    let first = chapter_ids(&reader)[0];
    let anchors: Vec<_> = reader
        .read_extra_anchors(first)?
        .into_iter()
        .map(|anchor| (anchor.name, anchor.offset))
        .collect();
    assert_eq!(
        anchors,
        [
            ("fig-1".to_owned(), named_offset("fig-1")),
            ("remark".to_owned(), named_offset("remark"))
        ]
    );
    Ok(())
}

#[test]
fn a_chapter_without_an_anchor_table_has_no_extra_anchors() -> Result<()> {
    let mut reader = open(annotated()?)?;
    let second = chapter_ids(&reader)[1];
    assert_eq!(reader.read_extra_anchors(second)?.len(), 0);
    Ok(())
}

#[test]
fn extra_anchors_of_an_id_from_another_book_are_refused() -> Result<()> {
    let other = open(annotated()?)?;
    let foreign = chapter_ids(&other)[2];
    let mut reader = open(sample::bytes()?)?;
    assert!(matches!(
        reader.read_extra_anchors(foreign),
        Err(Error::NoSuchResource(9))
    ));
    Ok(())
}

#[test]
fn a_link_to_an_extra_anchor_is_known_to_need_the_anchor_table() -> Result<()> {
    let reader = open(annotated()?)?;
    let ids = chapter_ids(&reader);
    assert_eq!(
        reader.resolve(ids[2], "ch01.md#fig-1"),
        Ok(Destination::ExtraAnchor(ids[0]))
    );
    Ok(())
}

#[test]
fn read_resolve_follows_a_link_to_an_extra_anchor() -> Result<()> {
    let mut reader = open(annotated()?)?;
    let ids = chapter_ids(&reader);
    let position = Position {
        chapter: ids[0],
        offset: named_offset("remark"),
    };
    assert_eq!(
        reader.read_resolve(ids[2], "ch01.md#remark")?,
        Destination::Chapter(position)
    );
    Ok(())
}

#[test]
fn read_resolve_answers_like_resolve_when_no_table_is_needed() -> Result<()> {
    let mut reader = open(annotated()?)?;
    let from = chapter_ids(&reader)[2];
    for destination in [
        "ch01.md#the-station",
        "ch02.md",
        "img/fig-1.avif",
        "https://example.org",
    ] {
        let known = reader.resolve(from, destination);
        assert_eq!(reader.read_resolve(from, destination).ok(), known.ok());
    }
    Ok(())
}

#[test]
fn read_resolve_says_why_a_destination_leads_nowhere() -> Result<()> {
    let mut reader = open(annotated()?)?;
    let from = chapter_ids(&reader)[2];
    let reasons = ["ch01.md#nope", "ch02.md#nope", "ch09.md"].map(|destination| {
        match reader.read_resolve(from, destination) {
            Err(Error::Unresolved(reason)) => Some(reason),
            _ => None,
        }
    });
    assert_eq!(
        reasons,
        [
            Some(ResolveError::UnknownAnchor),
            Some(ResolveError::UnknownAnchor),
            Some(ResolveError::NotFound)
        ]
    );
    Ok(())
}

#[test]
fn extra_anchors_survive_being_written_and_read() -> Result<()> {
    let book = sample::annotated()?;
    let read = Book::read(written(&book)?.as_slice())?;
    assert_eq!(read.chapters(), book.chapters());
    Ok(())
}

#[test]
fn a_lazily_opened_book_loads_its_extra_anchors() -> Result<()> {
    let book = sample::annotated()?;
    let loaded = open(written(&book)?)?.load()?;
    assert_eq!(loaded.chapters(), book.chapters());
    Ok(())
}

#[test]
fn a_loaded_book_with_extra_anchors_writes_back_to_the_same_bytes() -> Result<()> {
    let first = annotated()?;
    let second = written(&Book::read(first.as_slice())?)?;
    assert!(first == second);
    Ok(())
}

#[test]
fn a_book_whose_anchor_definitions_are_removed_gets_no_anchor_table() -> Result<()> {
    let mut book = sample::book()?;
    let Some(chapter) = book.chapter_mut("ch01.md") else {
        panic!("the example has a first chapter");
    };
    chapter.set_text(CH01_NAMED)?;
    chapter.set_text(CH01)?;
    assert!(written(&book)? == FIXTURE);
    Ok(())
}

#[test]
fn the_text_of_a_chapter_keeps_its_anchor_definitions() -> Result<()> {
    let mut reader = open(annotated()?)?;
    let first = chapter_ids(&reader)[0];
    assert_eq!(reader.read_chapter(first)?, CH01_NAMED);
    Ok(())
}

#[test]
fn a_loaded_chapter_keeps_its_extra_anchors_after_an_edit() -> Result<()> {
    let mut book = Book::read(annotated()?.as_slice())?;
    let Some(chapter) = book.chapter_mut("ch01.md") else {
        panic!("the example has a first chapter");
    };
    chapter.set_text(chapter.text().replace("late", "very late"))?;
    let shift = u32::try_from("very ".len()).unwrap_or(0);
    let anchors: Vec<_> = chapter
        .extra_anchors()
        .iter()
        .map(|anchor| (anchor.name.as_str(), anchor.offset))
        .collect();
    assert_eq!(
        anchors,
        [
            ("fig-1", named_offset("fig-1") + shift),
            ("remark", named_offset("remark") + shift)
        ]
    );
    assert!(book.check().is_ok());
    Ok(())
}

#[test]
fn an_anchor_table_above_the_resource_limit_is_refused() -> Result<()> {
    // A table larger than its chapter comes only from another writer.
    let name = "n".repeat(600);
    let raw = Raw::example().with(|raw| raw.insert_anchor_table(6, &[0], &[&name]));
    let book = Book::read(raw.build().map_err(Error::Container)?.as_slice())?;
    let mut options = WriteOptions::default();
    options.limits.max_resource_bytes = 500;
    assert!(matches!(
        book.write_with(Vec::new(), &options),
        Err(Error::Limit(_))
    ));
    Ok(())
}
