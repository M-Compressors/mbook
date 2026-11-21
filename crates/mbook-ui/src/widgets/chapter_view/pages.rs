//! The pages of a chapter: where each one starts in the column of text, and
//! how much of the column it shows. Plain functions, tested without a window.
//!
//! The Markdown view of GPUI Kit lays a chapter out as one column, a list
//! item for each top-level block, and tells the height of every item. A page
//! is a stretch of that column no taller than the window that ends between
//! two lines:
//!
//! * a paragraph is lines of one height, so it is divided between any two of
//!   them, but not so that one line is left alone at the bottom of a page or
//!   at the top of the next;
//! * every other block (a heading, a picture, a list, a table, code) goes to
//!   the next page whole when it does not fit. One that is taller than a page
//!   is cut on the grid of the lines of the text, which may fall inside a
//!   line of its own: the Kit does not tell where its lines are;
//! * a heading is never the last thing on a page.

/// What kind of block an item of the list is, for dividing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BlockKind {
    /// A paragraph: lines of the height of the text.
    Text,
    /// A heading, which stays with what follows it.
    Heading,
    /// Anything else: kept whole when it fits on a page.
    Whole,
}

/// One item of the list, as laid out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Block {
    /// Its height in the column, in pixels, with the gap under it.
    pub(super) height: f32,
    pub(super) kind: BlockKind,
}

/// The measures a chapter is divided by, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Measures {
    /// The height of the window of the text.
    pub(super) page: f32,
    /// The height of a line of a paragraph.
    pub(super) line: f32,
    /// The gap the Kit leaves under a paragraph that another block follows.
    pub(super) gap: f32,
}

/// One page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Page {
    /// The item its first line is in.
    pub(super) item: usize,
    /// How far into that item the page starts, in pixels.
    pub(super) offset: f32,
    /// Where it starts in the column, in pixels from the start of the text.
    pub(super) top: f32,
    /// How much of the column it shows, in pixels; at most the height of the
    /// window.
    pub(super) extent: f32,
}

/// What a sub-pixel of layout may differ by.
const EPSILON: f32 = 0.5;

/// The kind of the block whose source starts with `source`, by the rules of
/// `CommonMark` for the first line of a block. A heading underlined with `=` or
/// `-` reads as a paragraph: it is then divided like one.
pub(super) fn kind_of(source: &str) -> BlockKind {
    let line = source.lines().next().unwrap_or("");
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent >= 4 || line.trim_start_matches(' ').starts_with('\t') {
        return BlockKind::Whole;
    }
    let line = line.trim_start();
    let hashes = line.bytes().take_while(|&byte| byte == b'#').count();
    if (1..=6).contains(&hashes)
        && line
            .as_bytes()
            .get(hashes)
            .is_none_or(|&byte| byte == b' ' || byte == b'\t')
    {
        return BlockKind::Heading;
    }
    let list = line
        .strip_prefix(['-', '*', '+'])
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']) || rule(line));
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    let ordered = (1..=9).contains(&digits)
        && line[digits..]
            .strip_prefix(['.', ')'])
            .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']));
    let other = line.starts_with(['>', '|'])
        || line.starts_with("```")
        || line.starts_with("~~~")
        || line.starts_with("![")
        || line.starts_with("[^")
        || line.starts_with('<')
        || rule(line);
    if list || ordered || other {
        BlockKind::Whole
    } else {
        BlockKind::Text
    }
}

/// Whether `line` is a thematic break: three or more of one of `-`, `*`, `_`.
fn rule(line: &str) -> bool {
    let marks: Vec<u8> = line
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    marks.len() >= 3
        && matches!(marks[0], b'-' | b'*' | b'_')
        && marks.iter().all(|&mark| mark == marks[0])
}

/// How many lines a paragraph of `height` has: the height is the lines and,
/// unless it is the last block, the gap. `None` when the height is neither,
/// as for a paragraph that holds a picture.
fn lines(height: f32, measures: Measures) -> Option<usize> {
    if measures.line < 1.0 {
        return None;
    }
    [measures.gap, 0.0].into_iter().find_map(|gap| {
        let count = ((height - gap) / measures.line).round();
        let exact = (height - gap - count * measures.line).abs() <= EPSILON;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a count of lines, checked to be at least one"
        )]
        (exact && count >= 1.0).then_some(count as usize)
    })
}

