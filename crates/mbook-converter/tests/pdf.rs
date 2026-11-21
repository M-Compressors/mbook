//! PDF, through the public API.
//!
//! Every source is assembled in memory by [`Pdf`], a small writer of PDF 1.4
//! files: objects, a cross-reference table with the byte offset of each,
//! a trailer. Text is set in the standard `Helvetica`, so a file needs no
//! font program.
#![cfg(feature = "pdf")]

use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder, RgbImage};
use mbook_converter::mbook_core::{Book, Toc, TocItem};
use mbook_converter::{ConvertOptions, Converted, Converter, Error, Format, WarningKind};
use std::fmt::Write as _;

type Outcome = Result<(), Box<dyn std::error::Error>>;

/// A writer of PDF files. Object `n` is the `n`th object added; ids are
/// known before the objects are, so objects may refer to each other.
#[derive(Default)]
struct Pdf {
    objects: Vec<Vec<u8>>,
}

impl Pdf {
    /// Adds an object and returns its number.
    fn add(&mut self, body: impl Into<Vec<u8>>) -> usize {
        self.objects.push(body.into());
        self.objects.len()
    }

    /// The bytes of the file: the header, the objects, the cross-reference
    /// table with the offset of each, and the trailer. `trailer` is the
    /// entries of the trailer dictionary besides `/Size` and `/Root`, which
    /// is object 1.
    fn finish(&self, trailer: &str) -> Vec<u8> {
        let mut file = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
        let mut offsets = Vec::new();
        for (index, body) in self.objects.iter().enumerate() {
            offsets.push(file.len());
            file.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
            file.extend_from_slice(body);
            file.extend_from_slice(b"\nendobj\n");
        }
        let xref = file.len();
        file.extend_from_slice(format!("xref\n0 {}\n", self.objects.len() + 1).as_bytes());
        file.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets {
            file.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        let size = self.objects.len() + 1;
        file.extend_from_slice(
            format!(
                "trailer\n<< /Size {size} /Root 1 0 R {trailer} >>\nstartxref\n{xref}\n%%EOF\n"
            )
            .as_bytes(),
        );
        file
    }
}

/// A stream object: its dictionary entries, then the data.
fn stream(entries: &str, data: &[u8]) -> Vec<u8> {
    let mut object = format!("<< {entries} /Length {} >>\nstream\n", data.len()).into_bytes();
    object.extend_from_slice(data);
    object.extend_from_slice(b"\nendstream");
    object
}

/// A JPEG file of a gradient.
fn jpeg(width: u32, height: u32) -> Result<Vec<u8>, image::ImageError> {
    let picture = RgbImage::from_fn(width, height, |x, y| {
        image::Rgb([
            u8::try_from(x * 255 / width).unwrap_or(255),
            u8::try_from(y * 255 / height).unwrap_or(255),
            128,
        ])
    });
    let mut bytes = Vec::new();
    JpegEncoder::new(&mut bytes).write_image(
        picture.as_raw(),
        width,
        height,
        ExtendedColorType::Rgb8,
    )?;
    Ok(bytes)
}

/// A page of a test document.
#[derive(Clone, Default)]
struct Page {
    /// Lines of text, each with the size of its type in points.
    lines: Vec<(u32, String)>,
    /// Whether the page shows the picture of the document.
    picture: bool,
    /// Whether the picture covers the page, as the scan of a page does.
    fills: bool,
}

impl Page {
    /// A page with a heading of 24 points and three lines of body text.
    fn chapter(title: &str) -> Self {
        Self::with_heading(24, title)
    }

    /// A page with a heading set in `size` points and three lines of text.
    fn with_heading(size: u32, title: &str) -> Self {
        let mut lines = vec![(size, title.to_owned())];
        for line in 1..=3 {
            lines.push((
                12,
                format!("Body line {line} of {title}, long enough to be text."),
            ));
        }
        Self {
            lines,
            ..Self::default()
        }
    }

    /// A page with only body text.
    fn plain(text: &str) -> Self {
        Self {
            lines: (0..3)
                .map(|line| (12, format!("{text} line {line}, long enough to be text.")))
                .collect(),
            ..Self::default()
        }
    }

    /// A page that shows the picture below its text.
    fn with_picture(mut self) -> Self {
        self.picture = true;
        self
    }

