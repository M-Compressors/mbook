//! One HTML document, through the public API.
#![cfg(feature = "html")]

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use image::{ImageFormat, Rgba, RgbaImage};
use mbook_converter::mbook::{Book, BookReader, Chapter};
use mbook_converter::{ConvertOptions, Converted, Converter, Error, Format, WarningKind};
use std::io::Cursor;

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn html_converter() -> Converter {
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Html);
    Converter::with_options(options)
}

fn convert(html: &str) -> Result<Converted, Error> {
    html_converter().load_slice(html.as_bytes())
}

/// The texts of the chapters of a converted book.
fn texts(converted: &Converted) -> Vec<&str> {
    converted
        .book
        .chapters()
        .iter()
        .map(Chapter::text)
        .collect()
}

/// The text of the one chapter of a converted book.
fn only_text(converted: &Converted) -> Result<&str, String> {
    match texts(converted).as_slice() {
        [text] => Ok(text),
        other => Err(format!("{} chapters, one expected", other.len())),
    }
}

fn kinds(converted: &Converted) -> Vec<&WarningKind> {
    converted
        .report
        .warnings
        .iter()
        .map(|warning| &warning.kind)
        .collect()
}

fn markup(what: &str) -> WarningKind {
    WarningKind::MarkupDropped(what.to_owned())
}

/// A page with a title and a language around some body.
fn page(body: &str) -> String {
    format!(
        "<!DOCTYPE html><html lang=\"en\"><head><title>The Example</title></head><body>{body}</body></html>"
    )
}

/// A PNG of a given size as a `data:` address.
fn png_address(width: u32, height: u32) -> Result<String, Box<dyn std::error::Error>> {
    let picture = RgbaImage::from_pixel(width, height, Rgba([200, 30, 30, 255]));
    let mut bytes = Cursor::new(Vec::new());
    picture.write_to(&mut bytes, ImageFormat::Png)?;
    Ok(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(bytes.into_inner())
    ))
}

#[test]
fn the_metadata_of_the_head_is_the_metadata_of_the_book() -> Outcome {
    let html = "<!DOCTYPE html><html lang=\"de\"><head><title>Der Titel</title>\
        <meta name=\"author\" content=\"A. Autor\"><meta name=\"author\" content=\"B. Autor\">\
        <meta name=\"description\" content=\"Kurz.\"><meta name=\"keywords\" content=\"roman, krimi ,, \">\
        </head><body><p>Text.</p></body></html>";
    let converted = convert(html)?;
    let meta = converted.book.meta();
    assert_eq!((meta.title(), meta.language()), ("Der Titel", "de"));
    assert_eq!(meta.authors().collect::<Vec<_>>(), ["A. Autor", "B. Autor"]);
    assert_eq!(meta.annotation(), Some("Kurz."));
    assert_eq!(meta.genres().collect::<Vec<_>>(), ["roman", "krimi"]);
    assert!(
        !kinds(&converted)
            .iter()
            .any(|kind| matches!(kind, WarningKind::MetadataMissing(_)))
    );
    Ok(())
}

#[test]
fn a_document_without_a_head_has_stand_ins_for_what_the_book_requires() -> Outcome {
    let converted = convert("<p>Text.</p>")?;
    let meta = converted.book.meta();
    assert_eq!((meta.title(), meta.language()), ("Untitled", "und"));
    assert_eq!(
        kinds(&converted),
        [
            &WarningKind::MetadataMissing("title".into()),
            &WarningKind::MetadataMissing("lang".into())
        ]
    );
    assert_eq!(converted.report.format, Format::Html);
    Ok(())
}

#[test]
fn paragraphs_keep_their_inline_styles() -> Outcome {
    let converted = convert(&page(
        "<p>Plain, <b>bold</b>, <i>italic</i>, <s>struck</s> and <code>code</code>.</p><p>Two<br>lines.</p><hr><p>End.</p>",
    ))?;
    assert_eq!(
        only_text(&converted)?,
        "Plain, **bold**, *italic*, ~~struck~~ and `code`.\n\nTwo  \nlines.\n\n- - -\n\nEnd.\n"
    );
    assert_eq!(converted.report.warnings, []);
    Ok(())
}

