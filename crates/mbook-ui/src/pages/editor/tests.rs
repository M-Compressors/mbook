//! The editor in headless windows: each size class, the keyboard, the
//! states of opening, saving and leaving, and the errors at their controls.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Focusable as _, Role, TestAppContext, Window,
};
use markdown_editor::{
    BlockKind, BlockPosition, BlockSelection, ColumnAlign, EditorCommand, TableEdit,
};
use mbook::{Book, BookReader};

use super::commands::{ChapterOp, Origin, TYPING_PAUSE};
use super::encode::Target;
use super::files::record_save;
use super::session::{Control, Level, Phase, Section, ViewMode};
use super::{EditorEvent, EditorPage};
use crate::entities::catalogue::{Library, Store};
use crate::features::appearance::Appearance;
use crate::features::edit_book::command::{Command, MetaChange};
use crate::features::edit_book::error::EditError;
use crate::shared::model::BookKey;
use crate::shared::roots::Roots;
use crate::testing::{self, Fixture};

/// An editor over a book of the library.
struct Setup {
    fixture: Fixture,
    key: BookKey,
    window: AnyWindowHandle,
    page: Entity<EditorPage>,
    events: Rc<RefCell<Vec<EditorEvent>>>,
}

/// The chapters every test book has.
const CHAPTERS: [(&str, &str); 3] = [
    ("ch01.md", "# One\n\nFirst text.\n"),
    ("ch02.md", "# Two\n\nSecond text.\n"),
    ("ch03.md", "# Three\n\nThird text.\n"),
];

/// Writes a book into the folder of the books, as the library keeps it.
fn write_book(roots: &Roots, chapters: &[(&str, &str)]) -> BookKey {
    let key = BookKey::generate();
    let mut book = Book::new("Dune", "en").unwrap();
    book.meta_mut().add_author("Frank Herbert").unwrap();
    for (path, text) in chapters {
        book.add_chapter(*path, *text).unwrap();
    }
    book.save(roots.book(key)).unwrap();
    key
}

/// Opens the editor of a book with `chapters`, `width` pixels wide. The
/// background has not run yet.
fn start_with(cx: &mut TestAppContext, width: f32, chapters: &[(&str, &str)]) -> Setup {
    let fixture = Fixture::new();
    let key = write_book(&fixture.roots, chapters);
    record_save(&fixture.roots, &fixture.catalogue, key).unwrap();
    open(cx, fixture, key, width)
}

fn open(cx: &mut TestAppContext, fixture: Fixture, key: BookKey, width: f32) -> Setup {
    let (roots, catalogue) = (fixture.roots.clone(), fixture.catalogue.clone());
    let (window, page) = testing::open(cx, width, 800.0, move |window, cx| {
        Store { roots, catalogue }.install(cx);
        let appearance = cx.new(Appearance::new);
        Appearance::install(appearance, cx);
        let library = cx.new(Library::new);
        Library::install(library, cx);
        EditorPage::new(key, window, cx)
    });
    cx.update(crate::app::bind_keys);
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    cx.update(|cx| {
        cx.subscribe(&page, move |_, event: &EditorEvent, _| {
            seen.borrow_mut().push(*event);
        })
        .detach();
    });
    Setup {
        fixture,
        key,
        window,
        page,
        events,
    }
}

/// An editor whose book has loaded.
fn started(cx: &mut TestAppContext, width: f32) -> Setup {
    let setup = start_with(cx, width, &CHAPTERS);
    cx.run_until_parked();
    setup
}

/// Runs something on the page with its window.
fn act<R>(
    setup: &Setup,
    cx: &mut TestAppContext,
    run: impl FnOnce(&mut EditorPage, &mut Window, &mut gpui_kit::Context<'_, EditorPage>) -> R,
) -> R {
    let page = setup.page.clone();
    let result = cx
        .update_window(setup.window, |_, window, cx| {
            page.update(cx, |page, cx| run(page, window, cx))
        })
        .unwrap();
    cx.run_until_parked();
    result
}

fn state<R>(setup: &Setup, cx: &TestAppContext, read: impl FnOnce(&EditorPage) -> R) -> R {
    setup.page.read_with(cx, |page, _| read(page))
}

fn frame<R>(
    setup: &Setup,
    cx: &mut TestAppContext,
    inspect: impl FnOnce(&mut Window, &mut gpui_kit::App) -> R,
) -> R {
    testing::frame(cx, setup.window, inspect)
}

fn click(setup: &Setup, id: &str, cx: &mut TestAppContext) {
    let id = id.to_owned();
    frame(setup, cx, move |window, cx| window.click(id, cx));
    cx.run_until_parked();
}

fn click_named(setup: &Setup, label: &str, cx: &mut TestAppContext) {
    let label = label.to_owned();
    frame(setup, cx, move |window, cx| {
        let point = window.find_by_label(&label).bounds().center();
        window.pointer_down(point, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(point, gpui_kit::MouseButton::Left, cx);
    });
    cx.run_until_parked();
}

fn select_visual(
    setup: &Setup,
    leaf_text: &str,
    range: std::ops::Range<usize>,
    cx: &mut TestAppContext,
) {
    let leaf_text = leaf_text.to_owned();
    act(setup, cx, move |page, _, cx| {
        page.visual.input.update(cx, |input, cx| {
            let leaf = input
                .model()
                .document()
                .leaves()
                .find(|leaf| leaf.text() == Some(leaf_text.as_str()))
                .unwrap()
                .id();
            input
                .set_selection(
                    BlockSelection::Text {
                        anchor: BlockPosition::new(leaf, range.start),
                        head: BlockPosition::new(leaf, range.end),
                    },
                    cx,
                )
                .unwrap();
        });
    });
}

fn press(setup: &Setup, key: &str, cx: &mut TestAppContext) {
    let key = key.to_owned();
    frame(setup, cx, move |window, cx| window.press(&key, cx));
    cx.run_until_parked();
}

fn press_save(setup: &Setup, cx: &mut TestAppContext) {
    let mac = cx.update(|cx| cx.platform_info().os == gpui_kit::OperatingSystem::Mac);
    press(setup, if mac { "cmd-s" } else { "ctrl-s" }, cx);
}

fn focus_editor(setup: &Setup, cx: &mut TestAppContext) {
    act(setup, cx, |page, window, cx| {
        page.text_focus(cx).focus(window, cx);
    });
}

fn editor_text(setup: &Setup, cx: &mut TestAppContext) -> String {
    setup
        .page
        .read_with(cx, |page, cx| page.preview.read(cx).source().to_string())
}

fn saved_titles_and_texts(setup: &Setup) -> (String, Vec<String>) {
    let mut reader = BookReader::open(setup.fixture.roots.book(setup.key)).unwrap();
    let book = reader.load().unwrap();
    (
        book.meta().title().to_owned(),
        book.chapters()
            .iter()
            .map(|chapter| chapter.text().to_owned())
            .collect(),
    )
}

fn dialog_is_open(setup: &Setup, cx: &mut TestAppContext) -> bool {
    cx.update_window(setup.window, |_, window, cx| window.has_active_dialog(cx))
        .unwrap()
}

#[gpui_kit::test]
fn the_page_says_that_the_book_is_opening_until_the_background_has_run(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &CHAPTERS);
    frame(&setup, cx, |window, _| {
        let status = window.find("editor-opening");
        assert_eq!(status.role(), Some(Role::Status));
        assert_eq!(status.label(), Some("Opening the book"));
        assert_eq!(window.find_by_label("Back").role(), Some(Role::Button));
        assert!(window.try_find("save").is_none());
    });
    assert_eq!(
        state(&setup, cx, |page| page.session.phase.clone()),
        Phase::Loading
    );
}

#[gpui_kit::test]
fn a_book_that_cannot_be_opened_shows_the_reason_and_can_be_left(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = BookKey::generate();
    let setup = open(cx, fixture, key, 1280.0);
    cx.run_until_parked();
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("empty-title").label(),
            Some("This book could not be opened")
        );
        assert!(window.try_find("save").is_none());
        assert_eq!(window.find_by_label("Back").role(), Some(Role::Button));
    });
    let reason = state(&setup, cx, |page| page.session.phase.clone());
    assert!(
        matches!(&reason, Phase::Failed(text) if text.contains("could not be opened")),
        "{reason:?}"
    );
    click(&setup, "back", cx);
    assert_eq!(setup.events.borrow().as_slice(), [EditorEvent::Close]);
}