    /// A page of nothing but the picture over its whole area.
    fn scan() -> Self {
        Self {
            picture: true,
            fills: true,
            ..Self::default()
        }
    }

    /// The content stream of the page.
    fn content(&self, width: u32, height: u32) -> Vec<u8> {
        let mut content = String::new();
        if self.fills {
            let _ = writeln!(content, "q {width} 0 0 {height} 0 0 cm /Im1 Do Q");
        } else if self.picture {
            content.push_str("q 200 0 0 160 72 300 cm /Im1 Do Q\n");
        }
        let mut y = height.saturating_sub(72);
        for (size, text) in &self.lines {
            let _ = writeln!(content, "BT /F1 {size} Tf 72 {y} Td ({text}) Tj ET");
            y = y.saturating_sub(size + 14);
        }
        content.into_bytes()
    }
}

/// An entry of the outline of a test document.
struct Entry {
    title: &'static str,
    /// The index of the page it leads to.
    page: usize,
    children: Vec<Entry>,
}

impl Entry {
    fn new(title: &'static str, page: usize) -> Self {
        Self {
            title,
            page,
            children: Vec::new(),
        }
    }

    fn with(mut self, child: Entry) -> Self {
        self.children.push(child);
        self
    }
}

/// A test document.
struct Document {
    pages: Vec<Page>,
    /// The size of every page in points.
    size: (u32, u32),
    /// The entries of the `/Info` dictionary.
    info: Vec<(&'static str, &'static str)>,
    outline: Vec<Entry>,
    /// The size of the picture the pages show.
    picture: (u32, u32),
    /// Whether the file declares itself encrypted.
    encrypted: bool,
}

impl Document {
    fn new(pages: Vec<Page>) -> Self {
        Self {
            pages,
            size: (612, 792),
            info: Vec::new(),
            outline: Vec::new(),
            picture: (100, 80),
            encrypted: false,
        }
    }

