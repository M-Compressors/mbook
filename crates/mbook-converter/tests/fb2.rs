//! `FictionBook` 2, through the public API.
#![cfg(feature = "fb2")]

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use mbook_converter::mbook::{Book, Chapter};
use mbook_converter::{ConvertOptions, Converted, Converter, Error, Format, WarningKind};
use std::fmt::Write as _;
use std::io::{Cursor, Write};
use zip::ZipWriter;
use zip::unstable::write::FileOptionsExt as _;
use zip::write::SimpleFileOptions;

type Outcome = Result<(), Box<dyn std::error::Error>>;

/// The head every document of these tests has.
const HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
"#;

/// A plain description.
const DESCRIPTION: &str = r"<description><title-info><genre>sf</genre>
<author><first-name>Ann</first-name><last-name>Author</last-name></author>
<book-title>The Test</book-title><lang>en</lang></title-info></description>
";

fn fb2_converter() -> Converter {
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Fb2);
    Converter::with_options(options)
}

/// A document with this text after the description.
fn document(rest: &str) -> String {
    format!("{HEAD}{DESCRIPTION}{rest}\n</FictionBook>\n")
}

/// A document with this description and text.
fn described(description: &str, rest: &str) -> String {
    format!("{HEAD}{description}\n{rest}\n</FictionBook>\n")
}

/// A document with a main body of these sections.
fn with_sections(sections: &str) -> String {
    document(&format!("<body>{sections}</body>"))
}

fn convert(xml: &str) -> Result<Converted, Error> {
    fb2_converter().load_slice(xml.as_bytes())
}

/// A picture of a size, as a PNG file.
fn png(width: u32, height: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    image::RgbImage::new(width, height)
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)?;
    Ok(bytes)
}

/// A `binary` element holding a PNG.
fn binary(id: &str, width: u32, height: u32) -> Result<String, Box<dyn std::error::Error>> {
    let data = STANDARD.encode(png(width, height)?);
    Ok(format!(
        "<binary id=\"{id}\" content-type=\"image/png\">{data}</binary>"
    ))
}

fn texts(converted: &Converted) -> Vec<&str> {
    converted
        .book
        .chapters()
        .iter()
        .map(Chapter::text)
        .collect()
}

fn warnings(converted: &Converted) -> Vec<(&WarningKind, Option<&str>)> {
    converted
        .report
        .warnings
        .iter()
        .map(|w| (&w.kind, w.location.as_deref()))
        .collect()
}

/// The text of the only chapter.
fn only_text(xml: &str) -> Result<String, Box<dyn std::error::Error>> {
    let converted = convert(xml)?;
    let [chapter] = converted.book.chapters() else {
        return Err(format!("{} chapters", converted.book.chapters().len()).into());
    };
    Ok(chapter.text().to_owned())
}

#[test]
fn a_full_book_has_metadata_and_one_chapter_per_section() -> Outcome {
    let xml = described(
        r#"<description><title-info><genre>sf_fantasy</genre><genre>adventure</genre>
<author><first-name>Ann</first-name><middle-name>Q.</middle-name><last-name>Author</last-name></author>
<author><nickname>pen</nickname></author>
<book-title>The Test</book-title>
<annotation><p>First para.</p><empty-line/><p>Second <emphasis>para</emphasis>.</p></annotation>
<date value="1999-05-01">May 1999</date><lang>en</lang>
<translator><first-name>Tom</first-name><last-name>Trans</last-name></translator>
<sequence name="Saga" number="2"/></title-info>
<document-info><author><nickname>doc</nickname></author><date>today</date><id>doc-123</id><version>1.0</version></document-info>
<publish-info><publisher>Pub House</publisher><year>2001</year><isbn>978-3-16-148410-0</isbn></publish-info>
</description>"#,
        r"<body>
<section><title><p>One</p></title><p>First.</p></section>
<section><title><p>Two</p></title><p>Second.</p></section>
</body>",
    );
    let converted = convert(&xml)?;
    let meta = converted.book.meta();
    assert_eq!(meta.title(), "The Test");
    assert_eq!(meta.language(), "en");
    assert_eq!(meta.authors().collect::<Vec<_>>(), ["Ann Q. Author", "pen"]);
    assert_eq!(meta.translators().collect::<Vec<_>>(), ["Tom Trans"]);
    let series = meta.series().map(|s| (s.name, s.index));
    assert_eq!(series, Some(("Saga", Some("2"))));
    assert_eq!(meta.publisher(), Some("Pub House"));
    assert_eq!(meta.date(), Some("2001"));
    assert_eq!(meta.isbns().collect::<Vec<_>>(), ["9783161484100"]);
    assert_eq!(
        meta.genres().collect::<Vec<_>>(),
        ["sf_fantasy", "adventure"]
    );
    assert_eq!(meta.annotation(), Some("First para.\n\nSecond para."));
    assert_eq!(meta.id(), "doc-123");
    assert_eq!(texts(&converted).len(), 2);
    assert!(texts(&converted)[0].starts_with("# One\n\nFirst."));
    assert!(texts(&converted)[1].starts_with("# Two\n\nSecond."));
    assert_eq!(warnings(&converted), []);
    Ok(())
}

#[test]
fn the_date_of_the_book_stands_in_for_a_missing_year() -> Outcome {
    let xml = described(
        r#"<description><title-info><genre>sf</genre><author><nickname>n</nickname></author>
<book-title>T</book-title><date value="1999-05-01">May 1999</date><lang>en</lang></title-info></description>"#,
        "<body><section><p>x</p></section></body>",
    );
    assert_eq!(convert(&xml)?.book.meta().date(), Some("1999-05-01"));
    Ok(())
}

#[test]
fn nested_sections_get_headings_of_their_depth() -> Outcome {
    let text = only_text(&with_sections(
        r"<section><title><p>Part</p><p>One</p></title>
<section><title><p>Chapter</p></title><subtitle>Aside</subtitle><p>In chapter.</p>
<section><title><p>Scene</p></title><p>In scene.</p></section></section></section>",
    ))?;
    assert_eq!(
        text,
        "# Part One\n\n## Chapter\n\n### Aside\n\nIn chapter.\n\n### Scene\n\nIn scene."
    );
    Ok(())
}

#[test]
fn headings_stop_at_level_six() -> Outcome {
    let mut xml = String::new();
    for depth in 1..=8 {
        write!(xml, "<section><title><p>D{depth}</p></title>")?;
    }
    xml.push_str("<p>deep</p>");
    xml.push_str(&"</section>".repeat(8));
    let text = only_text(&with_sections(&xml))?;
    assert!(
        text.contains("###### D6\n\n###### D7\n\n###### D8"),
        "{text}"
    );
    Ok(())
}

