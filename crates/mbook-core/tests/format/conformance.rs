//! Files that follow or break the rules of RFC 0001, built by hand so that
//! they do not depend on the writer of this crate.

use crate::raw::{ANCHORS, Column, META, RESOURCES, Raw, TOC};
use crate::texts::{CH01, CH02, station_offset};
use mbook_core::{Book, BookReader, Error, ResolveError, ResourceKind, Result, Size, Toc, Version};
use std::io::Cursor;

type Reader = BookReader<Cursor<Vec<u8>>>;

fn bytes(raw: &Raw) -> Result<Vec<u8>> {
    raw.build().map_err(Error::Container)
}

fn open(raw: &Raw) -> Result<Reader> {
    BookReader::new(Cursor::new(bytes(raw)?))
}

/// The example with one change.
fn changed(change: impl FnOnce(&mut Raw)) -> Raw {
    Raw::example().with(change)
}

/// The number of the table a changed example is rejected for.
fn rejected_table(change: impl FnOnce(&mut Raw)) -> Result<u32> {
    match open(&changed(change)) {
        Err(Error::Malformed { resource, .. }) => Ok(resource),
        Ok(_) => panic!("the file must be rejected"),
        Err(other) => Err(other),
    }
}

#[test]
fn a_file_built_by_hand_reads_like_one_written_by_this_crate() -> Result<()> {
    let mut reader = open(&Raw::example())?;
    let ids: Vec<_> = reader.chapters().map(|chapter| chapter.id).collect();
    let mut texts = Vec::new();
    for id in ids {
        texts.push(reader.read_chapter(id)?);
    }
    assert_eq!(texts, [CH01, CH02]);
    assert_eq!(reader.toc().len(), 3);
    Ok(())
}

#[test]
fn a_newer_minor_version_is_accepted() -> Result<()> {
    let reader = open(&changed(|raw| raw.header[7] = 9))?;
    assert_eq!(reader.version(), Version { major: 1, minor: 9 });
    Ok(())
}

#[test]
fn a_newer_major_version_is_refused_by_name() -> Result<()> {
    let raw = changed(|raw| raw.header[6] = 2);
    let refused = Version { major: 2, minor: 0 };
    assert!(matches!(open(&raw), Err(Error::UnsupportedVersion(version)) if version == refused));
    assert!(matches!(
        Book::read(bytes(&raw)?.as_slice()),
        Err(Error::UnsupportedVersion(version)) if version == refused
    ));
    Ok(())
}

#[test]
fn major_version_zero_is_malformed() -> Result<()> {
    assert_eq!(rejected_table(|raw| raw.header[6] = 0)?, 0);
    Ok(())
}

#[test]
fn a_container_without_the_format_header_is_not_an_mbook() {
    let wrong_prefix = changed(|raw| raw.header[0] = b'X');
    let short = changed(|raw| raw.header.truncate(7));
    let long = changed(|raw| raw.header.push(0));
    for raw in [wrong_prefix, short, long] {
        assert!(matches!(open(&raw), Err(Error::NotMbook)));
    }
}

#[test]
fn a_file_with_fewer_than_five_resources_is_malformed() -> Result<()> {
    let table = rejected_table(|raw| {
        raw.tables.truncate(2);
        raw.contents.clear();
    })?;
    assert_eq!(table, 0);
    Ok(())
}

#[test]
fn a_table_with_a_missing_column_is_rejected() -> Result<()> {
    let table = rejected_table(|raw| {
        raw.tables[TOC].pop();
    })?;
    assert_eq!(table, 3);
    Ok(())
}

#[test]
fn a_column_of_another_type_is_rejected() -> Result<()> {
    let table = rejected_table(|raw| {
        raw.tables[ANCHORS][2] = Column::Words(vec![1, 2, 3]);
    })?;
    assert_eq!(table, 4);
    Ok(())
}

#[test]
fn a_numeric_column_of_another_width_is_rejected() -> Result<()> {
    let table = rejected_table(|raw| {
        raw.tables[RESOURCES][0] = Column::Words(vec![0, 0, 0, 0, 0, 2, 1, 1, 2]);
    })?;
    assert_eq!(table, 2);
    Ok(())
}

#[test]
fn columns_of_different_lengths_are_rejected() -> Result<()> {
    let table = rejected_table(|raw| {
        raw.tables[META][1].text_mut().pop();
    })?;
    assert_eq!(table, 1);
    Ok(())
}

#[test]
fn a_column_added_by_a_later_minor_version_is_ignored() -> Result<()> {
    let reader = open(&changed(|raw| {
        raw.tables[TOC].push(Column::text(&["a", "b", "c"]));
        raw.tables[META].push(Column::Bytes(vec![0; 5]));
    }))?;
    assert_eq!(
        (reader.toc().len(), reader.meta().title()),
        (3, "The Example")
    );
    Ok(())
}

#[test]
fn a_meta_key_this_version_does_not_define_is_kept() -> Result<()> {
    let reader = open(&changed(|raw| {
        raw.tables[META][0].text_mut().push("narrator".to_owned());
        raw.tables[META][1].text_mut().push("N. Reader".to_owned());
    }))?;
    assert_eq!(reader.meta().get("narrator"), Some("N. Reader"));
    Ok(())
}

