//! The view of a chapter, in headless windows.

use std::fmt::Write as _;
use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ClickEvent, Context, Entity, IntoElement, Render, Role,
    TestAppContext, Window,
};

use super::*;
use crate::features::appearance::Appearance;
use crate::testing::{self, Fixture};

/// A window whose only content is a view.
struct Host {
    view: Entity<ChapterView>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        self.view.clone()
    }
}

struct Started {
    _fixture: Fixture,
    window: AnyWindowHandle,
    view: Entity<ChapterView>,
}

/// A chapter of a heading and `paragraphs` paragraphs.
fn chapter(paragraphs: usize) -> String {
    let mut text = String::from("# Heading\n\n");
    for n in 1..=paragraphs {
        writeln!(text, "Paragraph {n} says something that takes room.\n").unwrap();
    }
    text
}

fn offset_of(text: &str, needle: &str) -> u32 {
    u32::try_from(text.find(needle).unwrap()).unwrap()
}

fn start(cx: &mut TestAppContext, width: f32, height: f32) -> Started {
    let fixture = Fixture::new();
    cx.update(gpui_kit::init);
    fixture.install(cx);
    cx.update(|cx| {
        let appearance = cx.new(Appearance::new);
        Appearance::install(appearance, cx);
    });
    cx.run_until_parked();
    let (window, host) = testing::open(cx, width, height, |window, cx| Host {
        view: cx.new(|cx| ChapterView::new(window, cx)),
    });
    let view = host.read_with(cx, |host, _| host.view.clone());
    Started {
        _fixture: fixture,
        window,
        view,
    }
}

/// Draws a few frames, letting the background and the next frame run.
fn settle(started: &Started, cx: &mut TestAppContext) {
    for _ in 0..6 {
        cx.run_until_parked();
        testing::frame(cx, started.window, |_, _| ());
    }
}

fn top(started: &Started, cx: &mut TestAppContext) -> u32 {
    started.view.read_with(cx, ChapterView::top_offset)
}

#[gpui_kit::test]
fn the_text_of_a_chapter_is_shown_at_each_width(cx: &mut TestAppContext) {
    for width in [390.0, 800.0, 1280.0] {
        let started = start(cx, width, 600.0);
        started
            .view
            .update(cx, |view, cx| view.set_text(&chapter(3), cx));
        settle(&started, cx);
        let shown = started.view.read_with(cx, ChapterView::shown_text);
        assert!(shown.contains("Paragraph 2"), "{width}: {shown:?}");
    }
}

#[gpui_kit::test]
fn the_text_is_one_article_named_by_its_label(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 600.0);
    started.view.update(cx, |view, cx| {
        view.set_label("Chapter One", cx);
        view.set_text(&chapter(2), cx);
    });
    settle(&started, cx);
    testing::frame(cx, started.window, |window, _| {
        let article = window.find("chapter-text");
        assert_eq!(article.role(), Some(Role::Article));
        assert_eq!(article.label(), Some("Chapter One"));
    });
}

#[gpui_kit::test]
fn the_text_does_not_take_more_than_the_column(cx: &mut TestAppContext) {
    let started = start(cx, 1280.0, 600.0);
    started
        .view
        .update(cx, |view, cx| view.set_text(&chapter(2), cx));
    settle(&started, cx);
    testing::frame(cx, started.window, |window, _| {
        // 38 em of 18 pixels.
        let width = window.find("chapter-text").bounds().size.width;
        assert!(width >= gpui_kit::px(1200.0), "{width:?}");
    });
}

#[gpui_kit::test]
fn a_block_is_revealed_at_the_top_of_the_window(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    let text = chapter(60);
    let blocks = blocks::block_offsets(&text);
    let target = offset_of(&text, "Paragraph 30 says");
    started.view.update(cx, |view, cx| {
        view.set_chapter(&text, blocks, cx);
        view.reveal(target + 5, cx);
    });
    settle(&started, cx);
    assert_eq!(top(&started, cx), target);
}

#[gpui_kit::test]
fn the_place_asked_for_is_the_place_until_the_text_is_laid_out(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    let text = chapter(60);
    started.view.update(cx, |view, cx| {
        view.set_text(&text, cx);
        view.reveal(1234, cx);
    });
    assert_eq!(top(&started, cx), 1234);
}

