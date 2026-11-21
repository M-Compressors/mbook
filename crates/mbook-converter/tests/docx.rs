//! Word documents, through the public API.
//!
//! The documents are built in memory by [`Package`]: a minimal but real
//! `.docx` archive with the parts the cases need.
#![cfg(feature = "docx")]

use image::{ImageBuffer, ImageFormat, Rgb};
use markdown::mdast::Node;
use markdown::{ParseOptions, to_mdast};
use mbook_converter::mbook::{Book, Chapter, Picture, Size};
use mbook_converter::{ConvertOptions, Converted, Converter, Error, Format, WarningKind};
use std::error::Error as StdError;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

type Outcome = Result<(), Box<dyn StdError>>;

const WORD: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
    xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
    xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
    xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
    xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\" \
    xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\"";

const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// The styles a heading needs: `undoc` takes a paragraph for a heading when
/// its style has an outline level.
const STYLES: &str = "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
    <w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:pPr><w:outlineLvl w:val=\"0\"/></w:pPr></w:style>\
    <w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/><w:pPr><w:outlineLvl w:val=\"1\"/></w:pPr></w:style>\
    <w:style w:type=\"character\" w:styleId=\"VerbatimChar\"><w:name w:val=\"Verbatim Char\"/></w:style>\
    </w:styles>";

/// A bullet list (`numId` 1) and a decimal one (`numId` 2).
const NUMBERING: &str = "<w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
    <w:abstractNum w:abstractNumId=\"0\"><w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"-\"/></w:lvl></w:abstractNum>\
    <w:abstractNum w:abstractNumId=\"1\"><w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/></w:lvl></w:abstractNum>\
    <w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>\
    <w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num>\
    </w:numbering>";

/// A Word document being built: the XML of its body, the relationships the
/// body uses and the parts beyond the standard ones.
struct Package {
    body: String,
    relationships: String,
    parts: Vec<(String, Vec<u8>)>,
}

impl Package {
    /// A document whose body holds these block elements.
    fn new(body: &str) -> Self {
        Self {
            body: body.to_owned(),
            relationships: String::new(),
            parts: Vec::new(),
        }
    }

    /// Adds a relationship of the main part, the XML of one `Relationship`.
    fn relationship(mut self, xml: &str) -> Self {
        self.relationships.push_str(xml);
        self
    }

    /// Adds a part of the archive.
    fn part(mut self, name: &str, content: impl Into<Vec<u8>>) -> Self {
        self.parts.push((name.to_owned(), content.into()));
        self
    }

    /// Adds the core properties with these elements.
    fn core(self, elements: &str) -> Self {
        let xml = format!(
            "<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
             xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\">{elements}</cp:coreProperties>"
        );
        self.part("docProps/core.xml", xml)
    }

    /// Adds a picture as `word/media/<name>`, related as `id`.
    fn picture(self, id: &str, name: &str, content: Vec<u8>) -> Self {
        let relationship = format!(
            "<Relationship Id=\"{id}\" Type=\"{RELATIONSHIPS}/image\" Target=\"media/{name}\"/>"
        );
        self.relationship(&relationship)
            .part(&format!("word/media/{name}"), content)
    }

    /// The archive.
    fn build(&self) -> Result<Vec<u8>, Box<dyn StdError>> {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let document = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document {WORD}><w:body>{}</w:body></w:document>",
            self.body
        );
        let relationships = format!(
            "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{}</Relationships>",
            self.relationships
        );
        let standard: [(&str, &str); 6] = [
            (
                "[Content_Types].xml",
                "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
                 <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
                 <Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
                 </Types>",
            ),
            (
                "_rels/.rels",
                "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
                 <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
                 </Relationships>",
            ),
            ("word/document.xml", &document),
            ("word/_rels/document.xml.rels", &relationships),
            ("word/styles.xml", STYLES),
            ("word/numbering.xml", NUMBERING),
        ];
        for (name, content) in standard {
            archive.start_file(name, options)?;
            archive.write_all(content.as_bytes())?;
        }
        for (name, content) in &self.parts {
            archive.start_file(name.as_str(), options)?;
            archive.write_all(content)?;
        }
        Ok(archive.finish()?.into_inner())
    }
}