#[gpui_kit::test]
fn a_file_that_is_not_a_book_shows_why(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = fixture.add_book("Fake");
    let setup = open(cx, fixture, key, 1280.0);
    cx.run_until_parked();
    let phase = state(&setup, cx, |page| page.session.phase.clone());
    assert!(
        matches!(&phase, Phase::Failed(text) if text.contains("not an mbook file")),
        "{phase:?}"
    );
}

#[gpui_kit::test]
fn an_expanded_window_shows_the_sections_the_source_and_the_preview(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-split", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("editor-page").role(), Some(Role::Main));
        assert_eq!(window.find("page-heading").label(), Some("Dune"));
        for section in Section::ALL {
            assert_eq!(window.find(section.id()).role(), Some(Role::Button));
        }
        for (path, _) in CHAPTERS {
            assert!(
                window.try_find(format!("chapter-{path}")).is_some(),
                "{path}"
            );
        }
        assert!(window.try_find("chapter-source").is_some());
        assert!(window.try_find("chapter-text").is_some());
        assert_eq!(window.find_by_label("Split").role(), Some(Role::Button));
        assert!(window.try_find("open-sections").is_none());
    });
    assert_eq!(
        state(&setup, &*cx, |page| page.session.mode),
        ViewMode::Split
    );
    assert_eq!(editor_text(&setup, cx), CHAPTERS[0].1);
}

#[gpui_kit::test]
fn a_medium_window_shows_visual_and_slides_the_sections_over_it(cx: &mut TestAppContext) {
    let setup = started(cx, 800.0);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("visual-viewport").is_some());
        assert!(
            window.try_find("chapter-text").is_none(),
            "split is for expanded windows"
        );
        assert!(window.try_find("section-details").is_none());
        assert_eq!(window.find("open-sections").role(), Some(Role::Button));
    });
    click(&setup, "open-sections", cx);
    cx.run_until_parked();
    let sheet = cx
        .update_window(setup.window, |_, window, cx| window.has_active_sheet(cx))
        .unwrap();
    assert!(sheet);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("section-details").is_some());
        assert!(window.try_find("chapter-ch02.md").is_some());
    });
    click(&setup, "chapter-ch02.md", cx);
    assert_eq!(
        state(&setup, cx, |page| page.session.open.clone()).as_deref(),
        Some("ch02.md")
    );
    assert_eq!(editor_text(&setup, cx), CHAPTERS[1].1);
}

#[gpui_kit::test]
fn selecting_the_active_preview_mode_keeps_it_selected(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.choose_section(Section::Chapters, window, cx);
    });
    click(&setup, "mode-preview", cx);
    click(&setup, "mode-preview", cx);
    assert_eq!(
        state(&setup, cx, |page| page.session.mode),
        ViewMode::Preview
    );
}

#[gpui_kit::test]
fn the_source_editor_uses_the_markdown_highlighter(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    setup.page.read_with(cx, |page, cx| {
        assert_eq!(page.text.read(cx).language_name().as_str(), "markdown");
    });
}

#[gpui_kit::test]
fn visual_text_edits_save_as_markdown_without_rewriting_the_surroundings(cx: &mut TestAppContext) {
    let original = "# One\n\nA **word** and [a link](https://example.com).\n";
    let setup = start_with(cx, 1280.0, &[("ch01.md", original)]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "A word and a link.", 3..5, cx);
    frame(&setup, cx, |window, cx| window.input("ор", cx));
    click(&setup, "save", cx);
    assert_eq!(
        saved_titles_and_texts(&setup).1,
        ["# One\n\nA **wорd** and [a link](https://example.com).\n"]
    );
}

#[gpui_kit::test]
fn enter_in_visual_prose_moves_the_caret_down_one_line_and_saves_a_paragraph(
    cx: &mut TestAppContext,
) {
    let setup = start_with(cx, 1280.0, &[("ch01.md", "First")]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    press(&setup, "end", cx);
    press(&setup, "enter", cx);
    frame(&setup, cx, |window, cx| window.input("Second", cx));

    setup.page.read_with(cx, |page, cx| {
        let paragraphs: Vec<_> = page
            .visual
            .input
            .read(cx)
            .model()
            .document()
            .leaves()
            .filter_map(|leaf| leaf.text())
            .collect();
        assert_eq!(paragraphs, ["First", "Second"]);
    });
    click(&setup, "save", cx);
    assert_eq!(saved_titles_and_texts(&setup).1, ["First\n\nSecond"]);
}

#[gpui_kit::test]
fn visual_formatting_can_be_undone_and_redone_before_switching_to_source(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "First text.", 0..5, cx);
    let mac = cx.update(|cx| cx.platform_info().os == gpui_kit::OperatingSystem::Mac);
    click(&setup, "format-bold", cx);
    assert_eq!(editor_text(&setup, cx), "# One\n\n**First** text.\n");
    press(&setup, if mac { "cmd-z" } else { "ctrl-z" }, cx);
    assert_eq!(editor_text(&setup, cx), CHAPTERS[0].1);
    click(&setup, "visual-redo", cx);
    click(&setup, "mode-source", cx);
    assert_eq!(editor_text(&setup, cx), "# One\n\n**First** text.\n");
}

#[gpui_kit::test]
fn visual_paragraph_style_changes_the_heading_level(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    focus_editor(&setup, cx);
    select_visual(&setup, "One", 0..0, cx);
    click(&setup, "visual-paragraph-style", cx);
    click_named(&setup, "Heading 2", cx);
    assert_eq!(editor_text(&setup, cx), "## One\n\nFirst text.\n");
}

#[gpui_kit::test]
fn visual_quote_formats_the_current_paragraph(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    focus_editor(&setup, cx);
    select_visual(&setup, "First text.", 0..0, cx);
    click(&setup, "format-quote", cx);
    assert_eq!(editor_text(&setup, cx), "# One\n\n> First text.\n");
}