#[gpui_kit::test]
fn a_text_without_given_blocks_finds_them_on_the_background(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    let text = chapter(60);
    let target = offset_of(&text, "Paragraph 40 says");
    started.view.update(cx, |view, cx| {
        view.set_text(&text, cx);
        view.reveal(target, cx);
    });
    settle(&started, cx);
    assert_eq!(top(&started, cx), target);
}

#[gpui_kit::test]
fn a_screen_forward_moves_the_text_and_a_screen_back_returns(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    let text = chapter(60);
    started.view.update(cx, |view, cx| {
        view.set_chapter(&text, blocks::block_offsets(&text), cx);
    });
    settle(&started, cx);
    assert!(started.view.read_with(cx, ChapterView::at_start));

    started
        .view
        .update(cx, |view, cx| view.scroll_by_screen(Direction::Forward, cx));
    settle(&started, cx);
    let moved = top(&started, cx);
    assert!(moved > 0, "{moved}");
    assert!(!started.view.read_with(cx, ChapterView::at_start));

    started
        .view
        .update(cx, |view, cx| view.scroll_by_screen(Direction::Back, cx));
    settle(&started, cx);
    assert_eq!(top(&started, cx), 0);
}

#[gpui_kit::test]
fn a_screen_is_a_little_less_than_the_window(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    let text = chapter(60);
    started.view.update(cx, |view, cx| {
        view.set_chapter(&text, blocks::block_offsets(&text), cx);
    });
    settle(&started, cx);
    let before = top(&started, cx);
    started
        .view
        .update(cx, |view, cx| view.scroll_by_screen(Direction::Forward, cx));
    settle(&started, cx);
    // About 15 lines of a window of 400 pixels at 29 pixels a line: the top
    // block has moved, but not past the whole chapter.
    let after = top(&started, cx);
    assert!(after > before && after < offset_of(&text, "Paragraph 30 says"));
}

#[gpui_kit::test]
fn the_end_of_a_chapter_is_the_end(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    let text = chapter(30);
    started.view.update(cx, |view, cx| {
        view.set_chapter(&text, blocks::block_offsets(&text), cx);
    });
    settle(&started, cx);
    assert!(!started.view.read_with(cx, ChapterView::at_end));
    started.view.update(cx, ChapterView::reveal_end);
    settle(&started, cx);
    assert!(started.view.read_with(cx, ChapterView::at_end));
}

#[gpui_kit::test]
fn a_chapter_that_fits_the_window_is_at_its_start_and_its_end(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 600.0);
    started
        .view
        .update(cx, |view, cx| view.set_text("Short.\n", cx));
    settle(&started, cx);
    assert!(started.view.read_with(cx, ChapterView::at_start));
    assert!(started.view.read_with(cx, ChapterView::at_end));
}

#[gpui_kit::test]
fn a_text_that_is_waiting_for_its_place_is_not_at_its_end(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    started.view.update(cx, |view, cx| {
        view.set_text("Short.\n", cx);
        view.reveal(0, cx);
    });
    assert!(!started.view.read_with(cx, ChapterView::at_end));
}

#[gpui_kit::test]
fn a_new_text_starts_at_the_top(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    let text = chapter(60);
    started.view.update(cx, |view, cx| {
        view.set_chapter(&text, blocks::block_offsets(&text), cx);
        view.reveal(offset_of(&text, "Paragraph 30 says"), cx);
    });
    settle(&started, cx);
    let other = chapter(50);
    started.view.update(cx, |view, cx| {
        view.set_chapter(&other, blocks::block_offsets(&other), cx);
        view.reveal_start(cx);
    });
    settle(&started, cx);
    assert_eq!(top(&started, cx), 0);
}

#[gpui_kit::test]
fn the_list_has_one_item_for_each_block_the_book_found(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    let text =
        "# T\n\nOne.\n\n- a\n- b\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n```\ncode\n```\n\n> quote\n";
    let blocks = blocks::block_offsets(text);
    started
        .view
        .update(cx, |view, cx| view.set_chapter(text, blocks.clone(), cx));
    settle(&started, cx);
    let items = started.view.read_with(cx, ChapterView::item_count);
    assert_eq!(items, blocks.len(), "the Kit and mbook agree on the blocks");
}