#[test]
fn the_front_of_the_main_body_is_a_section_of_its_own() -> Outcome {
    let converted = convert(&document(
        r"<body><title><p>The Book</p></title><epigraph><p>Motto.</p></epigraph>
<section><title><p>One</p></title><p>Text.</p></section></body>",
    ))?;
    let texts = texts(&converted);
    assert_eq!(texts.len(), 2);
    assert_eq!(texts[0], "# The Book\n\n> Motto.");
    assert!(texts[1].starts_with("# One"));
    Ok(())
}

#[test]
fn inline_styles_become_markdown() -> Outcome {
    let text = only_text(&with_sections(
        "<section><p>A <strong>bold</strong>, <emphasis>italic</emphasis>, \
<strikethrough>struck</strikethrough> and <code>a`b</code> word.</p></section>",
    ))?;
    assert_eq!(text, "A **bold**, *italic*, ~~struck~~ and ``a`b`` word.");
    Ok(())
}

#[test]
fn markdown_characters_in_text_are_escaped() -> Outcome {
    let text = only_text(&with_sections(
        "<section><p>1. # not * a_list [x] &lt;b&gt; a|b ~c~</p><p>- dash</p></section>",
    ))?;
    assert_eq!(
        text,
        "1\\. # not \\* a\\_list \\[x\\] \\<b\\> a\\|b \\~c\\~\n\n\\- dash"
    );
    Ok(())
}

#[test]
fn subscripts_superscripts_and_styles_lose_their_markup_once_per_section() -> Outcome {
    let converted = convert(&with_sections(
        r#"<section><p>H<sub>2</sub>O, x<sub>1</sub>, E<sup>2</sup>, <style name="x">named</style> <style name="y">again</style></p></section>
<section><p>y<sub>3</sub></p></section>"#,
    ))?;
    assert!(texts(&converted)[0].contains("named"));
    let kinds: Vec<_> = warnings(&converted)
        .into_iter()
        .map(|(kind, location)| (kind.clone(), location.map(str::to_owned)))
        .collect();
    let drop = |what: &str, section: &str| {
        (
            WarningKind::MarkupDropped(what.to_owned()),
            Some(section.to_owned()),
        )
    };
    assert_eq!(
        kinds,
        [
            drop("sub", "s1"),
            drop("sup", "s1"),
            drop("style", "s1"),
            drop("sub", "s2")
        ]
    );
    Ok(())
}

#[test]
fn a_poem_is_a_paragraph_per_stanza_with_hard_line_breaks() -> Outcome {
    let text = only_text(&with_sections(
        r"<section><poem><title><p>Ode</p></title>
<stanza><v>Line one</v><v>Line two</v></stanza><stanza><v>Third</v></stanza>
<text-author>The Poet</text-author></poem></section>",
    ))?;
    assert_eq!(text, "**Ode**\n\nLine one\\\nLine two\n\nThird\n\nThe Poet");
    Ok(())
}

#[test]
fn epigraphs_and_citations_are_block_quotes_ending_with_their_author() -> Outcome {
    let text = only_text(&with_sections(
        r"<section><epigraph><p>Motto.</p><text-author>Someone</text-author></epigraph>
<cite><p>Cited.</p><empty-line/><p>More.</p><text-author>Other</text-author></cite></section>",
    ))?;
    assert_eq!(
        text,
        "> Motto.\n>\n> Someone\n\n> Cited.\n>\n> More.\n>\n> Other"
    );
    Ok(())
}

#[test]
fn a_table_is_a_table_with_a_header_row() -> Outcome {
    let converted = convert(&with_sections(
        r"<section><table><tr><th>A</th><th>B|C</th></tr><tr><td>1</td><td><strong>2</strong></td></tr><tr><td>3</td></tr></table></section>",
    ))?;
    assert_eq!(
        texts(&converted),
        ["| A | B\\|C |\n| --- | --- |\n| 1 | **2** |\n| 3 |  |"]
    );
    assert_eq!(warnings(&converted), []);
    Ok(())
}

#[test]
fn a_table_with_merged_cells_is_flattened_and_reported() -> Outcome {
    let converted = convert(&with_sections(
        r#"<section><table><tr><th colspan="2">A</th></tr><tr><td>1</td><td rowspan="2">2</td></tr></table></section>"#,
    ))?;
    assert_eq!(texts(&converted), ["| A |  |\n| --- | --- |\n| 1 | 2 |"]);
    assert_eq!(
        warnings(&converted),
        [(
            &WarningKind::MarkupDropped("table with merged cells".to_owned()),
            Some("s1")
        )]
    );
    Ok(())
}

#[test]
fn a_block_image_is_a_figure_and_an_inline_image_stays_inline() -> Outcome {
    let xml = document(&format!(
        r##"<body><section><title><p>Pics</p></title>
<image l:href="#fig" title="A caption" alt="A figure"/>
<p>Text <image l:href="#dot" alt="dot"/> more.</p></section></body>
{}{}"##,
        binary("fig", 30, 20)?,
        binary("dot", 8, 8)?
    ));
    let converted = convert(&xml)?;
    let sizes: Vec<_> = converted
        .book
        .pictures()
        .iter()
        .map(|p| (p.path(), p.size().width, p.size().height))
        .collect();
    assert_eq!(sizes, [("img/0001.avif", 30, 20), ("img/0002.avif", 8, 8)]);
    assert_eq!(
        texts(&converted),
        ["# Pics\n\n![A figure](img/0001.avif \"A caption\")\n\nText ![dot](img/0002.avif) more."]
    );
    assert_eq!(warnings(&converted), []);
    Ok(())
}

#[test]
fn an_image_without_a_binary_is_reported_and_leaves_its_alt_text() -> Outcome {
    let converted = convert(&with_sections(
        r##"<section><image l:href="#nope" alt="Lost"/><p>x</p></section>"##,
    ))?;
    assert_eq!(texts(&converted), ["Lost\n\nx"]);
    assert_eq!(
        warnings(&converted),
        [(&WarningKind::PictureDropped("nope".to_owned()), Some("s1"))]
    );
    Ok(())
}

#[test]
fn the_cover_page_is_the_cover() -> Outcome {
    let xml = described(
        r##"<description><title-info><genre>sf</genre><author><nickname>n</nickname></author>
<book-title>T</book-title><coverpage><image l:href="#cov"/></coverpage><lang>en</lang></title-info></description>"##,
        &format!(
            "<body><section><p>x</p></section></body>{}",
            binary("cov", 12, 16)?
        ),
    );
    let converted = convert(&xml)?;
    assert_eq!(converted.book.meta().cover(), Some("img/cover.avif"));
    let cover = converted.book.picture("img/cover.avif");
    assert_eq!(
        cover.map(|p| (p.size().width, p.size().height)),
        Some((12, 16))
    );
    Ok(())
}