    /// The file.
    fn build(&self) -> Result<Vec<u8>, image::ImageError> {
        let (width, height) = self.size;
        let mut pdf = Pdf::default();
        let count = self.pages.len();
        // Object numbers: 1 catalog, 2 page tree, 3 font, 4 picture, then a
        // page and its content for every page, then the rest.
        let page_id = |index: usize| 5 + 2 * index;
        let after_pages = 5 + 2 * count;
        let info_id = after_pages;
        let encrypt_id = after_pages + 1;
        let outline_root = after_pages + 2;

        let outlines = if self.outline.is_empty() {
            String::new()
        } else {
            format!("/Outlines {outline_root} 0 R")
        };
        pdf.add(format!("<< /Type /Catalog /Pages 2 0 R {outlines} >>"));
        let kids: Vec<String> = (0..count).map(|i| format!("{} 0 R", page_id(i))).collect();
        pdf.add(format!(
            "<< /Type /Pages /Kids [{}] /Count {count} >>",
            kids.join(" ")
        ));
        pdf.add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
        let (picture_width, picture_height) = self.picture;
        pdf.add(stream(
            &format!(
                "/Type /XObject /Subtype /Image /Width {picture_width} /Height {picture_height} \
                 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode"
            ),
            &jpeg(picture_width, picture_height)?,
        ));
        for (index, page) in self.pages.iter().enumerate() {
            let pictures = if page.picture {
                "/XObject << /Im1 4 0 R >>"
            } else {
                ""
            };
            pdf.add(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] \
                 /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> {pictures} >> >>",
                page_id(index) + 1
            ));
            pdf.add(stream("", &page.content(width, height)));
        }
        let mut info = String::from("<<");
        for (key, value) in &self.info {
            let _ = write!(info, " /{key} ({value})");
        }
        info.push_str(" >>");
        pdf.add(info);
        // The dictionary of an encryption nothing derives a key from: the
        // empty password does not open the file.
        let hash = "(01234567890123456789012345678901)";
        pdf.add(format!(
            "<< /Filter /Standard /V 1 /R 2 /Length 40 /P -1 /O {hash} /U {hash} >>"
        ));
        if !self.outline.is_empty() {
            self.add_outline(&mut pdf, outline_root, &page_id);
        }
        let mut trailer = format!("/Info {info_id} 0 R");
        if self.encrypted {
            let _ = write!(
                trailer,
                " /Encrypt {encrypt_id} 0 R /ID [(0123456789abcdef) (0123456789abcdef)]"
            );
        }
        Ok(pdf.finish(&trailer))
    }

    /// Adds the outline root and its items. The items of a level are
    /// numbered together, after the items of the level above.
    fn add_outline(&self, pdf: &mut Pdf, root: usize, page_id: &dyn Fn(usize) -> usize) {
        /// The objects of the items of one level and of those below them, with
        /// their numbers.
        fn level(
            entries: &[Entry],
            parent: usize,
            next_id: &mut usize,
            page_id: &dyn Fn(usize) -> usize,
        ) -> Vec<(usize, String)> {
            let ids: Vec<usize> = (*next_id..*next_id + entries.len()).collect();
            *next_id += entries.len();
            let mut written = Vec::new();
            for (index, entry) in entries.iter().enumerate() {
                let mut body = format!(
                    "<< /Title ({}) /Parent {parent} 0 R /Dest [{} 0 R /Fit]",
                    entry.title,
                    page_id(entry.page)
                );
                if let Some(before) = index.checked_sub(1).and_then(|before| ids.get(before)) {
                    let _ = write!(body, " /Prev {before} 0 R");
                }
                if let Some(after) = ids.get(index + 1) {
                    let _ = write!(body, " /Next {after} 0 R");
                }
                let id = ids[index];
                let below = level(&entry.children, id, next_id, page_id);
                if let (Some(first), Some(last)) = (
                    below
                        .iter()
                        .find(|(_, body)| body.contains(&format!("/Parent {id} 0 R"))),
                    below
                        .iter()
                        .rfind(|(_, body)| body.contains(&format!("/Parent {id} 0 R"))),
                ) {
                    let _ = write!(body, " /First {} 0 R /Last {} 0 R", first.0, last.0);
                }
                body.push_str(" >>");
                written.push((id, body));
                written.extend(below);
            }
            written
        }
        let mut next_id = root + 1;
        let mut items = level(&self.outline, root, &mut next_id, page_id);
        items.sort_by_key(|(id, _)| *id);
        pdf.add(format!(
            "<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {} >>",
            root + 1,
            root + self.outline.len(),
            items.len()
        ));
        for (_, body) in items {
            pdf.add(body);
        }
    }
}

fn pdf_converter() -> Converter {
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Pdf);
    Converter::with_options(options)
}

fn convert(document: &Document) -> Result<Converted, Box<dyn std::error::Error>> {
    Ok(pdf_converter().load_slice(&document.build()?)?)
}

/// The texts of the chapters of a book.
fn texts(converted: &Converted) -> Vec<&str> {
    converted
        .book
        .chapters()
        .iter()
        .map(mbook_converter::mbook_core::Chapter::text)
        .collect()
}

/// An item of a table of contents: level, title, chapter and anchor.
type Item<'a> = (u8, &'a str, &'a str, Option<&'a str>);

/// The items of an explicit table of contents.
fn items(converted: &Converted) -> Result<Vec<Item<'_>>, &'static str> {
    match converted.book.toc() {
        Toc::Explicit(items) => Ok(items
            .iter()
            .map(
                |TocItem {
                     level,
                     title,
                     chapter,
                     anchor,
                 }| {
                    (*level, title.as_str(), chapter.as_str(), anchor.as_deref())
                },
            )
            .collect()),
        Toc::FromHeadings { .. } => Err("the table of contents is not explicit"),
    }
}

/// Fails when a conversion lost markup.
fn assert_no_markup_dropped(converted: &Converted) {
    let dropped: Vec<_> = converted
        .report
        .warnings
        .iter()
        .filter(|warning| matches!(warning.kind, WarningKind::MarkupDropped(_)))
        .collect();
    assert!(dropped.is_empty(), "markup was dropped: {dropped:?}");
}