#[test]
fn the_document_is_cut_at_its_top_level_headings() -> Outcome {
    let converted = convert(&page(
        "<p>Front matter.</p><h1>One</h1><p>First.</p><h2>Sub</h2><p>Deeper.</p><h1>Two</h1><p>Second.</p>",
    ))?;
    assert_eq!(
        texts(&converted),
        [
            "Front matter.\n",
            "# One\n\nFirst.\n\n## Sub\n\nDeeper.\n",
            "# Two\n\nSecond.\n"
        ]
    );
    let paths: Vec<_> = converted
        .book
        .chapters()
        .iter()
        .map(Chapter::path)
        .collect();
    assert_eq!(paths, ["ch0001.md", "ch0002.md", "ch0003.md"]);
    Ok(())
}

#[test]
fn a_document_that_starts_with_a_heading_has_no_chapter_in_front_of_it() -> Outcome {
    let converted = convert(&page("<h1>One</h1><p>A.</p><h1>Two</h1><p>B.</p>"))?;
    assert_eq!(texts(&converted), ["# One\n\nA.\n", "# Two\n\nB.\n"]);
    Ok(())
}

#[test]
fn a_link_to_an_element_of_another_part_leads_to_its_chapter_and_names_the_element() -> Outcome {
    let converted = convert(&page(
        "<h1>One</h1><p>See <a href=\"#remark\">the remark</a>.</p>\
         <h1>Two</h1><p id=\"remark\">A remark.</p>",
    ))?;
    let chapters = converted.book.chapters();
    assert!(
        chapters[0]
            .text()
            .contains("[the remark](ch0002.md#remark)")
    );
    assert_eq!(
        chapters[1].text(),
        "# Two\n\n[remark]: #remark\n\nA remark.\n"
    );
    let names: Vec<_> = chapters[1]
        .extra_anchors()
        .iter()
        .map(|anchor| anchor.name.as_str())
        .collect();
    assert_eq!(names, ["remark"]);
    assert_eq!(converted.report.warnings, []);
    Ok(())
}

#[test]
fn a_link_to_an_element_of_its_own_part_names_the_element() -> Outcome {
    let converted = convert(&page(
        "<p><a href=\"#end\">Down</a></p><p>Middle.</p><p id=\"end\">End.</p>",
    ))?;
    assert_eq!(
        only_text(&converted)?,
        "[Down](ch0001.md#end)\n\nMiddle.\n\n[end]: #end\n\nEnd.\n"
    );
    Ok(())
}

#[test]
fn a_link_to_a_heading_leads_to_the_anchor_of_the_heading() -> Outcome {
    let converted = convert(&page(
        "<h1>One</h1><p><a href=\"#two\">to two</a> and <a href=\"#sub\">to sub</a></p>\
         <h1 id=\"two\">Two</h1><h2 id=\"sub\">Sub</h2>",
    ))?;
    let chapters = converted.book.chapters();
    // A heading has an anchor of its own, so its mark is not kept.
    assert_eq!(
        chapters[0].text(),
        "# One\n\n[to two](ch0002.md#two) and [to sub](ch0002.md#sub)\n"
    );
    assert_eq!(chapters[1].text(), "# Two\n\n## Sub\n");
    assert_eq!(chapters[1].extra_anchors(), []);
    let headings: Vec<_> = chapters[1]
        .anchors()
        .iter()
        .map(|anchor| anchor.name.as_str())
        .collect();
    assert_eq!(headings, ["two", "sub"]);
    Ok(())
}

#[test]
fn a_link_to_an_element_that_does_not_exist_is_dropped_and_keeps_its_text() -> Outcome {
    let converted = convert(&page("<p>See <a href=\"#nowhere\">there</a>.</p>"))?;
    assert_eq!(only_text(&converted)?, "See there.\n");
    assert_eq!(
        kinds(&converted),
        [&WarningKind::LinkDropped("#nowhere".into())]
    );
    Ok(())
}

#[test]
fn a_link_to_nothing_is_dropped() -> Outcome {
    let converted = convert(&page("<p><a href=\"#\">top</a></p>"))?;
    assert_eq!(only_text(&converted)?, "top\n");
    assert_eq!(kinds(&converted), [&WarningKind::LinkDropped("#".into())]);
    Ok(())
}

