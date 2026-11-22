//! The view of a chapter, in headless windows.

use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ClickEvent, Context, Entity, IntoElement, Render, Role,
    TestAppContext, Window,
};

use super::*;
use crate::entities::figure::{FigureKind, FigureSource, ImageReference};
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
    start_with(cx, width, height, ChapterView::new)
}

/// A window with a view that shows its text a page at a time.
fn start_paged(cx: &mut TestAppContext, width: f32, height: f32) -> Started {
    start_with(cx, width, height, ChapterView::paged)
}

fn start_with(
    cx: &mut TestAppContext,
    width: f32,
    height: f32,
    build: fn(&mut Window, &mut Context<'_, ChapterView>) -> ChapterView,
) -> Started {
    let fixture = Fixture::new();
    cx.update(gpui_kit::init);
    fixture.install(cx);
    cx.update(|cx| {
        let appearance = cx.new(Appearance::new);
        Appearance::install(appearance, cx);
    });
    cx.run_until_parked();
    let (window, host) = testing::open(cx, width, height, |window, cx| Host {
        view: cx.new(|cx| build(window, cx)),
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
    for width in [320.0, 360.0, 390.0, 430.0, 768.0, 1280.0, 1920.0] {
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
        let width = window.find("chapter-text").bounds().size.width;
        assert!(width >= gpui_kit::px(1200.0), "{width:?}");
    });
}

#[gpui_kit::test]
fn the_requested_offset_within_a_block_is_restored(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    let text = chapter(60);
    let blocks = blocks::block_offsets(&text);
    let target = offset_of(&text, "Paragraph 30 says");
    started.view.update(cx, |view, cx| {
        view.set_chapter(&text, blocks, cx);
        view.reveal(target + 5, cx);
    });
    settle(&started, cx);
    assert_eq!(top(&started, cx), target + 5);
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
        .update(cx, |view, cx| view.turn_page(Direction::Forward, cx));
    settle(&started, cx);
    let moved = top(&started, cx);
    assert!(moved > 0, "{moved}");
    assert!(!started.view.read_with(cx, ChapterView::at_start));

    started
        .view
        .update(cx, |view, cx| view.turn_page(Direction::Back, cx));
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
        .update(cx, |view, cx| view.turn_page(Direction::Forward, cx));
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
    Arc::new(move |_, _, _, _, _| state())
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
    // The window width minus its two 32-pixel margins, with the picture at half that height.
    assert_eq!(sizes, vec![(720, 360)]);
}

#[gpui_kit::test]
fn a_square_picture_taller_than_the_page_stays_on_one_page(cx: &mut TestAppContext) {
    let started = start_paged(cx, 1000.0, 400.0);
    started.view.update(cx, |view, cx| {
        view.set_pictures(
            picture_resolver(|| Some(PictureState::Pending((1024, 1024)))),
            cx,
        );
        view.set_text(WITH_PICTURE, cx);
    });
    settle(&started, cx);
    started.view.read_with(cx, |view, _| {
        let paging = view.paging.as_ref().unwrap();
        assert!(paging.heights[1] <= 400.0, "{:?}", paging.heights);
        assert!(paging.pages.iter().all(|page| page.offset == 0.0));
    });
}

#[gpui_kit::test]
fn a_picture_on_the_last_page_stays_inside_the_visible_page(cx: &mut TestAppContext) {
    let started = start_paged(cx, 1000.0, 400.0);
    started.view.update(cx, |view, cx| {
        view.set_pictures(
            picture_resolver(|| Some(PictureState::Pending((1024, 1024)))),
            cx,
        );
        view.set_text("Before.\n\nMore.\n\n![A grey cat](cat.avif)\n", cx);
    });
    settle(&started, cx);
    started.view.update(cx, ChapterView::reveal_end);
    settle(&started, cx);
    started.view.read_with(cx, |view, cx| {
        let paging = view.paging.as_ref().unwrap();
        let page = paging.pages[paging.page];
        assert_eq!(page.item, 2, "{:?}", paging.pages);
        let list = view.state.read(cx).list_state();
        let picture = list.bounds_for_item(2).unwrap();
        let origin = list.viewport_bounds().top();
        assert!(
            picture.bottom() <= origin + px(page.extent),
            "{picture:?}, {page:?}"
        );
    });
}

#[gpui_kit::test]
fn adjacent_full_page_pictures_are_shown_on_separate_pages(cx: &mut TestAppContext) {
    let started = start_paged(cx, 1000.0, 400.0);
    started.view.update(cx, |view, cx| {
        view.set_pictures(
            picture_resolver(|| Some(PictureState::Pending((1024, 1024)))),
            cx,
        );
        view.set_text(
            "# Chapter\n\nBefore the pictures.\n\n![First](first.avif)\n\n![Second](second.avif)\n",
            cx,
        );
    });
    settle(&started, cx);
    for picture in [2, 3] {
        started
            .view
            .update(cx, |view, cx| view.turn_page(Direction::Forward, cx));
        settle(&started, cx);
        started.view.read_with(cx, |view, cx| {
            let list = view.state.read(cx).list_state();
            assert_eq!(
                list.logical_scroll_top(),
                ListOffset {
                    item_ix: picture,
                    offset_in_item: px(0.0)
                },
            );
            let image_block = list.bounds_for_item(picture).unwrap();
            assert!(image_block.bottom() <= list.viewport_bounds().bottom());
        });
    }
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

/// A chapter of a heading and `paragraphs` paragraphs of several lines each.
fn long_chapter(paragraphs: usize) -> String {
    let mut text = String::from("# Heading\n\n");
    for n in 1..=paragraphs {
        write!(text, "Paragraph {n} begins.").unwrap();
        for _ in 0..12 {
            text.push_str(" It goes on with words enough to fill a line of the column and more.");
        }
        text.push_str("\n\n");
    }
    text
}

fn page_number(started: &Started, cx: &mut TestAppContext) -> (usize, usize) {
    started
        .view
        .read_with(cx, |view, _| view.page_number())
        .unwrap()
}

/// Shows `text` a page at a time in a window of 800 by 400 pixels.
fn paged_with(cx: &mut TestAppContext, text: &str) -> Started {
    let started = start_paged(cx, 800.0, 400.0);
    started.view.update(cx, |view, cx| {
        view.set_chapter(text, blocks::block_offsets(text), cx);
    });
    settle(&started, cx);
    started
}

#[gpui_kit::test]
fn a_paragraph_is_as_high_as_its_lines_and_the_reading_gap(cx: &mut TestAppContext) {
    // What the pages are divided by: a line of 20 pixels at 1.6 is 32 pixels
    // high, and a paragraph that another follows has 0.7 em under it.
    let started = paged_with(cx, "One line.\n\nAnother.\n");
    let heights = started
        .view
        .read_with(cx, |view, _| view.paging.as_ref().unwrap().heights.clone());
    assert_eq!(heights, vec![32.0 + 14.0, 32.0]);
}

#[gpui_kit::test]
fn a_long_chapter_is_divided_into_pages(cx: &mut TestAppContext) {
    let started = paged_with(cx, &long_chapter(12));
    let (page, count) = page_number(&started, cx);
    assert_eq!(page, 1);
    assert!(count > 3, "{count}");
    assert!(started.view.read_with(cx, ChapterView::at_start));
    assert!(!started.view.read_with(cx, ChapterView::at_end));
}

#[gpui_kit::test]
fn a_page_ends_between_two_lines_of_a_paragraph(cx: &mut TestAppContext) {
    let started = paged_with(cx, &long_chapter(12));
    started.view.read_with(cx, |view, _| {
        let paging = view.paging.as_ref().unwrap();
        let divided: Vec<_> = paging
            .pages
            .iter()
            .filter(|page| page.offset > 0.0)
            .collect();
        assert!(!divided.is_empty(), "no paragraph runs over a page");
        for page in &paging.pages {
            assert!(page.extent <= 400.0, "{page:?}");
        }
        for page in divided {
            let lines = page.offset / 32.0;
            assert!((lines - lines.round()).abs() < 0.01, "{page:?}");
        }
    });
}

#[gpui_kit::test]
fn the_list_of_the_kit_starts_where_the_page_starts(cx: &mut TestAppContext) {
    let started = paged_with(cx, &long_chapter(12));
    started
        .view
        .update(cx, |view, cx| view.turn_page(Direction::Forward, cx));
    settle(&started, cx);
    let (page, top) = started.view.read_with(cx, |view, cx| {
        let paging = view.paging.as_ref().unwrap();
        (
            paging.pages[paging.page],
            view.state.read(cx).list_state().logical_scroll_top(),
        )
    });
    assert_eq!(top.item_ix, page.item);
    assert!((top.offset_in_item.as_f32() - page.offset).abs() < 0.01);
}

#[gpui_kit::test]
fn a_page_is_turned_forward_and_back(cx: &mut TestAppContext) {
    let text = long_chapter(12);
    let started = paged_with(cx, &text);
    started
        .view
        .update(cx, |view, cx| view.turn_page(Direction::Forward, cx));
    settle(&started, cx);
    assert_eq!(page_number(&started, cx).0, 2);
    let second = top(&started, cx);
    assert!(second > 0);
    assert!(!started.view.read_with(cx, ChapterView::at_start));

    started
        .view
        .update(cx, |view, cx| view.turn_page(Direction::Back, cx));
    settle(&started, cx);
    assert_eq!(page_number(&started, cx).0, 1);
    assert_eq!(top(&started, cx), 0);
}

#[gpui_kit::test]
fn the_first_page_has_none_before_it_and_the_last_none_after_it(cx: &mut TestAppContext) {
    let started = paged_with(cx, &long_chapter(12));
    started
        .view
        .update(cx, |view, cx| view.turn_page(Direction::Back, cx));
    assert_eq!(page_number(&started, cx).0, 1);

    started.view.update(cx, ChapterView::reveal_end);
    settle(&started, cx);
    let (page, count) = page_number(&started, cx);
    assert_eq!(page, count);
    assert!(started.view.read_with(cx, ChapterView::at_end));
    started
        .view
        .update(cx, |view, cx| view.turn_page(Direction::Forward, cx));
    assert_eq!(page_number(&started, cx).0, count);
}

#[gpui_kit::test]
fn a_place_is_shown_on_its_page_and_kept_as_the_first_line_of_it(cx: &mut TestAppContext) {
    let text = long_chapter(12);
    let started = paged_with(cx, &text);
    for page in 1..=page_number(&started, cx).1 {
        assert_eq!(page_number(&started, cx).0, page);
        // The place that is kept leads back to the page it was kept on.
        let kept = top(&started, cx);
        started.view.update(cx, |view, cx| view.reveal(kept, cx));
        settle(&started, cx);
        assert_eq!(page_number(&started, cx).0, page, "the place {kept}");
        started
            .view
            .update(cx, |view, cx| view.turn_page(Direction::Forward, cx));
        settle(&started, cx);
    }
}

#[gpui_kit::test]
fn a_block_is_shown_on_the_page_that_holds_it(cx: &mut TestAppContext) {
    let text = long_chapter(12);
    let started = paged_with(cx, &text);
    let target = offset_of(&text, "Paragraph 9 begins");
    started.view.update(cx, |view, cx| view.reveal(target, cx));
    settle(&started, cx);
    let (page, count) = page_number(&started, cx);
    assert!(page > 1 && page <= count, "{page} of {count}");
    let place = top(&started, cx);
    assert!(place <= target, "{place} is after {target}");
}

#[gpui_kit::test]
fn a_paged_text_is_not_shown_before_it_has_its_pages(cx: &mut TestAppContext) {
    let started = start_paged(cx, 800.0, 400.0);
    started
        .view
        .update(cx, |view, cx| view.set_text(&long_chapter(12), cx));
    assert_eq!(
        started.view.read_with(cx, |view, _| view.page_number()),
        None
    );
    assert!(!started.view.read_with(cx, ChapterView::at_end));
    settle(&started, cx);
    assert_eq!(page_number(&started, cx).0, 1);
}

#[gpui_kit::test]
fn a_short_paged_text_is_one_page(cx: &mut TestAppContext) {
    let started = paged_with(cx, "Short.\n");
    assert_eq!(page_number(&started, cx), (1, 1));
    assert!(started.view.read_with(cx, ChapterView::at_start));
    assert!(started.view.read_with(cx, ChapterView::at_end));
}

#[gpui_kit::test]
fn an_empty_paged_text_is_one_page(cx: &mut TestAppContext) {
    let started = paged_with(cx, "");
    assert_eq!(page_number(&started, cx), (1, 1));
}

#[gpui_kit::test]
fn a_scrolled_text_has_no_pages(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    started
        .view
        .update(cx, |view, cx| view.set_text(&chapter(60), cx));
    settle(&started, cx);
    assert_eq!(
        started.view.read_with(cx, |view, _| view.page_number()),
        None
    );
}

#[gpui_kit::test]
fn a_larger_text_makes_more_pages_and_keeps_the_place(cx: &mut TestAppContext) {
    let text = long_chapter(12);
    let started = paged_with(cx, &text);
    let target = offset_of(&text, "Paragraph 7 begins");
    started.view.update(cx, |view, cx| view.reveal(target, cx));
    settle(&started, cx);
    let (_, before) = page_number(&started, cx);
    cx.update(|cx| {
        Appearance::global(cx).update(cx, |appearance, cx| {
            appearance.update(|settings| settings.text_size = 28.0, cx);
        });
    });
    settle(&started, cx);
    let (_, after) = page_number(&started, cx);
    assert!(after > before, "{after} pages after {before}");
    // The page still shows the paragraph it showed.
    let place = top(&started, cx);
    let next = offset_of(&text, "Paragraph 8 begins");
    assert!(place <= target && target < next, "{place}");
    assert!(place >= offset_of(&text, "Paragraph 6 begins"), "{place}");
}

#[gpui_kit::test]
fn a_shorter_window_recalculates_pages_around_the_reading_place(cx: &mut TestAppContext) {
    let started = paged_with(cx, &long_chapter(40));
    started
        .view
        .update(cx, |view, cx| view.turn_page(Direction::Forward, cx));
    settle(&started, cx);
    let before = top(&started, cx);
    let (_, total) = page_number(&started, cx);
    cx.simulate_window_resize(
        started.window,
        gpui_kit::size(gpui_kit::px(800.0), gpui_kit::px(300.0)),
    );
    settle(&started, cx);
    assert!(page_number(&started, cx).1 > total);
    assert!(top(&started, cx) <= before);
    started
        .view
        .update(cx, |view, cx| view.turn_page(Direction::Forward, cx));
    assert!(top(&started, cx) > before);
}

#[gpui_kit::test]
fn a_new_paged_text_starts_at_its_first_page(cx: &mut TestAppContext) {
    let started = paged_with(cx, &long_chapter(12));
    started.view.update(cx, ChapterView::reveal_end);
    settle(&started, cx);
    let other = long_chapter(8);
    started.view.update(cx, |view, cx| {
        view.set_chapter(&other, blocks::block_offsets(&other), cx);
    });
    settle(&started, cx);
    assert_eq!(page_number(&started, cx).0, 1);
    assert_eq!(top(&started, cx), 0);
}

/// What the view emitted, collected.
fn events(started: &Started, cx: &mut TestAppContext) -> Arc<Mutex<Vec<ChapterViewEvent>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    cx.update(|cx| {
        cx.subscribe(&started.view, move |_, event, _| {
            sink.lock().unwrap().push(*event);
        })
        .detach();
    });
    seen
}

fn turns(events: &Arc<Mutex<Vec<ChapterViewEvent>>>) -> Vec<Direction> {
    events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| match event {
            ChapterViewEvent::Turn(direction) => Some(*direction),
            ChapterViewEvent::Moved => None,
        })
        .collect()
}

#[gpui_kit::test]
fn the_wheel_scrolls_the_preview_without_turning_a_page(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    started.view.update(cx, |view, cx| {
        view.set_text(&long_chapter(12), cx);
    });
    settle(&started, cx);
    let seen = events(&started, cx);
    cx.update_window(started.window, |_, window, cx| {
        window.scroll(
            "chapter-text",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.0), gpui_kit::px(-120.0))),
            cx,
        );
    })
    .unwrap();
    settle(&started, cx);
    assert!(top(&started, cx) > 0);
    assert_eq!(turns(&seen), Vec::<Direction>::new());
}

