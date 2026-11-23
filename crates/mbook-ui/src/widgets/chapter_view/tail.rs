//! The room after a scrolling text.
//!
//! The controls of the reader lie over the bottom of a scrolling text, so
//! that the text runs under them. Its last lines would stay under them too,
//! since a list of the Kit ends where its last item ends and takes no
//! padding from outside. The widget therefore follows the text with one more
//! block, [`TAIL`], which [`EndRoom`] draws as empty room as tall as the
//! controls: scrolled to its end, the text stops above them.

use gpui_kit::component::text::{
    MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast::Node,
};
use gpui_kit::{App, IntoElement, Styled as _, Window, div, px};

/// What a scrolling text is followed by: a paragraph of invisible
/// characters, after a blank line that closes the block before it. Should an
/// unclosed block of the text take it in, it shows nothing there either.
pub(super) const TAIL: &str = "\n\n\u{2060}\u{200B}\u{2060}";

/// The name of the node [`EndRoom`] makes.
const NAME: &str = "mbook-end-room";

/// Draws the paragraph of [`TAIL`] as empty room of the given height, in
/// pixels.
pub(super) struct EndRoom(pub(super) f32);

impl MarkdownPlugin for EndRoom {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        NAME
    }

    fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Paragraph(_) = node else {
            return None;
        };
        (cx.node_source(node)? == TAIL.trim_start()).then(|| MarkdownNode::new(NAME, ()))
    }

    fn render(&self, _: &MarkdownNode, _: &mut Window, _: &mut App) -> impl IntoElement {
        div().w_full().h(px(self.0.max(0.0)))
    }
}

/// Whether `rendered` is `shown` followed by [`TAIL`].
pub(super) fn is_followed(rendered: &str, shown: &str) -> bool {
    rendered.len() == shown.len() + TAIL.len()
        && rendered.starts_with(shown)
        && rendered.ends_with(TAIL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tail_is_a_paragraph_of_its_own_after_any_text() {
        let text = format!("# Title\n\nThe last line.{TAIL}");
        let tree = markdown::to_mdast(&text, &markdown::ParseOptions::gfm()).unwrap();
        let last = tree.children().and_then(|blocks| blocks.last()).unwrap();
        assert!(matches!(last, Node::Paragraph(_)));
        let position = last.position().unwrap();
        assert_eq!(
            &text[position.start.offset..position.end.offset],
            TAIL.trim_start()
        );
    }

    #[test]
    fn a_text_followed_by_the_tail_is_told_from_others() {
        let followed = format!("Text{TAIL}");
        assert!(is_followed(&followed, "Text"));
        assert!(!is_followed("Text", "Text"));
        assert!(!is_followed(&followed, "Other"));
        assert!(!is_followed(&format!("Text{TAIL}!"), "Text"));
    }
}
