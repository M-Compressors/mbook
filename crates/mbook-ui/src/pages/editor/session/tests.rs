//! The state of the page and the words it shows, without a window.

use super::*;
use crate::features::edit_book::outline::SeriesOutline;

fn chapter(path: &str) -> ChapterOutline {
    ChapterOutline {
        path: path.to_owned(),
        len: 10,
        heading: None,
    }
}

fn meta() -> MetaOutline {
    MetaOutline {
        title: "Dune".to_owned(),
        authors: vec!["Frank Herbert".to_owned()],
        language: "en".to_owned(),
        series: None,
        publisher: None,
        date: None,
        annotation: None,
        cover: None,
    }
}

fn outline(paths: &[&str], unsaved: bool) -> Outline {
    Outline {
        meta: meta(),
        chapters: paths.iter().map(|path| chapter(path)).collect(),
        landmarks: Vec::new(),
        pictures: Vec::new(),
        problems: Vec::new(),
        unsaved,
    }
}

fn problem(path: Option<&str>, offset: Option<u32>, message: &str) -> ProblemOutline {
    ProblemOutline {
        path: path.map(str::to_owned),
        offset,
        message: message.to_owned(),
    }
}

fn session_with(paths: &[&str]) -> Session {
    let mut session = Session::new();
    session.apply_outline(1, outline(paths, false));
    session
}

#[test]
fn a_new_session_is_loading_and_has_nothing_unsaved() {
    let session = Session::new();
    assert_eq!(session.phase, Phase::Loading);
    assert!(!session.has_unsaved());
    assert_eq!(session.status(false), None);
    assert_eq!(session.landmarks(), []);
}

#[test]
fn a_notes_change_counts_as_unsaved_until_the_actor_replies() {
    let mut session = session_with(&["chapter.md"]);
    session.landmarks_pending = true;
    assert!(session.has_unsaved());
}

#[test]
fn the_first_outline_opens_the_first_chapter() {
    let mut session = Session::new();
    let applied = session.apply_outline(1, outline(&["a.md", "b.md"], false));
    assert!(applied.taken && applied.first && applied.open_changed);
    assert_eq!(session.phase, Phase::Ready);
    assert_eq!(session.open.as_deref(), Some("a.md"));
}

#[test]
fn a_book_without_chapters_opens_none() {
    let mut session = Session::new();
    session.apply_outline(1, outline(&[], false));
    assert_eq!(session.open, None);
}

#[test]
fn an_outline_that_is_older_than_the_last_one_is_dropped() {
    let mut session = session_with(&["a.md"]);
    session.apply_outline(3, outline(&["a.md", "b.md"], true));
    let applied = session.apply_outline(2, outline(&["a.md"], false));
    assert!(!applied.taken);
    assert_eq!(session.chapters().len(), 2);
}

#[test]
fn removing_the_open_chapter_opens_its_neighbour() {
    let mut session = session_with(&["a.md", "b.md", "c.md"]);
    session.show_chapter("b.md");
    let applied = session.apply_outline(2, outline(&["a.md", "c.md"], true));
    assert!(applied.open_changed);
    assert_eq!(session.open.as_deref(), Some("c.md"));
}

#[test]
fn removing_the_last_chapter_opens_the_one_before() {
    let mut session = session_with(&["a.md", "b.md"]);
    session.show_chapter("b.md");
    session.apply_outline(2, outline(&["a.md"], true));
    assert_eq!(session.open.as_deref(), Some("a.md"));
}

#[test]
fn removing_every_chapter_leaves_none_open() {
    let mut session = session_with(&["a.md"]);
    session.apply_outline(2, outline(&[], true));
    assert_eq!(session.open, None);
}

#[test]
fn moving_the_open_chapter_keeps_it_open() {
    let mut session = session_with(&["a.md", "b.md"]);
    let applied = session.apply_outline(2, outline(&["b.md", "a.md"], true));
    assert!(!applied.open_changed);
    assert_eq!(session.open.as_deref(), Some("a.md"));
}