#[test]
fn a_link_to_a_paragraph_leads_to_its_chapter_and_marks_it() -> Outcome {
    let converted = convert(&with_sections(
        r##"<section><p>See <a l:href="#target">there</a>.</p></section>
<section><p>Before.</p><p id="target">Target paragraph.</p></section>"##,
    ))?;
    let chapters = converted.book.chapters();
    let path = chapters[1].path();
    assert!(
        chapters[0]
            .text()
            .contains(&format!("[there]({path}#target)")),
        "{}",
        chapters[0].text()
    );
    let names: Vec<_> = chapters[1]
        .extra_anchors()
        .iter()
        .map(|anchor| anchor.name.as_str())
        .collect();
    assert_eq!(names, ["target"]);
    assert_eq!(warnings(&converted), []);
    Ok(())
}

#[test]
fn a_link_to_a_nested_section_leads_to_the_chapter_that_holds_it() -> Outcome {
    let converted = convert(&with_sections(
        r##"<section><p><a l:href="#inner">go</a></p></section>
<section><title><p>Outer</p></title><section id="inner"><title><p>Inner</p></title><p>x</p></section></section>"##,
    ))?;
    let chapters = converted.book.chapters();
    let path = chapters[1].path();
    assert!(
        chapters[0].text().contains(&format!("[go]({path}#")),
        "{}",
        chapters[0].text()
    );
    assert_eq!(warnings(&converted), []);
    Ok(())
}

#[test]
fn a_link_to_nothing_is_dropped_by_the_assembler() -> Outcome {
    let converted = convert(&with_sections(
        r##"<section><p>See <a l:href="#nowhere">there</a>.</p></section>"##,
    ))?;
    assert_eq!(texts(&converted), ["See there."]);
    assert_eq!(converted.report.warnings.len(), 1);
    assert!(matches!(
        converted.report.warnings[0].kind,
        WarningKind::LinkDropped(_)
    ));
    Ok(())
}

#[test]
fn links_outside_the_book_are_written_as_they_are() -> Outcome {
    let converted = convert(&with_sections(
        r#"<section><p><a l:href="https://example.com/a?b=1">site</a> and <a l:href="mailto:a@b.c">mail</a>
and <a l:href="ftp://x">ftp</a>.</p></section>"#,
    ))?;
    assert_eq!(
        texts(&converted),
        ["[site](https://example.com/a?b=1) and [mail](mailto:a@b.c) and ftp."]
    );
    assert_eq!(
        warnings(&converted),
        [(&WarningKind::LinkDropped("ftp://x".to_owned()), Some("s1"))]
    );
    Ok(())
}

const NOTED: &str = r##"<body>
<section><title><p>A</p></title><p>Text<a type="note" l:href="#n1">[1]</a> and again<a type="note" l:href="#n1">[1]</a>.</p></section>
<section><title><p>B</p></title><p>Other<a type="note" l:href="#n1">[1]</a></p></section></body>
<body name="notes"><section id="n1"><title><p>1</p></title><p>First note.</p><p>Second paragraph.</p></section></body>"##;

#[test]
fn a_note_is_defined_once_at_the_end_of_each_section_that_refers_to_it() -> Outcome {
    let converted = convert(&document(NOTED))?;
    assert_eq!(
        texts(&converted),
        [
            "# A\n\nText[^n1] and again[^n1].\n\n[^n1]: First note.\n\n    Second paragraph.",
            "# B\n\nOther[^n1]\n\n[^n1]: First note.\n\n    Second paragraph."
        ]
    );
    assert_eq!(warnings(&converted), []);
    Ok(())
}

#[test]
fn a_note_reference_without_its_note_keeps_the_text_of_the_link() -> Outcome {
    let converted = convert(&with_sections(
        r##"<section><p>Text<a type="note" l:href="#n9">[9]</a>.</p></section>"##,
    ))?;
    assert_eq!(texts(&converted), ["Text\\[9\\]."]);
    assert_eq!(
        warnings(&converted),
        [(&WarningKind::LinkDropped("#n9".to_owned()), Some("s1"))]
    );
    Ok(())
}

#[test]
fn a_body_of_comments_produces_no_chapter() -> Outcome {
    let converted = convert(&document(
        r#"<body><section><p>Text.</p></section></body>
<body name="comments"><section id="c1"><p>Comment.</p></section></body>"#,
    ))?;
    assert_eq!(texts(&converted), ["Text."]);
    Ok(())
}

#[test]
fn a_ids_inside_a_quote_are_marked_in_front_of_the_quote() -> Outcome {
    let converted = convert(&with_sections(
        r##"<section><p><a l:href="#q">q</a></p></section>
<section><cite><p id="q">Quoted.</p></cite></section>"##,
    ))?;
    let chapters = converted.book.chapters();
    assert_eq!(chapters[1].text(), "[q]: #q\n\n> Quoted.");
    let names: Vec<_> = chapters[1]
        .extra_anchors()
        .iter()
        .map(|a| a.name.as_str())
        .collect();
    assert_eq!(names, ["q"]);
    Ok(())
}

/// A Windows-1251 document with a Cyrillic title and paragraph.
fn windows_1251() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(
        br#"<?xml version="1.0" encoding="windows-1251"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0"><description><title-info>
<genre>sf</genre><author><nickname>n</nickname></author><book-title>"#,
    );
    // Книга
    bytes.extend_from_slice(&[0xca, 0xed, 0xe8, 0xe3, 0xe0]);
    bytes.extend_from_slice(
        b"</book-title><lang>ru</lang></title-info></description><body><section><p>",
    );
    // Привет
    bytes.extend_from_slice(&[0xcf, 0xf0, 0xe8, 0xe2, 0xe5, 0xf2]);
    bytes.extend_from_slice(b"</p></section></body></FictionBook>");
    bytes
}

#[test]
fn a_windows_1251_document_is_decoded() -> Outcome {
    let converted = Converter::new().load_slice(&windows_1251())?;
    assert_eq!(converted.book.meta().title(), "Книга");
    assert_eq!(texts(&converted), ["Привет"]);
    Ok(())
}

/// An archive with these entries.
fn archive(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in entries {
        writer.start_file(*name, SimpleFileOptions::default())?;
        writer.write_all(data)?;
    }
    Ok(writer.finish()?.into_inner())
}

#[test]
fn a_book_in_a_zip_archive_is_detected_and_read() -> Outcome {
    let xml = with_sections("<section><p>Zipped.</p></section>");
    let zipped = archive(&[("book.fb2", xml.as_bytes())])?;
    let converted = Converter::new().load_slice(&zipped)?;
    assert_eq!(converted.report.format, Format::Fb2);
    assert_eq!(texts(&converted), ["Zipped."]);
    Ok(())
}

#[test]
fn an_archive_without_an_fb2_file_is_a_source_error() -> Outcome {
    let zipped = archive(&[("notes.txt", b"hello")])?;
    let result = fb2_converter().load_slice(&zipped);
    assert!(matches!(
        result,
        Err(Error::Source {
            format: Format::Fb2,
            ..
        })
    ));
    Ok(())
}

