//! What the write side learns from the Markdown of a chapter.

use crate::types::Anchor;

/// One anchor for every heading of a chapter, named by the rule of RFC 0001
/// and sorted by offset.
pub(crate) fn anchors(_text: &str) -> Vec<Anchor> {
    todo!("derive heading anchors from the chapter with the `markdown` crate")
}