#[test]
fn typing_marks_the_chapter_edited_and_the_book_unsaved() {
    let mut session = session_with(&["a.md", "b.md"]);
    session.text_typed("a.md");
    assert!(session.is_edited("a.md"));
    assert!(!session.is_edited("b.md"));
    assert!(session.has_unsaved());
    assert_eq!(session.status(false), Some("Unsaved changes"));
}

#[test]
fn sending_the_text_keeps_the_chapter_edited_until_the_book_is_saved() {
    let mut session = session_with(&["a.md"]);
    session.text_typed("a.md");
    assert_eq!(session.take_pending().as_deref(), Some("a.md"));
    assert!(session.is_edited("a.md"));
    session.apply_outline(2, outline(&["a.md"], true));
    assert!(session.is_edited("a.md"));
    session.apply_outline(3, outline(&["a.md"], false));
    assert!(!session.is_edited("a.md"));
    assert!(!session.has_unsaved());
}

#[test]
fn text_that_waits_to_be_sent_is_unsaved_whatever_the_outline_says() {
    let mut session = session_with(&["a.md"]);
    session.text_typed("a.md");
    session.apply_outline(2, outline(&["a.md"], false));
    assert!(session.has_pending());
    assert!(session.has_unsaved());
}

#[test]
fn a_pending_text_is_taken_once() {
    let mut session = session_with(&["a.md"]);
    session.text_typed("a.md");
    assert!(session.take_pending().is_some());
    assert_eq!(session.take_pending(), None);
}

#[test]
fn a_refused_text_is_kept_as_a_draft_with_its_error_at_the_editor() {
    let mut session = session_with(&["a.md"]);
    session.text_refused("a.md", "bad".to_owned(), "Refused.".to_owned());
    assert_eq!(session.draft("a.md"), Some("bad"));
    assert_eq!(session.error(&Control::ChapterText), Some("Refused."));
    assert!(session.has_refused_text());
    assert!(session.has_unsaved());
    assert_eq!(session.first_refused(), Some("a.md"));
}

#[test]
fn a_text_the_book_takes_later_clears_the_draft_and_its_error() {
    let mut session = session_with(&["a.md"]);
    session.text_refused("a.md", "bad".to_owned(), "Refused.".to_owned());
    session.text_accepted("a.md");
    assert_eq!(session.draft("a.md"), None);
    assert_eq!(session.error(&Control::ChapterText), None);
    assert!(!session.has_refused_text());
}

#[test]
fn the_error_of_a_chapter_that_is_not_open_is_not_shown_at_the_editor() {
    let mut session = session_with(&["a.md", "b.md"]);
    session.text_refused("b.md", "bad".to_owned(), "Refused.".to_owned());
    assert_eq!(session.error(&Control::ChapterText), None);
    assert!(session.is_edited("b.md"));
}

#[test]
fn renaming_a_chapter_moves_what_is_kept_for_it() {
    let mut session = session_with(&["a.md", "b.md"]);
    session.text_typed("a.md");
    session.text_refused("a.md", "bad".to_owned(), "Refused.".to_owned());
    session.rename("a.md", "z.md");
    assert_eq!(session.open.as_deref(), Some("z.md"));
    assert!(session.is_edited("z.md") && !session.is_edited("a.md"));
    assert_eq!(session.draft("z.md"), Some("bad"));
    assert_eq!(session.take_pending().as_deref(), Some("z.md"));
}

#[test]
fn forgetting_a_chapter_drops_its_pending_text_and_draft() {
    let mut session = session_with(&["a.md"]);
    session.text_typed("a.md");
    session.text_refused("a.md", "bad".to_owned(), "Refused.".to_owned());
    session.forget("a.md");
    assert!(!session.has_pending());
    assert!(!session.has_refused_text());
    assert!(!session.is_edited("a.md"));
}

#[test]
fn a_picture_that_is_being_encoded_counts_as_unsaved() {
    let mut session = session_with(&[]);
    session.encoding = 1;
    assert!(session.has_unsaved());
}