#[gpui_kit::test]
fn the_wheel_over_a_paged_text_asks_for_a_page_and_scrolls_nothing(cx: &mut TestAppContext) {
    let started = paged_with(cx, &long_chapter(12));
    let seen = events(&started, cx);
    cx.update_window(started.window, |_, window, cx| {
        window.scroll(
            "chapter-text",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.0), gpui_kit::px(-120.0))),
            cx,
        );
    })
    .unwrap();
    settle(&started, cx);
    assert_eq!(turns(&seen), vec![Direction::Forward]);
    // The owner turns the page; the list of the Kit has not moved.
    assert_eq!(page_number(&started, cx).0, 1);
    let at = started.view.read_with(cx, |view, cx| {
        view.state.read(cx).list_state().logical_scroll_top()
    });
    assert_eq!(at.item_ix, 0);
    assert!(at.offset_in_item.as_f32().abs() < 0.01);
}

#[gpui_kit::test]
fn a_click_beside_a_paged_text_asks_for_a_page(cx: &mut TestAppContext) {
    let started = paged_with(cx, &long_chapter(12));
    let seen = events(&started, cx);
    cx.update_window(started.window, |_, window, cx| {
        // The column has margins of 32 pixels.
        for x in [780.0, 20.0] {
            let at = gpui_kit::point(gpui_kit::px(x), gpui_kit::px(200.0));
            window.pointer_down(at, gpui_kit::MouseButton::Left, cx);
            window.pointer_up(at, gpui_kit::MouseButton::Left, cx);
        }
    })
    .unwrap();
    assert_eq!(turns(&seen), vec![Direction::Forward, Direction::Back]);
}