/// The XML of text, as the content of an element.
fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A paragraph of one plain run.
fn para(text: &str) -> String {
    format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        xml(text)
    )
}

/// A paragraph in the style of a heading of this level.
fn heading(level: u8, text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Heading{level}\"/></w:pPr><w:r><w:t>{}</w:t></w:r></w:p>",
        xml(text)
    )
}

/// A paragraph of a list: `numbering` is 1 for bullets and 2 for numbers.
fn item(numbering: u8, text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"{numbering}\"/></w:numPr></w:pPr>\
         <w:r><w:t>{}</w:t></w:r></w:p>",
        xml(text)
    )
}

/// A paragraph with a picture of this relationship.
fn figure(id: &str, description: &str) -> String {
    format!(
        "<w:p><w:r><w:drawing><wp:inline><wp:docPr id=\"1\" name=\"Picture\" descr=\"{description}\"/>\
         <a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed=\"{id}\"/></pic:blipFill></pic:pic>\
         </a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
    )
}

/// A paragraph that refers to a footnote.
fn note_reference(text: &str, id: u32) -> String {
    format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r><w:r><w:footnoteReference w:id=\"{id}\"/></w:r></w:p>",
        xml(text)
    )
}

/// A PNG of this size, with noise that does not compress.
fn png(width: u32, height: u32) -> Result<Vec<u8>, Box<dyn StdError>> {
    let mut state = 0x2545_f491_u32;
    let picture = ImageBuffer::from_fn(width, height, |_, _| {
        let mut next = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            state.to_be_bytes()[0]
        };
        Rgb([next(), next(), next()])
    });
    let mut out = Cursor::new(Vec::new());
    picture.write_to(&mut out, ImageFormat::Png)?;
    Ok(out.into_inner())
}

/// A converter that is told the source is a Word document.
fn converter() -> Converter {
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Docx);
    Converter::with_options(options)
}

/// Converts a document, naming its format.
fn convert(package: &Package) -> Result<Converted, Box<dyn StdError>> {
    Ok(converter().load_slice(&package.build()?)?)
}

/// The texts of the chapters of a book.
fn texts(book: &Book) -> Vec<&str> {
    book.chapters().iter().map(Chapter::text).collect()
}

/// The text a reader of Markdown sees: the text of its paragraphs.
fn rendered(markdown: &str) -> Result<String, Box<dyn StdError>> {
    fn visit(node: &Node, out: &mut String) {
        match node {
            Node::Text(text) => out.push_str(&text.value),
            Node::Html(html) => {
                out.push_str("<html:");
                out.push_str(&html.value);
                out.push('>');
            }
            Node::Emphasis(_) | Node::Strong(_) | Node::Link(_) | Node::Code(_) => {
                out.push_str("<markup>");
            }
            _ => {}
        }
        for child in node.children().map(Vec::as_slice).unwrap_or_default() {
            visit(child, out);
        }
    }
    let tree = to_mdast(markdown, &ParseOptions::gfm()).map_err(|message| message.to_string())?;
    let mut out = String::new();
    visit(&tree, &mut out);
    Ok(out)
}