#[test]
fn links_out_of_the_book_stay() -> Outcome {
    let converted = convert(&page(
        "<p><a href=\"http://example.org/a?b=1&amp;c=2\">http</a> <a href=\"https://example.org/\">https</a> \
         <a href=\"mailto:me@example.org\">mail</a></p>",
    ))?;
    assert_eq!(
        only_text(&converted)?,
        "[http](http://example.org/a?b=1\\&c=2) [https](https://example.org/) [mail](mailto:me@example.org)\n"
    );
    assert_eq!(converted.report.warnings, []);
    Ok(())
}

#[test]
fn a_relative_link_has_nothing_to_lead_to_and_is_dropped() -> Outcome {
    let converted = convert(&page(
        "<p>Read <a href=\"chapter2.html#x\">on</a> or <a href=\"javascript:go()\">go</a>.</p>",
    ))?;
    assert_eq!(only_text(&converted)?, "Read on or go.\n");
    assert_eq!(
        kinds(&converted),
        [
            &WarningKind::LinkDropped("chapter2.html#x".into()),
            &WarningKind::LinkDropped("javascript:go()".into())
        ]
    );
    Ok(())
}

#[test]
fn a_picture_in_a_data_address_is_stored() -> Outcome {
    let address = png_address(40, 30)?;
    let converted = convert(&page(&format!(
        "<p>Before.</p><p><img src=\"{address}\" alt=\"A red box\"></p>"
    )))?;
    let pictures = converted.book.pictures();
    assert_eq!(pictures.len(), 1);
    assert_eq!(pictures[0].path(), "img/0001.avif");
    assert_eq!(
        (pictures[0].size().width, pictures[0].size().height),
        (40, 30)
    );
    assert_eq!(
        only_text(&converted)?,
        "Before.\n\n![A red box](img/0001.avif)\n"
    );
    assert_eq!(converted.report.warnings, []);
    Ok(())
}

#[test]
fn the_same_picture_twice_is_stored_once() -> Outcome {
    let address = png_address(8, 8)?;
    let converted = convert(&page(&format!(
        "<p><img src=\"{address}\" alt=\"A\"></p><p><img src=\"{address}\" alt=\"B\"></p>"
    )))?;
    assert_eq!(converted.book.pictures().len(), 1);
    assert_eq!(
        only_text(&converted)?,
        "![A](img/0001.avif)\n\n![B](img/0001.avif)\n"
    );
    Ok(())
}

#[test]
fn a_picture_that_is_not_a_data_address_is_dropped_and_keeps_its_alternative_text() -> Outcome {
    let converted = convert(&page(
        "<p>Look: <img src=\"https://example.org/cat.png\" alt=\"a cat\"> and <img src=\"cat.png\" alt=\"local\"></p>",
    ))?;
    assert_eq!(converted.book.pictures(), []);
    assert_eq!(only_text(&converted)?, "Look: a cat and local\n");
    assert_eq!(
        kinds(&converted),
        [
            &WarningKind::PictureDropped("https://example.org/cat.png".into()),
            &WarningKind::PictureDropped("cat.png".into())
        ]
    );
    Ok(())
}

#[test]
fn data_that_is_not_a_picture_is_dropped() -> Outcome {
    let converted = convert(&page(
        "<p><img src=\"data:image/png;base64,AAAA\" alt=\"broken\"></p><p>Text.</p>",
    ))?;
    assert_eq!(converted.book.pictures(), []);
    assert_eq!(only_text(&converted)?, "broken\n\nText.\n");
    assert_eq!(kinds(&converted).len(), 1);
    assert!(matches!(
        kinds(&converted)[0],
        WarningKind::PictureDropped(_)
    ));
    Ok(())
}

#[test]
fn a_figure_with_a_caption_is_a_picture_with_a_title() -> Outcome {
    let address = png_address(10, 10)?;
    let converted = convert(&page(&format!(
        "<figure id=\"fig\"><img src=\"{address}\" alt=\"A box\"><figcaption>Figure <i>1</i>. A box.</figcaption></figure>\
         <p><a href=\"#fig\">The figure</a>.</p>"
    )))?;
    assert_eq!(
        only_text(&converted)?,
        "[fig]: #fig\n\n![A box](img/0001.avif \"Figure 1. A box.\")\n\n[The figure](ch0001.md#fig).\n"
    );
    Ok(())
}

