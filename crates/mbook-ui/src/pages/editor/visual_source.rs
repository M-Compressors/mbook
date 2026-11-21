//! Keeps original Markdown around the blocks edited in the visual editor.

use std::mem::{Discriminant, discriminant};
use std::ops::Range;

use gpui_kit::SharedString;
use markdown::{ParseOptions, mdast::Node};
use similar::{Algorithm, DiffTag, capture_diff_slices};

struct Block {
    span: Range<usize>,
    kind: Discriminant<Node>,
}

/// The original chapter and the editor's serialization of it before any edit.
pub(super) struct VisualSource {
    original: SharedString,
    original_blocks: Vec<Block>,
    canonical: SharedString,
    canonical_blocks: Vec<Block>,
}

impl VisualSource {
    /// Pairs corresponding blocks before the editor changes them.
    pub(super) fn new(original: SharedString, canonical: SharedString) -> Option<Self> {
        let original_blocks = blocks(&original)?;
        let canonical_blocks = blocks(&canonical)?;
        same_kinds(&original_blocks, &canonical_blocks).then_some(Self {
            original,
            original_blocks,
            canonical,
            canonical_blocks,
        })
    }

    /// Restores the original spelling and spacing of untouched blocks.
    pub(super) fn restore(&self, edited: SharedString) -> SharedString {
        if edited == self.canonical {
            return self.original.clone();
        }
        let Some(new_blocks) = blocks(&edited) else {
            return edited;
        };
        let (Some(old_first), Some(old_last), Some(new_first), Some(new_last)) = (
            self.canonical_blocks.first(),
            self.canonical_blocks.last(),
            new_blocks.first(),
            new_blocks.last(),
        ) else {
            return edited;
        };

        let old: Vec<&str> = self
            .canonical_blocks
            .iter()
            .map(|block| &self.canonical[block.span.clone()])
            .collect();
        let new: Vec<&str> = new_blocks
            .iter()
            .map(|block| &edited[block.span.clone()])
            .collect();
        let mut restored = String::with_capacity(edited.len());
        let mut from = new_first.span.start;
        let leading = &edited[..from];
        if leading == &self.canonical[..old_first.span.start] {
            restored.push_str(&self.original[..self.original_blocks[0].span.start]);
        } else {
            restored.push_str(leading);
        }

        for op in capture_diff_slices(Algorithm::Patience, &old, &new) {
            if op.tag() != DiffTag::Equal {
                continue;
            }
            let old_range = op.old_range();
            let new_range = op.new_range();
            let start = new_blocks[new_range.start].span.start;
            restored.push_str(&edited[from..start]);
            let originals = &self.original_blocks[old_range];
            let start = originals[0].span.start;
            let end = originals[originals.len() - 1].span.end;
            restored.push_str(&self.original[start..end]);
            from = new_blocks[new_range.end - 1].span.end;
        }

        restored.push_str(&edited[from..new_last.span.end]);
        let trailing = &edited[new_last.span.end..];
        if trailing == &self.canonical[old_last.span.end..] {
            let original_last = &self.original_blocks[self.original_blocks.len() - 1];
            restored.push_str(&self.original[original_last.span.end..]);
        } else {
            restored.push_str(trailing);
        }

        // A source block can depend on its original position. For example,
        // an unclosed code fence would swallow a new block appended after it.
        if restored == edited.as_str() {
            edited
        } else if blocks(&restored).is_some_and(|restored| same_kinds(&new_blocks, &restored)) {
            restored.into()
        } else {
            edited
        }
    }
}

fn blocks(source: &str) -> Option<Vec<Block>> {
    let Node::Root(root) = markdown::to_mdast(source, &ParseOptions::gfm()).ok()? else {
        return None;
    };
    root.children
        .iter()
        .map(|node| {
            let position = node.position()?;
            let start = position.start.offset;
            let end = start
                + source
                    .get(start..position.end.offset)?
                    .trim_end_matches(['\n', '\r'])
                    .len();
            Some(Block {
                span: start..end,
                kind: discriminant(node),
            })
        })
        .collect()
}

fn same_kinds(first: &[Block], second: &[Block]) -> bool {
    first.len() == second.len()
        && first
            .iter()
            .zip(second)
            .all(|(first, second)| first.kind == second.kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restoring_an_edit_keeps_other_headings_and_reference_spelling() {
        let original =
            "Title  with  spaces\n===================\n\nFirst.\n\nSee [spot].\n\n[spot]: #spot\n";
        let canonical = "# Title with spaces\n\nFirst.\n\nSee [spot](#spot).\n\n[spot]: #spot";
        let model = VisualSource::new(original.into(), canonical.into()).unwrap();
        let edited = canonical.replace("First.", "Changed.");
        assert_eq!(
            model.restore(edited.into()),
            original.replace("First.", "Changed.")
        );
    }

    #[test]
    fn restoring_the_original_revision_keeps_its_exact_bytes() {
        let model = VisualSource::new("one    two\n".into(), "one two\n\n".into()).unwrap();
        assert_eq!(model.restore("one two\n\n".into()), "one    two\n");
    }

    #[test]
    fn a_new_paragraph_keeps_its_separator_when_it_is_last() {
        let model = VisualSource::new("First".into(), "First".into()).unwrap();
        assert_eq!(model.restore("First\n\n".into()), "First\n\n");
    }

    #[test]
    fn an_unclosed_fence_does_not_swallow_a_new_block() {
        let model = VisualSource::new("```\ncode".into(), "```\ncode\n```".into()).unwrap();
        let source = "```\ncode\n```\n\nAfter";
        assert_eq!(model.restore(source.into()), source);
    }

    #[test]
    fn an_import_that_loses_a_block_cannot_be_mapped() {
        assert!(VisualSource::new("# Title\n\nText".into(), "# Title".into()).is_none());
    }
}
