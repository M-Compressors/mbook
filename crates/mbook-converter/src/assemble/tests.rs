//! The rules of "Assembling the book", on drafts written by hand.

use super::{Destination, Layout, Target, assemble, clean};
use crate::draft::{Draft, DraftMeta, Section};
use crate::error::Error;
use crate::markdown as md;
use crate::options::ConvertOptions;
use crate::report::WarningKind;
use mbook::Book;
use std::collections::HashMap;

fn section(name: &str, markdown: &str) -> Section {
    Section {
        name: name.to_owned(),
        markdown: markdown.to_owned(),
    }
}

/// A draft with a title and these sections.
fn draft(sections: &[(&str, &str)]) -> Draft {
    Draft {
        meta: DraftMeta {
            title: Some("The Example".to_owned()),
            languages: vec!["en".to_owned()],
            ..DraftMeta::default()
        },
        sections: sections
            .iter()
            .map(|(name, markdown)| section(name, markdown))
            .collect(),
        ..Draft::default()
    }
}

fn assembled(draft: Draft) -> (Book, Vec<WarningKind>) {
    let (book, warnings) = assemble(draft, b"source", &ConvertOptions::default()).unwrap();
    (book, warnings.into_iter().map(|w| w.kind).collect())
}

/// The texts of the chapters of the book a draft becomes.
fn chapters(sections: &[(&str, &str)]) -> (Vec<String>, Vec<WarningKind>) {
    let (book, warnings) = assembled(draft(sections));
    let texts = book.chapters().iter().map(|c| c.text().to_owned());
    (texts.collect(), warnings)
}

#[test]
fn every_section_becomes_a_chapter_with_a_numbered_path() {
    let (book, warnings) = assembled(draft(&[("a", "# One\n"), ("b", "# Two\n")]));
    let paths: Vec<_> = book.chapters().iter().map(mbook::Chapter::path).collect();
    assert_eq!((paths, warnings), (vec!["ch0001.md", "ch0002.md"], vec![]));
}

#[test]
fn a_section_without_text_produces_no_chapter() {
    let (texts, _) = chapters(&[("a", "  \n\n"), ("b", "# Two\n")]);
    assert_eq!(texts, ["# Two\n"]);
}

#[test]
fn a_link_to_a_section_leads_to_its_chapter() {
    let (texts, warnings) = chapters(&[
        ("a.xhtml", "See [the second](b%20c.xhtml \"A title\").\n"),
        ("b c.xhtml", "# Two\n"),
    ]);
    assert_eq!(texts[0], "See [the second](ch0002.md \"A title\").\n");
    assert_eq!(warnings, []);
}

#[test]
fn a_link_to_a_mark_keeps_the_mark_as_an_anchor_definition() {
    let (book, warnings) = assembled(draft(&[
        ("a", "See [the remark](b#remark).\n"),
        ("b", "# Two\n\n[remark]: #remark\n\nA remark.\n"),
    ]));
    assert_eq!(
        book.chapters()[0].text(),
        "See [the remark](ch0002.md#remark).\n"
    );
    assert_eq!(
        book.chapters()[1].text(),
        "# Two\n\n[remark]: #remark\n\nA remark.\n"
    );
    let names: Vec<_> = book.chapters()[1]
        .extra_anchors()
        .iter()
        .map(|anchor| anchor.name.as_str())
        .collect();
    assert_eq!((names, warnings), (vec!["remark"], vec![]));
}

#[test]
fn a_mark_no_link_leads_to_is_removed() {
    let (texts, _) = chapters(&[(
        "a",
        "# One\n\n[unused]: #unused\n\nText.\n\n[last]: #last\n",
    )]);
    assert_eq!(texts, ["# One\n\nText.\n\n"]);
}

#[test]
fn a_link_inside_its_own_section_needs_no_section_name() {
    let (texts, _) = chapters(&[("a", "[Down](#end).\n\n[end]: #end\n\nThe end.\n")]);
    assert_eq!(
        texts,
        ["[Down](ch0001.md#end).\n\n[end]: #end\n\nThe end.\n"]
    );
}