#[test]
fn lists_keep_their_nesting_and_numbers() -> Outcome {
    let converted = convert(&page(
        "<ul><li>One<ul><li>Inner</li></ul></li><li>Two</li></ul><ol start=\"4\"><li>Four</li><li>Five</li></ol>",
    ))?;
    assert_eq!(
        only_text(&converted)?,
        "- One\n  - Inner\n- Two\n\n4. Four\n5. Five\n"
    );
    Ok(())
}

#[test]
fn a_block_quote_is_a_block_quote() -> Outcome {
    let converted = convert(&page(
        "<blockquote><p>First.</p><p>Second.</p></blockquote><p>After.</p>",
    ))?;
    assert_eq!(
        only_text(&converted)?,
        "> First.\n> \n> Second.\n\nAfter.\n"
    );
    Ok(())
}

#[test]
fn a_code_block_is_fenced_and_its_text_is_not_escaped() -> Outcome {
    let converted = convert(&page(
        "<pre><code class=\"language-rust\">let a = *b * c; // # not a heading\n</code></pre>",
    ))?;
    assert_eq!(
        only_text(&converted)?,
        "```rust\nlet a = *b * c; // # not a heading\n```\n"
    );
    Ok(())
}

#[test]
fn a_table_is_a_table() -> Outcome {
    let converted = convert(&page(
        "<table><tr><th>Name</th><th>Size</th></tr><tr><td>a</td><td>1</td></tr><tr><td>b</td><td>2</td></tr></table>",
    ))?;
    assert_eq!(
        only_text(&converted)?,
        "| Name | Size |\n| ---- | ---- |\n| a    | 1    |\n| b    | 2    |\n"
    );
    assert_eq!(converted.report.warnings, []);
    Ok(())
}

#[test]
fn a_table_without_a_header_row_has_its_first_row_as_the_header() -> Outcome {
    let converted = convert(&page(
        "<table><tr><td>a</td><td>b</td></tr><tr><td>1</td><td>2</td></tr></table>",
    ))?;
    assert_eq!(only_text(&converted)?, "| a | b |\n| - | - |\n| 1 | 2 |\n");
    assert_eq!(converted.report.warnings, []);
    Ok(())
}

#[test]
fn a_table_with_merged_cells_is_flattened_with_a_warning() -> Outcome {
    let converted = convert(&page(
        "<table><tr><th colspan=\"2\">Both</th></tr><tr><td>a</td><td>1</td></tr></table>",
    ))?;
    assert_eq!(
        only_text(&converted)?,
        "| Both |   |\n| ---- | - |\n| a    | 1 |\n"
    );
    assert_eq!(kinds(&converted), [&markup("table with merged cells")]);
    Ok(())
}

#[test]
fn a_note_reference_becomes_a_footnote_in_the_same_chapter() -> Outcome {
    let converted = convert(&page(
        "<h1>One</h1><p>A claim<a epub:type=\"noteref\" href=\"#n1\">1</a>.</p>\
         <h1>Two</h1><p>Another<a epub:type=\"noteref\" href=\"#n2\">2</a>.</p>\
         <aside epub:type=\"footnote\" id=\"n1\"><p>Proof one.</p></aside>\
         <aside epub:type=\"footnote\" id=\"n2\"><p>Proof two.</p><p>More.</p></aside>",
    ))?;
    assert_eq!(
        texts(&converted),
        [
            "# One\n\nA claim[^n1].\n\n[^n1]: Proof one.\n",
            "# Two\n\nAnother[^n2].\n\n[^n2]: Proof two.\n\n    More.\n"
        ]
    );
    assert_eq!(converted.report.warnings, []);
    Ok(())
}

#[test]
fn subscripts_and_superscripts_keep_their_text_and_are_reported() -> Outcome {
    let converted = convert(&page(
        "<p>H<sub>2</sub>O and E=mc<sup>2</sup> and x<sup>3</sup>.</p>",
    ))?;
    assert_eq!(only_text(&converted)?, "H2O and E=mc2 and x3.\n");
    assert_eq!(kinds(&converted), [&markup("sub"), &markup("sup")]);
    Ok(())
}

#[test]
fn scripts_and_styles_leave_nothing_behind() -> Outcome {
    let converted = convert(
        "<html lang=\"en\"><head><title>T</title><style>body { color: red }</style>\
         <script>var x = '<p>no</p>';</script></head><body><script>alert(1)</script>\
         <p>Text.</p><style>p { margin: 0 }</style><!-- a comment --><noscript>enable it</noscript></body></html>",
    )?;
    assert_eq!(only_text(&converted)?, "Text.\n");
    assert_eq!(converted.report.warnings, []);
    Ok(())
}