#[gpui_kit::test]
fn visual_table_edits_keep_other_blocks_and_save_the_table(cx: &mut TestAppContext) {
    let original = "Heading  with  spaces\n=====================\n\nSee [spot] and [^note].\n\n| Label | Value |\n| :--- | ---: |\n| One | 7 |\n\n[spot]: #spot\n\n[^note]: A *footnote*.\n";
    let setup = start_with(cx, 1280.0, &[("ch01.md", original)]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "7", 0..1, cx);
    frame(&setup, cx, |window, cx| window.input("9", cx));
    act(&setup, cx, |page, _, cx| {
        page.visual.input.update(cx, |input, cx| {
            assert!(
                input
                    .run(EditorCommand::EditTable(TableEdit::AddRowBelow), cx)
                    .unwrap()
            );
            assert!(
                input
                    .run(EditorCommand::EditTable(TableEdit::AddColumnRight), cx)
                    .unwrap()
            );
            assert!(
                input
                    .run(
                        EditorCommand::EditTable(TableEdit::SetColumnAlign(ColumnAlign::Center)),
                        cx
                    )
                    .unwrap()
            );
        });
    });
    click(&setup, "save", cx);
    let source = &saved_titles_and_texts(&setup).1[0];
    assert!(
        source.starts_with(
            "Heading  with  spaces\n=====================\n\nSee [spot] and [^note].\n\n"
        ),
        "{source}"
    );
    assert!(
        source.ends_with("[spot]: #spot\n\n[^note]: A *footnote*.\n"),
        "{source}"
    );
    let markdown::mdast::Node::Root(root) =
        markdown::to_mdast(source, &markdown::ParseOptions::gfm()).unwrap()
    else {
        panic!("chapter root");
    };
    let table = root
        .children
        .iter()
        .find_map(|block| match block {
            markdown::mdast::Node::Table(table) => Some(table),
            _ => None,
        })
        .unwrap();
    assert_eq!(table.children.len(), 3);
    assert_eq!(table.align.len(), 3);
    assert!(table.align.contains(&markdown::mdast::AlignKind::Center));
    assert!(table.children[1].to_string().contains('9'));
}

#[gpui_kit::test]
fn the_visual_insert_menu_creates_a_table(cx: &mut TestAppContext) {
    let setup = start_with(cx, 390.0, &[("ch01.md", "")]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    click_named(&setup, "Insert", cx);
    click_named(&setup, "Table", cx);
    setup.page.read_with(cx, |page, cx| {
        assert_eq!(
            page.visual
                .input
                .read(cx)
                .model()
                .document()
                .root()
                .children()
                .next()
                .unwrap()
                .kind(),
            BlockKind::Table
        );
    });
}

#[gpui_kit::test]
fn the_visual_link_dialog_wraps_selected_text(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[("ch01.md", "See the book.")]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "See the book.", 4..12, cx);
    click(&setup, "visual-link", cx);
    frame(&setup, cx, |window, cx| {
        window.input("https://example.com", cx);
    });
    click_named(&setup, "Apply", cx);
    assert_eq!(
        editor_text(&setup, cx),
        "See [the book](https://example.com)."
    );
}

#[gpui_kit::test]
fn a_link_can_use_a_chapter_from_the_picker(cx: &mut TestAppContext) {
    let setup = start_with(
        cx,
        1280.0,
        &[
            ("part/a.md", "Read more."),
            ("extras/next chapter.md", "More text."),
        ],
    );
    cx.run_until_parked();
    focus_editor(&setup, cx);
    select_visual(&setup, "Read more.", 0..9, cx);
    click(&setup, "visual-link", cx);
    click_named(&setup, "Choose chapter", cx);
    frame(&setup, cx, |window, cx| {
        let item = window
            .find_all_by_label("extras/next chapter.md")
            .into_iter()
            .find(|item| item.role() == Some(Role::MenuItem))
            .unwrap();
        let point = item.bounds().center();
        window.pointer_down(point, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(point, gpui_kit::MouseButton::Left, cx);
    });
    cx.run_until_parked();
    click_named(&setup, "Apply", cx);
    assert_eq!(
        editor_text(&setup, cx),
        "[Read more](../extras/next%20chapter.md)."
    );
}

#[gpui_kit::test]
fn removing_a_visual_link_keeps_its_text(cx: &mut TestAppContext) {
    let setup = start_with(
        cx,
        1280.0,
        &[("ch01.md", "See [the book](https://example.com).")],
    );
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "See the book.", 6..6, cx);
    click(&setup, "visual-unlink", cx);
    assert_eq!(editor_text(&setup, cx), "See the book.");
}

#[gpui_kit::test]
fn cancelling_the_visual_link_dialog_keeps_the_chapter(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[("ch01.md", "See the book.")]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "See the book.", 4..12, cx);
    click(&setup, "visual-link", cx);
    click_named(&setup, "Cancel", cx);
    assert_eq!(editor_text(&setup, cx), "See the book.");
}

#[gpui_kit::test]
fn visual_inserts_and_previews_a_picture_relative_to_the_chapter(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = BookKey::generate();
    let mut book = Book::new("Dune", "en").unwrap();
    book.add_chapter("part/ch01.md", "").unwrap();
    book.add_picture("img/sand.avif", avif_of(8, 6)).unwrap();
    book.save(fixture.roots.book(key)).unwrap();
    record_save(&fixture.roots, &fixture.catalogue, key).unwrap();
    let setup = open(cx, fixture, key, 1280.0);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    click(&setup, "visual-image", cx);
    click_named(&setup, "img/sand.avif", cx);
    click(&setup, "visual-image-preview", cx);
    frame(&setup, cx, |_, _| ());
    cx.run_until_parked();
    assert!(state(&setup, cx, |page| page
        .visual
        .pictures
        .lock()
        .unwrap()
        .has_cached("img/sand.avif", (8, 6))));
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("chapter-picture-preview").label(),
            Some("Preview of img/sand.avif")
        );
    });
    click_named(&setup, "Close", cx);
    click(&setup, "save", cx);
    assert_eq!(
        saved_titles_and_texts(&setup).1,
        ["![sand](../img/sand.avif)"]
    );
}

#[gpui_kit::test]
fn the_chapter_picture_preview_can_switch_images(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = BookKey::generate();
    let mut book = Book::new("Pictures", "en").unwrap();
    book.add_chapter(
        "ch01.md",
        "![A pond](first.avif)\n\n![A forest][forest]\n\n[forest]: second.avif",
    )
    .unwrap();
    book.add_picture("first.avif", avif_of(8, 6)).unwrap();
    book.add_picture("second.avif", avif_of(6, 8)).unwrap();
    book.save(fixture.roots.book(key)).unwrap();
    record_save(&fixture.roots, &fixture.catalogue, key).unwrap();
    let setup = open(cx, fixture, key, 1280.0);
    cx.run_until_parked();
    click(&setup, "visual-image-preview", cx);
    click_named(&setup, "Choose picture", cx);
    click_named(&setup, "second.avif", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("chapter-picture-preview").label(),
            Some("Preview of second.avif")
        );
    });
}

#[gpui_kit::test]
fn the_native_clipboard_actions_use_the_visual_selection(cx: &mut TestAppContext) {
    use gpui_kit::Action as _;
    use gpui_kit::component::input::{Copy, Cut, Undo};

    let setup = start_with(cx, 1280.0, &[("ch01.md", "A **word**.")]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "A word.", 2..6, cx);
    frame(&setup, cx, |window, cx| {
        window.dispatch_action(Copy.boxed_clone(), cx);
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        "**word**"
    );
    frame(&setup, cx, |window, cx| {
        window.dispatch_action(Cut.boxed_clone(), cx);
    });
    cx.run_until_parked();
    assert_eq!(editor_text(&setup, cx), "A .");
    frame(&setup, cx, |window, cx| {
        window.dispatch_action(Undo.boxed_clone(), cx);
    });
    cx.run_until_parked();
    assert_eq!(editor_text(&setup, cx), "A **word**.");
}

#[gpui_kit::test]
fn the_native_edit_actions_select_all_and_redo_in_visual(cx: &mut TestAppContext) {
    use gpui_kit::Action as _;
    use gpui_kit::component::input::{Redo, SelectAll, Undo};

    let setup = start_with(cx, 1280.0, &[("ch01.md", "First line.\n\nSecond line.")]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| {
        window.dispatch_action(SelectAll.boxed_clone(), cx);
    });
    cx.run_until_parked();
    frame(&setup, cx, |window, cx| window.input("Replaced", cx));
    assert_eq!(editor_text(&setup, cx), "Replaced");
    frame(&setup, cx, |window, cx| {
        window.dispatch_action(Undo.boxed_clone(), cx);
    });
    cx.run_until_parked();
    frame(&setup, cx, |window, cx| {
        window.dispatch_action(Redo.boxed_clone(), cx);
    });
    cx.run_until_parked();
    assert_eq!(editor_text(&setup, cx), "Replaced");
}

#[gpui_kit::test]
fn a_visual_code_block_uses_the_selected_language(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[("ch01.md", "```\nlet answer = 42;\n```")]);
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    select_visual(&setup, "let answer = 42;", 0..0, cx);
    click(&setup, "visual-code-language", cx);
    click_named(&setup, "Rust", cx);
    assert_eq!(editor_text(&setup, cx), "```rust\nlet answer = 42;\n```");
}