#[test]
fn core_properties_become_metadata() -> Outcome {
    let package = Package::new(&para("Text.")).core(
        "<dc:title>The Example</dc:title><dc:creator>Ann Lee; Bo Ray</dc:creator>\
         <dc:subject>A subject</dc:subject><dc:description>A description</dc:description>\
         <cp:keywords>fiction, short; tale</cp:keywords><dc:language>uk-UA</dc:language>\
         <dcterms:created>2001-02-03T00:00:00Z</dcterms:created>",
    );
    let converted = convert(&package)?;
    let meta = converted.book.meta();
    assert_eq!(meta.title(), "The Example");
    assert_eq!(meta.language(), "uk-UA");
    assert_eq!(meta.authors().collect::<Vec<_>>(), ["Ann Lee", "Bo Ray"]);
    assert_eq!(
        meta.genres().collect::<Vec<_>>(),
        ["fiction", "short", "tale"]
    );
    assert_eq!(meta.annotation(), Some("A subject\n\nA description"));
    assert_eq!(meta.date(), None);
    assert_eq!(converted.report.format, Format::Docx);
    assert_eq!(converted.report.warnings, []);
    Ok(())
}

#[test]
fn a_document_without_core_properties_gets_stand_ins() -> Outcome {
    let converted = convert(&Package::new(&para("Text.")))?;
    let meta = converted.book.meta();
    assert_eq!((meta.title(), meta.language()), ("Untitled", "und"));
    let kinds: Vec<_> = converted.report.warnings.iter().map(|w| &w.kind).collect();
    assert_eq!(
        kinds,
        [
            &WarningKind::MetadataMissing("title".into()),
            &WarningKind::MetadataMissing("lang".into())
        ]
    );
    Ok(())
}

#[test]
fn bold_and_italic_runs_are_emphasis() -> Outcome {
    let body = "<w:p><w:r><w:t xml:space=\"preserve\">Plain </w:t></w:r>\
        <w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r>\
        <w:r><w:t xml:space=\"preserve\"> and </w:t></w:r>\
        <w:r><w:rPr><w:i/></w:rPr><w:t>italic</w:t></w:r><w:r><w:t>.</w:t></w:r></w:p>";
    let converted = convert(&Package::new(body))?;
    assert_eq!(texts(&converted.book), ["Plain **bold** and *italic*.\n"]);
    Ok(())
}

#[test]
fn paragraphs_are_separated_by_empty_lines() -> Outcome {
    let body = format!("{}{}", para("First."), para("Second."));
    let converted = convert(&Package::new(&body))?;
    assert_eq!(texts(&converted.book), ["First.\n\nSecond.\n"]);
    Ok(())
}

#[test]
fn top_level_headings_cut_the_document_into_chapters() -> Outcome {
    let body = [
        heading(1, "One"),
        para("First text."),
        heading(2, "One and a half"),
        para("Deeper text."),
        heading(1, "Two"),
        para("Second text."),
    ]
    .concat();
    let converted = convert(&Package::new(&body))?;
    assert_eq!(
        texts(&converted.book),
        [
            "# One\n\nFirst text.\n\n## One and a half\n\nDeeper text.\n",
            "# Two\n\nSecond text.\n"
        ]
    );
    Ok(())
}

#[test]
fn text_before_the_first_heading_is_a_chapter_of_its_own() -> Outcome {
    let body = [para("Preface."), heading(1, "One"), para("First text.")].concat();
    let converted = convert(&Package::new(&body))?;
    assert_eq!(
        texts(&converted.book),
        ["Preface.\n", "# One\n\nFirst text.\n"]
    );
    Ok(())
}

#[test]
fn a_document_without_headings_is_one_chapter() -> Outcome {
    let body = [para("First."), para("Second."), para("Third.")].concat();
    let converted = convert(&Package::new(&body))?;
    assert_eq!(texts(&converted.book), ["First.\n\nSecond.\n\nThird.\n"]);
    Ok(())
}

#[test]
fn a_document_with_only_deeper_headings_is_one_chapter() -> Outcome {
    let body = [heading(2, "Part"), para("Text."), heading(2, "Next")].concat();
    let converted = convert(&Package::new(&body))?;
    assert_eq!(texts(&converted.book), ["## Part\n\nText.\n\n## Next\n"]);
    Ok(())
}