#[gpui_kit::test]
fn a_turn_is_drawn_in_two_steps_when_motion_is_allowed(cx: &mut TestAppContext) {
    let started = paged_with(cx, &long_chapter(12));
    cx.update(|cx| {
        cx.set_reduce_motion(false);
        Appearance::global(cx).update(cx, |appearance, cx| {
            appearance.update(
                |settings| settings.reading.flow = ReadingFlow::AnimatedPages,
                cx,
            );
        });
    });
    started
        .view
        .update(cx, |view, cx| view.turn_page(Direction::Forward, cx));
    // The page is turned at once; the one before it is drawn while it leaves.
    assert_eq!(page_number(&started, cx).0, 2);
    let leaving = |cx: &mut TestAppContext| {
        testing::frame(cx, started.window, |_, _| ());
        started.view.read_with(cx, |view, cx| {
            let top = view.state.read(cx).list_state().logical_scroll_top();
            (top.item_ix, top.offset_in_item.as_f32())
        })
    };
    assert_eq!(leaving(cx), (0, 0.0));
    cx.executor().advance_clock(LEAVE + ARRIVE + LEAVE);
    let arrived = leaving(cx);
    assert_ne!(arrived, (0, 0.0));
    testing::frame(cx, started.window, |_, _| ());
    assert!(
        started
            .view
            .read_with(cx, |view, _| view.paging.as_ref().unwrap().turn.is_none())
    );
}