#[gpui_kit::test]
fn changing_chapters_flushes_the_visual_editor(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("New ", cx));
    click(&setup, "chapter-ch02.md", cx);
    click(&setup, "save", cx);
    assert_eq!(
        saved_titles_and_texts(&setup).1,
        ["# New One\n\nFirst text.\n", CHAPTERS[1].1, CHAPTERS[2].1]
    );
}

#[gpui_kit::test]
fn a_medium_window_offers_the_source_or_the_preview(cx: &mut TestAppContext) {
    let setup = started(cx, 800.0);
    act(&setup, cx, |page, _, cx| {
        page.session.mode = ViewMode::Preview;
        cx.notify();
    });
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("chapter-text").is_some());
        assert!(window.try_find("chapter-source").is_none());
        assert!(window.try_find_by_label("Split").is_none());
        assert!(window.try_find_by_label("Source").is_some());
    });
}

#[gpui_kit::test]
fn a_compact_window_is_a_stack_that_starts_in_visual(cx: &mut TestAppContext) {
    let setup = started(cx, 390.0);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("visual-viewport").is_some());
    });
    click(&setup, "back", cx);
    click(&setup, "back", cx);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("sections").is_some());
        assert!(window.try_find("chapter-source").is_none());
        assert!(window.try_find("chapter-ch01.md").is_none());
    });
    click(&setup, "section-details", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("section-heading").label(), Some("Details"));
        assert!(window.try_find("section-details").is_none());
    });
    click(&setup, "back", cx);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("sections").is_some());
    });
    assert!(
        setup.events.borrow().is_empty(),
        "back steps through the stack first"
    );
}

#[gpui_kit::test]
fn a_compact_window_goes_from_the_chapters_to_a_chapter_and_back(cx: &mut TestAppContext) {
    let setup = started(cx, 390.0);
    click(&setup, "back", cx);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("chapter-ch02.md").is_some());
        assert!(window.try_find("add-chapter").is_some());
    });
    click(&setup, "chapter-ch02.md", cx);
    assert_eq!(state(&setup, cx, |page| page.session.level), Level::Chapter);
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("visual-viewport").is_some());
        assert!(window.try_find("chapter-text").is_none());
    });
    assert_eq!(editor_text(&setup, cx), CHAPTERS[1].1);
    click(&setup, "back", cx);
    assert_eq!(state(&setup, cx, |page| page.session.level), Level::Section);
    click(&setup, "back", cx);
    click(&setup, "back", cx);
    assert_eq!(setup.events.borrow().as_slice(), [EditorEvent::Close]);
}

#[gpui_kit::test]
fn the_path_button_opens_the_rename_dialog(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "chapter-path", cx);
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .try_find_by_label("New path of the chapter")
                .is_some()
        );
    });
}

#[gpui_kit::test]
fn the_remove_button_asks_before_removing_the_chapter(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "chapter-remove", cx);
    assert!(dialog_is_open(&setup, cx));
    assert_eq!(state(&setup, cx, |page| page.session.chapters().len()), 3);
}

#[gpui_kit::test]
fn every_control_of_every_section_has_a_role_and_a_name(cx: &mut TestAppContext) {
    const CONTROLS: [Role; 7] = [
        Role::Button,
        Role::Link,
        Role::CheckBox,
        Role::RadioButton,
        Role::Switch,
        Role::TextInput,
        Role::ComboBox,
    ];
    for width in [1280.0, 800.0, 390.0] {
        let setup = started(cx, width);
        for section in Section::ALL {
            act(&setup, cx, |page, window, cx| {
                page.choose_section(section, window, cx);
            });
            frame(&setup, cx, |window, _| {
                let elements = window.elements();
                assert!(
                    !elements.is_empty(),
                    "{width} {section:?}: nothing is reported"
                );
                for element in elements {
                    let role = element.role();
                    let decoration = testing::is_kit_decoration(&format!("{:?}", element.path()));
                    assert!(
                        role.is_some() || decoration,
                        "{width} {section:?}: no role: {element:?}"
                    );
                    if role.is_some_and(|role| CONTROLS.contains(&role)) {
                        assert!(
                            element
                                .label()
                                .is_some_and(|label| !label.trim().is_empty()),
                            "{width} {section:?}: a control without a name: {element:?}"
                        );
                    }
                }
            });
        }
    }
}

#[gpui_kit::test]
fn the_current_chapter_and_an_edited_one_are_told_in_words(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .try_find_by_label("ch01.md, One, current chapter")
                .is_some()
        );
        assert!(window.try_find_by_label("ch02.md, Two").is_some());
        assert!(
            window
                .try_find_by_label("Chapters, current section")
                .is_some()
        );
    });
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("More. ", cx));
    cx.run_until_parked();
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .try_find_by_label("ch01.md, One, current chapter, edited")
                .is_some()
        );
        assert_eq!(
            window.find("editor-status").label(),
            Some("Unsaved changes")
        );
    });
}

#[gpui_kit::test]
fn typing_updates_the_preview_at_once_and_the_book_after_a_pause(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-split", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("Hello ", cx));
    assert!(editor_text(&setup, cx).starts_with("Hello "));
    assert!(state(&setup, cx, |page| page.session.has_pending()));
    let book_text = |setup: &Setup, cx: &mut TestAppContext| {
        act(setup, cx, |page, window, cx| {
            let reply = page.handle.chapter_text("ch01.md");
            cx.spawn_in(window, async move |_, _| drop(reply.await))
                .detach();
        });
    };
    book_text(&setup, cx);
    cx.executor()
        .advance_clock(TYPING_PAUSE + Duration::from_millis(1));
    cx.run_until_parked();
    assert!(!state(&setup, cx, |page| page.session.has_pending()));
    let outline_len = state(&setup, cx, |page| page.session.chapters()[0].len);
    assert_eq!(outline_len, "Hello ".len() + CHAPTERS[0].1.len());
}

#[gpui_kit::test]
fn opening_another_chapter_sends_the_text_first(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-source", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("Hello ", cx));
    click(&setup, "chapter-ch02.md", cx);
    assert_eq!(editor_text(&setup, cx), CHAPTERS[1].1);
    click(&setup, "chapter-ch01.md", cx);
    assert!(editor_text(&setup, cx).starts_with("Hello "));
}