#[test]
fn a_link_to_a_mark_in_front_of_a_heading_leads_to_the_heading() {
    let (book, warnings) = assembled(draft(&[
        ("a", "See [the part](b#sec-2) and [the same](b#also).\n"),
        (
            "b",
            "# One\n\n[sec-2]: #sec-2\n\n[also]: #also\n\n## The Second Part\n\nText.\n",
        ),
    ]));
    assert_eq!(
        book.chapters()[0].text(),
        "See [the part](ch0002.md#the-second-part) and [the same](ch0002.md#the-second-part).\n"
    );
    assert_eq!(
        book.chapters()[1].text(),
        "# One\n\n## The Second Part\n\nText.\n"
    );
    assert_eq!(book.chapters()[1].extra_anchors(), []);
    assert_eq!(warnings, []);
}

#[test]
fn a_mark_named_like_the_heading_it_stands_before_is_that_heading() {
    let (texts, _) = chapters(&[("a", "[intro]: #intro\n\n# Intro\n\n[Back](#intro).\n")]);
    assert_eq!(texts, ["# Intro\n\n[Back](ch0001.md#intro).\n"]);
}

#[test]
fn a_mark_named_like_a_heading_gets_another_name() {
    let (texts, _) = chapters(&[(
        "a",
        "# Intro\n\n[intro]: #intro\n\nA paragraph. [Back](#intro).\n",
    )]);
    assert_eq!(
        texts,
        ["# Intro\n\n[intro-2]: #intro-2\n\nA paragraph. [Back](ch0001.md#intro-2).\n"]
    );
}

#[test]
fn the_first_of_two_marks_with_one_name_is_the_target() {
    let (texts, _) = chapters(&[("a", "[x]: #x\n\nFirst.\n\n[x]: #x\n\nSecond. [Up](#x).\n")]);
    assert_eq!(texts, ["[x]: #x\n\nFirst.\n\nSecond. [Up](ch0001.md#x).\n"]);
}

#[test]
fn a_link_that_leads_nowhere_becomes_its_text() {
    let (texts, warnings) = chapters(&[(
        "a",
        "See [*nine*](ch9), [a mark](#gone) and [a file](file:///etc/passwd).\n",
    )]);
    assert_eq!(texts, ["See *nine*, a mark and a file.\n"]);
    assert_eq!(
        warnings,
        [
            WarningKind::LinkDropped("ch9".into()),
            WarningKind::LinkDropped("#gone".into()),
            WarningKind::LinkDropped("file:///etc/passwd".into())
        ]
    );
}

#[test]
fn links_out_of_the_book_stay_as_they_are() {
    let text = "[A site](https://example.org/a?b=c \"T\"), <https://example.org>, www.example.org and <mailto:a@example.org>.\n";
    let (texts, warnings) = chapters(&[("a", text)]);
    assert_eq!((texts, warnings), (vec![text.to_owned()], vec![]));
}

#[test]
fn an_address_with_another_scheme_is_written_out() {
    let (texts, warnings) = chapters(&[("a", "Get it from <ftp://example.org/a_b>.\n")]);
    assert_eq!(texts, ["Get it from ftp://example.org/a\\_b.\n"]);
    assert_eq!(
        warnings,
        [WarningKind::LinkDropped("ftp://example.org/a_b".into())]
    );
}

#[test]
fn a_link_to_a_section_without_text_is_dropped() {
    let (texts, warnings) = chapters(&[("a", "[Empty](b).\n"), ("b", "\n")]);
    assert_eq!(texts, ["Empty.\n"]);
    assert_eq!(warnings, [WarningKind::LinkDropped("b".into())]);
}

#[test]
fn reference_links_follow_their_definitions() {
    let (texts, warnings) = chapters(&[
        (
            "a",
            "[Two][two], [nowhere][gone] and [out][site].\n\n[two]: b \"Second\"\n[gone]: ch9\n[site]: https://example.org\n",
        ),
        ("b", "# Two\n"),
    ]);
    assert_eq!(
        texts[0],
        "[Two][two], nowhere and [out][site].\n\n[two]: ch0002.md \"Second\"\n[site]: https://example.org\n"
    );
    assert_eq!(warnings, [WarningKind::LinkDropped("ch9".into())]);
}

