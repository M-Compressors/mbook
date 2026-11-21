//! One Markdown document, through the public API.
#![cfg(feature = "md")]

use mbook_converter::mbook_core::{Toc, TocItem};
use mbook_converter::{ConvertOptions, Converted, Converter, Error, Format, WarningKind};

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn converted(text: &str) -> Result<Converted, Error> {
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Markdown);
    Converter::with_options(options).load_slice(text.as_bytes())
}

/// The texts of the chapters a document becomes.
fn chapters(text: &str) -> Result<Vec<String>, Error> {
    let converted = converted(text)?;
    let chapters = converted.book.chapters();
    Ok(chapters.iter().map(|c| c.text().to_owned()).collect())
}

fn warnings(converted: &Converted) -> Vec<&WarningKind> {
    converted.report.warnings.iter().map(|w| &w.kind).collect()
}

const BOOK: &str = "\
# The Example

A book in one file.

# Arrival

The train was *late*[^1]. See [the station](#the-station).

[^1]: Almost an hour.

## The station

| Platform | Train |
| --- | --- |
| 1 | 9:15 |

# The letter

Back to [the start](#arrival), or out to <https://example.org>.
";

#[test]
fn a_document_becomes_one_chapter_per_top_level_heading() -> Outcome {
    let chapters = chapters(BOOK)?;
    assert_eq!(chapters.len(), 3);
    assert_eq!(chapters[0], "# The Example\n\nA book in one file.\n\n");
    assert!(chapters[2].starts_with("# The letter\n"));
    Ok(())
}

#[test]
fn markdown_that_needs_no_change_is_kept_as_written() -> Outcome {
    let chapters = chapters(BOOK)?;
    assert!(chapters[1].contains("The train was *late*[^1]."));
    assert!(chapters[1].contains("[^1]: Almost an hour.\n"));
    assert!(chapters[1].contains("| Platform | Train |\n| --- | --- |\n| 1 | 9:15 |\n"));
    Ok(())
}

#[test]
fn links_to_headings_lead_to_the_chapter_that_holds_them() -> Outcome {
    let converted = converted(BOOK)?;
    let chapters = converted.book.chapters();
    assert!(
        chapters[1]
            .text()
            .contains("See [the station](ch0002.md#the-station).")
    );
    assert!(
        chapters[2]
            .text()
            .contains("Back to [the start](ch0002.md#arrival), or out to <https://example.org>.")
    );
    // The links lead to the anchors the headings have anyway: nothing is
    // added to the text.
    assert!(chapters[1].text().starts_with("# Arrival\n"));
    assert_eq!(chapters[1].extra_anchors(), []);
    assert_eq!(
        warnings(&converted),
        [&WarningKind::MetadataMissing("lang".into())]
    );
    Ok(())
}

#[test]
fn the_book_is_named_by_its_first_heading() -> Outcome {
    let converted = converted(BOOK)?;
    assert_eq!(converted.book.meta().title(), "The Example");
    assert_eq!(converted.book.meta().language(), "und");
    assert_eq!(converted.report.format, Format::Markdown);
    Ok(())
}

#[test]
fn the_contents_come_from_the_headings() -> Outcome {
    let converted = converted(BOOK)?;
    assert_eq!(converted.book.toc(), &Toc::FromHeadings { max_level: 6 });
    let mut file = Vec::new();
    converted.book.write(&mut file)?;
    let read = mbook_converter::mbook_core::Book::read(file.as_slice())?;
    let Toc::Explicit(items) = read.toc() else {
        return Err("a stored book lists its contents".into());
    };
    let titles: Vec<_> = items
        .iter()
        .map(|item: &TocItem| item.title.as_str())
        .collect();
    assert_eq!(
        titles,
        ["The Example", "Arrival", "The station", "The letter"]
    );
    Ok(())
}

#[test]
fn raw_html_is_removed() -> Outcome {
    let converted = converted("# One\n\nH<sub>2</sub>O<br>and more.\n\n<div>gone</div>\n")?;
    assert_eq!(
        converted.book.chapters()[0].text(),
        "# One\n\nH2O\\\nand more.\n\n"
    );
    assert!(warnings(&converted).contains(&&WarningKind::MarkupDropped("sub".into())));
    assert!(warnings(&converted).contains(&&WarningKind::MarkupDropped("div".into())));
    Ok(())
}

#[test]
fn pictures_and_links_to_other_files_cannot_be_followed() -> Outcome {
    let converted = converted("# One\n\n![A map](map.png) and [the next part](part2.md).\n")?;
    assert_eq!(
        converted.book.chapters()[0].text(),
        "# One\n\nA map and the next part.\n"
    );
    let kinds = warnings(&converted);
    assert!(kinds.contains(&&WarningKind::PictureDropped("map.png".into())));
    assert!(kinds.contains(&&WarningKind::LinkDropped("part2.md".into())));
    Ok(())
}

#[test]
fn front_matter_is_not_shown_as_text() -> Outcome {
    let converted = converted("---\ntitle: Hidden\nauthor: A. Writer\n---\n\n# Shown\n\nText.\n")?;
    assert_eq!(converted.book.chapters().len(), 1);
    assert_eq!(converted.book.chapters()[0].text(), "# Shown\n\nText.\n");
    assert_eq!(converted.book.meta().title(), "Shown");
    assert!(warnings(&converted).contains(&&WarningKind::MarkupDropped("front matter".into())));
    Ok(())
}

#[test]
fn a_document_in_another_encoding_is_read() -> Outcome {
    // "# Глава" in UTF-16 with a byte order mark.
    let mut bytes = vec![0xff, 0xfe];
    for unit in "# Глава\n\nТекст главы.\n".encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Markdown);
    let converted = Converter::with_options(options).load_slice(&bytes)?;
    assert_eq!(
        converted.book.chapters()[0].text(),
        "# Глава\n\nТекст главы.\n"
    );
    Ok(())
}

#[test]
fn markdown_is_not_detected() {
    let result = Converter::new().load_slice(BOOK.as_bytes());
    assert!(matches!(result, Err(Error::UnknownFormat)));
}

#[test]
fn an_empty_document_holds_nothing_to_convert() {
    assert!(matches!(converted(" \n"), Err(Error::Empty)));
}