#[test]
fn a_table_is_a_markdown_table() -> Outcome {
    let cell = |text: &str| format!("<w:tc><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>");
    let row = |a: &str, b: &str| format!("<w:tr>{}{}</w:tr>", cell(a), cell(b));
    let body = format!(
        "<w:tbl>{}{}</w:tbl>",
        row("Name", "Count"),
        row("Apples", "3")
    );
    let converted = convert(&Package::new(&body))?;
    assert_eq!(
        texts(&converted.book),
        ["| Name | Count |\n| --- | --- |\n| Apples | 3 |\n"]
    );
    assert_eq!(converted.report.warnings.len(), 2);
    Ok(())
}

#[test]
fn bulleted_and_numbered_items_are_lists() -> Outcome {
    let body = [
        item(1, "Apples"),
        item(1, "Pears"),
        para("Between."),
        item(2, "First"),
        item(2, "Second"),
    ]
    .concat();
    let converted = convert(&Package::new(&body))?;
    assert_eq!(
        texts(&converted.book),
        ["- Apples\n- Pears\n\nBetween.\n\n1. First\n2. Second\n"]
    );
    Ok(())
}

#[test]
fn an_external_hyperlink_stays_a_link() -> Outcome {
    let body = "<w:p><w:r><w:t xml:space=\"preserve\">See </w:t></w:r>\
        <w:hyperlink r:id=\"rIdLink\"><w:r><w:t>the site</w:t></w:r></w:hyperlink>\
        <w:r><w:t>.</w:t></w:r></w:p>";
    let package = Package::new(body).relationship(&format!(
        "<Relationship Id=\"rIdLink\" Type=\"{RELATIONSHIPS}/hyperlink\" Target=\"https://example.com/a?b=1\" TargetMode=\"External\"/>"
    ));
    let converted = convert(&package)?;
    assert_eq!(
        texts(&converted.book),
        ["See [the site](https://example.com/a?b=1).\n"]
    );
    Ok(())
}

#[test]
fn a_link_to_a_bookmark_is_plain_text() -> Outcome {
    let body = "<w:p><w:bookmarkStart w:id=\"0\" w:name=\"target\"/><w:r><w:t>Here</w:t></w:r><w:bookmarkEnd w:id=\"0\"/></w:p>\
        <w:p><w:hyperlink w:anchor=\"target\"><w:r><w:t>jump</w:t></w:r></w:hyperlink></w:p>";
    let converted = convert(&Package::new(body))?;
    assert_eq!(texts(&converted.book), ["Here\n\njump\n"]);
    Ok(())
}

#[test]
fn an_embedded_picture_is_stored_and_shown() -> Outcome {
    let package = Package::new(&[para("Before."), figure("rIdImg", "A cat")].concat()).picture(
        "rIdImg",
        "image1.png",
        png(24, 12)?,
    );
    let converted = convert(&package)?;
    let book = &converted.book;
    assert_eq!(book.pictures().len(), 1);
    assert_eq!(book.pictures()[0].path(), "img/0001.avif");
    assert_eq!(
        book.pictures()[0].size(),
        Size {
            width: 24,
            height: 12
        }
    );
    assert_eq!(texts(book), ["Before.\n\n![A cat](img/0001.avif)\n"]);
    assert_eq!(converted.report.warnings.len(), 2);
    Ok(())
}

#[test]
fn a_picture_the_text_does_not_show_is_left_out() -> Outcome {
    let package = Package::new(&para("Text.")).picture("rIdImg", "image1.png", png(8, 8)?);
    let converted = convert(&package)?;
    assert_eq!(converted.book.pictures().len(), 0);
    assert_eq!(texts(&converted.book), ["Text.\n"]);
    Ok(())
}