#[test]
fn characters_that_are_markdown_are_text() -> Outcome {
    let converted = convert(&page(
        "<p># not a heading</p><p>* not *a list*, _nor_ ~this~ [x](y) &lt;b&gt;tag&lt;/b&gt; 1. one | two</p>",
    ))?;
    assert_eq!(
        only_text(&converted)?,
        "\\# not a heading\n\n\\* not \\*a list\\*, \\_nor\\_ \\~this\\~ \\[x\\](y) \\<b>tag\\</b> 1. one | two\n"
    );
    assert_eq!(converted.report.warnings, []);
    Ok(())
}

#[test]
fn a_document_in_windows_1251_is_decoded_by_its_meta_charset() -> Outcome {
    let html = "<html><head><meta charset=\"windows-1251\"><title>Заголовок</title></head>\
        <body><p>Привет, мир!</p></body></html>";
    let bytes = windows_1251(html);
    let converted = html_converter().load_slice(&bytes)?;
    assert_eq!(only_text(&converted)?, "Привет, мир!\n");
    assert_eq!(converted.book.meta().title(), "Заголовок");
    Ok(())
}

#[test]
fn a_content_type_meta_declares_the_encoding_too() -> Outcome {
    let html = "<html><head><meta http-equiv=\"Content-Type\" content=\"text/html; charset=windows-1251\">\
        </head><body><p>Привет!</p></body></html>";
    let bytes = windows_1251(html);
    let converted = html_converter().load_slice(&bytes)?;
    assert_eq!(only_text(&converted)?, "Привет!\n");
    Ok(())
}

/// Windows-1251 bytes, written by hand: every Cyrillic letter is one byte.
fn windows_1251(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| match c {
            'А'..='я' => u8::try_from(u32::from(c) - u32::from('А') + 0xC0).unwrap_or(b'?'),
            'Ё' => 0xA8,
            'ё' => 0xB8,
            c => u8::try_from(u32::from(c)).unwrap_or(b'?'),
        })
        .collect()
}

#[test]
fn a_document_in_utf_16_with_a_byte_order_mark_is_decoded() -> Outcome {
    let html = page("<p>Привет, мир!</p>");
    let mut bytes = vec![0xff, 0xfe];
    for unit in html.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let converted = html_converter().load_slice(&bytes)?;
    assert_eq!(only_text(&converted)?, "Привет, мир!\n");
    let mut be = vec![0xfe, 0xff];
    for unit in html.encode_utf16() {
        be.extend_from_slice(&unit.to_be_bytes());
    }
    assert_eq!(
        only_text(&html_converter().load_slice(&be)?)?,
        "Привет, мир!\n"
    );
    Ok(())
}

#[test]
fn a_utf_8_byte_order_mark_is_not_text() -> Outcome {
    let mut bytes = b"\xef\xbb\xbf".to_vec();
    bytes.extend_from_slice(page("<p>Text.</p>").as_bytes());
    let converted = html_converter().load_slice(&bytes)?;
    assert_eq!(only_text(&converted)?, "Text.\n");
    Ok(())
}

#[test]
fn a_document_with_a_doctype_is_detected() -> Outcome {
    let converted = Converter::new().load_slice(b"<!DOCTYPE html>\n<p>Detected.</p>")?;
    assert_eq!(converted.report.format, Format::Html);
    assert_eq!(only_text(&converted)?, "Detected.\n");
    Ok(())
}

#[test]
fn a_document_that_starts_with_the_html_element_is_detected() -> Outcome {
    let converted = Converter::new().load_slice(b"  <HTML><body><p>Detected.</p></body></HTML>")?;
    assert_eq!(converted.report.format, Format::Html);
    assert_eq!(only_text(&converted)?, "Detected.\n");
    Ok(())
}