#[test]
fn the_header_says_saving_while_a_save_runs() {
    let session = session_with(&[]);
    assert_eq!(session.status(true), Some("Saving"));
}

#[test]
fn a_failed_open_keeps_the_reason() {
    let mut session = Session::new();
    session.fail("It is gone.".to_owned());
    assert_eq!(session.phase, Phase::Failed("It is gone.".to_owned()));
}

#[test]
fn showing_a_section_is_a_screen_of_its_own() {
    let mut session = session_with(&["a.md"]);
    session.show_section(Section::Details);
    assert_eq!(
        (session.section, session.level),
        (Section::Details, Level::Section)
    );
}

#[test]
fn opening_a_chapter_shows_the_chapters_and_the_chapter_screen() {
    let mut session = session_with(&["a.md", "b.md"]);
    session.show_section(Section::Details);
    session.show_chapter("b.md");
    assert_eq!(
        (session.section, session.level, session.open.as_deref()),
        (Section::Chapters, Level::Chapter, Some("b.md"))
    );
}

#[test]
fn stepping_back_goes_from_the_chapter_to_the_section_to_the_list() {
    let mut session = session_with(&["a.md"]);
    session.show_chapter("a.md");
    assert!(session.step_back());
    assert_eq!(session.level, Level::Section);
    assert!(session.step_back());
    assert_eq!(session.level, Level::Sections);
    assert!(!session.step_back());
}

#[test]
fn an_error_is_kept_at_its_control_until_cleared() {
    let mut session = Session::new();
    session.set_error(Control::Cover, "No.".to_owned());
    assert_eq!(session.error_text(&Control::Cover).as_deref(), Some("No."));
    assert_eq!(session.error(&Control::Pictures), None);
    session.clear_error(&Control::Cover);
    assert_eq!(session.error(&Control::Cover), None);
}

#[test]
fn the_places_of_problems_are_forgotten_when_there_are_none() {
    let mut session = session_with(&["a.md"]);
    session.places.insert(("a.md".to_owned(), 3), (1, 4));
    session.apply_outline(2, outline(&["a.md"], true));
    assert!(session.places.is_empty());
}

#[test]
fn the_views_a_window_offers_follow_its_size_class() {
    assert_eq!(ViewMode::available(SizeClass::Expanded).len(), 4);
    assert_eq!(
        ViewMode::available(SizeClass::Medium),
        &[ViewMode::Visual, ViewMode::Source, ViewMode::Preview]
    );
    assert_eq!(
        ViewMode::available(SizeClass::Compact),
        &[ViewMode::Visual, ViewMode::Source, ViewMode::Preview]
    );
}

#[test]
fn split_is_drawn_as_source_below_an_expanded_window() {
    assert_eq!(
        ViewMode::Split.effective(SizeClass::Expanded),
        ViewMode::Split
    );
    assert_eq!(
        ViewMode::Split.effective(SizeClass::Medium),
        ViewMode::Source
    );
    assert_eq!(
        ViewMode::Split.effective(SizeClass::Compact),
        ViewMode::Source
    );
    assert_eq!(
        ViewMode::Preview.effective(SizeClass::Compact),
        ViewMode::Preview
    );
}

#[test]
fn every_section_has_a_name_and_an_identifier_of_its_own() {
    let labels: BTreeSet<_> = Section::ALL.iter().map(|section| section.label()).collect();
    let ids: BTreeSet<_> = Section::ALL.iter().map(|section| section.id()).collect();
    assert_eq!(
        (labels.len(), ids.len()),
        (Section::ALL.len(), Section::ALL.len())
    );
}

#[test]
fn a_chapter_is_proposed_the_next_numbered_path() {
    assert_eq!(propose_chapter_path(&[]), "ch01.md");
    assert_eq!(
        propose_chapter_path(&[chapter("ch01.md"), chapter("ch02.md")]),
        "ch03.md"
    );
}