#[test]
fn a_text_document_becomes_a_chapter_with_its_metadata() -> Outcome {
    let mut document = Document::new(vec![Page::chapter("Alpha")]);
    document.info = vec![
        ("Title", "The Alpha Book"),
        ("Author", "Ann Lee; Bo Kim"),
        ("Keywords", "fiction, short stories; classics"),
        ("Subject", "About the first letter."),
        ("CreationDate", "D:19990101000000Z"),
    ];
    let converted = convert(&document)?;
    assert_eq!(
        texts(&converted),
        ["# Alpha\n\nBody line 1 of Alpha, long enough to be text. \
          Body line 2 of Alpha, long enough to be text. \
          Body line 3 of Alpha, long enough to be text."]
    );
    let meta = converted.book.meta();
    assert_eq!(meta.title(), "The Alpha Book");
    assert_eq!(meta.authors().collect::<Vec<_>>(), ["Ann Lee", "Bo Kim"]);
    assert_eq!(
        meta.genres().collect::<Vec<_>>(),
        ["fiction", "short stories", "classics"]
    );
    assert_eq!(meta.annotation(), Some("About the first letter."));
    assert_eq!(meta.language(), "und");
    assert_eq!(meta.date(), None);
    assert_eq!(converted.report.format, Format::Pdf);
    assert_no_markup_dropped(&converted);
    Ok(())
}

#[test]
fn an_author_without_a_semicolon_is_one_author() -> Outcome {
    let mut document = Document::new(vec![Page::chapter("Alpha")]);
    document.info = vec![("Author", "Ann Lee and Bo Kim")];
    let converted = convert(&document)?;
    assert_eq!(
        converted.book.meta().authors().collect::<Vec<_>>(),
        ["Ann Lee and Bo Kim"]
    );
    Ok(())
}

#[test]
fn the_first_level_of_the_outline_cuts_the_sections() -> Outcome {
    let mut document = Document::new(vec![
        Page::plain("Front matter"),
        Page::chapter("Alpha"),
        Page::plain("More Alpha"),
        Page::chapter("Beta"),
    ]);
    document.outline = vec![Entry::new("Alpha", 1), Entry::new("Beta", 3)];
    let converted = convert(&document)?;
    let texts = texts(&converted);
    assert_eq!(texts.len(), 3);
    assert!(texts[0].starts_with("Front matter line 0") && !texts[0].contains("Alpha"));
    assert!(texts[1].starts_with("# Alpha") && texts[1].contains("More Alpha line 2"));
    assert!(texts[2].starts_with("# Beta") && !texts[2].contains("Alpha"));
    assert_no_markup_dropped(&converted);
    Ok(())
}

#[test]
fn the_outline_becomes_the_contents_with_an_anchor_for_a_matching_heading() -> Outcome {
    let mut document = Document::new(vec![
        Page::chapter("Alpha"),
        Page {
            lines: vec![
                (24, "Beta".to_owned()),
                (12, "Body line of Beta, long enough to be text.".to_owned()),
                (12, "Body line of Beta, long enough to be text.".to_owned()),
                (18, "The Details".to_owned()),
                (12, "Detail line, long enough to be text.".to_owned()),
                (12, "Detail line, long enough to be text.".to_owned()),
            ],
            ..Page::default()
        },
    ]);
    document.outline = vec![
        Entry::new("Alpha", 0),
        Entry::new("Beta", 1)
            .with(Entry::new("  the   DETAILS ", 1))
            .with(Entry::new("No Such Heading", 1)),
    ];
    let converted = convert(&document)?;
    let texts = texts(&converted);
    assert_eq!(texts.len(), 2);
    // The entry leads to the anchor the heading has anyway: the mark the
    // front end put in front of it is not kept.
    assert!(texts[1].contains("\n## The Details"), "{}", texts[1]);
    assert!(!texts[1].contains("]: #"), "{}", texts[1]);
    assert_eq!(
        items(&converted)?,
        [
            (1, "Alpha", "ch0001.md", None),
            (1, "Beta", "ch0002.md", None),
            (2, "the   DETAILS", "ch0002.md", Some("the-details")),
            (2, "No Such Heading", "ch0002.md", Some("the-details")),
        ]
    );
    assert!(converted.report.warnings.iter().all(|warning| {
        !matches!(
            warning.kind,
            WarningKind::LinkDropped(_) | WarningKind::LinkRetargeted(_)
        )
    }));
    Ok(())
}