#[test]
fn a_resource_of_an_unknown_kind_is_listed_and_skipped() -> Result<()> {
    let raw = changed(|raw| raw.push_resource(9, "data/index.bin", b"\x01\x02\x03"));
    let mut reader = open(&raw)?;
    let last = reader.resources().last().map(|r| (r.kind, r.path));
    assert_eq!(last, Some((ResourceKind::Unknown(9), "data/index.bin")));
    assert_eq!((reader.chapters().len(), reader.pictures().len()), (2, 2));
    let Some(from) = reader.chapter_id(6) else {
        panic!("resource 6 is the first chapter");
    };
    assert_eq!(
        reader.resolve(from, "data/index.bin"),
        Err(ResolveError::NotFound)
    );
    // A loaded book leaves the resource behind.
    let book = reader.load()?;
    assert_eq!((book.chapters().len(), book.pictures().len()), (2, 2));
    Ok(())
}

#[test]
fn a_resource_table_shorter_than_the_container_is_rejected() -> Result<()> {
    let table = rejected_table(|raw| raw.contents.push(b"extra".to_vec()))?;
    assert_eq!(table, 2);
    Ok(())
}

#[test]
fn a_path_that_leaves_the_book_is_rejected() -> Result<()> {
    let table = rejected_table(|raw| {
        raw.tables[RESOURCES][1].text_mut()[6] = "../ch01.md".to_owned();
    })?;
    assert_eq!(table, 2);
    Ok(())
}

#[test]
fn a_contents_row_inside_a_chapter_without_an_anchor_is_rejected() -> Result<()> {
    let table = rejected_table(|raw| {
        let anchors = &mut raw.tables[ANCHORS];
        anchors[0].words_mut().remove(1);
        anchors[1].words_mut().remove(1);
        anchors[2].text_mut().remove(1);
    })?;
    assert_eq!(table, 3);
    Ok(())
}

#[test]
fn a_picture_without_a_size_is_still_listed() -> Result<()> {
    // The size is a rule of picture resources, not of the table, so the file
    // opens and the reader reports what the table says.
    let reader = open(&changed(|raw| {
        raw.tables[RESOURCES][2].words_mut()[5] = 0;
        raw.tables[RESOURCES][3].words_mut()[5] = 0;
    }))?;
    let size = reader.cover().map(|cover| cover.size);
    assert_eq!(
        size,
        Some(Size {
            width: 0,
            height: 0
        })
    );
    Ok(())
}

#[test]
fn a_position_inside_a_line_fails_when_its_chapter_is_read() -> Result<()> {
    let inside = station_offset() + 1;
    let raw = changed(|raw| {
        raw.tables[TOC][3].words_mut()[1] = inside;
        raw.tables[ANCHORS][1].words_mut()[1] = inside;
    });
    // The tables cannot tell: the file opens, and the other chapter reads.
    let mut reader = open(&raw)?;
    let (Some(first), Some(second)) = (reader.chapter_id(6), reader.chapter_id(7)) else {
        panic!("the example has two chapters");
    };
    assert!(matches!(
        reader.read_chapter(first),
        Err(Error::Malformed { resource: 6, .. })
    ));
    assert_eq!(reader.read_chapter(second)?, CH02);
    assert!(matches!(
        Book::read(bytes(&raw)?.as_slice()),
        Err(Error::Malformed { resource: 6, .. })
    ));
    Ok(())
}

#[test]
fn a_chapter_that_is_not_utf8_fails_alone() -> Result<()> {
    let raw = changed(|raw| raw.contents[1] = vec![0xff; CH01.len()]);
    let mut reader = open(&raw)?;
    let (Some(first), Some(second)) = (reader.chapter_id(6), reader.chapter_id(7)) else {
        panic!("the example has two chapters");
    };
    assert!(matches!(
        reader.read_chapter(first),
        Err(Error::Malformed {
            resource: 6,
            reason: "the text is not UTF-8"
        })
    ));
    assert_eq!(reader.read_chapter(second)?, CH02);
    Ok(())
}

#[test]
fn a_book_without_contents_or_anchors_is_valid() -> Result<()> {
    let raw = changed(|raw| {
        for table in [TOC, ANCHORS] {
            for column in &mut raw.tables[table] {
                *column = match column {
                    Column::Bytes(_) => Column::Bytes(Vec::new()),
                    Column::Words(_) => Column::Words(Vec::new()),
                    Column::Text(_) => Column::Text(Vec::new()),
                };
            }
        }
    });
    let book = Book::read(bytes(&raw)?.as_slice())?;
    assert_eq!(book.toc(), &Toc::Explicit(Vec::new()));
    assert!(
        book.chapters()
            .iter()
            .all(|chapter| chapter.anchors().is_empty())
    );
    Ok(())
}

#[test]
fn a_compressed_picture_is_read_like_a_stored_one() -> Result<()> {
    // The hand-built file compresses every resource, pictures included.
    let mut reader = open(&Raw::example())?;
    assert_eq!(reader.read_cover()?.as_deref(), Some(crate::texts::PLAIN));
    Ok(())
}