#[test]
fn a_proposed_chapter_path_skips_one_that_is_used() {
    assert_eq!(propose_chapter_path(&[chapter("ch02.md")]), "ch03.md");
    assert_eq!(
        propose_chapter_path(&[chapter("intro.md"), chapter("ch02.md")]),
        "ch03.md"
    );
}

#[test]
fn a_picture_is_proposed_its_file_name_with_the_avif_extension() {
    assert_eq!(propose_picture_path("pond.png", &[]), "pond.avif");
    assert_eq!(propose_picture_path("my.photo.JPG", &[]), "my.photo.avif");
    assert_eq!(propose_picture_path("pond", &[]), "pond.avif");
}

#[test]
fn a_picture_without_a_usable_name_is_proposed_picture() {
    assert_eq!(propose_picture_path(".png", &[]), "picture.avif");
    assert_eq!(propose_picture_path("", &[]), "picture.avif");
}

#[test]
fn a_picture_path_that_is_used_gets_a_number() {
    let used = |path: &str| PictureOutline {
        path: path.to_owned(),
        width: 1,
        height: 1,
        len: 1,
    };
    assert_eq!(
        propose_picture_path("pond.png", &[used("pond.avif"), used("pond-2.avif")]),
        "pond-3.avif"
    );
}

#[test]
fn a_path_is_trimmed_and_an_empty_one_is_refused() {
    assert_eq!(clean_path("  ch01.md "), Ok("ch01.md".to_owned()));
    assert_eq!(clean_path("   "), Err("Enter a path, such as ch01.md."));
    assert_eq!(clean_path(""), Err("Enter a path, such as ch01.md."));
}

#[test]
fn authors_are_the_lines_that_are_not_blank() {
    assert_eq!(
        authors_from_text(" Ann \n\n  \nBob\r\n"),
        vec!["Ann".to_owned(), "Bob".to_owned()]
    );
    assert_eq!(authors_from_text(""), Vec::<String>::new());
}

#[test]
fn the_form_shows_the_metadata_of_the_book() {
    let mut meta = meta();
    meta.series = Some(SeriesOutline {
        name: "Dune".to_owned(),
        index: Some("1".to_owned()),
    });
    meta.authors.push("Brian Herbert".to_owned());
    let values = MetaValues::of(&meta);
    assert_eq!(values.authors, "Frank Herbert\nBrian Herbert");
    assert_eq!(
        (values.series_name.as_str(), values.series_index.as_str()),
        ("Dune", "1")
    );
    assert_eq!(values.publisher, "");
}

#[test]
fn a_field_that_agrees_with_the_book_changes_nothing() {
    let meta = meta();
    let values = MetaValues::of(&meta);
    for field in MetaField::ALL {
        assert_eq!(meta_change(field, &values, &meta), None, "{field:?}");
    }
}

#[test]
fn a_new_title_is_sent_without_its_edges() {
    let mut values = MetaValues::of(&meta());
    values.title = "  Dune Messiah ".to_owned();
    assert_eq!(
        meta_change(MetaField::Title, &values, &meta()),
        Some(MetaChange::Title("Dune Messiah".to_owned()))
    );
}

#[test]
fn an_empty_title_is_sent_so_that_the_book_refuses_it() {
    let mut values = MetaValues::of(&meta());
    values.title = " ".to_owned();
    assert_eq!(
        meta_change(MetaField::Title, &values, &meta()),
        Some(MetaChange::Title(String::new()))
    );
}

#[test]
fn authors_are_sent_as_the_whole_list() {
    let mut values = MetaValues::of(&meta());
    values.authors = "Ann\nBob".to_owned();
    assert_eq!(
        meta_change(MetaField::Authors, &values, &meta()),
        Some(MetaChange::Authors(vec![
            "Ann".to_owned(),
            "Bob".to_owned()
        ]))
    );
    values.authors = String::new();
    assert_eq!(
        meta_change(MetaField::Authors, &values, &meta()),
        Some(MetaChange::Authors(Vec::new()))
    );
}