#[test]
fn pictures_of_one_file_name_stay_two_pictures() -> Outcome {
    let body = [figure("rIdA", "First"), figure("rIdB", "Second")].concat();
    let package = Package::new(&body)
        .relationship(&format!(
            "<Relationship Id=\"rIdA\" Type=\"{RELATIONSHIPS}/image\" Target=\"media/image1.png\"/>"
        ))
        .part("word/media/image1.png", png(10, 10)?)
        .relationship(&format!(
            "<Relationship Id=\"rIdB\" Type=\"{RELATIONSHIPS}/image\" Target=\"other/image1.png\"/>"
        ))
        .part("word/other/image1.png", png(20, 30)?);
    let converted = convert(&package)?;
    let sizes: Vec<_> = converted
        .book
        .pictures()
        .iter()
        .map(Picture::size)
        .collect();
    assert_eq!(
        sizes,
        [
            Size {
                width: 10,
                height: 10
            },
            Size {
                width: 20,
                height: 30
            }
        ]
    );
    assert_eq!(
        texts(&converted.book),
        ["![First](img/0001.avif)\n\n![Second](img/0002.avif)\n"]
    );
    Ok(())
}

#[test]
fn a_picture_with_an_unusual_file_name_is_found_again() -> Outcome {
    let package =
        Package::new(&figure("rIdImg", "Odd")).picture("rIdImg", "my 100% (odd).png", png(6, 6)?);
    let converted = convert(&package)?;
    assert_eq!(converted.book.pictures().len(), 1);
    assert_eq!(texts(&converted.book), ["![Odd](img/0001.avif)\n"]);
    Ok(())
}

#[test]
fn a_picture_that_cannot_be_decoded_is_dropped_with_a_warning() -> Outcome {
    let package = Package::new(&[para("Text."), figure("rIdImg", "Plan")].concat()).picture(
        "rIdImg",
        "plan.emf",
        b"not a picture".to_vec(),
    );
    let converted = convert(&package)?;
    assert_eq!(converted.book.pictures().len(), 0);
    assert_eq!(texts(&converted.book), ["Text.\n\nPlan\n"]);
    assert!(
        converted
            .report
            .warnings
            .iter()
            .any(|w| w.kind == WarningKind::PictureDropped("plan.emf".into()))
    );
    Ok(())
}

#[test]
fn markdown_characters_in_the_text_are_escaped() -> Outcome {
    let text = "*star* _under_ `tick` [link](x) <b>tag</b> a|b & c ~strike~";
    let body = [para(text), para("# not a heading"), para("1. not a list")].concat();
    let converted = convert(&Package::new(&body))?;
    let chapter = texts(&converted.book);
    assert_eq!(chapter.len(), 1);
    assert_eq!(
        rendered(chapter[0])?,
        format!("{text}# not a heading1. not a list")
    );
    assert_eq!(converted.book.chapters()[0].anchors(), []);
    Ok(())
}

#[test]
fn a_line_break_inside_a_paragraph_is_kept() -> Outcome {
    let body = "<w:p><w:r><w:t>Roses are red</w:t><w:br/><w:t>Violets are blue</w:t></w:r></w:p>";
    let converted = convert(&Package::new(body))?;
    assert_eq!(
        texts(&converted.book),
        ["Roses are red  \nViolets are blue\n"]
    );
    Ok(())
}

#[test]
fn underline_is_raw_html_that_the_converter_removes_with_a_warning() -> Outcome {
    let body = "<w:p><w:r><w:rPr><w:u w:val=\"single\"/></w:rPr><w:t>marked</w:t></w:r></w:p>";
    let converted = convert(&Package::new(body))?;
    assert_eq!(texts(&converted.book), ["marked\n"]);
    assert!(
        converted
            .report
            .warnings
            .iter()
            .any(|w| w.kind == WarningKind::MarkupDropped("u".into()))
    );
    Ok(())
}