/// The sources of a formula and of a diagram, by the destinations a text
/// shows them with.
fn figure_sources(formula: &str, diagram: &str) -> FigureSources {
    let figure = |kind, source: &str| FigureSource {
        kind,
        source: source.into(),
    };
    FigureSources {
        by_destination: HashMap::from([
            (
                "math/e.tex".to_owned(),
                figure(FigureKind::Formula, formula),
            ),
            (
                "dia/way.mmd".to_owned(),
                figure(FigureKind::Diagram, diagram),
            ),
        ]),
        by_reference: HashMap::from([(
            "way".to_owned(),
            ImageReference {
                destination: "dia/way.mmd".to_owned(),
                title: Some("The route through the station".to_owned()),
            },
        )]),
    }
}

const FLOW: &str = "flowchart TD\n  A[Start] --> B[Middle]\n  B --> C[End]";
const WITH_FIGURES: &str = "Before.\n\n![The way](dia/way.mmd)\n\nMass is ![Energy](math/e.tex) here.\n\n![The energy](math/e.tex)\n\nAfter.\n";

/// The figures that are drawn as pictures, by their names.
fn drawn_figures(started: &Started, cx: &mut TestAppContext) -> Vec<(String, f32, f32)> {
    testing::frame(cx, started.window, |window, _| {
        window
            .find_all_by_role(Role::Image)
            .iter()
            .map(|image| {
                let size = image.bounds().size;
                (
                    image.label().unwrap_or_default().to_owned(),
                    size.width.as_f32(),
                    size.height.as_f32(),
                )
            })
            .collect()
    })
}