#[test]
fn a_series_needs_a_name_and_may_have_a_number() {
    let mut values = MetaValues::of(&meta());
    values.series_name = "Cycle".to_owned();
    assert_eq!(
        meta_change(MetaField::Series, &values, &meta()),
        Some(MetaChange::Series(Some(("Cycle".to_owned(), None))))
    );
    values.series_index = " 2 ".to_owned();
    assert_eq!(
        meta_change(MetaField::Series, &values, &meta()),
        Some(MetaChange::Series(Some((
            "Cycle".to_owned(),
            Some("2".to_owned())
        ))))
    );
}

#[test]
fn clearing_the_name_of_the_series_removes_it() {
    let mut with_series = meta();
    with_series.series = Some(SeriesOutline {
        name: "Cycle".to_owned(),
        index: Some("2".to_owned()),
    });
    let mut values = MetaValues::of(&with_series);
    values.series_name = String::new();
    assert_eq!(
        meta_change(MetaField::Series, &values, &with_series),
        Some(MetaChange::Series(None))
    );
}

#[test]
fn an_empty_optional_field_removes_the_key() {
    let mut with_all = meta();
    with_all.publisher = Some("Chilton".to_owned());
    with_all.date = Some("1965".to_owned());
    with_all.annotation = Some("Sand.".to_owned());
    let mut values = MetaValues::of(&with_all);
    values.publisher = String::new();
    values.date = "  ".to_owned();
    values.annotation = String::new();
    assert_eq!(
        meta_change(MetaField::Publisher, &values, &with_all),
        Some(MetaChange::Publisher(None))
    );
    assert_eq!(
        meta_change(MetaField::Date, &values, &with_all),
        Some(MetaChange::Date(None))
    );
    assert_eq!(
        meta_change(MetaField::Annotation, &values, &with_all),
        Some(MetaChange::Annotation(None))
    );
}

#[test]
fn a_new_language_and_a_new_publisher_are_sent() {
    let mut values = MetaValues::of(&meta());
    values.language = "ru".to_owned();
    values.publisher = "Mir".to_owned();
    assert_eq!(
        meta_change(MetaField::Language, &values, &meta()),
        Some(MetaChange::Language("ru".to_owned()))
    );
    assert_eq!(
        meta_change(MetaField::Publisher, &values, &meta()),
        Some(MetaChange::Publisher(Some("Mir".to_owned())))
    );
}

#[test]
fn a_line_and_a_column_count_from_one() {
    assert_eq!(line_and_column("abc", 0), (1, 1));
    assert_eq!(line_and_column("abc\ndef", 5), (2, 2));
    assert_eq!(line_and_column("abc\n", 4), (2, 1));
}

#[test]
fn a_column_counts_characters_not_bytes() {
    assert_eq!(line_and_column("\u{43f}\u{440}x", 4), (1, 3));
}

#[test]
fn an_offset_past_the_end_or_inside_a_character_is_clamped() {
    assert_eq!(line_and_column("ab", 99), (1, 3));
    assert_eq!(line_and_column("\u{43f}x", 1), (1, 1));
    assert_eq!(clamp_to_boundary("\u{43f}x", 1), 0);
    assert_eq!(clamp_to_boundary("", 5), 0);
}

#[test]
fn a_problem_is_told_with_its_chapter_and_place() {
    let found = problem(Some("a.md"), Some(5), "The link leads nowhere.");
    assert_eq!(
        problem_words(&found, Some((2, 3))),
        "a.md, line 2, column 3: The link leads nowhere."
    );
    assert_eq!(
        problem_words(&found, None),
        "a.md, byte 5: The link leads nowhere."
    );
}

#[test]
fn a_problem_without_a_place_or_a_chapter_is_just_its_message() {
    assert_eq!(
        problem_words(&problem(Some("a.md"), None, "Bad."), None),
        "a.md: Bad."
    );
    assert_eq!(problem_words(&problem(None, None, "Bad."), None), "Bad.");
}