#[test]
fn a_broken_archive_is_a_source_error() {
    let result = fb2_converter().load_slice(b"PK\x03\x04 broken");
    assert!(matches!(
        result,
        Err(Error::Source {
            format: Format::Fb2,
            ..
        })
    ));
}

#[test]
fn an_encrypted_entry_is_a_protected_source() -> Outcome {
    let xml = with_sections("<section><p>Secret.</p></section>");
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer.start_file(
        "book.fb2",
        SimpleFileOptions::default().with_deprecated_encryption(b"password")?,
    )?;
    writer.write_all(xml.as_bytes())?;
    let zipped = writer.finish()?.into_inner();
    let result = fb2_converter().load_slice(&zipped);
    assert!(matches!(result, Err(Error::Protected(Format::Fb2))));
    Ok(())
}

#[test]
fn an_entry_above_the_limit_is_refused() -> Outcome {
    let xml = with_sections("<section><p>Large enough.</p></section>");
    let zipped = archive(&[("book.fb2", xml.as_bytes())])?;
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Fb2);
    options.limits.book.max_file_bytes = 100;
    let result = Converter::with_options(options).load_slice(&zipped);
    assert!(matches!(result, Err(Error::Limit(_))));
    Ok(())
}

#[test]
fn plain_fb2_is_detected_from_its_content() -> Outcome {
    let xml = with_sections("<section><p>Detected.</p></section>");
    let converted = Converter::new().load_slice(xml.as_bytes())?;
    assert_eq!(converted.report.format, Format::Fb2);
    assert_eq!(texts(&converted), ["Detected."]);
    Ok(())
}

#[test]
fn malformed_xml_is_a_source_error() {
    let result = convert("<FictionBook><description><title-info>");
    assert!(matches!(
        result,
        Err(Error::Source {
            format: Format::Fb2,
            ..
        })
    ));
}

#[test]
fn xml_that_is_not_fb2_is_a_source_error() {
    let result = convert("<html><body><p>No book.</p></body></html>");
    assert!(matches!(
        result,
        Err(Error::Source {
            format: Format::Fb2,
            ..
        })
    ));
}

#[test]
fn a_picture_above_the_limit_is_refused() -> Outcome {
    let xml = document(&format!(
        "<body><section><p>x</p></section></body>{}",
        binary("big", 100, 100)?
    ));
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Fb2);
    options.limits.book.max_resource_bytes = 50;
    let result = Converter::with_options(options).load_slice(xml.as_bytes());
    assert!(matches!(result, Err(Error::Limit(_))));
    Ok(())
}

#[test]
fn pictures_above_the_limit_together_are_refused() -> Outcome {
    let xml = document(&format!(
        "<body><section><p>x</p></section></body>{}{}",
        binary("a", 100, 100)?,
        binary("b", 100, 100)?
    ));
    let one = u64::try_from(png(100, 100)?.len())?;
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Fb2);
    options.limits.book.max_file_bytes = one + one / 2;
    let result = Converter::with_options(options).load_slice(xml.as_bytes());
    assert!(matches!(result, Err(Error::Limit(_))));
    Ok(())
}

#[test]
fn a_binary_with_damaged_base64_is_reported_and_left_out() -> Outcome {
    let xml = document(
        r#"<body><section><p>x</p></section></body><binary id="bad" content-type="image/png">!!!!</binary>"#,
    );
    let converted = convert(&xml)?;
    assert_eq!(
        converted
            .report
            .warnings
            .iter()
            .map(|w| &w.kind)
            .collect::<Vec<_>>(),
        [&WarningKind::PictureDropped("bad".to_owned())]
    );
    Ok(())
}

#[test]
fn a_document_without_sections_has_nothing_to_convert() {
    let result = convert(&document("<body><section/></body>"));
    assert!(matches!(result, Err(Error::Empty)));
}

#[test]
fn a_document_with_only_notes_has_nothing_to_convert() {
    let result = convert(&document(
        r#"<body name="notes"><section id="n1"><p>Note.</p></section></body>"#,
    ));
    assert!(matches!(result, Err(Error::Empty)));
}

#[test]
fn a_converted_book_is_written_and_read_back() -> Outcome {
    let xml = with_sections("<section><title><p>One</p></title><p>Text.</p></section>");
    let mut file = Vec::new();
    fb2_converter().convert_slice(xml.as_bytes(), &mut file)?;
    let book = Book::read(file.as_slice())?;
    assert_eq!(book.meta().title(), "The Test");
    Ok(())
}

#[test]
fn poems_keep_subtitles_stanza_titles_epigraphs_and_dates() -> Outcome {
    let text = only_text(&with_sections(
        r##"<section><poem id="po"><title><p>Ode</p></title>
<epigraph><p>Motto.</p></epigraph>
<subtitle>Part</subtitle>
<stanza><title><p>First</p></title><subtitle>Sub</subtitle><v id="v1">One</v><v>Two</v></stanza>
<date>1900</date></poem><p><a l:href="#v1">verse</a></p></section>"##,
    ))?;
    assert!(
        text.starts_with(
            "**Ode**\n\n> Motto.\n\n**Part**\n\n**First**\n\n**Sub**\n\n[v1]: #v1\n\nOne\\\nTwo\n\n1900\n\n[verse]("
        ),
        "{text}"
    );
    Ok(())
}

#[test]
fn subtitles_and_annotations_inside_quotes_are_bold_paragraphs_in_the_quote() -> Outcome {
    let text = only_text(&with_sections(
        r"<section><annotation><p>Summary.</p><subtitle>Sub</subtitle><cite><p>Inner.</p></cite>
<poem><stanza><v>A</v></stanza></poem><table><tr><td>t</td></tr></table></annotation>
<cite><subtitle>Cited</subtitle><poem><stanza><v>B</v></stanza></poem>
<table><tr><td>c</td></tr></table></cite><p>Body.</p></section>",
    ))?;
    assert!(
        text.starts_with("> Summary.\n>\n> **Sub**\n>\n> > Inner.\n>\n> A\n>\n> | t |"),
        "{text}"
    );
    assert!(
        text.contains("> **Cited**\n>\n> B\n>\n> | c |\n> | --- |\n\nBody."),
        "{text}"
    );
    Ok(())
}

#[test]
fn an_epigraph_may_hold_a_poem_and_a_citation() -> Outcome {
    let text = only_text(&with_sections(
        r"<section><epigraph><poem><stanza><v>Rhyme</v></stanza></poem><cite><p>Cited.</p></cite></epigraph><p>x</p></section>",
    ))?;
    assert_eq!(text, "> Rhyme\n>\n> > Cited.\n\nx");
    Ok(())
}