#[test]
fn footnotes_go_to_the_chapter_that_refers_to_them() -> Outcome {
    let body = [
        heading(1, "One"),
        note_reference("First claim.", 2),
        heading(1, "Two"),
        note_reference("Second claim.", 3),
        note_reference("Third claim.", 2),
    ]
    .concat();
    let notes = "<w:footnotes xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
        <w:footnote w:type=\"separator\" w:id=\"0\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>\
        <w:footnote w:type=\"continuationSeparator\" w:id=\"1\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>\
        <w:footnote w:id=\"2\"><w:p><w:r><w:t>Source A.</w:t></w:r></w:p></w:footnote>\
        <w:footnote w:id=\"3\"><w:p><w:r><w:t>Source B.</w:t></w:r></w:p></w:footnote>\
        </w:footnotes>";
    let converted = convert(&Package::new(&body).part("word/footnotes.xml", notes))?;
    assert_eq!(
        texts(&converted.book),
        [
            "# One\n\nFirst claim.[^2]\n\n[^2]: Source A.\n",
            "# Two\n\nSecond claim.[^3]\n\nThird claim.[^2]\n\n[^2]: Source A.\n\n[^3]: Source B.\n"
        ]
    );
    Ok(())
}

#[test]
fn an_empty_document_has_nothing_to_convert() {
    let result = Package::new("")
        .build()
        .map_err(|error| error.to_string())
        .map(|bytes| converter().load_slice(&bytes));
    assert!(matches!(result, Ok(Err(Error::Empty))));
}

#[test]
fn a_document_is_detected_from_its_content() -> Outcome {
    let source = Package::new(&para("Detected.")).build()?;
    let converted = Converter::new().load_slice(&source)?;
    assert_eq!(converted.report.format, Format::Docx);
    assert_eq!(texts(&converted.book), ["Detected.\n"]);
    Ok(())
}

#[test]
fn a_reader_is_converted_like_a_slice_and_written_as_an_mbook_file() -> Outcome {
    let source = Package::new(&[heading(1, "One"), para("Text.")].concat()).build()?;
    let mut file = Vec::new();
    let report = Converter::new().convert(source.as_slice(), &mut file)?;
    assert_eq!(report.format, Format::Docx);
    let book = Book::read(file.as_slice())?;
    assert_eq!(book.chapters()[0].text(), "# One\n\nText.\n");
    Ok(())
}

/// An OLE container (the container of an encrypted document and of a
/// document of Word 97 to 2003) with one empty stream of this name.
fn ole_container(stream: &str) -> Vec<u8> {
    const NO_SECTOR: u32 = 0xFFFF_FFFF;
    const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
    const FAT_SECTOR: u32 = 0xFFFF_FFFD;
    fn put(out: &mut [u8], at: usize, bytes: &[u8]) {
        out[at..at + bytes.len()].copy_from_slice(bytes);
    }
    let mut header = vec![0_u8; 512];
    put(
        &mut header,
        0,
        &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1],
    );
    put(&mut header, 24, &0x003E_u16.to_le_bytes());
    put(&mut header, 26, &3_u16.to_le_bytes());
    put(&mut header, 28, &0xFFFE_u16.to_le_bytes());
    put(&mut header, 30, &9_u16.to_le_bytes());
    put(&mut header, 32, &6_u16.to_le_bytes());
    put(&mut header, 44, &1_u32.to_le_bytes());
    put(&mut header, 48, &1_u32.to_le_bytes());
    put(&mut header, 56, &4096_u32.to_le_bytes());
    put(&mut header, 60, &END_OF_CHAIN.to_le_bytes());
    put(&mut header, 68, &END_OF_CHAIN.to_le_bytes());
    for index in 0..109_usize {
        let sector = if index == 0 { 0 } else { NO_SECTOR };
        put(&mut header, 76 + 4 * index, &sector.to_le_bytes());
    }

    let mut allocation = vec![0_u8; 512];
    for index in 0..128_usize {
        let entry = match index {
            0 => FAT_SECTOR,
            1 => END_OF_CHAIN,
            _ => NO_SECTOR,
        };
        put(&mut allocation, 4 * index, &entry.to_le_bytes());
    }

    let mut directory = vec![0_u8; 512];
    let mut entry = |index: usize, name: &str, kind: u8, child: u32| {
        let at = 128 * index;
        let units: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        put(&mut directory, at, &units);
        let length = u16::try_from(units.len() + 2).unwrap_or_default();
        put(&mut directory, at + 64, &length.to_le_bytes());
        directory[at + 66] = kind;
        directory[at + 67] = 1;
        put(&mut directory, at + 68, &NO_SECTOR.to_le_bytes());
        put(&mut directory, at + 72, &NO_SECTOR.to_le_bytes());
        put(&mut directory, at + 76, &child.to_le_bytes());
        put(&mut directory, at + 116, &END_OF_CHAIN.to_le_bytes());
    };
    entry(0, "Root Entry", 5, 1);
    entry(1, stream, 2, NO_SECTOR);
    entry(2, "", 0, NO_SECTOR);
    entry(3, "", 0, NO_SECTOR);
    [header, allocation, directory].concat()
}