#[gpui_kit::test]
fn tab_indents_and_escape_then_tab_leaves_the_editor(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-source", cx);
    focus_editor(&setup, cx);
    press(&setup, "tab", cx);
    assert!(editor_text(&setup, cx).starts_with(' '), "Tab indents");
    assert!(state(&setup, &*cx, |page| !page.escaped));
    press(&setup, "escape", cx);
    assert!(state(&setup, &*cx, |page| page.escaped));
    assert!(
        setup.events.borrow().is_empty(),
        "Escape in the editor does not leave"
    );
    let focused = act(&setup, cx, |page, window, cx| {
        page.text_focus(cx).contains_focused(window, cx)
    });
    assert!(focused);
    press(&setup, "tab", cx);
    let focused = act(&setup, cx, |page, window, cx| {
        page.text_focus(cx).contains_focused(window, cx)
    });
    assert!(!focused, "Tab after Escape moves the focus on");
    assert!(state(&setup, &*cx, |page| !page.escaped));
    assert!(setup.events.borrow().is_empty());
}

#[gpui_kit::test]
fn escape_then_tab_leaves_the_visual_table(cx: &mut TestAppContext) {
    let setup = start_with(
        cx,
        1280.0,
        &[("ch01.md", "| First | Second |\n| --- | --- |")],
    );
    cx.run_until_parked();
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    select_visual(&setup, "First", 0..0, cx);
    press(&setup, "escape", cx);
    assert!(state(&setup, &*cx, |page| page.escaped));
    press(&setup, "tab", cx);
    let focused = act(&setup, cx, |page, window, cx| {
        page.text_focus(cx).contains_focused(window, cx)
    });
    assert!(!focused);
    assert!(setup.events.borrow().is_empty());
}

#[gpui_kit::test]
fn the_editor_says_in_text_how_to_leave_it(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-source", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("editor-hint").label(),
            Some("Tab indents. Press Escape, then Tab, to leave the editor.")
        );
        assert_eq!(window.find("chapter-source").role(), Some(Role::Group));
    });
}

#[gpui_kit::test]
fn escape_outside_the_editor_leaves_a_book_without_changes(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.focus_handle.focus(window, cx);
    });
    press(&setup, "escape", cx);
    assert_eq!(setup.events.borrow().as_slice(), [EditorEvent::Close]);
}

#[gpui_kit::test]
fn leaving_with_unsaved_changes_asks_and_discard_leaves(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("x", cx));
    click(&setup, "back", cx);
    assert!(setup.events.borrow().is_empty());
    assert!(dialog_is_open(&setup, cx));
    frame(&setup, cx, |window, _| {
        for id in ["leave-cancel", "leave-discard", "leave-save"] {
            assert_eq!(window.find(id).role(), Some(Role::Button), "{id}");
        }
    });
    click(&setup, "leave-discard", cx);
    assert_eq!(setup.events.borrow().as_slice(), [EditorEvent::Close]);
    let (_, texts) = saved_titles_and_texts(&setup);
    assert_eq!(texts[0], CHAPTERS[0].1, "nothing was written");
}

#[gpui_kit::test]
fn cancelling_the_question_keeps_the_editor_open(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("x", cx));
    click(&setup, "back", cx);
    click(&setup, "leave-cancel", cx);
    assert!(setup.events.borrow().is_empty());
    assert!(!dialog_is_open(&setup, cx));
}

#[gpui_kit::test]
fn saving_from_the_question_saves_and_then_leaves(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-source", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("x", cx));
    click(&setup, "back", cx);
    click(&setup, "leave-save", cx);
    cx.run_until_parked();
    assert_eq!(
        setup.events.borrow().as_slice(),
        [EditorEvent::Saved(setup.key), EditorEvent::Close]
    );
    let (_, texts) = saved_titles_and_texts(&setup);
    assert!(texts[0].starts_with('x'));
}

#[gpui_kit::test]
fn the_save_key_saves_what_was_typed_without_waiting_for_the_pause(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-source", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("Typed. ", cx));
    press_save(&setup, cx);
    cx.run_until_parked();
    let (_, texts) = saved_titles_and_texts(&setup);
    assert!(texts[0].starts_with("Typed. "), "{:?}", texts[0]);
    assert_eq!(
        setup.events.borrow().as_slice(),
        [EditorEvent::Saved(setup.key)]
    );
    assert!(!state(&setup, cx, |page| page.session.has_unsaved()));
}

#[gpui_kit::test]
fn the_save_key_saves_while_the_visual_editor_has_focus(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-visual", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("Chapter ", cx));
    press_save(&setup, cx);
    assert_eq!(
        saved_titles_and_texts(&setup).1[0],
        "# Chapter One\n\nFirst text.\n"
    );
}

#[gpui_kit::test]
fn saving_with_the_title_field_focused_commits_its_pending_text(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-details", cx);
    act(&setup, cx, |page, window, cx| {
        page.fields.title.focus_handle(cx).focus(window, cx);
        page.fields.title.update(cx, |input, cx| {
            input.select_all(window, cx);
        });
    });
    frame(&setup, cx, |window, cx| window.input("A new title", cx));
    press_save(&setup, cx);
    assert_eq!(saved_titles_and_texts(&setup).0, "A new title");
}

#[gpui_kit::test]
fn saving_makes_the_row_of_the_library_again(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.send(
            Command::SetMeta(MetaChange::Title("Dune Messiah".to_owned())),
            Origin::Meta(super::session::MetaField::Title),
            window,
            cx,
        );
    });
    assert_eq!(
        state(&setup, cx, |page| page.session.title().map(str::to_owned)).as_deref(),
        Some("Dune Messiah")
    );
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("page-heading").label(), Some("Dune Messiah"));
    });
    click(&setup, "save", cx);
    cx.run_until_parked();
    let row = setup.fixture.catalogue.book(setup.key).unwrap().unwrap();
    assert_eq!(row.title, "Dune Messiah");
    assert_eq!(saved_titles_and_texts(&setup).0, "Dune Messiah");
}

#[gpui_kit::test]
fn a_book_with_problems_is_not_written_and_the_problems_are_shown(cx: &mut TestAppContext) {
    let setup = start_with(
        cx,
        1280.0,
        &[
            ("ch01.md", "# One\n\nText.\n"),
            ("ch02.md", "# Two\n\nText.\n"),
        ],
    );
    cx.run_until_parked();
    click(&setup, "mode-source", cx);
    let before = std::fs::read(setup.fixture.roots.book(setup.key)).unwrap();
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| {
        window.input("[gone](missing.md) ", cx);
    });
    click(&setup, "save", cx);
    cx.run_until_parked();
    assert_eq!(
        std::fs::read(setup.fixture.roots.book(setup.key)).unwrap(),
        before
    );
    assert!(setup.events.borrow().is_empty());
    assert_eq!(
        state(&setup, cx, |page| page.session.section),
        Section::Problems
    );
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("problems-summary").label(),
            Some("The book has 1 problem and cannot be saved until they are fixed.")
        );
        let problem = window.find("problem-0");
        assert_eq!(problem.role(), Some(Role::Button));
        let label = problem.label().unwrap();
        assert!(label.starts_with("ch01.md, line 1, column 1: "), "{label}");
        assert!(label.contains("leads nowhere"), "{label}");
        assert_eq!(
            window.find_by_label("Problems, 1, current section").role(),
            Some(Role::Button)
        );
        assert!(window.try_find("rebuild-contents").is_none());
    });
    assert!(state(&setup, cx, |page| page.session.has_unsaved()));
}

#[gpui_kit::test]
fn choosing_a_problem_opens_its_chapter_and_puts_the_caret_there(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "chapter-ch02.md", cx);
    act(&setup, cx, |page, window, cx| {
        page.text.update(cx, |state, cx| {
            state.replace_all("# Two\n\n[gone](missing.md)\n", window, cx);
        });
    });
    click(&setup, "chapter-ch01.md", cx);
    click(&setup, "section-problems", cx);
    click(&setup, "check", cx);
    frame(&setup, cx, |window, _| {
        let label = window.find("problem-0").label().unwrap().to_owned();
        assert!(label.starts_with("ch02.md, line 3, column 1: "), "{label}");
    });
    click(&setup, "problem-0", cx);
    assert_eq!(
        state(&setup, cx, |page| page.session.open.clone()).as_deref(),
        Some("ch02.md")
    );
    assert_eq!(
        state(&setup, cx, |page| page.session.section),
        Section::Chapters
    );
    let cursor = setup
        .page
        .read_with(cx, |page, cx| page.text.read(cx).cursor());
    assert_eq!(cursor, 7);
}