#[gpui_kit::test]
fn a_diagram_and_a_formula_alone_in_a_paragraph_are_drawn_as_blocks(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 900.0);
    started.view.update(cx, |view, cx| {
        view.set_figures(figure_sources("E = mc^2", FLOW), cx);
        view.set_text(WITH_FIGURES, cx);
    });
    settle(&started, cx);
    let drawn = drawn_figures(&started, cx);
    let names: Vec<&str> = drawn.iter().map(|figure| figure.0.as_str()).collect();
    assert_eq!(names, ["The way", "The energy"]);
    for (name, width, height) in &drawn {
        // Each takes the whole column and is as high as its drawing.
        assert!((*width - 720.0).abs() < 1.0, "{name}: {width}");
        assert!(*height > 10.0, "{name}: {height}");
    }
    assert!(
        drawn[0].2 > drawn[1].2,
        "a diagram is taller than a formula"
    );
}

#[gpui_kit::test]
fn the_text_around_a_figure_is_shown_with_its_alternative_text(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 900.0);
    started.view.update(cx, |view, cx| {
        view.set_figures(figure_sources("E = mc^2", FLOW), cx);
        view.set_text(WITH_FIGURES, cx);
    });
    settle(&started, cx);
    let shown = started.view.read_with(cx, ChapterView::shown_text);
    for words in ["Before.", "Mass is", "Energy", "here.", "After."] {
        assert!(shown.contains(words), "{words}: {shown:?}");
    }
    assert!(!shown.contains("math/e.tex"), "{shown:?}");
}

#[gpui_kit::test]
fn a_figure_in_a_line_does_not_make_a_block_of_its_own(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 600.0);
    started.view.update(cx, |view, cx| {
        view.set_figures(figure_sources("x", FLOW), cx);
        view.set_text("Let ![x](math/e.tex) be a number.\n", cx);
    });
    settle(&started, cx);
    assert_eq!(drawn_figures(&started, cx), Vec::new());
    assert_eq!(started.view.read_with(cx, ChapterView::item_count), 1);
}