fn picture_resolver(
    state: impl Fn() -> Option<PictureState> + Send + Sync + 'static,
) -> PictureResolver {
    Arc::new(move |_, _, _, _| state())
}

const WITH_PICTURE: &str = "Before.\n\n![A grey cat](cat.avif)\n\nAfter.\n";

#[gpui_kit::test]
fn a_picture_that_is_not_ready_holds_its_place_at_its_size(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 600.0);
    started.view.update(cx, |view, cx| {
        view.set_pictures(
            picture_resolver(|| Some(PictureState::Pending((1600, 800)))),
            cx,
        );
        view.set_text(WITH_PICTURE, cx);
    });
    settle(&started, cx);
    let sizes = started
        .view
        .read_with(cx, |view, _| view.placeholder_sizes());
    // The column of 684 pixels, and the proportions of the picture.
    assert_eq!(sizes, vec![(684, 342)]);
}

#[gpui_kit::test]
fn a_ready_picture_is_drawn_without_a_placeholder(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 600.0);
    let image = crate::entities::picture::Bitmap::from_rgba(4, 2, vec![255; 32])
        .unwrap()
        .into_render_image();
    started.view.update(cx, |view, cx| {
        view.set_pictures(
            picture_resolver(move || Some(PictureState::Ready(Arc::clone(&image)))),
            cx,
        );
        view.set_text(WITH_PICTURE, cx);
    });
    settle(&started, cx);
    assert_eq!(
        started
            .view
            .read_with(cx, |view, _| view.placeholder_sizes()),
        Vec::new()
    );
}

#[gpui_kit::test]
fn a_picture_that_cannot_be_shown_is_replaced_by_its_alternative_text(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 600.0);
    started.view.update(cx, |view, cx| {
        view.set_pictures(picture_resolver(|| Some(PictureState::Failed)), cx);
        view.set_text(WITH_PICTURE, cx);
    });
    settle(&started, cx);
    let shown = started.view.read_with(cx, ChapterView::shown_text);
    assert!(shown.contains("A grey cat"), "{shown:?}");
    assert!(shown.contains("Before.") && shown.contains("After."));
}

#[gpui_kit::test]
fn a_picture_that_failed_keeps_the_place_of_the_reader(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    let mut text = String::from("![A grey cat](cat.avif)\n\n");
    text.push_str(&chapter(60));
    let target = offset_of(&text, "Paragraph 30 says");
    started.view.update(cx, |view, cx| {
        view.set_pictures(picture_resolver(|| Some(PictureState::Failed)), cx);
        view.set_text(&text, cx);
        view.reveal(target, cx);
    });
    settle(&started, cx);
    assert_eq!(top(&started, cx), target);
}

#[gpui_kit::test]
fn a_picture_the_owner_does_not_know_is_not_fetched(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 600.0);
    started.view.update(cx, |view, cx| {
        view.set_pictures(picture_resolver(|| None), cx);
        view.set_text("![remote](https://example.org/x.png)\n", cx);
    });
    settle(&started, cx);
    assert_eq!(
        started
            .view
            .read_with(cx, |view, _| view.placeholder_sizes()),
        Vec::new()
    );
}

#[gpui_kit::test]
fn a_change_of_the_appearance_restyles_the_text_at_once(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    let text = chapter(60);
    let target = offset_of(&text, "Paragraph 30 says");
    started.view.update(cx, |view, cx| {
        view.set_chapter(&text, blocks::block_offsets(&text), cx);
        view.reveal(target, cx);
    });
    settle(&started, cx);
    cx.update(|cx| {
        Appearance::global(cx).update(cx, |appearance, cx| {
            appearance.update(|settings| settings.text_size = 28.0, cx);
        });
    });
    settle(&started, cx);
    // The reader is still at the block it was at.
    assert_eq!(top(&started, cx), target);
}

#[test]
fn a_main_click_a_key_and_a_tap_follow_a_link_and_a_long_press_does_not() {
    use gpui_kit::{KeyboardClickEvent, TouchClickEvent};
    assert!(followed(&ClickEvent::Keyboard(
        KeyboardClickEvent::default()
    )));
    let touch = |long_press| {
        ClickEvent::Touch(TouchClickEvent {
            position: gpui_kit::Point::default(),
            tap_count: 1,
            long_press,
        })
    };
    assert!(followed(&touch(false)));
    assert!(!followed(&touch(true)));
}