#[test]
fn a_document_without_an_outline_is_cut_at_its_top_level_headings() -> Outcome {
    let document = Document::new(vec![
        Page::plain("Preface"),
        Page::chapter("Alpha"),
        Page {
            lines: vec![
                (24, "Alpha Two".to_owned()),
                (
                    12,
                    "Body line of Alpha Two, long enough to be text.".to_owned(),
                ),
                (
                    12,
                    "Body line of Alpha Two, long enough to be text.".to_owned(),
                ),
                (18, "Alpha Sub".to_owned()),
                (12, "Sub line, long enough to be text.".to_owned()),
                (12, "Sub line, long enough to be text.".to_owned()),
            ],
            ..Page::default()
        },
        Page::chapter("Beta"),
    ]);
    let converted = convert(&document)?;
    let texts = texts(&converted);
    assert_eq!(texts.len(), 4, "{texts:?}");
    assert!(texts[0].starts_with("Preface line 0"), "{}", texts[0]);
    assert!(texts[1].starts_with("# Alpha\n\n"), "{}", texts[1]);
    assert!(texts[2].starts_with("# Alpha Two\n\n"), "{}", texts[2]);
    assert!(texts[2].contains("## Alpha Sub\n\n"), "{}", texts[2]);
    assert!(texts[3].starts_with("# Beta\n\n"), "{}", texts[3]);
    assert!(matches!(converted.book.toc(), Toc::FromHeadings { .. }));
    assert_no_markup_dropped(&converted);
    Ok(())
}

#[test]
fn a_document_with_neither_outline_nor_headings_is_one_chapter() -> Outcome {
    let document = Document::new(vec![Page::plain("First"), Page::plain("Second")]);
    let converted = convert(&document)?;
    let texts = texts(&converted);
    assert_eq!(texts.len(), 1);
    assert!(texts[0].contains("First line 0") && texts[0].contains("Second line 2"));
    Ok(())
}

#[test]
fn an_outline_entry_that_leads_nowhere_stays_in_the_section_before() -> Outcome {
    let mut document = Document::new(vec![Page::chapter("Alpha"), Page::chapter("Beta")]);
    document.outline = vec![Entry::new("Alpha", 0), Entry::new("Beta", 1)];
    // A destination given by name is one `unpdf` does not resolve to a page.
    let file =
        String::from_utf8_lossy(&document.build()?).replace("/Dest [7 0 R /Fit]", "/Dest (named)");
    let converted = pdf_converter().load_slice(file.as_bytes())?;
    assert_eq!(texts(&converted).len(), 1);
    assert_eq!(
        items(&converted)?,
        [
            (1, "Alpha", "ch0001.md", None),
            (1, "Beta", "ch0001.md", None)
        ]
    );
    Ok(())
}

#[test]
fn a_picture_of_a_text_document_is_stored_and_shown() -> Outcome {
    let document = Document::new(vec![
        Page::chapter("Alpha").with_picture(),
        Page::chapter("Beta"),
    ]);
    let converted = convert(&document)?;
    let pictures = converted.book.pictures();
    assert_eq!(pictures.len(), 1);
    assert_eq!(pictures[0].path(), "img/0001.avif");
    assert_eq!(
        (pictures[0].size().width, pictures[0].size().height),
        (100, 80)
    );
    let texts = texts(&converted);
    assert!(
        texts[0].trim_end().ends_with("\n\n![](img/0001.avif)"),
        "{}",
        texts[0]
    );
    assert!(!texts[1].contains("!["));
    assert!(converted.book.meta().cover().is_none());
    assert!(
        converted
            .report
            .warnings
            .iter()
            .all(|warning| !matches!(warning.kind, WarningKind::PictureDropped(_)))
    );
    Ok(())
}

/// A document of two scanned pages, each 300 by 400 points.
fn scan() -> Document {
    let mut document = Document::new(vec![Page::scan(), Page::scan()]);
    document.size = (300, 400);
    document.picture = (60, 80);
    document.info = vec![("Title", "A Scan")];
    document
}