#[gpui_kit::test]
fn a_figure_shown_through_a_definition_is_drawn(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 900.0);
    started.view.update(cx, |view, cx| {
        view.set_figures(figure_sources("x", FLOW), cx);
        view.set_text(
            "![The way][way]\n\n[way]: dia/way.mmd \"The route through the station\"\n",
            cx,
        );
    });
    settle(&started, cx);
    let drawn = drawn_figures(&started, cx);
    assert_eq!(drawn.len(), 1);
    assert_eq!(drawn[0].0, "The way");
    let shown = started.view.read_with(cx, ChapterView::shown_text);
    assert!(shown.contains("The route through the station"));
}

#[gpui_kit::test]
fn a_click_on_a_diagram_does_not_follow_a_mermaid_link(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 600.0);
    let followed = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&followed);
    let source = format!("{FLOW}\n  click A \"https://example.com/diagram\"");
    started.view.update(cx, |view, cx| {
        view.on_link(move |url, _, _| seen.lock().unwrap().push(url.to_owned()));
        view.set_figures(figure_sources("x", &source), cx);
        view.set_text("![The way](dia/way.mmd)\n", cx);
    });
    settle(&started, cx);
    testing::frame(cx, started.window, |window, cx| {
        let image = window.find_by_label("The way");
        assert_eq!(image.role(), Some(Role::Image));
        let at = image.bounds().center();
        window.pointer_down(at, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(at, gpui_kit::MouseButton::Left, cx);
    });
    assert_eq!(*followed.lock().unwrap(), Vec::<String>::new());
}

#[gpui_kit::test]
fn a_figure_that_cannot_be_drawn_shows_its_alternative_text(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 600.0);
    started.view.update(cx, |view, cx| {
        view.set_figures(figure_sources(r"\frac{", "no diagram at all"), cx);
        view.set_text(WITH_FIGURES, cx);
    });
    settle(&started, cx);
    assert_eq!(drawn_figures(&started, cx), Vec::new());
    let shown = started.view.read_with(cx, ChapterView::shown_text);
    assert!(
        shown.contains("The way") && shown.contains("Energy"),
        "{shown:?}"
    );
}

#[gpui_kit::test]
fn a_text_waits_for_its_figures_and_a_text_without_them_does_not(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 600.0);
    started.view.update(cx, |view, cx| {
        view.set_figures(figure_sources("x", FLOW), cx);
        view.set_text(WITH_FIGURES, cx);
        assert!(view.held, "the figures are not drawn yet");
    });
    settle(&started, cx);
    started.view.update(cx, |view, cx| {
        assert!(!view.held);
        // The same figures are drawn already: the next text is not kept.
        view.set_text("Only ![x](math/e.tex).\n", cx);
        assert!(!view.held);
        view.set_figures(FigureSources::default(), cx);
        view.set_text("No figures.\n", cx);
        assert!(!view.held);
    });
}

#[gpui_kit::test]
fn figures_named_after_the_text_are_drawn_into_it(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 900.0);
    started
        .view
        .update(cx, |view, cx| view.set_text(WITH_FIGURES, cx));
    settle(&started, cx);
    assert_eq!(drawn_figures(&started, cx), Vec::new());
    started.view.update(cx, |view, cx| {
        view.set_figures(figure_sources("E = mc^2", FLOW), cx);
    });
    settle(&started, cx);
    assert_eq!(drawn_figures(&started, cx).len(), 2);
    // Taking them away gives the pictures back to the Kit.
    started.view.update(cx, |view, cx| {
        view.set_figures(FigureSources::default(), cx);
    });
    settle(&started, cx);
    assert_eq!(drawn_figures(&started, cx), Vec::new());
}

#[gpui_kit::test]
fn figures_that_arrive_keep_the_place_of_the_reader(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    let mut text = String::from("![The way](dia/way.mmd)\n\n");
    text.push_str(&chapter(60));
    let target = offset_of(&text, "Paragraph 30 says");
    started.view.update(cx, |view, cx| {
        view.set_text(&text, cx);
        view.reveal(target, cx);
    });
    settle(&started, cx);
    started.view.update(cx, |view, cx| {
        view.set_figures(figure_sources("x", FLOW), cx);
    });
    settle(&started, cx);
    assert_eq!(top(&started, cx), target);
}