#[gpui_kit::test]
fn removing_a_chapter_of_a_book_with_contents_offers_to_rebuild_them(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = BookKey::generate();
    let mut book = Book::new("Dune", "en").unwrap();
    book.add_chapter("ch01.md", "# One\n").unwrap();
    book.add_chapter("ch02.md", "# Two\n").unwrap();
    book.set_toc(mbook::Toc::Explicit(vec![
        mbook::TocItem::new(1, "One", "ch01.md"),
        mbook::TocItem::new(1, "Two", "ch02.md"),
    ]));
    book.save(fixture.roots.book(key)).unwrap();
    record_save(&fixture.roots, &fixture.catalogue, key).unwrap();
    let setup = open(cx, fixture, key, 1280.0);
    cx.run_until_parked();
    act(&setup, cx, |page, window, cx| {
        page.remove_chapter("ch02.md".to_owned(), window, cx);
    });
    assert_eq!(state(&setup, cx, |page| page.session.chapters().len()), 1);
    click(&setup, "save", cx);
    cx.run_until_parked();
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("rebuild-contents").role(), Some(Role::Button));
        assert!(
            window
                .find("problem-0")
                .label()
                .unwrap()
                .contains("contents item")
        );
    });
    click(&setup, "rebuild-contents", cx);
    assert_eq!(state(&setup, cx, |page| page.session.problems().len()), 0);
    click(&setup, "save", cx);
    cx.run_until_parked();
    assert_eq!(
        setup.events.borrow().as_slice(),
        [EditorEvent::Saved(setup.key)]
    );
}

#[gpui_kit::test]
fn a_text_the_book_refuses_is_told_at_the_editor_with_its_place(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.text
            .update(cx, |state, cx| state.replace_all("ab\u{1}c", window, cx));
    });
    cx.executor()
        .advance_clock(TYPING_PAUSE + Duration::from_millis(1));
    cx.run_until_parked();
    frame(&setup, cx, |window, _| {
        let message = window
            .find("chapter-text-error")
            .label()
            .unwrap()
            .to_owned();
        assert!(message.contains("line 1, column 3"), "{message}");
        assert!(message.contains("control character"), "{message}");
    });
    assert_eq!(
        setup
            .page
            .read_with(cx, |page, cx| page.text.read(cx).cursor()),
        2
    );
    assert!(state(&setup, cx, |page| page.session.has_refused_text()));
}

#[gpui_kit::test]
fn a_refused_text_is_kept_when_another_chapter_is_opened_and_blocks_a_save(
    cx: &mut TestAppContext,
) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.text
            .update(cx, |state, cx| state.replace_all("ab\u{1}c", window, cx));
    });
    cx.executor()
        .advance_clock(TYPING_PAUSE + Duration::from_millis(1));
    cx.run_until_parked();
    click(&setup, "chapter-ch02.md", cx);
    assert_eq!(editor_text(&setup, cx), CHAPTERS[1].1);
    click(&setup, "save", cx);
    frame(&setup, cx, |window, _| {
        let message = window.find("save-error").label().unwrap().to_owned();
        assert!(message.contains("ch01.md"), "{message}");
    });
    let (_, texts) = saved_titles_and_texts(&setup);
    assert_eq!(texts[0], CHAPTERS[0].1);
    click(&setup, "chapter-ch01.md", cx);
    assert_eq!(editor_text(&setup, cx), "ab\u{1}c");
}

#[gpui_kit::test]
fn a_chapter_moves_up_and_down_with_its_controls(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "chapter-ch02.md", cx);
    click(&setup, "chapter-down", cx);
    let order = |setup: &Setup, cx: &TestAppContext| {
        state(setup, cx, |page| {
            page.session
                .chapters()
                .iter()
                .map(|chapter| chapter.path.clone())
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(order(&setup, cx), ["ch01.md", "ch03.md", "ch02.md"]);
    click(&setup, "chapter-up", cx);
    click(&setup, "chapter-up", cx);
    assert_eq!(order(&setup, cx), ["ch02.md", "ch01.md", "ch03.md"]);
    assert_eq!(
        state(&setup, cx, |page| page.session.open.clone()).as_deref(),
        Some("ch02.md")
    );
    assert!(state(&setup, cx, |page| page.session.has_unsaved()));
}

#[gpui_kit::test]
fn the_first_chapter_cannot_move_up_and_the_last_cannot_move_down(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.move_chapter("ch01.md", -1, window, cx);
        page.move_chapter("ch03.md", 1, window, cx);
    });
    assert!(!state(&setup, cx, |page| page.session.has_unsaved()));
    assert!(state(&setup, cx, |page| page
        .session
        .error(&Control::Chapters)
        .is_none()));
}

#[gpui_kit::test]
fn a_chapter_that_gets_another_path_keeps_its_text_and_its_place(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-source", cx);
    focus_editor(&setup, cx);
    frame(&setup, cx, |window, cx| window.input("Kept. ", cx));
    act(&setup, cx, |page, window, cx| {
        page.flush_text(window, cx);
        page.send(
            Command::MoveChapterPath {
                from: "ch01.md".to_owned(),
                to: "start.md".to_owned(),
            },
            Origin::Chapter(ChapterOp::Rename {
                from: "ch01.md".to_owned(),
                to: "start.md".to_owned(),
            }),
            window,
            cx,
        );
    });
    assert_eq!(
        state(&setup, cx, |page| page.session.open.clone()).as_deref(),
        Some("start.md")
    );
    assert_eq!(
        state(&setup, cx, |page| page.loaded.clone()).as_deref(),
        Some("start.md")
    );
    assert!(editor_text(&setup, cx).starts_with("Kept. "));
    let paths = state(&setup, cx, |page| {
        page.session
            .chapters()
            .iter()
            .map(|chapter| chapter.path.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(paths, ["start.md", "ch02.md", "ch03.md"]);
    assert!(state(&setup, cx, |page| page.session.is_edited("start.md")));
}

#[gpui_kit::test]
fn a_path_the_book_refuses_is_told_at_the_chapters(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.send(
            Command::AddChapter {
                path: "ch02.md".to_owned(),
                text: String::new(),
            },
            Origin::Chapter(ChapterOp::Add("ch02.md".to_owned())),
            window,
            cx,
        );
    });
    frame(&setup, cx, |window, _| {
        let message = window.find("chapters-error").label().unwrap().to_owned();
        assert!(message.contains("already used"), "{message}");
    });
    assert_eq!(state(&setup, cx, |page| page.session.chapters().len()), 3);
}

#[gpui_kit::test]
fn a_chapter_that_is_added_is_opened(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.send(
            Command::AddChapter {
                path: "ch04.md".to_owned(),
                text: String::new(),
            },
            Origin::Chapter(ChapterOp::Add("ch04.md".to_owned())),
            window,
            cx,
        );
    });
    assert_eq!(
        state(&setup, cx, |page| page.session.open.clone()).as_deref(),
        Some("ch04.md")
    );
    assert_eq!(editor_text(&setup, cx), "");
    assert_eq!(state(&setup, cx, |page| page.session.chapters().len()), 4);
}