#[test]
fn a_section_may_start_with_an_image() -> Outcome {
    let xml = document(&format!(
        r##"<body><section id="s"><title><p>T</p></title><image l:href="#pic" alt="Alt"/><p>x</p></section></body>{}"##,
        binary("pic", 5, 5)?
    ));
    let text = only_text(&xml)?;
    assert_eq!(text, "# T\n\n![Alt](img/0001.avif)\n\nx");
    Ok(())
}

#[test]
fn a_note_may_be_a_nested_section_and_without_a_type() -> Outcome {
    let converted = convert(&document(
        r##"<body><section><p>One<a l:href="#n1">1</a> two<a l:href="#n2">2</a> three<a l:href="#n3">3</a>.</p></section></body>
<body name="Notes"><section><title><p>Notes</p></title>
<section id="n1"><title><p>1</p></title><p>First <strong>note</strong>.</p></section>
<section id="n2"><title><p>2</p></title></section>
<section id="n3"><p>Third<a type="note" l:href="#n1">1</a>.</p><p id="inside">More.</p></section></section></body>"##,
    ))?;
    assert_eq!(
        texts(&converted),
        [
            "One[^n1] two[^n2] three[^n3].\n\n[^n1]: First **note**.\n\n[^n2]: 2\n\n[^n3]: Third1.\n\n    More."
        ]
    );
    Ok(())
}

#[test]
fn a_note_without_any_text_is_defined_with_an_ellipsis() -> Outcome {
    let converted = convert(&document(
        r##"<body><section><p>One<a type="note" l:href="#n1">1</a>.</p></section></body>
<body name="notes"><section id="n1"/></body>"##,
    ))?;
    assert_eq!(texts(&converted), ["One[^n1].\n\n[^n1]: \u{2026}"]);
    Ok(())
}

#[test]
fn a_link_without_an_address_or_text_leaves_nothing() -> Outcome {
    let converted = convert(&with_sections(
        r##"<section><p>A<a>bare</a>, <a l:href="#x"/><a l:href="https://e.org"/>z</p></section>"##,
    ))?;
    assert_eq!(texts(&converted).len(), 1);
    assert!(texts(&converted)[0].contains("bare"));
    Ok(())
}

#[test]
fn an_address_is_made_safe_as_a_destination() -> Outcome {
    let converted = convert(&with_sections(
        r#"<section><p><a l:href="https://e.org/a b(c)">x</a></p></section>"#,
    ))?;
    assert_eq!(texts(&converted), ["[x](https://e.org/a%20b%28c%29)"]);
    Ok(())
}

#[test]
fn duplicate_binaries_and_marks_are_kept_once() -> Outcome {
    let xml = document(&format!(
        r##"<body><section><p id="d">One</p><p id="d">Two</p><p><a l:href="#d">d</a><image l:href="#a"/></p></section></body>{}{}"##,
        binary("a", 4, 4)?,
        binary("a", 9, 9)?
    ));
    let converted = convert(&xml)?;
    let sizes: Vec<_> = converted
        .book
        .pictures()
        .iter()
        .map(|p| p.size().width)
        .collect();
    assert_eq!(sizes, [4]);
    let anchors = converted.book.chapters()[0].extra_anchors().len();
    assert_eq!(anchors, 1);
    Ok(())
}

#[test]
fn the_cover_image_of_a_missing_binary_is_no_cover() -> Outcome {
    let xml = described(
        r##"<description><title-info><genre>sf</genre><author><nickname>n</nickname></author>
<book-title>T</book-title><coverpage><image l:href="#none"/></coverpage><lang>en</lang></title-info></description>"##,
        "<body><section><p>x</p></section></body>",
    );
    assert_eq!(convert(&xml)?.book.meta().cover(), None);
    Ok(())
}

#[test]
fn a_series_of_the_publisher_stands_in_for_a_missing_one() -> Outcome {
    let xml = described(
        r#"<description><title-info><genre>sf</genre><author><first-name>A</first-name><last-name>B</last-name></author>
<book-title>T</book-title><lang>en</lang></title-info>
<publish-info><sequence name="Other" number="3"/></publish-info></description>"#,
        "<body><section><p>x</p></section></body>",
    );
    let converted = convert(&xml)?;
    let series = converted.book.meta().series().map(|s| (s.name, s.index));
    assert_eq!(series, Some(("Other", Some("3"))));
    Ok(())
}

#[test]
fn an_annotation_with_poems_and_citations_is_plain_text() -> Outcome {
    let xml = described(
        r##"<description><title-info><genre>sf</genre><author><nickname>n</nickname></author>
<book-title>T</book-title><annotation><subtitle>Sub</subtitle><poem><stanza><v>A</v><v>B</v></stanza></poem>
<cite><p>Quoted <a l:href="#x">link</a></p></cite><table><tr><td>t</td></tr></table></annotation><lang>en</lang></title-info></description>"##,
        "<body><section><p>x</p></section></body>",
    );
    assert_eq!(
        convert(&xml)?.book.meta().annotation(),
        Some("Sub\n\nA\nB\n\nQuoted link")
    );
    Ok(())
}

#[test]
fn a_book_without_names_has_stand_ins_for_its_metadata() -> Outcome {
    let xml = described(
        r"<description><title-info><genre>sf</genre><author><first-name> </first-name></author>
<book-title> </book-title><lang/></title-info></description>",
        "<body><section><p>x</p></section></body>",
    );
    let converted = convert(&xml)?;
    assert_eq!(converted.book.meta().title(), "Untitled");
    assert_eq!(converted.book.meta().authors().count(), 0);
    Ok(())
}

#[test]
fn text_with_a_private_use_marker_cannot_forge_a_link() -> Outcome {
    let converted = convert(&with_sections(
        "<section><p>a\u{e000}x\u{e001}b</p></section>",
    ))?;
    assert_eq!(texts(&converted), ["axb"]);
    Ok(())
}

#[test]
fn a_zip_entry_name_is_matched_without_regard_to_case() -> Outcome {
    let xml = with_sections("<section><p>Upper.</p></section>");
    let zipped = archive(&[("dir/", b""), ("BOOK.FB2", xml.as_bytes())])?;
    assert_eq!(texts(&fb2_converter().load_slice(&zipped)?), ["Upper."]);
    Ok(())
}