#[test]
fn a_reference_through_a_mark_keeps_the_mark() {
    let (texts, _) = chapters(&[("a", "See [the line][here].\n\n[here]: #here\n\nA line.\n")]);
    assert_eq!(
        texts,
        ["See [the line][here].\n\n[here]: #here\n\nA line.\n"]
    );
}

#[test]
fn inline_html_is_removed_and_its_text_kept() {
    let (texts, warnings) = chapters(&[(
        "a",
        "H<sub>2</sub>O and E = mc<sup>2</sup>, <sub>again</sub>.<!-- note -->\n",
    )]);
    assert_eq!(texts, ["H2O and E = mc2, again.\n"]);
    assert_eq!(
        warnings,
        [
            WarningKind::MarkupDropped("sub".into()),
            WarningKind::MarkupDropped("sup".into()),
            WarningKind::MarkupDropped("comment".into())
        ]
    );
}

#[test]
fn a_line_break_of_html_becomes_one_of_markdown() {
    let (texts, warnings) = chapters(&[(
        "a",
        "One<br>two<br/>\nthree<br>\n\n# A<br>heading\n\nLast<br>",
    )]);
    assert_eq!(texts, ["One\\\ntwo\\\nthree\n\n# A heading\n\nLast"]);
    assert_eq!(warnings, []);
}

#[test]
fn a_block_of_html_is_removed() {
    let (texts, warnings) = chapters(&[(
        "a",
        "Before.\n\n<div class=\"x\">\n<p>Inside</p>\n</div>\n\nAfter.\n",
    )]);
    assert_eq!(texts, ["Before.\n\nAfter.\n"]);
    assert_eq!(warnings, [WarningKind::MarkupDropped("div".into())]);
}

#[test]
fn html_inside_a_quote_keeps_the_quote_intact() {
    let (texts, _) = chapters(&[("a", "> <div>x</div>\n>\n> Quoted.\n")]);
    assert_eq!(texts, ["> \n>\n> Quoted.\n"]);
}

#[test]
fn footnotes_pass_through() {
    let text = "A claim[^1].\n\n[^1]: Its source.\n";
    assert_eq!(chapters(&[("a", text)]).0, [text]);
}

#[test]
fn text_is_cleaned_before_it_is_stored() {
    assert_eq!(
        clean("\u{feff}Line one\r\nline\u{1b} two\rcafe\u{301}\ttab\u{0}"),
        "Line one\nline two\ncaf\u{e9}\ttab"
    );
}

#[test]
fn a_long_section_becomes_several_chapters_and_links_find_their_part() {
    let filler = "word ".repeat(40_000);
    let long = format!("# One\n\n{filler}\n\n# Two\n\n[target]: #target\n\nHere.\n\n{filler}\n");
    let (book, _) = assembled(draft(&[
        ("a", "[Go](b#target) and [start](b).\n"),
        ("b", &long),
    ]));
    let paths: Vec<_> = book.chapters().iter().map(mbook::Chapter::path).collect();
    assert_eq!(paths, ["ch0001.md", "ch0002.md", "ch0003.md"]);
    assert_eq!(
        book.chapters()[0].text(),
        "[Go](ch0003.md#target) and [start](ch0002.md).\n"
    );
}

#[test]
fn a_draft_without_text_is_empty() {
    let result = assemble(draft(&[("a", " ")]), b"", &ConvertOptions::default());
    assert!(matches!(result, Err(Error::Empty)));
}

#[test]
fn the_same_source_gets_the_same_identifier() {
    let id = |source: &[u8]| {
        let (book, _) = assemble(
            draft(&[("a", "Text.\n")]),
            source,
            &ConvertOptions::default(),
        )
        .unwrap();
        book.meta().id().to_owned()
    };
    assert_eq!(id(b"one"), id(b"one"));
    assert_ne!(id(b"one"), id(b"two"));
}

#[test]
fn a_book_without_a_title_is_named_by_its_first_top_level_heading() {
    let mut untitled = draft(&[
        ("a", "## Preface\n\nText.\n"),
        ("b", "# The *Real* Title\n"),
    ]);
    untitled.meta.title = None;
    let (book, warnings) = assembled(untitled);
    assert_eq!(book.meta().title(), "The Real Title");
    assert_eq!(warnings, []);
}