#[test]
fn problems_about_the_contents_are_told_from_the_others() {
    for message in [
        "The contents item \"One\" leads nowhere.",
        "A contents item has no title.",
        "The level of the contents item \"One\" does not continue the tree.",
        "The contents item \"One\" is out of reading order.",
    ] {
        assert!(
            is_contents_problem(&problem(None, None, message)),
            "{message}"
        );
    }
    assert!(!is_contents_problem(&problem(
        Some("a.md"),
        Some(1),
        "The link to \"x\" leads nowhere."
    )));
}

#[test]
fn the_contents_problems_mbook_reports_are_the_ones_the_page_recognises() {
    let mut book = mbook::Book::new("T", "en").unwrap();
    book.add_chapter("a.md", "# One\n").unwrap();
    book.set_toc(mbook::Toc::Explicit(vec![
        mbook::TocItem::new(1, "Gone", "gone.md"),
        mbook::TocItem::new(1, "", "a.md"),
    ]));
    let listed = ProblemOutline::all(&book.check().unwrap_err());
    assert_eq!(listed.len(), 2, "{listed:?}");
    assert!(listed.iter().all(is_contents_problem), "{listed:?}");
}

#[test]
fn a_problem_of_a_chapter_is_not_a_contents_problem_in_mbook_either() {
    let mut book = mbook::Book::new("T", "en").unwrap();
    book.add_chapter("a.md", "# One\n\n[x](missing.md)\n")
        .unwrap();
    let listed = ProblemOutline::all(&book.check().unwrap_err());
    assert!(!listed.iter().any(is_contents_problem), "{listed:?}");
}

#[test]
fn the_announcement_of_a_failed_save_counts_the_problems() {
    assert_eq!(
        unsaved_announcement(2),
        "The book has 2 problems and was not saved"
    );
    assert_eq!(
        unsaved_announcement(1),
        "The book has 1 problem and was not saved"
    );
}

#[test]
fn a_chapter_is_named_with_the_state_colour_also_shows() {
    assert_eq!(chapter_label("a.md", None, false, false), "a.md");
    assert_eq!(
        chapter_label("a.md", Some("One"), true, true),
        "a.md, One, current chapter, edited"
    );
}

#[test]
fn sizes_are_told_in_the_unit_a_person_reads() {
    assert_eq!(size_label(0), "0 bytes");
    assert_eq!(size_label(1023), "1023 bytes");
    assert_eq!(size_label(1024), "1.0 KiB");
    assert_eq!(size_label(1536), "1.5 KiB");
    assert_eq!(size_label(1024 * 1024), "1.0 MiB");
}

#[test]
fn a_command_error_says_what_went_wrong_and_what_to_do() {
    assert_eq!(
        edit_error_words(&EditError::NoSuchPath("a.md".to_owned())),
        "Nothing in the book has the path a.md."
    );
    assert!(edit_error_words(&EditError::Ended).contains("open the book again"));
}

#[test]
fn a_refusal_of_the_book_carries_the_reason() {
    let error = EditError::from(mbook::Error::DuplicatePath("a.md".to_owned()));
    assert_eq!(
        edit_error_words(&error),
        "The book refused the change: the path \"a.md\" is already used."
    );
}

#[test]
fn a_refused_path_carries_every_source_of_the_chain() {
    let mut book = mbook::Book::new("T", "en").unwrap();
    let refused = book.add_chapter("", "x").unwrap_err();
    let words = edit_error_words(&EditError::from(refused));
    assert!(words.contains("invalid path"), "{words}");
    assert!(words.contains("the path is empty"), "{words}");
}

#[test]
fn a_failed_save_says_it_was_not_saved() {
    let error = EditError::Save(mbook::Error::NotMbook);
    assert_eq!(
        edit_error_words(&error),
        "The book could not be saved: not an mbook file."
    );
}

#[test]
fn a_failed_opening_says_so() {
    let failure = crate::features::edit_book::error::OpenFailure::new(&mbook::Error::NotMbook);
    assert_eq!(
        edit_error_words(&EditError::Open(failure)),
        "The book could not be opened: not an mbook file."
    );
}
