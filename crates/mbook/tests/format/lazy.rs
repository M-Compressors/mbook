//! A reader takes from its source only what it is asked for.
//!
//! RFC 0001 promises that a book opens without being read in full and that
//! one resource is decoded at a time. These tests count the bytes a reader
//! takes from a book far larger than its tables.

use mbook::{Book, BookReader, Result};
use std::cell::Cell;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::rc::Rc;

/// The number of chapters of the book the tests read.
const CHAPTERS: u64 = 32;
/// The length of one chapter, in bytes of text.
const CHAPTER_LEN: usize = 64 * 1024;

/// A source that counts the bytes read from it.
struct Counting<R> {
    inner: R,
    read: Rc<Cell<u64>>,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buffer)?;
        let count_wide = u64::try_from(count).unwrap_or(u64::MAX);
        self.read.set(self.read.get().saturating_add(count_wide));
        Ok(count)
    }
}

impl<R: Seek> Seek for Counting<R> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        self.inner.seek(to)
    }
}

/// Text of a chapter that does not compress to nothing: words of letters
/// from a small generator, so that the file is about as large as its text.
fn chapter_text(number: u64) -> String {
    let mut state = number.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut text = format!("# Chapter {number}\n\n");
    while text.len() < CHAPTER_LEN {
        for _ in 0..12 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let mut word = state >> 24;
            for _ in 0..7 {
                let letter = u8::try_from(word % 26).unwrap_or(0);
                text.push(char::from(b'a' + letter));
                word /= 26;
            }
            text.push(' ');
        }
        text.push_str("\n\n");
    }
    text
}

/// A book of [`CHAPTERS`] chapters of [`CHAPTER_LEN`] bytes each.
fn large_book() -> Result<Vec<u8>> {
    let mut book = Book::new("A long book", "en")?;
    for number in 0..CHAPTERS {
        book.add_chapter(format!("ch{number:02}.md"), chapter_text(number))?;
    }
    let mut file = Vec::new();
    book.write(&mut file)?;
    Ok(file)
}

/// The large book as a source that counts.
struct Counted {
    /// The book.
    source: Counting<Cursor<Vec<u8>>>,
    /// The length of the file, in bytes.
    len: u64,
    /// The bytes read from the source so far.
    read: Rc<Cell<u64>>,
}

/// The large book, with a counter of the bytes read from it.
fn counted() -> Result<Counted> {
    let file = large_book()?;
    let len = u64::try_from(file.len()).unwrap_or(u64::MAX);
    let read = Rc::new(Cell::new(0));
    let source = Counting {
        inner: Cursor::new(file),
        read: Rc::clone(&read),
    };
    Ok(Counted { source, len, read })
}

#[test]
fn the_book_of_these_tests_is_far_larger_than_one_chapter() -> Result<()> {
    let Counted { len, .. } = counted()?;
    // Otherwise the bounds below would prove nothing.
    assert!(len > 1024 * 1024, "the file has {len} bytes");
    Ok(())
}

#[test]
fn opening_a_book_reads_its_tables_and_none_of_its_chapters() -> Result<()> {
    let Counted { source, len, read } = counted()?;

    let reader = BookReader::new(source)?;

    assert_eq!(reader.chapters().len(), 32);
    assert!(
        read.get() < len / 20,
        "{} of {len} bytes were read to open the book",
        read.get()
    );
    Ok(())
}

#[test]
fn looking_into_the_tables_reads_nothing() -> Result<()> {
    let Counted { source, read, .. } = counted()?;
    let reader = BookReader::new(source)?;
    let opened = read.get();

    let first = reader.chapters().next().map(|chapter| chapter.id);
    let titles = reader.toc().len();
    let found = reader.find_chapter("ch31.md").is_some();
    let resolved = first.map(|chapter| reader.resolve(chapter, "ch07.md#chapter-7"));

    assert_eq!(titles, 32);
    assert!(found);
    assert!(matches!(resolved, Some(Ok(_))));
    assert_eq!(read.get(), opened);
    Ok(())
}

#[test]
fn reading_a_chapter_reads_that_chapter_and_no_other() -> Result<()> {
    let Counted { source, len, read } = counted()?;
    let mut reader = BookReader::new(source)?;
    let Some(chapter) = reader.chapters().nth(17).map(|chapter| chapter.id) else {
        panic!("the book has thirty-two chapters");
    };
    let opened = read.get();

    let text = reader.read_chapter(chapter)?;

    assert_eq!(text, chapter_text(17));
    let taken = read.get() - opened;
    // One chapter is a thirty-second of the file; twice that leaves room for
    // the frame around it.
    assert!(
        taken < 2 * len / CHAPTERS,
        "{taken} of {len} bytes were read for one chapter"
    );
    Ok(())
}

#[test]
fn reading_every_chapter_once_reads_the_file_about_once() -> Result<()> {
    let Counted { source, len, read } = counted()?;
    let mut reader = BookReader::new(source)?;
    let chapters: Vec<_> = reader.chapters().map(|chapter| chapter.id).collect();

    for chapter in chapters {
        reader.read_chapter(chapter)?;
    }

    assert!(
        read.get() < len + len / 10,
        "{} bytes were read from a file of {len}",
        read.get()
    );
    Ok(())
}