#[gpui_kit::test]
fn a_page_tone_draws_the_figures_again(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 900.0);
    started.view.update(cx, |view, cx| {
        view.set_figures(figure_sources("E = mc^2", FLOW), cx);
        view.set_text(WITH_FIGURES, cx);
    });
    settle(&started, cx);
    let before = started.view.read_with(cx, |view, _| view.drawn_in);
    assert!(before.is_some());
    cx.update(|cx| {
        Appearance::global(cx).update(cx, |appearance, cx| {
            appearance.update(
                |settings| {
                    settings.page_tone = Some(crate::shared::theme::PageTone::Night);
                },
                cx,
            );
        });
    });
    settle(&started, cx);
    let after = started.view.read_with(cx, |view, _| view.drawn_in);
    assert_eq!(
        after,
        Some(Palette::from(crate::shared::theme::PageTone::Night))
    );
    assert_ne!(before, after);
    assert_eq!(drawn_figures(&started, cx).len(), 2);
}

#[gpui_kit::test]
fn a_tall_diagram_stays_on_one_page(cx: &mut TestAppContext) {
    let started = start_paged(cx, 1000.0, 400.0);
    let tall =
        "flowchart TD\n  A --> B\n  B --> C\n  C --> D\n  D --> E\n  E --> F\n  F --> G\n  G --> H";
    started.view.update(cx, |view, cx| {
        view.set_figures(figure_sources("x", tall), cx);
        view.set_text("Before.\n\n![The way](dia/way.mmd)\n\nAfter.\n", cx);
    });
    settle(&started, cx);
    started.view.read_with(cx, |view, _| {
        let paging = view.paging.as_ref().unwrap();
        assert_eq!(paging.heights.len(), 3, "{:?}", paging.heights);
        assert!(paging.heights[1] <= 400.0, "{:?}", paging.heights);
        assert!(paging.pages.iter().all(|page| page.offset == 0.0));
    });
}

#[gpui_kit::test]
fn a_paged_text_with_figures_is_divided_into_pages(cx: &mut TestAppContext) {
    let mut text = String::from("![The way](dia/way.mmd)\n\n");
    text.push_str(&long_chapter(40));
    let started = start_paged(cx, 800.0, 600.0);
    started.view.update(cx, |view, cx| {
        view.set_figures(figure_sources("x", FLOW), cx);
        view.set_text(&text, cx);
    });
    settle(&started, cx);
    let (page, pages) = page_number(&started, cx);
    assert_eq!(page, 1);
    assert!(pages > 1, "{pages}");
}

#[gpui_kit::test]
fn changing_reading_flow_without_turning_a_page_keeps_the_source_position(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 400.0);
    let paragraph =
        "Ёжик наблюдал за утренней росой, а рядом, за высоким забором, шумели ветки. ".repeat(100);
    let at = paragraph.char_indices().nth(3500).unwrap().0;
    let offset = u32::try_from(at).unwrap();
    started.view.update(cx, |view, cx| {
        view.set_chapter(&paragraph, vec![0], cx);
        view.reveal(offset, cx);
    });
    settle(&started, cx);
    let before = top(&started, cx);
    assert!(before.abs_diff(offset) <= 1);
    started.view.update(cx, |view, cx| view.set_paged(true, cx));
    settle(&started, cx);
    assert!(page_number(&started, cx).0 > 1);
    started
        .view
        .update(cx, |view, cx| view.set_paged(false, cx));
    settle(&started, cx);
    assert!(top(&started, cx).abs_diff(before) <= 1);
}

#[gpui_kit::test]
fn a_large_mixed_script_chapter_reaches_its_final_passage(cx: &mut TestAppContext) {
    let started = start(cx, 320.0, 640.0);
    let text = format!(
        "# Глава\n\n{}Последняя строка. API и ₽.\n",
        "В саду ещё темно, и Анна находит _ёжика_. За́мок на воротах закрыт.\n\n".repeat(3_050)
    );
    let blocks = blocks::block_offsets(&text);
    let final_offset = offset_of(&text, "Последняя строка.");
    started.view.update(cx, |view, cx| {
        view.set_chapter(&text, blocks, cx);
        view.reveal(final_offset, cx);
    });
    settle(&started, cx);
    started.view.update(cx, ChapterView::reveal_end);
    settle(&started, cx);
    assert!(top(&started, cx) > final_offset - 1024);
}

// Pictures and figures that open larger

/// What the view asked to enlarge, in order.
type Asked = Arc<Mutex<Vec<Enlarged>>>;

fn ask_to_enlarge(started: &Started, cx: &mut TestAppContext) -> Asked {
    let asked = Asked::default();
    let log = Arc::clone(&asked);
    started.view.update(cx, |view, _| {
        view.on_enlarge(move |enlarged, _, _| log.lock().unwrap().push(enlarged));
    });
    asked
}