#[test]
fn a_scanned_document_is_one_chapter_of_figures() -> Outcome {
    let converted = convert(&scan())?;
    // The first page is the cover, which the book stores under its own path.
    assert_eq!(
        texts(&converted),
        ["![](img/cover.avif)\n\n![](img/0001.avif)\n"]
    );
    let sizes: Vec<_> = converted
        .book
        .pictures()
        .iter()
        .map(|picture| (picture.size().width, picture.size().height))
        .collect();
    // 300 by 400 points at 150 dpi.
    assert_eq!(sizes, [(625, 833), (625, 833)]);
    assert!(converted.book.meta().cover().is_some());
    assert_eq!(converted.book.meta().title(), "A Scan");
    assert_eq!(converted.book.toc(), &Toc::default());
    assert_no_markup_dropped(&converted);
    Ok(())
}

#[test]
fn a_scanned_page_with_too_many_pixels_is_painted_smaller() -> Outcome {
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Pdf);
    options.limits.max_picture_pixels = 100_000;
    let converted = Converter::with_options(options).load_slice(&scan().build()?)?;
    let pictures = converted.book.pictures();
    assert_eq!(pictures.len(), 2);
    for picture in pictures {
        let size = picture.size();
        let pixels = u64::from(size.width) * u64::from(size.height);
        assert!(pixels <= 100_000 && pixels > 50_000, "{size:?}");
        // The page keeps its proportions.
        assert!(size.width * 4 >= size.height * 3 - 4 && size.width * 4 <= size.height * 3 + 4);
    }
    assert_eq!(texts(&converted).len(), 1);
    assert!(converted.book.meta().cover().is_some());
    Ok(())
}

#[test]
fn a_scanned_document_with_more_pages_than_resources_is_refused() {
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Pdf);
    options.limits.book.max_resources = 1;
    let result = scan()
        .build()
        .map(|file| Converter::with_options(options).load_slice(&file));
    assert!(matches!(result, Ok(Err(Error::Limit(_)))));
}

#[test]
fn a_page_that_cannot_fit_the_picture_limit_is_refused() {
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Pdf);
    options.limits.max_picture_pixels = 0;
    let result = scan()
        .build()
        .map(|file| Converter::with_options(options).load_slice(&file));
    assert!(matches!(result, Ok(Err(Error::Limit(_)))));
}

#[test]
fn painted_pages_beyond_the_size_of_the_book_are_refused() {
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Pdf);
    options.limits.book.max_file_bytes = 1000;
    let result = scan()
        .build()
        .map(|file| Converter::with_options(options).load_slice(&file));
    assert!(matches!(result, Ok(Err(Error::Limit(_)))));
}

#[test]
fn an_encrypted_document_is_protected() -> Outcome {
    let mut document = Document::new(vec![Page::chapter("Alpha")]);
    document.encrypted = true;
    let result = pdf_converter().load_slice(&document.build()?);
    assert!(matches!(result, Err(Error::Protected(Format::Pdf))));
    Ok(())
}

#[test]
fn bytes_that_are_not_a_pdf_are_a_source_error() {
    for bytes in [&b"not a pdf at all"[..], b"%PDF-1.4\nbroken", b""] {
        let result = pdf_converter().load_slice(bytes);
        assert!(
            matches!(
                result,
                Err(Error::Source {
                    format: Format::Pdf,
                    ..
                })
            ),
            "{result:?}"
        );
    }
}

#[test]
fn a_document_without_pages_has_nothing_to_convert() -> Outcome {
    let result = pdf_converter().load_slice(&Document::new(Vec::new()).build()?);
    assert!(matches!(result, Err(Error::Empty)), "{result:?}");
    Ok(())
}

#[test]
fn a_pdf_is_detected_from_its_content() -> Outcome {
    let file = Document::new(vec![Page::chapter("Alpha")]).build()?;
    let converted = Converter::new().load_slice(&file)?;
    assert_eq!(converted.report.format, Format::Pdf);
    assert_eq!(converted.book.chapters().len(), 1);
    Ok(())
}

#[test]
fn a_conversion_writes_an_mbook_file() -> Outcome {
    let file = Document::new(vec![Page::chapter("Alpha").with_picture()]).build()?;
    let mut written = Vec::new();
    let report = Converter::new().convert_slice(&file, &mut written)?;
    assert_eq!(report.format, Format::Pdf);
    let book = Book::read(written.as_slice())?;
    assert_eq!((book.chapters().len(), book.pictures().len()), (1, 1));
    Ok(())
}