/// A count of lines as a length.
fn span(count: usize, line: f32) -> f32 {
    // A count of lines of one block, far below 2^24.
    #[expect(clippy::cast_precision_loss, reason = "a small count of lines")]
    let count = count as f32;
    count * line
}

/// Where a page that starts at `start` ends, when the block `index`, whose
/// top is at `top`, is the first that does not fit above `limit`.
fn break_in(block: Block, top: f32, start: f32, limit: f32, measures: Measures) -> f32 {
    let line = measures.line.max(1.0);
    let count = match block.kind {
        BlockKind::Text => lines(block.height, measures),
        BlockKind::Heading | BlockKind::Whole => None,
    };
    let Some(count) = count else {
        if top > start + EPSILON {
            // It goes to the next page whole.
            return top;
        }
        // Taller than a page: cut on the grid of the text.
        let fit = ((measures.page + EPSILON) / line).floor().max(1.0);
        return start + fit * line;
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a count of lines, not negative"
    )]
    let mut fit = (((limit + EPSILON - top) / line).floor().max(0.0) as usize).min(count);
    let begins_here = top > start + EPSILON;
    // One line is not left alone at the bottom of a page, nor at the top of
    // the next.
    if fit == 1 && count > 1 && begins_here {
        fit = 0;
    }
    if count - fit == 1 && fit >= 3 {
        fit -= 1;
    }
    let cut = top + span(fit, line);
    if cut > start + EPSILON {
        cut
    } else {
        // Not one line fits: the page is lower than a line.
        start + line.max(measures.page)
    }
}

/// Divides the column of `blocks` into pages. A text without blocks has one
/// empty page.
pub(super) fn paginate(blocks: &[Block], measures: Measures) -> Vec<Page> {
    let mut tops = Vec::with_capacity(blocks.len() + 1);
    let mut total = 0.0_f32;
    for block in blocks {
        tops.push(total);
        total += block.height.max(0.0);
    }
    let first = Page {
        item: 0,
        offset: 0.0,
        top: 0.0,
        extent: total.min(measures.page.max(0.0)),
    };
    if measures.page < 1.0 || total <= measures.page + EPSILON {
        return vec![first];
    }
    let mut pages = Vec::new();
    let mut start = 0.0_f32;
    let mut item = 0_usize;
    while start < total - EPSILON && item < blocks.len() {
        let limit = start + measures.page;
        // The first block whose text does not end above the limit.
        let mut index = item;
        let mut cut = total;
        while index < blocks.len() {
            let block = blocks[index];
            let top = tops[index];
            let text_end = match (block.kind, lines(block.height, measures)) {
                (BlockKind::Text, Some(count)) => top + span(count, measures.line),
                _ => top + block.height,
            };
            if text_end > limit + EPSILON {
                cut = break_in(block, top, start, limit, measures);
                break;
            }
            index += 1;
        }
        // A heading goes with what follows it.
        while index < blocks.len()
            && index > 0
            && (cut - tops[index]).abs() <= EPSILON
            && blocks[index - 1].kind == BlockKind::Heading
            && tops[index - 1] > start + EPSILON
        {
            index -= 1;
            cut = tops[index];
        }
        pages.push(Page {
            item,
            offset: start - tops[item],
            top: start,
            extent: (cut - start).min(measures.page),
        });
        start = cut;
        while item < blocks.len() && tops[item] + blocks[item].height.max(0.0) <= start + EPSILON {
            item += 1;
        }
    }
    if pages.is_empty() {
        pages.push(first);
    }
    pages
}