#[gpui_kit::test]
fn removing_the_open_chapter_opens_the_next_one(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "chapter-ch02.md", cx);
    act(&setup, cx, |page, window, cx| {
        page.remove_chapter("ch02.md".to_owned(), window, cx);
    });
    assert_eq!(
        state(&setup, cx, |page| page.session.open.clone()).as_deref(),
        Some("ch03.md")
    );
    assert_eq!(editor_text(&setup, cx), CHAPTERS[2].1);
}

#[gpui_kit::test]
fn a_book_without_chapters_offers_to_add_one(cx: &mut TestAppContext) {
    let setup = start_with(cx, 1280.0, &[]);
    cx.run_until_parked();
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("empty-title").label(),
            Some("This book has no chapters")
        );
        assert_eq!(window.find("add-first-chapter").role(), Some(Role::Button));
    });
    click(&setup, "add-first-chapter", cx);
    assert!(dialog_is_open(&setup, cx));
}

#[gpui_kit::test]
fn the_dialog_for_a_new_chapter_proposes_the_next_path(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "add-chapter", cx);
    assert!(dialog_is_open(&setup, cx));
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window
                .find_by_label("Path of the chapter in the book")
                .role(),
            Some(Role::TextInput)
        );
        assert_eq!(window.find_by_label("Add").role(), Some(Role::Button));
        assert_eq!(window.find_by_label("Cancel").role(), Some(Role::Button));
    });
}

#[gpui_kit::test]
fn the_details_form_shows_the_metadata_with_visible_labels(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-details", cx);
    frame(&setup, cx, |window, _| {
        for label in [
            "Title",
            "Authors (one per line)",
            "Language (a code such as en or ru)",
            "Series",
            "Number in the series",
            "Publisher",
            "Date",
            "Annotation",
        ] {
            let field = window.find_by_label(label);
            assert!(field.role().is_some(), "{label}");
        }
    });
    let values = setup.page.read_with(cx, EditorPage::meta_values);
    assert_eq!(values.title, "Dune");
    assert_eq!(values.authors, "Frank Herbert");
    assert_eq!(values.language, "en");
}

#[gpui_kit::test]
fn a_field_is_sent_when_it_loses_the_focus_and_a_bad_value_is_told_at_the_field(
    cx: &mut TestAppContext,
) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-details", cx);
    act(&setup, cx, |page, window, cx| {
        page.fields
            .title
            .update(cx, |state, cx| state.set_value("", window, cx));
        page.commit_meta(super::session::MetaField::Title, window, cx);
    });
    frame(&setup, cx, |window, _| {
        let message = window.find("error-title").label().unwrap().to_owned();
        assert!(message.contains("the title is empty"), "{message}");
    });
    act(&setup, cx, |page, window, cx| {
        page.fields
            .title
            .update(cx, |state, cx| state.set_value("Dune II", window, cx));
        page.commit_meta(super::session::MetaField::Title, window, cx);
    });
    frame(&setup, cx, |window, _| {
        assert!(window.try_find("error-title").is_none());
    });
    assert_eq!(
        state(&setup, cx, |page| page.session.title().map(str::to_owned)).as_deref(),
        Some("Dune II")
    );
}

#[gpui_kit::test]
fn a_field_is_sent_after_a_pause_in_typing(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-details", cx);
    act(&setup, cx, |page, window, cx| {
        let title = page.fields.title.clone();
        title.update(cx, |state, cx| state.focus(window, cx));
    });
    frame(&setup, cx, |window, cx| window.input("!", cx));
    cx.executor()
        .advance_clock(super::commands::FIELD_PAUSE + Duration::from_millis(1));
    cx.run_until_parked();
    let title = state(&setup, cx, |page| page.session.title().map(str::to_owned));
    assert_eq!(title.as_deref(), Some("Dune!"));
}

#[gpui_kit::test]
fn a_field_that_agrees_with_the_book_sends_nothing(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.commit_meta(super::session::MetaField::Title, window, cx);
    });
    assert!(!state(&setup, cx, |page| page.session.has_unsaved()));
}

#[gpui_kit::test]
fn the_cover_section_says_when_there_is_no_cover_and_offers_to_add_one(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-cover", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("cover-state").label(),
            Some("This book has no cover.")
        );
        assert_eq!(
            window.find_by_label("Add a cover…").role(),
            Some(Role::Button)
        );
    });
}

#[gpui_kit::test]
fn the_picture_gallery_shows_a_thumbnail_and_a_remove_control(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let key = BookKey::generate();
    let mut book = Book::new("Dune", "en").unwrap();
    book.add_chapter("ch01.md", "# One\n").unwrap();
    let avif = avif_of(280, 280);
    book.set_cover("img/p.avif", avif).unwrap();
    book.save(fixture.roots.book(key)).unwrap();
    record_save(&fixture.roots, &fixture.catalogue, key).unwrap();
    let setup = open(cx, fixture, key, 1280.0);
    cx.run_until_parked();
    click(&setup, "section-pictures", cx);
    frame(&setup, cx, |_, _| ());
    cx.run_until_parked();
    frame(&setup, cx, |window, _| {
        let thumb = window.find("preview-img/p.avif");
        let remove = window.find_by_label("Remove img/p.avif");
        assert_eq!(thumb.role(), Some(Role::Image));
        assert_eq!(remove.role(), Some(Role::Button));
        assert!(
            thumb.bounds().bottom() <= remove.bounds().top(),
            "{thumb:?} overlaps {remove:?}"
        );
    });
    cx.run_until_parked();
    assert!(state(&setup, cx, |page| page
        .visual
        .pictures
        .lock()
        .unwrap()
        .has_cached("img/p.avif", (192, 192))));
    click(&setup, "section-cover", cx);
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("cover-image").role(), Some(Role::Image));
        assert_eq!(window.find_by_label("Replace…").role(), Some(Role::Button));
    });
}

/// A small AVIF picture.
fn avif_of(width: u32, height: u32) -> Vec<u8> {
    use crate::entities::picture::{EncodeOptions, encode_avif};
    let picture = image::RgbImage::from_pixel(width, height, image::Rgb([200, 30, 30]));
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(picture)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let options = EncodeOptions {
        speed: 10,
        quality: 50,
        ..EncodeOptions::default()
    };
    encode_avif(&png.into_inner(), options).unwrap()
}

#[gpui_kit::test]
fn a_picture_file_is_encoded_at_the_resolution_selected_in_the_dialog(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pond.png");
    let picture = image::RgbImage::from_pixel(16, 8, image::Rgb([1, 2, 3]));
    picture.save(&file).unwrap();
    act(&setup, cx, |page, window, cx| {
        page.encode_files(vec![file.clone()], Target::Picture, window, cx);
    });
    assert_eq!(state(&setup, cx, |page| page.session.encoding), 0);
    assert!(dialog_is_open(&setup, cx));
    click_named(&setup, "Max width (px)", cx);
    frame(&setup, cx, |window, cx| window.input("8", cx));
    click_named(&setup, "Add", cx);
    click(&setup, "save", cx);
    let reader = BookReader::open(setup.fixture.roots.book(setup.key)).unwrap();
    let picture = reader.find_picture("pond.avif").unwrap();
    assert_eq!((picture.size.width, picture.size.height), (8, 4));
}

#[gpui_kit::test]
fn an_invalid_picture_resolution_keeps_the_dialog_open(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    let file = setup.fixture.roots.data().join("pond.png");
    act(&setup, cx, |page, window, cx| {
        page.encode_files(vec![file.clone()], Target::Picture, window, cx);
    });
    click_named(&setup, "Max height (px)", cx);
    frame(&setup, cx, |window, cx| window.input("0", cx));
    click_named(&setup, "Add", cx);
    assert!(dialog_is_open(&setup, cx));
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .find("picture-settings-error")
                .label()
                .unwrap()
                .contains("positive number")
        );
    });
    click_named(&setup, "Cancel", cx);
    assert!(!state(&setup, cx, |page| page.session.has_unsaved()));
}