#[test]
fn a_password_protected_document_is_protected() {
    let result = converter().load_slice(&ole_container("EncryptedPackage"));
    assert!(matches!(result, Err(Error::Protected(Format::Docx))));
}

#[test]
fn a_document_of_word_97_is_a_source_error() {
    let result = converter().load_slice(&ole_container("WordDocument"));
    assert!(matches!(
        result,
        Err(Error::Source {
            format: Format::Docx,
            ..
        })
    ));
}

#[test]
fn an_ole_container_that_cannot_be_read_is_a_source_error() {
    let mut container = ole_container("EncryptedPackage");
    container.truncate(300);
    let result = converter().load_slice(&container);
    assert!(matches!(result, Err(Error::Source { .. })));
}

#[test]
fn an_archive_that_is_not_a_document_is_a_source_error() -> Outcome {
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    archive.start_file("notes.txt", SimpleFileOptions::default())?;
    archive.write_all(b"just a file")?;
    let source = archive.finish()?.into_inner();
    let result = converter().load_slice(&source);
    let Err(Error::Source { format, source }) = result else {
        return Err("a source error was expected".into());
    };
    assert_eq!(format, Format::Docx);
    assert!(source.to_string().contains("word/document.xml"));
    Ok(())
}

#[test]
fn bytes_that_are_no_archive_are_a_source_error() {
    let result = converter().load_slice(b"These bytes are nothing like a document at all.");
    assert!(matches!(
        result,
        Err(Error::Source {
            format: Format::Docx,
            ..
        })
    ));
}

#[test]
fn a_document_part_that_is_not_xml_text_is_a_source_error() -> Outcome {
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    archive.start_file("word/document.xml", SimpleFileOptions::default())?;
    archive.write_all(&[0xFF, 0xFE, 0xFD, 0xFC, 0xFB])?;
    let source = archive.finish()?.into_inner();
    let result = converter().load_slice(&source);
    assert!(matches!(result, Err(Error::Source { .. })));
    Ok(())
}

#[test]
fn a_picture_above_the_limit_is_refused() -> Outcome {
    let package = Package::new(&figure("rIdImg", "Big")).picture("rIdImg", "big.png", png(64, 64)?);
    let source = package.build()?;
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Docx);
    options.limits.book.max_resource_bytes = 4096;
    let result = Converter::with_options(options).load_slice(&source);
    assert!(matches!(result, Err(Error::Limit(_))));

    options.limits.book.max_resource_bytes = 1 << 20;
    assert!(Converter::with_options(options).load_slice(&source).is_ok());
    Ok(())
}

#[test]
fn a_document_that_unpacks_to_more_than_the_file_limit_is_refused() -> Outcome {
    let source = Package::new(&para("Text.")).build()?;
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Docx);
    options.limits.book.max_file_bytes = 1000;
    let result = Converter::with_options(options).load_slice(&source);
    assert!(matches!(result, Err(Error::Limit(_))));
    Ok(())
}