/// The page that shows the place `y` of the column.
pub(super) fn page_at(pages: &[Page], y: f32) -> usize {
    pages
        .partition_point(|page| page.top <= y + EPSILON)
        .saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: Measures = Measures {
        page: 100.0,
        line: 10.0,
        gap: 4.0,
    };

    fn text(lines: usize) -> Block {
        Block {
            height: span(lines, M.line) + M.gap,
            kind: BlockKind::Text,
        }
    }

    fn whole(height: f32) -> Block {
        Block {
            height,
            kind: BlockKind::Whole,
        }
    }

    fn heading(height: f32) -> Block {
        Block {
            height,
            kind: BlockKind::Heading,
        }
    }

    /// The top of the item `item` in the column of `blocks`.
    fn top_of(blocks: &[Block], item: usize) -> f32 {
        blocks
            .iter()
            .take(item)
            .map(|block| block.height.max(0.0))
            .sum()
    }

    fn tops(pages: &[Page]) -> Vec<f32> {
        pages.iter().map(|page| page.top).collect()
    }

    #[test]
    fn the_first_line_of_a_block_tells_its_kind() {
        assert_eq!(kind_of("Plain words.\nMore."), BlockKind::Text);
        assert_eq!(kind_of("# Title"), BlockKind::Heading);
        assert_eq!(kind_of("  ###### Deep"), BlockKind::Heading);
        assert_eq!(kind_of("#hashtag"), BlockKind::Text);
        assert_eq!(kind_of("####### seven"), BlockKind::Text);
        assert_eq!(kind_of("#"), BlockKind::Heading);
        for other in [
            "- item",
            "* item",
            "+ item",
            "-",
            "1. one",
            "12) twelve",
            "> quote",
            "| a | b |",
            "```rust",
            "~~~",
            "![alt](p.avif)",
            "[^1]: note",
            "<div>",
            "---",
            "* * *",
            "___",
            "    indented code",
            "\tcode",
        ] {
            assert_eq!(kind_of(other), BlockKind::Whole, "{other:?}");
        }
        assert_eq!(kind_of("-not a list"), BlockKind::Text);
        assert_eq!(kind_of("1.5 is a number"), BlockKind::Text);
        assert_eq!(kind_of("1234567890. too long"), BlockKind::Text);
        assert_eq!(kind_of("**bold** start"), BlockKind::Text);
        assert_eq!(kind_of(""), BlockKind::Text);
    }

    #[test]
    fn a_text_that_fits_is_one_page() {
        let pages = paginate(&[text(3), text(4)], M);
        assert_eq!(pages.len(), 1);
        assert!((pages[0].extent - 78.0).abs() < 0.01);
    }

    #[test]
    fn a_text_without_blocks_is_one_empty_page() {
        let pages = paginate(&[], M);
        assert_eq!(pages.len(), 1);
        assert!(pages[0].extent.abs() < f32::EPSILON);
    }

    #[test]
    fn a_window_without_height_has_one_page() {
        let none = Measures { page: 0.0, ..M };
        assert_eq!(paginate(&[text(50)], none).len(), 1);
    }

    #[test]
    fn a_long_paragraph_is_divided_between_lines() {
        let pages = paginate(&[text(25)], M);
        assert_eq!(tops(&pages), [0.0, 100.0, 200.0]);
        assert!(pages.iter().all(|page| page.item == 0));
        assert!((pages[1].offset - 100.0).abs() < 0.01);
        assert!((pages[2].extent - 54.0).abs() < 0.01);
    }

    #[test]
    fn a_page_ends_at_the_last_whole_line() {
        let high = Measures { page: 105.0, ..M };
        let pages = paginate(&[text(25)], high);
        assert!((pages[0].extent - 100.0).abs() < 0.01);
        assert!((pages[1].top - 100.0).abs() < 0.01);
    }

    #[test]
    fn the_gap_under_a_paragraph_may_hang_over_the_page() {
        // Ten lines fill the page; the gap under them does not need room.
        let pages = paginate(&[text(10), text(2)], M);
        assert_eq!(tops(&pages), [0.0, 104.0]);
        assert!((pages[0].extent - 100.0).abs() < 0.01);
    }

    #[test]
    fn one_line_is_not_left_at_the_bottom_of_a_page() {
        // 84 pixels are taken; one line of the next paragraph would fit.
        let pages = paginate(&[text(8), text(5)], M);
        assert_eq!(tops(&pages), [0.0, 84.0]);
        assert_eq!(pages[1].item, 1);
    }

    #[test]
    fn one_line_is_not_left_at_the_top_of_a_page() {
        // Eleven lines: ten would fit, which would leave one.
        let pages = paginate(&[text(11)], M);
        assert_eq!(tops(&pages), [0.0, 90.0]);
    }

    #[test]
    fn a_paragraph_of_two_lines_is_not_divided_to_avoid_a_single_line() {
        let pages = paginate(&[text(9), text(2)], M);
        assert_eq!(tops(&pages), [0.0, 94.0]);
    }

    #[test]
    fn a_block_that_does_not_fit_goes_to_the_next_page_whole() {
        let pages = paginate(&[text(6), whole(60.0), text(2)], M);
        assert_eq!(tops(&pages), [0.0, 64.0]);
        assert!((pages[0].extent - 64.0).abs() < 0.01);
        assert_eq!(pages[1].item, 1);
    }

    #[test]
    fn a_block_taller_than_a_page_is_cut_on_the_lines_of_the_text() {
        let pages = paginate(&[whole(250.0)], M);
        assert_eq!(tops(&pages), [0.0, 100.0, 200.0]);
    }

    #[test]
    fn a_tall_block_starts_its_own_page() {
        let pages = paginate(&[text(2), whole(150.0)], M);
        assert_eq!(tops(&pages), [0.0, 24.0, 124.0]);
    }

    #[test]
    fn a_heading_goes_with_what_follows_it() {
        let pages = paginate(&[text(6), heading(20.0), whole(60.0)], M);
        assert_eq!(tops(&pages), [0.0, 64.0]);
        assert_eq!(pages[1].item, 1);
    }

    #[test]
    fn headings_in_a_row_go_together() {
        let pages = paginate(&[text(4), heading(20.0), heading(20.0), whole(60.0)], M);
        assert_eq!(tops(&pages), [0.0, 44.0]);
    }

    #[test]
    fn a_heading_at_the_top_of_a_page_stays_there() {
        let pages = paginate(&[heading(30.0), whole(90.0), text(1)], M);
        assert_eq!(tops(&pages), [0.0, 30.0]);
    }

    #[test]
    fn a_paragraph_whose_height_is_not_lines_is_kept_whole() {
        let odd = Block {
            height: 57.0,
            kind: BlockKind::Text,
        };
        let pages = paginate(&[text(6), odd], M);
        assert_eq!(tops(&pages), [0.0, 64.0]);
    }

    #[test]
    fn the_last_paragraph_has_no_gap() {
        let last = Block {
            height: 50.0,
            kind: BlockKind::Text,
        };
        let pages = paginate(&[text(8), last], M);
        // Lines without a gap: one alone would fit, so it starts the next page.
        assert_eq!(tops(&pages), [0.0, 84.0]);
        assert_eq!(lines(50.0, M), Some(5));
        assert_eq!(lines(54.0, M), Some(5));
        assert_eq!(lines(57.0, M), None);
        assert_eq!(lines(4.0, M), None);
    }

    #[test]
    fn a_page_lower_than_a_line_still_moves_on() {
        let low = Measures { page: 6.0, ..M };
        let pages = paginate(&[text(3)], low);
        assert!(pages.len() >= 3, "{pages:?}");
        assert!(pages.windows(2).all(|pair| pair[1].top > pair[0].top));
        let pages = paginate(&[whole(30.0)], low);
        assert!(pages.windows(2).all(|pair| pair[1].top > pair[0].top));
    }

    #[test]
    fn lines_without_a_height_divide_nothing() {
        let flat = Measures { line: 0.0, ..M };
        assert_eq!(lines(50.0, flat), None);
        let pages = paginate(&[text(30)], flat);
        assert!(pages.windows(2).all(|pair| pair[1].top > pair[0].top));
    }

    #[test]
    fn every_page_but_the_last_ends_where_the_next_starts() {
        let blocks = [
            heading(30.0),
            text(7),
            whole(45.0),
            text(31),
            heading(22.0),
            text(3),
            whole(180.0),
            text(12),
        ];
        let pages = paginate(&blocks, M);
        for pair in pages.windows(2) {
            assert!(pair[0].extent <= M.page + 0.01, "{pair:?}");
            assert!(
                pair[1].top >= pair[0].top + pair[0].extent - 0.01,
                "{pair:?}"
            );
            assert!(
                pair[1].top - pair[0].top <= M.page + M.gap + 0.01,
                "{pair:?}"
            );
        }
        for page in &pages {
            assert!((top_of(&blocks, page.item) + page.offset - page.top).abs() < 0.01);
        }
    }

    #[test]
    fn a_place_of_the_column_is_on_the_page_that_shows_it() {
        let pages = paginate(&[text(25)], M);
        assert_eq!(page_at(&pages, 0.0), 0);
        assert_eq!(page_at(&pages, 99.0), 0);
        assert_eq!(page_at(&pages, 100.0), 1);
        assert_eq!(page_at(&pages, 5000.0), 2);
        assert_eq!(page_at(&pages, -5.0), 0);
        assert_eq!(page_at(&[], 10.0), 0);
    }

    #[test]
    fn the_top_of_an_item_is_the_heights_before_it() {
        let blocks = [text(1), whole(30.0), text(2)];
        assert!(top_of(&blocks, 0).abs() < f32::EPSILON);
        assert!((top_of(&blocks, 2) - 44.0).abs() < 0.01);
        assert!((top_of(&blocks, 9) - 68.0).abs() < 0.01);
    }
}