#[test]
fn styled_text_inside_links_and_odd_blocks_are_written() -> Outcome {
    let xml = document(&format!(
        r##"<body><title><p>Book</p></title><image l:href="#pic" alt="Front"/>
<section id="a"><image l:href="#pic" alt="x"/><epigraph><p>E</p><empty-line/></epigraph>
<p><a l:href="https://e.org"><strong>s</strong> <emphasis>e</emphasis> <strikethrough>d</strikethrough> <style name="n">t</style>
<code>c</code> H<sub>2</sub> x<sup>2</sup></a> <image l:href="#gone" alt="Gone"/></p>
<poem><stanza><v>V</v></stanza><date value="1900-01-02"/></poem>
<annotation><empty-line/><p>Note.</p></annotation></section></body>{}"##,
        binary("pic", 6, 6)?
    ));
    let converted = convert(&xml)?;
    let texts = texts(&converted);
    assert_eq!(texts.len(), 2);
    assert!(
        texts[0].starts_with("# Book\n\n![Front](img/0001.avif)"),
        "{}",
        texts[0]
    );
    assert!(
        texts[1].contains("[**s** *e* ~~d~~ t `c` H2 x2](https://e.org) Gone"),
        "{}",
        texts[1]
    );
    assert!(texts[1].contains("V\n\n1900-01-02"), "{}", texts[1]);
    let kinds: Vec<_> = converted.report.warnings.iter().map(|w| &w.kind).collect();
    assert!(kinds.contains(&&WarningKind::PictureDropped("gone".to_owned())));
    Ok(())
}

/// Converts a document that must have one chapter and returns its text.
fn text_of(xml: &str) -> Result<String, Box<dyn std::error::Error>> {
    only_text(&with_sections(&format!("<section>{xml}</section>")))
}

/// A document with this sequence in its title information.
fn sequenced(sequence: &str) -> String {
    described(
        &format!(
            r"<description><title-info><genre>sf</genre><author><nickname>n</nickname></author>
<book-title>T</book-title><lang>en</lang>{sequence}</title-info></description>"
        ),
        "<body><section><p>x</p></section></body>",
    )
}