#[test]
fn code_in_the_first_heading_is_part_of_the_name_of_the_book() {
    let mut untitled = draft(&[("a", "# The `mbook` Format\n")]);
    untitled.meta.title = None;
    let (book, _) = assembled(untitled);
    assert_eq!(book.meta().title(), "The mbook Format");
}

#[test]
fn warnings_of_the_front_end_come_first() {
    let mut with_warning = draft(&[("a", "<i>x</i>\n")]);
    with_warning.warnings.push(crate::report::Warning::new(
        WarningKind::MarkupDropped("table with merged cells".into()),
        "a",
    ));
    let (_, warnings) = assembled(with_warning);
    assert_eq!(
        warnings,
        [
            WarningKind::MarkupDropped("table with merged cells".into()),
            WarningKind::MarkupDropped("i".into())
        ]
    );
}

#[test]
fn warnings_name_the_section_they_come_from() {
    let (_, warnings) = assemble(
        draft(&[("ch1.xhtml", "[x](gone)\n")]),
        b"",
        &ConvertOptions::default(),
    )
    .unwrap();
    assert_eq!(warnings[0].location.as_deref(), Some("ch1.xhtml"));
}

fn layout<'a>(names: &[&'a str]) -> Layout<'a> {
    Layout {
        sections: names.iter().copied().zip(0..).collect(),
        first_part: Vec::new(),
        parts: Vec::new(),
        marks: HashMap::new(),
    }
}

#[test]
fn destinations_are_told_apart() {
    let layout = layout(&["a", "b c"]);
    let internal = |section, mark: Option<&str>| {
        Destination::Internal(Target {
            section,
            mark: mark.map(str::to_owned),
        })
    };
    let classified = [
        "https://example.org",
        "MAILTO:a@example.org",
        "javascript:alert(1)",
        "b%20c",
        "a#x%20y",
        "#top",
        "",
        "missing#x",
        "a#%ff",
        "%ff",
    ]
    .map(|destination| layout.classify(1, destination));
    assert_eq!(
        classified,
        [
            Destination::External,
            Destination::External,
            Destination::Forbidden,
            internal(1, None),
            internal(0, Some("x y")),
            internal(1, Some("top")),
            internal(1, None),
            Destination::Unknown,
            Destination::Unknown,
            Destination::Unknown,
        ]
    );
}

#[test]
fn a_time_of_day_is_not_a_scheme() {
    let layout = layout(&["a"]);
    assert_eq!(layout.classify(0, "12:30"), Destination::Unknown);
    assert_eq!(layout.classify(0, ":x"), Destination::Unknown);
}

#[test]
fn helpers_and_the_assembler_agree_on_names() {
    let text = format!(
        "{}Target.\n\n{}\n",
        "[note-1]: #note-1\n\n",
        format_args!("[Go]({})", md::destination("a b", Some("note 1")))
    );
    let (texts, warnings) = chapters(&[("a b", &text)]);
    assert_eq!(
        texts,
        ["[note-1]: #note-1\n\nTarget.\n\n[Go](ch0001.md#note-1)\n"]
    );
    assert_eq!(warnings, []);
}

#[cfg(pictures)]
mod pictures {
    use super::{assembled, draft};
    use crate::draft::{Draft, PictureData, SourcePicture};
    use crate::report::WarningKind;
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    use std::io::Cursor;