/// Clicks the element an image of the text is reported by.
fn click_image(started: &Started, label: &str, cx: &mut TestAppContext) {
    let label = label.to_owned();
    testing::frame(cx, started.window, move |window, cx| {
        let image = window.find_by_label(&label);
        let id = image.path().last().unwrap().clone();
        window.click(id, cx);
    });
    settle(started, cx);
}

#[gpui_kit::test]
fn clicking_a_picture_alone_in_a_paragraph_asks_to_enlarge_it(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 600.0);
    let asked = ask_to_enlarge(&started, cx);
    started.view.update(cx, |view, cx| {
        view.set_pictures(
            picture_resolver(|| Some(PictureState::Pending((400, 200)))),
            cx,
        );
        view.set_text(WITH_PICTURE, cx);
    });
    settle(&started, cx);
    click_image(&started, "A grey cat", cx);
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert!(matches!(
        &asked[0],
        Enlarged::Picture { destination, alt }
            if destination == "cat.avif" && alt.as_ref() == "A grey cat"
    ));
}

#[gpui_kit::test]
fn a_picture_that_opens_larger_takes_the_room_the_kit_gives_it(cx: &mut TestAppContext) {
    let heights = |enlarge: bool, cx: &mut TestAppContext| {
        let started = start_paged(cx, 800.0, 600.0);
        if enlarge {
            ask_to_enlarge(&started, cx);
        }
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
        let heights = started
            .view
            .read_with(cx, |view, _| view.paging.as_ref().unwrap().heights.clone());
        (sizes, heights)
    };
    let (kit_sizes, kit) = heights(false, cx);
    let (sizes, ours) = heights(true, cx);
    assert_eq!(sizes, kit_sizes);
    assert_eq!(kit.len(), ours.len());
    // The picture is not the last block, so both have the gap under it.
    assert!((kit[1] - ours[1]).abs() < 1.0, "{kit:?} {ours:?}");
}

#[gpui_kit::test]
fn a_picture_in_a_line_of_text_stays_with_the_kit(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 600.0);
    let asked = ask_to_enlarge(&started, cx);
    started.view.update(cx, |view, cx| {
        view.set_pictures(
            picture_resolver(|| Some(PictureState::Pending((40, 20)))),
            cx,
        );
        view.set_text("A cat ![A grey cat](cat.avif) sleeps.\n", cx);
    });
    settle(&started, cx);
    let ours = testing::frame(cx, started.window, |window, _| {
        window.try_find_by_label("A grey cat").is_some()
    });
    assert!(!ours, "the picture stays in its line, with the Kit");
    assert!(asked.lock().unwrap().is_empty());
}

#[gpui_kit::test]
fn a_picture_without_a_handler_is_left_to_the_kit(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 600.0);
    started.view.update(cx, |view, cx| {
        view.set_pictures(
            picture_resolver(|| Some(PictureState::Pending((400, 200)))),
            cx,
        );
        view.set_text(WITH_PICTURE, cx);
    });
    settle(&started, cx);
    let ours = testing::frame(cx, started.window, |window, _| {
        window.try_find_by_label("A grey cat").is_some()
    });
    assert!(!ours, "the Kit reports no picture of its own");
}

#[gpui_kit::test]
fn clicking_a_figure_alone_in_a_paragraph_asks_to_enlarge_its_drawing(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 900.0);
    let asked = ask_to_enlarge(&started, cx);
    started.view.update(cx, |view, cx| {
        view.set_figures(figure_sources("E = mc^2", FLOW), cx);
        view.set_text(WITH_FIGURES, cx);
    });
    settle(&started, cx);
    click_image(&started, "The way", cx);
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert!(matches!(
        &asked[0],
        Enlarged::Figure { figure, alt } if alt.as_ref() == "The way" && figure.height > 10.0
    ));
}

#[gpui_kit::test]
fn a_picture_without_words_opens_but_is_not_announced(cx: &mut TestAppContext) {
    let started = start(cx, 800.0, 600.0);
    ask_to_enlarge(&started, cx);
    started.view.update(cx, |view, cx| {
        view.set_pictures(
            picture_resolver(|| Some(PictureState::Pending((400, 200)))),
            cx,
        );
        view.set_text("Before.\n\n![](cat.avif)\n\nAfter.\n", cx);
    });
    settle(&started, cx);
    let images = testing::frame(cx, started.window, |window, _| {
        window.find_all_by_role(Role::Image).len()
    });
    assert_eq!(images, 0);
    assert_eq!(
        started
            .view
            .read_with(cx, |view, _| view.placeholder_sizes()),
        vec![(400, 200)]
    );
}