#[test]
fn the_number_of_a_sequence_is_taken_as_written() -> Outcome {
    let converted = convert(&sequenced(r#"<sequence name="Saga" number=" 1 "/>"#))?;
    let series = converted.book.meta().series().map(|s| (s.name, s.index));
    assert_eq!(series, Some(("Saga", Some("1"))));
    Ok(())
}

#[test]
fn a_fractional_number_of_a_sequence_does_not_fail_the_document() -> Outcome {
    let converted = convert(&sequenced(r#"<sequence name="Saga" number="1.5"/>"#))?;
    assert_eq!(converted.book.meta().series().map(|s| s.name), Some("Saga"));
    assert_eq!(texts(&converted), ["x"]);
    Ok(())
}

#[test]
fn an_empty_number_of_a_sequence_gives_no_position() -> Outcome {
    let converted = convert(&sequenced(r#"<sequence name="Saga" number=""/>"#))?;
    let series = converted.book.meta().series().map(|s| (s.name, s.index));
    assert_eq!(series, Some(("Saga", None)));
    Ok(())
}

#[test]
fn a_self_closed_body_is_empty_and_not_an_error_of_the_source() {
    let result = convert(&document("<body/>"));
    assert!(matches!(result, Err(Error::Empty)));
}

#[test]
fn a_body_without_content_is_empty_and_not_an_error_of_the_source() {
    let result = convert(&document("<body></body>"));
    assert!(matches!(result, Err(Error::Empty)));
}

#[test]
fn an_empty_body_before_the_main_body_is_not_the_main_body_of_a_book_without_text() {
    let result = convert(&document("<body/><body name=\"notes\"/>"));
    assert!(matches!(result, Err(Error::Empty)));
}

#[test]
fn a_table_without_rows_writes_nothing() -> Outcome {
    assert_eq!(text_of("<table/><p>after</p>")?, "after");
    assert_eq!(text_of("<table></table><p>after</p>")?, "after");
    assert_eq!(text_of("<table><tr/></table><p>after</p>")?, "after");
    Ok(())
}

#[test]
fn a_document_with_only_an_empty_table_has_nothing_to_convert() {
    let result = convert(&with_sections("<section><table/></section>"));
    assert!(matches!(result, Err(Error::Empty)));
}

#[test]
fn a_section_holds_its_blocks_and_subsections_in_document_order() -> Outcome {
    let text = only_text(&with_sections(
        r"<section><title><p>Top</p></title><p>one</p>
<section><title><p>Sub</p></title><p>inside</p></section>
<p>two</p><subtitle>Late</subtitle><p>three</p></section>",
    ))?;
    assert_eq!(
        text,
        "# Top\n\none\n\n## Sub\n\ninside\n\ntwo\n\n## Late\n\nthree"
    );
    Ok(())
}

#[test]
fn a_genre_is_the_text_of_its_element_trimmed() -> Outcome {
    let xml = described(
        r"<description><title-info><genre> sf_fantasy
</genre><genre>made_up_genre</genre><genre> </genre>
<author><nickname>n</nickname></author><book-title>T</book-title><lang>en</lang></title-info></description>",
        "<body><section><p>x</p></section></body>",
    );
    let converted = convert(&xml)?;
    assert_eq!(
        converted.book.meta().genres().collect::<Vec<_>>(),
        ["sf_fantasy", "made_up_genre"]
    );
    Ok(())
}

#[test]
fn a_space_between_inline_elements_is_kept_when_the_source_has_one() -> Outcome {
    assert_eq!(
        text_of("<p><strong>a</strong> <emphasis>b</emphasis></p>")?,
        "**a** *b*"
    );
    Ok(())
}

#[test]
fn no_space_is_made_between_inline_elements_that_touch() -> Outcome {
    assert_eq!(
        text_of("<p><strong>a</strong><emphasis>b</emphasis></p>")?,
        "**a***b*"
    );
    assert_eq!(text_of("<p>x<strong>a</strong>y</p>")?, "x**a**y");
    Ok(())
}

#[test]
fn white_space_inside_a_paragraph_collapses_to_single_spaces() -> Outcome {
    let text = text_of("<p>\n      first   line\n      \tsecond\r\n   line  \n   </p>")?;
    assert_eq!(text, "first line second line");
    Ok(())
}

#[test]
fn spaces_at_the_ends_of_emphasis_stay_outside_its_markers() -> Outcome {
    assert_eq!(
        text_of("<p><strong>bold </strong>next and <emphasis> lean</emphasis></p>")?,
        "**bold** next and *lean*"
    );
    assert_eq!(text_of("<p>a<strong> b </strong>c</p>")?, "a **b** c");
    Ok(())
}

#[test]
fn a_space_in_the_text_around_a_picture_is_the_one_of_the_source() -> Outcome {
    let xml = document(&format!(
        r##"<body><section><p>a<image l:href="#dot" alt="d"/>b <image l:href="#dot" alt="d"/> c</p></section></body>{}"##,
        binary("dot", 4, 4)?
    ));
    assert_eq!(
        only_text(&xml)?,
        "a![d](img/0001.avif)b ![d](img/0001.avif) c"
    );
    Ok(())
}

#[test]
fn a_note_reference_stands_where_the_source_puts_it() -> Outcome {
    let converted = convert(&document(
        r##"<body><section><p>Near<a type="note" l:href="#n1">1</a> far <a type="note" l:href="#n1">1</a>.</p></section></body>
<body name="notes"><section id="n1"><p>Note.</p></section></body>"##,
    ))?;
    assert_eq!(texts(&converted), ["Near[^n1] far [^n1].\n\n[^n1]: Note."]);
    Ok(())
}

#[test]
fn a_link_keeps_the_spaces_of_its_neighbours_and_not_its_own() -> Outcome {
    assert_eq!(
        text_of(r#"<p>go <a l:href="https://e.org"> here </a>now</p>"#)?,
        "go [here](https://e.org) now"
    );
    Ok(())
}

#[test]
fn code_keeps_the_space_around_it_outside_the_fence() -> Outcome {
    assert_eq!(text_of("<p>a<code> b </code>c</p>")?, "a `b` c");
    Ok(())
}

#[test]
fn entity_and_character_references_in_text_are_replaced() -> Outcome {
    assert_eq!(
        text_of("<p>a &amp; b &lt;c&gt; &quot;d&quot; &apos;e&apos; &#8212; &#x2014;.</p>")?,
        "a \\& b \\<c\\> \"d\" 'e' \u{2014} \u{2014}."
    );
    Ok(())
}

#[test]
fn entity_and_character_references_in_attribute_values_are_replaced() -> Outcome {
    let converted = convert(&with_sections(
        r##"<section><image l:href="#nope" alt="A &amp; B &#8212; C &#x2014; &lt;D&gt;"/>
<p><a l:href="https://e.org/?a=1&amp;b=2">x</a></p></section>"##,
    ))?;
    assert_eq!(
        texts(&converted),
        ["A \\& B \u{2014} C \u{2014} \\<D\\>\n\n[x](https://e.org/?a=1&b=2)"]
    );
    Ok(())
}

#[test]
fn the_named_entities_of_html_are_understood() -> Outcome {
    let converted = convert(&with_sections(
        "<section><p>a&nbsp;b &mdash; c &laquo;d&raquo;</p></section>",
    ))?;
    assert_eq!(texts(&converted), ["a\u{a0}b \u{2014} c \u{ab}d\u{bb}"]);
    Ok(())
}

#[test]
fn an_unknown_entity_is_a_source_error() {
    let result = convert(&with_sections("<section><p>a&nosuch;b</p></section>"));
    assert!(matches!(
        result,
        Err(Error::Source {
            format: Format::Fb2,
            ..
        })
    ));
}

#[test]
fn cdata_is_text() -> Outcome {
    assert_eq!(
        text_of("<p>a <![CDATA[<b> & c]]> d</p>")?,
        "a \\<b\\> \\& c d"
    );
    Ok(())
}

#[test]
fn href_may_have_any_prefix_or_none() -> Outcome {
    let xml = format!(
        r##"{}<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:q="urn:q">
{DESCRIPTION}<body><section id="s"><p><a xlink:href="#t">one</a> <a q:href="#t">two</a> <a href="#t">three</a></p></section>
<section><p id="t">Target.</p></section></body></FictionBook>"##,
        r#"<?xml version="1.0" encoding="UTF-8"?>"#
    );
    let converted = convert(&xml)?;
    let chapters = converted.book.chapters();
    let path = chapters[1].path();
    assert_eq!(
        chapters[0].text(),
        format!("[one]({path}#t) [two]({path}#t) [three]({path}#t)")
    );
    Ok(())
}

#[test]
fn every_element_may_carry_a_prefix() -> Outcome {
    let xml = r#"<?xml version="1.0"?>
<fb:FictionBook xmlns:fb="http://www.gribuser.ru/xml/fictionbook/2.0">
<fb:description><fb:title-info><fb:genre>sf</fb:genre><fb:author><fb:nickname>n</fb:nickname></fb:author>
<fb:book-title>Prefixed</fb:book-title><fb:lang>en</fb:lang></fb:title-info></fb:description>
<fb:body><fb:section><fb:p>Text <fb:strong>bold</fb:strong>.</fb:p></fb:section></fb:body></fb:FictionBook>"#;
    let converted = convert(xml)?;
    assert_eq!(converted.book.meta().title(), "Prefixed");
    assert_eq!(texts(&converted), ["Text **bold**."]);
    Ok(())
}

#[test]
fn an_unknown_inline_element_keeps_its_text() -> Outcome {
    assert_eq!(
        text_of("<p>a <mystery>b <strong>c</strong> d</mystery> e<odd/>f</p>")?,
        "a b **c** d ef"
    );
    Ok(())
}

#[test]
fn an_unknown_block_element_is_a_paragraph() -> Outcome {
    assert_eq!(
        text_of("<p>one</p><note-ish>two <emphasis>three</emphasis></note-ish><p>four</p>")?,
        "one\n\ntwo *three*\n\nfour"
    );
    Ok(())
}

#[test]
fn an_unknown_block_element_that_holds_blocks_is_a_container() -> Outcome {
    assert_eq!(
        text_of("<wrapper><p>one</p><subtitle>sub</subtitle><p>two</p></wrapper>")?,
        "one\n\n## sub\n\ntwo"
    );
    Ok(())
}

#[test]
fn text_directly_in_a_section_is_a_paragraph() -> Outcome {
    assert_eq!(
        text_of("<p>one</p>\n loose text \n<p>two</p>")?,
        "one\n\nloose text\n\ntwo"
    );
    Ok(())
}

#[test]
fn what_stands_after_the_sections_of_a_body_is_a_section_of_its_own() -> Outcome {
    let converted = convert(&document(
        r"<body><title><p>Book</p></title><section><p>one</p></section><p>tail</p>
<section><p>two</p></section></body>",
    ))?;
    assert_eq!(texts(&converted), ["# Book", "one", "tail", "two"]);
    Ok(())
}

#[test]
fn a_section_that_writes_nothing_is_not_marked() -> Outcome {
    let converted = convert(&with_sections(
        r##"<section><p><a l:href="#empty">go</a></p></section><section id="empty"><empty-line/></section>"##,
    ))?;
    assert_eq!(texts(&converted), ["go"]);
    Ok(())
}

#[test]
fn a_binary_without_content_is_reported_and_left_out() -> Outcome {
    let converted = convert(&document(
        r#"<body><section><p>x</p></section></body><binary id="none" content-type="image/png"/><binary id="blank" content-type="image/png">
  </binary>"#,
    ))?;
    assert_eq!(
        converted
            .report
            .warnings
            .iter()
            .map(|w| &w.kind)
            .collect::<Vec<_>>(),
        [
            &WarningKind::PictureDropped("none".to_owned()),
            &WarningKind::PictureDropped("blank".to_owned())
        ]
    );
    Ok(())
}

#[test]
fn base64_with_line_breaks_and_without_padding_is_decoded() -> Outcome {
    let data = STANDARD.encode(png(6, 6)?);
    let mut wrapped = String::new();
    for chunk in data.as_bytes().chunks(20) {
        writeln!(wrapped, "{}\n    ", String::from_utf8_lossy(chunk))?;
    }
    let xml = document(&format!(
        r##"<body><section><image l:href="#p"/></section></body><binary id="p">{}</binary>"##,
        wrapped.trim_end_matches('=')
    ));
    let converted = convert(&xml)?;
    assert_eq!(converted.book.pictures().len(), 1);
    Ok(())
}

#[test]
fn mismatched_tags_are_a_source_error() {
    for xml in [
        "<FictionBook><description></body></FictionBook>",
        "<FictionBook><body><section><p>x</section></p></body></FictionBook>",
        "<FictionBook></Fiction>",
    ] {
        assert!(
            matches!(
                convert(xml),
                Err(Error::Source {
                    format: Format::Fb2,
                    ..
                })
            ),
            "{xml}"
        );
    }
}

#[test]
fn an_unclosed_element_at_the_end_is_a_source_error() {
    let result = convert(&format!("{HEAD}{DESCRIPTION}<body><section><p>x</p>"));
    assert!(matches!(
        result,
        Err(Error::Source {
            format: Format::Fb2,
            ..
        })
    ));
}

#[test]
fn a_root_element_in_other_case_is_not_fictionbook() {
    let result = convert("<fictionbook><body><section><p>x</p></section></body></fictionbook>");
    assert!(matches!(
        result,
        Err(Error::Source {
            format: Format::Fb2,
            ..
        })
    ));
}

#[test]
fn text_that_cannot_be_decoded_is_a_source_error() {
    let mut bytes = HEAD.as_bytes().to_vec();
    bytes.extend_from_slice(b"<body><section><p>\xff\xfe</p></section></body></FictionBook>");
    let result = fb2_converter().load_slice(&bytes);
    assert!(matches!(
        result,
        Err(Error::Source {
            format: Format::Fb2,
            ..
        })
    ));
}

/// The sections of a body nested `depth` deep, with a paragraph inside.
fn nested(depth: usize) -> String {
    format!(
        "{}<p>deep</p>{}",
        "<section>".repeat(depth),
        "</section>".repeat(depth)
    )
}

#[test]
fn nesting_deeper_than_the_bound_is_a_limit_error() {
    // FictionBook, body, 254 sections and a paragraph are 257 open elements.
    let result = convert(&document(&format!("<body>{}</body>", nested(254))));
    assert!(matches!(result, Err(Error::Limit(_))));
    let result = convert(&document(&format!("<body>{}</body>", nested(5000))));
    assert!(matches!(result, Err(Error::Limit(_))));
}

#[test]
fn nesting_at_the_bound_is_converted() -> Outcome {
    // FictionBook, body, 253 sections and a paragraph are 256 open elements.
    let text = only_text(&document(&format!("<body>{}</body>", nested(253))))?;
    assert!(text.ends_with("deep"), "{text}");
    Ok(())
}

#[test]
fn deeply_nested_inline_elements_are_a_limit_error() {
    let inline = format!("{}x{}", "<emphasis>".repeat(400), "</emphasis>".repeat(400));
    let result = convert(&with_sections(&format!(
        "<section><p>{inline}</p></section>"
    )));
    assert!(matches!(result, Err(Error::Limit(_))));
}

/// A KOI8-R document with a Cyrillic title and paragraph.
fn koi8_r() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(
        br#"<?xml version="1.0" encoding="koi8-r"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0"><description><title-info>
<genre>sf</genre><author><nickname>n</nickname></author><book-title>"#,
    );
    // Книга
    bytes.extend_from_slice(&[0xeb, 0xce, 0xc9, 0xc7, 0xc1]);
    bytes.extend_from_slice(
        b"</book-title><lang>ru</lang></title-info></description><body><section><p>",
    );
    // Привет
    bytes.extend_from_slice(&[0xf0, 0xd2, 0xc9, 0xd7, 0xc5, 0xd4]);
    bytes.extend_from_slice(b"</p></section></body></FictionBook>");
    bytes
}

#[test]
fn a_koi8_r_document_is_decoded() -> Outcome {
    let converted = Converter::new().load_slice(&koi8_r())?;
    assert_eq!(converted.book.meta().title(), "Книга");
    assert_eq!(texts(&converted), ["Привет"]);
    Ok(())
}

/// A document in UTF-16 with a byte order mark.
fn utf_16(big_endian: bool) -> Vec<u8> {
    let xml = r#"<?xml version="1.0" encoding="UTF-16"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0"><description><title-info>
<genre>sf</genre><author><nickname>n</nickname></author><book-title>Книга</book-title><lang>ru</lang></title-info></description>
<body><section><p>Привет, мир</p></section></body></FictionBook>"#;
    let mut bytes = if big_endian {
        vec![0xfe, 0xff]
    } else {
        vec![0xff, 0xfe]
    };
    for unit in xml.encode_utf16() {
        let pair = if big_endian {
            unit.to_be_bytes()
        } else {
            unit.to_le_bytes()
        };
        bytes.extend_from_slice(&pair);
    }
    bytes
}

#[test]
fn a_utf_16_document_with_a_byte_order_mark_is_decoded() -> Outcome {
    for big_endian in [false, true] {
        let converted = fb2_converter().load_slice(&utf_16(big_endian))?;
        assert_eq!(converted.book.meta().title(), "Книга");
        assert_eq!(texts(&converted), ["Привет, мир"]);
    }
    Ok(())
}

#[test]
fn a_utf_16_document_is_detected_from_its_content() -> Outcome {
    let converted = Converter::new().load_slice(&utf_16(false))?;
    assert_eq!(converted.report.format, Format::Fb2);
    assert_eq!(texts(&converted), ["Привет, мир"]);
    Ok(())
}

#[test]
fn a_utf_8_document_with_a_byte_order_mark_is_read() -> Outcome {
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend_from_slice(with_sections("<section><p>Marked.</p></section>").as_bytes());
    assert_eq!(texts(&fb2_converter().load_slice(&bytes)?), ["Marked."]);
    Ok(())
}

#[test]
fn a_date_without_a_value_is_its_text() -> Outcome {
    let xml = described(
        r"<description><title-info><genre>sf</genre><author><nickname>n</nickname></author>
<book-title>T</book-title><date> 1999-05 </date><lang>en</lang></title-info></description>",
        "<body><section><p>x</p></section></body>",
    );
    assert_eq!(convert(&xml)?.book.meta().date(), Some("1999-05"));
    Ok(())
}

#[test]
fn empty_lines_and_loose_text_in_titles_and_poems_are_harmless() -> Outcome {
    let text = text_of(
        "<title>Loose<empty-line/><p>Para</p></title><poem><empty-line/><stanza><empty-line/><v>One</v></stanza></poem>",
    )?;
    assert_eq!(text, "# Loose Para\n\nOne");
    Ok(())
}