#[test]
fn an_xhtml_document_with_an_xml_declaration_is_detected_and_converted() -> Outcome {
    let xhtml = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
        <!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n\
        <html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"fr\"><head><title>Un titre</title></head>\
        <body><a id=\"top\"/><h1>Début</h1><p>Texte<br/>suite. <a href=\"#top\">Haut</a></p><div class=\"sep\"/>\
        <p>Fin.</p></body></html>";
    let converted = Converter::new().load_slice(xhtml.as_bytes())?;
    assert_eq!(converted.report.format, Format::Html);
    assert_eq!(
        only_text(&converted)?,
        "# Début\n\nTexte  \nsuite. [Haut](ch0001.md#d%C3%A9but)\n\nFin.\n"
    );
    assert_eq!(
        (
            converted.book.meta().title(),
            converted.book.meta().language()
        ),
        ("Un titre", "fr")
    );
    Ok(())
}

#[test]
fn an_empty_body_has_nothing_to_convert() {
    let result = convert(&page("  <script>x</script>\n"));
    assert!(matches!(result, Err(Error::Empty)));
    assert!(matches!(convert(""), Err(Error::Empty)));
    assert!(matches!(
        convert("<!DOCTYPE html><html></html>"),
        Err(Error::Empty)
    ));
}

#[test]
fn a_picture_over_the_resource_limit_is_refused() -> Outcome {
    let address = png_address(64, 64)?;
    let html = page(&format!("<p><img src=\"{address}\" alt=\"A\"></p>"));
    assert!(convert(&html).is_ok());

    let mut options = ConvertOptions::default();
    options.format = Some(Format::Html);
    options.limits.book.max_resource_bytes = 100;
    let result = Converter::with_options(options).load_slice(html.as_bytes());
    assert!(matches!(result, Err(Error::Limit(_))));
    Ok(())
}

#[test]
fn pictures_over_the_file_limit_together_are_refused() -> Outcome {
    let (a, b) = (png_address(32, 32)?, png_address(31, 31)?);
    let html = page(&format!(
        "<p><img src=\"{a}\" alt=\"A\"><img src=\"{b}\" alt=\"B\"></p>"
    ));
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Html);
    options.limits.book.max_file_bytes = 300;
    let result = Converter::with_options(options).load_slice(html.as_bytes());
    assert!(matches!(result, Err(Error::Limit(_))));
    Ok(())
}

#[test]
fn a_big_document_with_every_construct_leaves_no_raw_html_behind() -> Outcome {
    let address = png_address(4, 4)?;
    let html = page(&format!(
        "<h1 id=\"a\" onclick=\"x()\">One</h1><p style=\"color:red\"><span>s</span><font>f</font><small>m</small>\
         <u>u</u><x-custom>c</x-custom><img src=\"{address}\" alt=\"i\" onerror=\"e()\"></p>\
         <div><center><section><article>deep</article></section></center></div>\
         <dl><dt>t</dt><dd>d</dd></dl><form><input></form><iframe src=\"x\"></iframe><object data=\"y\"></object>\
         <table><tr><td><p>p</p><ul><li>l</li></ul></td></tr></table><template><b>t</b></template>"
    ));
    let converted = convert(&html)?;
    // Raw HTML would have been removed with a warning of its own.
    let unexpected: Vec<_> = kinds(&converted)
        .into_iter()
        .filter(|kind| matches!(kind, WarningKind::MarkupDropped(name) if name != "u"))
        .collect();
    assert!(unexpected.is_empty(), "{unexpected:?}");
    assert!(!only_text(&converted)?.contains('<'));
    Ok(())
}

#[test]
fn a_converted_document_is_written_and_read_back() -> Outcome {
    let html = page("<h1>One</h1><p>First.</p><h1>Two</h1><p>Second.</p>");
    let mut file = Vec::new();
    let report = html_converter().convert_slice(html.as_bytes(), &mut file)?;
    assert_eq!(report.format, Format::Html);
    let mut reader = BookReader::new(Cursor::new(file.clone()))?;
    let ids: Vec<_> = reader.chapters().map(|chapter| chapter.id).collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(reader.read_chapter(ids[1])?, "# Two\n\nSecond.\n");
    assert_eq!(Book::read(file.as_slice())?.chapters().len(), 2);
    Ok(())
}

#[test]
fn converting_twice_gives_the_same_book() -> Outcome {
    let html = page("<h1>One</h1><p id=\"x\">A <a href=\"#x\">link</a></p><h1>Two</h1>");
    assert_eq!(convert(&html)?, convert(&html)?);
    Ok(())
}