    /// A PNG file of one colour.
    fn png(shade: u8) -> Vec<u8> {
        let picture = RgbImage::from_pixel(12, 8, Rgb([shade, shade, shade]));
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(picture)
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    fn with_pictures(text: &str, pictures: &[(&str, Vec<u8>)]) -> Draft {
        let mut draft = draft(&[("a", text)]);
        draft.pictures = pictures
            .iter()
            .map(|(name, data)| SourcePicture {
                name: (*name).to_owned(),
                data: PictureData::Encoded(data.clone()),
            })
            .collect();
        draft
    }

    #[test]
    fn a_picture_is_stored_as_avif_under_a_numbered_path() {
        let draft = with_pictures(
            "![A cat](images/cat%201.png \"Fig. 1\")\n",
            &[("images/cat 1.png", png(10))],
        );
        let (book, warnings) = assembled(draft);
        assert_eq!(
            book.chapters()[0].text(),
            "![A cat](img/0001.avif \"Fig. 1\")\n"
        );
        let stored: Vec<_> = book
            .pictures()
            .iter()
            .map(|p| (p.path(), p.size().width, p.size().height))
            .collect();
        assert_eq!((stored, warnings), (vec![("img/0001.avif", 12, 8)], vec![]));
    }

    #[test]
    fn a_picture_shown_twice_is_stored_once() {
        let draft = with_pictures(
            "![a](a.png) ![again](a.png) ![copy](b.png) ![other](c.png)\n",
            &[("a.png", png(10)), ("b.png", png(10)), ("c.png", png(200))],
        );
        let (book, _) = assembled(draft);
        assert_eq!(
            book.chapters()[0].text(),
            "![a](img/0001.avif) ![again](img/0001.avif) ![copy](img/0001.avif) ![other](img/0002.avif)\n"
        );
        assert_eq!(book.pictures().len(), 2);
    }

    #[test]
    fn a_picture_nobody_shows_is_not_stored() {
        let (book, _) = assembled(with_pictures("Text.\n", &[("a.png", png(10))]));
        assert_eq!(book.pictures().len(), 0);
    }

    #[test]
    fn a_picture_that_cannot_be_stored_becomes_its_alternative_text() {
        let draft = with_pictures(
            "![A *chart*](chart.svg) ![](gone.png) ![Remote](https://example.org/a.png)\n",
            &[("chart.svg", b"<svg/>".to_vec())],
        );
        let (book, warnings) = assembled(draft);
        assert_eq!(book.chapters()[0].text(), "A chart  Remote\n");
        assert_eq!(
            warnings,
            [
                WarningKind::PictureDropped("chart.svg".into()),
                WarningKind::PictureDropped("gone.png".into()),
                WarningKind::PictureDropped("https://example.org/a.png".into())
            ]
        );
    }

    #[test]
    fn the_cover_is_stored_under_its_own_path() {
        let mut draft = with_pictures("![The cover again](cover.png)\n", &[("cover.png", png(10))]);
        draft.cover = Some("cover.png".to_owned());
        let (book, warnings) = assembled(draft);
        assert_eq!(book.meta().cover(), Some("img/cover.avif"));
        assert_eq!(
            book.chapters()[0].text(),
            "![The cover again](img/cover.avif)\n"
        );
        assert_eq!((book.pictures().len(), warnings), (1, vec![]));
    }

    #[test]
    fn a_cover_that_cannot_be_stored_is_reported() {
        let mut draft = with_pictures("Text.\n", &[("cover.svg", b"<svg/>".to_vec())]);
        draft.cover = Some("cover.svg".to_owned());
        let (book, warnings) = assembled(draft);
        assert_eq!(book.meta().cover(), None);
        assert_eq!(warnings, [WarningKind::PictureDropped("cover.svg".into())]);
    }

    #[test]
    fn a_book_of_pictures_alone_is_not_empty() {
        let mut draft = with_pictures(" ", &[("cover.png", png(10))]);
        draft.cover = Some("cover.png".to_owned());
        let (book, _) = assembled(draft);
        assert_eq!((book.chapters().len(), book.pictures().len()), (0, 1));
    }

    #[test]
    fn pictures_shown_through_definitions_are_stored() {
        let draft = with_pictures(
            "![A cat][cat] and ![a dog][dog].\n\n[cat]: a.png\n[dog]: gone.png\n",
            &[("a.png", png(10))],
        );
        let (book, warnings) = assembled(draft);
        assert_eq!(
            book.chapters()[0].text(),
            "![A cat][cat] and a dog.\n\n[cat]: img/0001.avif\n"
        );
        assert_eq!(warnings, [WarningKind::PictureDropped("gone.png".into())]);
    }

    #[test]
    fn a_picture_inside_a_link_keeps_both() {
        let draft = with_pictures("[![A cat](a.png)](a)\n", &[("a.png", png(10))]);
        let (book, _) = assembled(draft);
        assert_eq!(
            book.chapters()[0].text(),
            "[![A cat](img/0001.avif)](ch0001.md)\n"
        );
    }
}

#[cfg(feature = "epub")]
mod landmarks {
    use super::{assembled, draft};
    use crate::draft::{Draft, DraftLandmark};
    use crate::report::WarningKind;

