//! Activates GFM footnote references without changing the source or copy text.

use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::base::text::{
    InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext, MarkdownPlugin,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::{App, Hsla, Styled as _, Window, px, relative};
use markdown::mdast::Node;

pub(super) type FollowNote = Arc<dyn Fn(u32, &mut Window, &mut App) + Send + Sync>;

const NAME: &str = "mbook-footnote";

pub(super) struct Notes {
    pub(super) definitions: Arc<HashMap<String, u32>>,
    pub(super) follow: FollowNote,
    pub(super) ink: Hsla,
}

impl MarkdownPlugin for Notes {
    fn name(&self) -> &str {
        NAME
    }

    fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::FootnoteReference(reference) = node else {
            return None;
        };
        let offset = *self.definitions.get(&reference.identifier)?;
        let source = cx.node_source(node)?;
        let mark = format!("[{}]", reference.identifier);
        Some(
            MarkdownNode::new(NAME, offset)
                .text(mark)
                .markdown(source.to_owned())
                .accessibility_label(format!("Footnote {}", reference.identifier)),
        )
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        style: &InlineRenderContext,
        _: &mut Window,
        _: &mut App,
    ) -> Option<InlineElement> {
        let offset = *node.data::<u32>()?;
        let id = node.source_range()?.start;
        let follow = self.follow.clone();
        let link = Button::new(("footnote", id))
            .link()
            .label(node.as_text().to_owned())
            .accessibility_label(node.accessibility_name().to_owned())
            .text_size(style.font_size())
            .font_family(style.text_style().font_family.clone())
            .text_color(self.ink)
            .min_w(px(0.0))
            .px_0()
            .py_0()
            .h(style.line_height())
            .line_height(relative(1.0))
            .on_click(move |_, window, cx| follow(offset, window, cx));
        Some(InlineElement::new(link))
    }
}
