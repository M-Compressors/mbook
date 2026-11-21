//! The blocks of a chapter and the places in the source that go with them.
//!
//! The Markdown view of GPUI Kit lays out one list item for every top-level
//! block of the document, in the order of the source. Which byte of the source
//! an item starts at is not something the Kit tells, so the widget keeps the
//! offsets itself and maps an item to one of them, as long as the number of
//! items is the number of offsets. When it is not, it falls back to the
//! position as a fraction of the text.

use markdown::ParseOptions;
use markdown::mdast::Node;

/// The byte offset of the line every top-level block starts at, in order,
/// with a first block at 0 for a text that does not start with one. The same
/// rule `mbook` applies when it prepares a chapter.
pub(super) fn block_offsets(text: &str) -> Vec<u32> {
    let mut blocks: Vec<u32> = markdown::to_mdast(text, &ParseOptions::gfm())
        .ok()
        .as_ref()
        .and_then(Node::children)
        .map_or_default(Vec::as_slice)
        .iter()
        .filter_map(Node::position)
        .map(|position| to_offset(line_start(text, position.start.offset)))
        .collect();
    blocks.sort_unstable();
    blocks.dedup();
    if blocks.first() != Some(&0) && !text.is_empty() {
        blocks.insert(0, 0);
    }
    blocks
}

fn line_start(text: &str, offset: usize) -> usize {
    text.as_bytes()
        .get(..offset)
        .and_then(|before| before.iter().rposition(|&byte| byte == b'\n'))
        .map_or(0, |newline| newline + 1)
}

fn to_offset(offset: usize) -> u32 {
    u32::try_from(offset).unwrap_or(u32::MAX)
}

/// The list item that shows the block of the byte `offset` of a text of
/// `len` bytes, when the view has `items` items and the text `blocks`.
pub(super) fn item_of_offset(blocks: &[u32], len: usize, items: usize, offset: u32) -> usize {
    if items == 0 {
        return 0;
    }
    if blocks.len() == items {
        return blocks
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
    }
    if len == 0 {
        return 0;
    }
    // Whole numbers of a text and a count of blocks, both far below 2^53.
    let fraction = f64::from(offset) / f64::from(to_offset(len));
    let item = fraction * f64::from(to_offset(items));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a position inside a count of blocks"
    )]
    let item = item.floor() as usize;
    item.min(items - 1)
}

/// The byte offset of the start of the list item `item`, the inverse of
/// [`item_of_offset`].
pub(super) fn offset_of_item(blocks: &[u32], len: usize, items: usize, item: usize) -> u32 {
    if items == 0 {
        return 0;
    }
    if blocks.len() == items {
        return blocks.get(item).copied().unwrap_or(0);
    }
    let item = item.min(items - 1);
    to_offset(len / items * item + len % items * item / items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_starts_at_its_line() {
        let text = "# Title\n\nA paragraph\nof two lines.\n\n- one\n- two\n";
        let offset = |needle: &str| u32::try_from(text.find(needle).unwrap()).unwrap();
        assert_eq!(
            block_offsets(text),
            vec![0, offset("A paragraph"), offset("- one")]
        );
    }

    #[test]
    fn an_empty_text_has_no_blocks() {
        assert_eq!(block_offsets(""), Vec::<u32>::new());
    }

    #[test]
    fn a_text_that_starts_with_blank_lines_has_a_first_block_at_zero() {
        assert_eq!(block_offsets("\n\nText\n"), vec![0, 2]);
    }

    #[test]
    fn an_item_is_the_block_that_contains_the_offset() {
        let blocks = [0, 10, 30];
        assert_eq!(item_of_offset(&blocks, 50, 3, 0), 0);
        assert_eq!(item_of_offset(&blocks, 50, 3, 9), 0);
        assert_eq!(item_of_offset(&blocks, 50, 3, 10), 1);
        assert_eq!(item_of_offset(&blocks, 50, 3, 49), 2);
        assert_eq!(item_of_offset(&blocks, 50, 3, u32::MAX), 2);
    }

    #[test]
    fn when_the_counts_differ_the_item_follows_the_fraction_of_the_text() {
        assert_eq!(item_of_offset(&[0, 10], 100, 4, 50), 2);
        assert_eq!(item_of_offset(&[0, 10], 100, 4, 99), 3);
        assert_eq!(item_of_offset(&[0, 10], 100, 4, 5000), 3);
    }

    #[test]
    fn without_items_the_item_is_zero() {
        assert_eq!(item_of_offset(&[0], 10, 0, 5), 0);
        assert_eq!(item_of_offset(&[], 0, 3, 5), 0);
    }

    #[test]
    fn the_offset_of_an_item_is_where_its_block_starts() {
        let blocks = [0, 10, 30];
        assert_eq!(offset_of_item(&blocks, 50, 3, 1), 10);
        assert_eq!(offset_of_item(&blocks, 50, 3, 7), 0);
    }

    #[test]
    fn when_the_counts_differ_the_offset_follows_the_fraction_of_the_text() {
        assert_eq!(offset_of_item(&[0], 100, 4, 2), 50);
        assert_eq!(offset_of_item(&[0], 100, 4, 99), 75);
        assert_eq!(offset_of_item(&[0], 100, 0, 1), 0);
    }
}