    fn with_landmarks(landmarks: &[(&str, &str, &str)]) -> Draft {
        let mut draft = draft(&[
            ("a", "# One\n\nText.\n"),
            (
                "b",
                "# Intro\n\n[intro]: #intro\n\nA paragraph.\n\n[g]: #g\n\nGlossary.\n",
            ),
            ("empty", " "),
        ]);
        draft.landmarks = landmarks
            .iter()
            .map(|(kind, title, target)| DraftLandmark {
                kind: (*kind).to_owned(),
                title: (*title).to_owned(),
                target: (*target).to_owned(),
            })
            .collect();
        draft
    }

    #[test]
    fn landmarks_lead_to_chapters_and_to_marked_places() {
        let (book, warnings) = assembled(with_landmarks(&[
            ("start", "", "a"),
            ("glossary", " Glossary of terms ", "b#g"),
        ]));
        let placed: Vec<_> = book
            .landmarks()
            .iter()
            .map(|l| {
                (
                    l.kind.as_str(),
                    l.title.as_str(),
                    l.chapter.as_str(),
                    l.anchor.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            placed,
            [
                ("start", "", "ch0001.md", None),
                ("glossary", "Glossary of terms", "ch0002.md", Some("g"))
            ]
        );
        assert_eq!(warnings, []);
        assert!(book.chapters()[1].text().contains("[g]: #g"));
    }

    #[test]
    fn a_landmark_follows_a_mark_that_was_renamed() {
        let (book, _) = assembled(with_landmarks(&[("preface", "", "b#intro")]));
        assert_eq!(book.landmarks()[0].anchor.as_deref(), Some("intro-2"));
    }

    #[test]
    fn landmarks_that_lead_nowhere_are_left_out() {
        let (book, warnings) = assembled(with_landmarks(&[
            ("index", "", "missing"),
            ("index", "", "b#gone"),
            ("index", "", "empty"),
            ("index", "", "https://example.org"),
            ("start", "", "a"),
        ]));
        assert_eq!(book.landmarks().len(), 1);
        assert_eq!(
            warnings,
            [
                WarningKind::LinkDropped("missing".into()),
                WarningKind::LinkDropped("b#gone".into()),
                WarningKind::LinkDropped("empty".into()),
                WarningKind::LinkDropped("https://example.org".into())
            ]
        );
    }

    #[test]
    fn a_second_start_and_a_kind_without_form_are_left_out() {
        let (book, warnings) = assembled(with_landmarks(&[
            ("start", "", "a"),
            ("start", "", "b"),
            ("Body Matter", "", "b"),
            ("", "", "b"),
        ]));
        let kinds: Vec<_> = book.landmarks().iter().map(|l| l.kind.as_str()).collect();
        assert_eq!(kinds, ["start"]);
        assert_eq!(warnings.len(), 3);
    }

    #[test]
    fn a_draft_without_landmarks_gives_a_book_without_them() {
        let (book, _) = assembled(with_landmarks(&[]));
        assert_eq!(book.landmarks(), []);
    }
}

#[cfg(outlines)]
mod outlines {
    use super::{assembled, draft};
    use crate::draft::{Draft, OutlineEntry};
    use crate::report::WarningKind;
    use mbook::{Toc, TocItem};

    const FIRST: &str =
        "[top]: #top\n\n# One\n\nText.\n\n[h2]: #h2\n\n## Inside\n\n[p]: #p\n\nA paragraph.\n";
    const SECOND: &str = "[lead]: #lead\n\nNo heading yet.\n\n[later]: #later\n\nLater.\n";

    fn with_outline(entries: &[(u8, &str, &str)]) -> Draft {
        let mut draft = draft(&[("a", FIRST), ("b", SECOND)]);
        draft.outline = entries
            .iter()
            .map(|(level, title, target)| OutlineEntry {
                level: *level,
                title: (*title).to_owned(),
                target: (*target).to_owned(),
            })
            .collect();
        draft
    }

    fn items(toc: &Toc) -> Vec<(u8, &str, &str, Option<&str>)> {
        fn parts(item: &TocItem) -> (u8, &str, &str, Option<&str>) {
            (
                item.level,
                item.title.as_str(),
                item.chapter.as_str(),
                item.anchor.as_deref(),
            )
        }
        let Toc::Explicit(items) = toc else {
            panic!("the contents are explicit");
        };
        items.iter().map(parts).collect()
    }

    #[test]
    fn entries_lead_to_chapters_and_to_marked_headings() {
        let (book, warnings) = assembled(with_outline(&[
            (1, "One", "a"),
            (2, "Inside", "a#h2"),
            (1, "Two", "b"),
        ]));
        assert_eq!(
            items(book.toc()),
            [
                (1, "One", "ch0001.md", None),
                (2, "Inside", "ch0001.md", Some("inside")),
                (1, "Two", "ch0002.md", None)
            ]
        );
        assert_eq!(warnings, []);
    }

    #[test]
    fn a_mark_in_front_of_the_first_heading_is_that_heading() {
        let (book, warnings) = assembled(with_outline(&[(1, "One", "a#top")]));
        assert_eq!(items(book.toc()), [(1, "One", "ch0001.md", Some("one"))]);
        assert_eq!(warnings, []);
    }

    #[test]
    fn an_entry_to_another_place_leads_to_the_heading_above_it() {
        let (book, warnings) = assembled(with_outline(&[(1, "A paragraph", "a#p")]));
        assert_eq!(
            items(book.toc()),
            [(1, "A paragraph", "ch0001.md", Some("inside"))]
        );
        assert_eq!(warnings, [WarningKind::LinkRetargeted("a#p".into())]);
    }

    #[test]
    fn an_entry_above_every_heading_leads_to_the_start_of_its_chapter() {
        let (book, warnings) = assembled(with_outline(&[
            (1, "Lead", "b#lead"),
            (1, "Later", "b#later"),
        ]));
        assert_eq!(
            items(book.toc()),
            [
                (1, "Lead", "ch0002.md", None),
                (1, "Later", "ch0002.md", None)
            ]
        );
        assert_eq!(warnings, [WarningKind::LinkRetargeted("b#later".into())]);
    }

    #[test]
    fn entries_that_lead_nowhere_or_have_no_title_are_left_out() {
        let (book, warnings) = assembled(with_outline(&[
            (1, "Gone", "c"),
            (1, "No mark", "a#gone"),
            (1, " ", "a"),
            (1, "Out", "https://example.org"),
            (1, "Two", "b"),
        ]));
        assert_eq!(items(book.toc()), [(1, "Two", "ch0002.md", None)]);
        assert_eq!(
            warnings,
            [
                WarningKind::LinkDropped("c".into()),
                WarningKind::LinkDropped("a#gone".into()),
                WarningKind::LinkDropped("a".into()),
                WarningKind::LinkDropped("https://example.org".into())
            ]
        );
    }

    #[test]
    fn levels_are_brought_into_a_tree_and_order_into_reading_order() {
        let (book, warnings) = assembled(with_outline(&[
            (3, "Two", "b"),
            (0, "One, too late", "a"),
            (5, "Later", "b"),
        ]));
        assert_eq!(
            items(book.toc()),
            [
                (1, "Two", "ch0002.md", None),
                (2, "Later", "ch0002.md", None)
            ]
        );
        assert_eq!(warnings, [WarningKind::LinkDropped("a".into())]);
    }

    #[test]
    fn a_draft_whose_outline_is_lost_gets_its_contents_from_the_headings() {
        let (book, _) = assembled(with_outline(&[(1, "Gone", "c")]));
        assert_eq!(book.toc(), &Toc::FromHeadings { max_level: 6 });
    }

    #[test]
    fn marks_used_by_the_contents_alone_are_kept() {
        let (book, _) = assembled(with_outline(&[(1, "A paragraph", "a#p")]));
        assert!(book.chapters()[0].text().contains("[p]: #p"));
    }

    #[test]
    fn a_mark_in_front_of_a_heading_is_replaced_by_the_anchor_of_the_heading() {
        let (book, _) = assembled(with_outline(&[(1, "Inside", "a#h2")]));
        assert_eq!(
            book.chapters()[0].text(),
            "# One\n\nText.\n\n## Inside\n\nA paragraph.\n"
        );
    }
}