#[gpui_kit::test]
fn a_picture_that_is_being_encoded_is_shown_as_encoding_and_counts_as_unsaved(
    cx: &mut TestAppContext,
) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-pictures", cx);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pond.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3]))
        .save(&file)
        .unwrap();
    let page = setup.page.clone();
    cx.update_window(setup.window, |_, window, cx| {
        page.update(cx, |page, cx| {
            page.encode_files(vec![file.clone()], Target::Picture, window, cx);
        });
    })
    .unwrap();
    frame(&setup, cx, |window, cx| {
        let point = window.find_by_label("Add").bounds().center();
        window.pointer_down(point, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(point, gpui_kit::MouseButton::Left, cx);
    });
    assert!(state(&setup, cx, |page| page.session.has_unsaved()));
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find("encoding-pond.png").label(),
            Some("Encoding pond.png…")
        );
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_file_that_is_not_a_picture_is_reported_at_the_control_and_nothing_is_added(
    cx: &mut TestAppContext,
) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-pictures", cx);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    std::fs::write(&file, b"not a picture").unwrap();
    act(&setup, cx, |page, window, cx| {
        page.encode_files(vec![file.clone()], Target::Picture, window, cx);
    });
    click_named(&setup, "Add", cx);
    frame(&setup, cx, |window, _| {
        let message = window.find("pictures-error").label().unwrap().to_owned();
        assert_eq!(
            message,
            "notes.txt was not added: it is not an AVIF, JPEG, PNG, GIF, WebP or BMP picture."
        );
    });
    assert!(!dialog_is_open(&setup, cx));
    assert!(!state(&setup, cx, |page| page.session.has_unsaved()));
}

#[gpui_kit::test]
fn a_cover_file_that_cannot_be_read_is_reported_at_the_cover(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-cover", cx);
    let missing = setup.fixture.roots.data().join("missing.png");
    act(&setup, cx, |page, window, cx| {
        page.encode_files(vec![missing.clone()], Target::Cover, window, cx);
    });
    click_named(&setup, "Add", cx);
    frame(&setup, cx, |window, _| {
        let message = window.find("cover-error").label().unwrap().to_owned();
        assert!(
            message.starts_with("missing.png was not added: it could not be read"),
            "{message}"
        );
    });
}

#[gpui_kit::test]
fn a_picture_is_added_to_the_book_by_its_path(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "section-pictures", cx);
    let avif = avif_of(8, 6);
    act(&setup, cx, |page, window, cx| {
        page.send(
            Command::AddPicture {
                path: "img/p.avif".to_owned(),
                avif: avif.clone(),
            },
            Origin::AddPicture,
            window,
            cx,
        );
    });
    frame(&setup, cx, |window, _| {
        assert_eq!(
            window.find_by_label("Remove img/p.avif").role(),
            Some(Role::Button)
        );
        assert!(window.try_find("no-pictures").is_none());
    });
    act(&setup, cx, |page, window, cx| {
        page.send(
            Command::AddPicture {
                path: "img/p.avif".to_owned(),
                avif,
            },
            Origin::AddPicture,
            window,
            cx,
        );
    });
    frame(&setup, cx, |window, _| {
        assert!(
            window
                .find("pictures-error")
                .label()
                .unwrap()
                .contains("already used")
        );
    });
}

#[gpui_kit::test]
fn a_save_waits_for_the_pictures_that_are_being_encoded(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    act(&setup, cx, |page, window, cx| {
        page.session.encoding = 1;
        page.save(window, cx);
    });
    assert!(state(&setup, cx, |page| page.saving.requested));
    assert!(!state(&setup, cx, |page| page.saving.running));
    frame(&setup, cx, |window, _| {
        assert_eq!(window.find("editor-status").label(), Some("Saving"));
    });
    act(&setup, cx, |page, window, cx| {
        page.session.encoding = 0;
        page.continue_save(window, cx);
    });
    assert!(!state(&setup, cx, |page| page.saving.requested));
    assert_eq!(
        setup.events.borrow().as_slice(),
        [EditorEvent::Saved(setup.key)]
    );
}

#[gpui_kit::test]
#[expect(
    clippy::used_underscore_binding,
    reason = "the field is only held, so that dropping the page ends the session; the test replaces it"
)]
fn dropping_the_task_that_owns_the_book_ends_the_session(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    let handle = state(&setup, cx, |page| page.handle.clone());
    // The page holds the task; when the page goes, this is what is dropped.
    act(&setup, cx, |page, _, _| {
        page._actor = gpui_kit::Task::ready(());
    });
    let reply = handle.chapter_text("ch01.md");
    let ended = cx.foreground_executor().block_test(reply);
    assert!(matches!(ended, Err(EditError::Ended)), "{ended:?}");
}

#[gpui_kit::test]
fn the_preview_is_the_chapter_view_of_the_reader(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    click(&setup, "mode-preview", cx);
    frame(&setup, cx, |window, _| {
        let preview = window.find("chapter-text");
        assert_eq!(preview.role(), Some(Role::Article));
        assert_eq!(preview.label(), Some("Preview of ch01.md"));
    });
}

#[gpui_kit::test]
fn record_save_keeps_the_day_and_the_format_of_the_row(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    let mut row = setup.fixture.catalogue.book(setup.key).unwrap().unwrap();
    row.added_ms = 42;
    row.source_format = Some("epub".to_owned());
    setup.fixture.catalogue.put_book(&row).unwrap();
    let mut book = BookReader::open(setup.fixture.roots.book(setup.key))
        .unwrap()
        .load()
        .unwrap();
    book.meta_mut().set_title("Changed").unwrap();
    book.save(setup.fixture.roots.book(setup.key)).unwrap();
    record_save(&setup.fixture.roots, &setup.fixture.catalogue, setup.key).unwrap();
    let after = setup.fixture.catalogue.book(setup.key).unwrap().unwrap();
    assert_eq!(after.title, "Changed");
    assert_eq!(after.added_ms, 42);
    assert_eq!(after.source_format.as_deref(), Some("epub"));
}

#[gpui_kit::test]
fn record_save_makes_the_thumbnail_of_the_cover_and_removes_it_without_one(
    cx: &mut TestAppContext,
) {
    let setup = started(cx, 1280.0);
    let thumbnail = setup.fixture.roots.cover(setup.key);
    let mut book = BookReader::open(setup.fixture.roots.book(setup.key))
        .unwrap()
        .load()
        .unwrap();
    book.set_cover("c.avif", avif_of(16, 16)).unwrap();
    book.save(setup.fixture.roots.book(setup.key)).unwrap();
    record_save(&setup.fixture.roots, &setup.fixture.catalogue, setup.key).unwrap();
    assert!(thumbnail.is_file());
    let row = setup.fixture.catalogue.book(setup.key).unwrap().unwrap();
    assert_eq!(row.cover, Some((16, 16)));
}

#[gpui_kit::test]
fn record_save_reports_a_file_that_is_gone(cx: &mut TestAppContext) {
    let setup = started(cx, 1280.0);
    std::fs::remove_file(setup.fixture.roots.book(setup.key)).unwrap();
    let error = record_save(&setup.fixture.roots, &setup.fixture.catalogue, setup.key).unwrap_err();
    assert_eq!(error.to_string(), "the saved file could not be measured");
}