#[test]
fn a_document_in_an_archive_without_core_properties_has_no_language() -> Outcome {
    let converted = convert(&Package::new(&para("Text.")).core("<dc:title>T</dc:title>"))?;
    assert_eq!(converted.book.meta().language(), "und");
    Ok(())
}

/// A table of one row whose cells hold these block elements.
fn table(cells: &[&str]) -> String {
    let cells = cells.join("</w:tc><w:tc>");
    format!("<w:tbl><w:tr><w:tc>{cells}</w:tc></w:tr></w:tbl>")
}

#[test]
fn a_literal_tilde_is_escaped_everywhere_in_the_text() -> Outcome {
    let inner = table(&[&para("~in~ nested")]);
    let body = [
        heading(1, "~Head~"),
        para("about ~5 to ~10 and ~~two~~"),
        table(&[&para("~cell~"), &format!("{inner}<w:p/>")]),
        note_reference("Noted.", 2),
    ]
    .concat();
    let notes = "<w:footnotes xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
        <w:footnote w:id=\"2\"><w:p><w:r><w:t>~note~</w:t></w:r></w:p></w:footnote></w:footnotes>";
    let converted = convert(&Package::new(&body).part("word/footnotes.xml", notes))?;
    let chapter = texts(&converted.book);
    assert_eq!(chapter.len(), 1);
    let tree = to_mdast(chapter[0], &ParseOptions::gfm()).map_err(|message| message.to_string())?;
    let mut struck = 0;
    count_struck(&tree, &mut struck);
    assert_eq!(struck, 0);
    assert!(chapter[0].contains("# \\~Head\\~\n"));
    assert!(chapter[0].contains("about \\~5 to \\~10 and \\~\\~two\\~\\~"));
    assert!(chapter[0].contains("[^2]: \\~note\\~"));
    assert!(chapter[0].contains("| \\~cell\\~ |"));
    assert!(chapter[0].contains("\\~in\\~ nested"));
    Ok(())
}

/// Counts the nodes of strikethrough.
fn count_struck(node: &Node, count: &mut usize) {
    if matches!(node, Node::Delete(_)) {
        *count += 1;
    }
    for child in node.children().map(Vec::as_slice).unwrap_or_default() {
        count_struck(child, count);
    }
}

#[test]
fn the_stand_in_character_of_the_converter_in_a_source_is_replaced() -> Outcome {
    let converted = convert(&Package::new(&para("a\u{FDD0}b ~c")))?;
    assert_eq!(texts(&converted.book), ["a\u{FFFD}b \\~c\n"]);
    Ok(())
}

#[test]
fn code_is_written_as_it_is() -> Outcome {
    let body =
        "<w:p><w:r><w:rPr><w:rStyle w:val=\"VerbatimChar\"/></w:rPr><w:t>a ~ b</w:t></w:r></w:p>";
    let converted = convert(&Package::new(body))?;
    assert_eq!(texts(&converted.book), ["`a ~ b`\n"]);
    Ok(())
}

#[test]
fn the_svg_original_of_a_picture_is_not_a_picture_of_its_own() -> Outcome {
    let body = "<w:p><w:r><w:drawing><wp:inline><wp:docPr id=\"1\" name=\"Picture\" descr=\"Logo\"/>\
        <a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed=\"rIdPng\"><a:extLst>\
        <a:ext uri=\"{96DAC541-7B7A-43D3-8B79-37D633B846F1}\"><asvg:svgBlip r:embed=\"rIdSvg\"/></a:ext>\
        </a:extLst></a:blip></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>";
    let package = Package::new(body)
        .picture("rIdPng", "logo.png", png(9, 9)?)
        .picture("rIdSvg", "logo.svg", b"<svg/>".to_vec());
    let converted = convert(&package)?;
    assert_eq!(converted.book.pictures().len(), 1);
    assert_eq!(texts(&converted.book), ["![Logo](img/0001.avif)\n"]);
    assert_eq!(converted.report.warnings.len(), 2);
    Ok(())
}
