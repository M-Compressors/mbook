//! Pictures and figures that open larger when they are clicked.
//!
//! The Markdown view of the Kit draws a picture as an image that answers no
//! click. A paragraph that holds nothing but one picture is therefore taken
//! from it by [`BlockPictures`], which draws the picture the way the Kit
//! does, from the same source, and tells the owner of the text when it is
//! clicked or tapped. A figure that stands alone answers the same way
//! ([`super::figures::BlockFigures`]). A picture inside a line of text, a
//! picture that is a link and a reference-style picture stay with the Kit.

use std::sync::Arc;

use gpui_kit::base::text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin};
use gpui_kit::{
    App, ClickEvent, ImageSource, InteractiveElement as _, IntoElement, MouseButton, ObjectFit,
    ParentElement as _, Role, SharedString, SharedUri, StatefulInteractiveElement as _,
    Styled as _, StyledImage as _, TestSupportExt as _, Window, div, img, relative,
};
use markdown::mdast::Node;

use super::figures::Table;
use crate::entities::figure::Figure;

/// The name of the nodes [`BlockPictures`] makes.
const NAME: &str = "mbook-picture";

/// What was clicked to be seen larger.
#[derive(Clone)]
pub(crate) enum Enlarged {
    /// A picture of the book, by its destination as written in the text.
    Picture {
        destination: String,
        alt: SharedString,
    },
    /// A diagram or a formula, as it was drawn for the text.
    Figure {
        figure: Arc<Figure>,
        alt: SharedString,
    },
}

impl Enlarged {
    /// The alternative text of what was clicked.
    pub(crate) fn alt(&self) -> &SharedString {
        match self {
            Self::Picture { alt, .. } | Self::Figure { alt, .. } => alt,
        }
    }
}

/// What happens when a picture or a figure of the text is clicked.
pub(crate) type EnlargeHandler = Arc<dyn Fn(Enlarged, &mut Window, &mut App) + Send + Sync>;

/// The source of an image of the text, as the view hands it to the Kit.
pub(super) type SourceOf = Arc<dyn Fn(&SharedUri) -> ImageSource + Send + Sync>;

/// Whether a click opens what it fell on: the main button, a key, or a tap
/// that is not a long press. A long press is left to the selection.
pub(super) fn opens(event: &ClickEvent) -> bool {
    match event {
        ClickEvent::Mouse(click) => click.up.button == MouseButton::Left,
        ClickEvent::Keyboard(_) => true,
        ClickEvent::Touch(click) => !click.long_press,
    }
}

/// A picture the parser found alone in a paragraph.
struct Alone {
    destination: SharedString,
    alt: SharedString,
}

/// The picture a paragraph holds and nothing else, unless it is a figure.
fn alone(node: &Node, figures: &Table) -> Option<Alone> {
    let Node::Paragraph(paragraph) = node else {
        return None;
    };
    let [Node::Image(image)] = paragraph.children.as_slice() else {
        return None;
    };
    if figures.get().shown_by(&image.url).is_some() {
        return None;
    }
    Some(Alone {
        destination: image.url.clone().into(),
        alt: image.alt.trim().to_owned().into(),
    })
}

/// Draws a paragraph that holds nothing but a picture, as the Kit does, and
/// tells `enlarge` when it is clicked. The Kit puts the gap of a paragraph
/// under it.
pub(super) struct BlockPictures {
    pub(super) figures: Table,
    pub(super) source: SourceOf,
    pub(super) enlarge: EnlargeHandler,
}

impl MarkdownPlugin for BlockPictures {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        NAME
    }

    fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let alone = alone(node, &self.figures)?;
        let alt = alone.alt.clone();
        Some(
            MarkdownNode::new(NAME, alone)
                .text(alt.clone())
                .markdown(cx.node_source(node).unwrap_or_default().to_owned())
                .accessibility_label(alt),
        )
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, _: &mut App) -> impl IntoElement {
        let at = node.source_range().map_or(0, |range| range.start);
        let frame = div().id((NAME, at)).w_full().flex();
        let Some(alone) = node.data::<Alone>() else {
            return frame;
        };
        let destination = alone.destination.to_string();
        let alt = alone.alt.clone();
        let enlarge = Arc::clone(&self.enlarge);
        let image = img((self.source)(&SharedUri::from(alone.destination.clone())))
            .object_fit(ObjectFit::Contain)
            .max_w(relative(1.));
        let picture = div()
            .id(("picture-image", at))
            .max_w_full()
            .cursor_pointer()
            .on_click(move |event, window, cx| {
                if opens(event) {
                    cx.stop_propagation();
                    enlarge(
                        Enlarged::Picture {
                            destination: destination.clone(),
                            alt: alt.clone(),
                        },
                        window,
                        cx,
                    );
                }
            })
            .child(image);
        // A picture without words is not announced, as the Kit does not.
        let picture = if alone.alt.is_empty() {
            picture.into_any_element()
        } else {
            picture
                .test_support()
                .role(Role::Image)
                .aria_label(alone.alt.clone())
                .into_any_element()
        };
        frame.child(picture)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use markdown::ParseOptions;

    use super::*;
    use crate::entities::figure::{FigureKind, FigureSource, FigureSources, Figures, Palette};

    fn first_block(text: &str) -> Node {
        let root = markdown::to_mdast(text, &ParseOptions::gfm()).unwrap();
        root.children().unwrap().first().unwrap().clone()
    }

    fn with_formula() -> Table {
        let table = Table::default();
        let sources = FigureSources {
            by_destination: HashMap::from([(
                "math/e.tex".to_owned(),
                FigureSource {
                    kind: FigureKind::Formula,
                    source: "E = mc^2".into(),
                },
            )]),
            by_reference: HashMap::new(),
        };
        table.set(Arc::new(Figures::draw(&sources, Palette::PAPER)));
        table
    }

    #[test]
    fn a_picture_alone_in_a_paragraph_opens_larger() {
        let found = alone(
            &first_block("![ A cat ](img/cat.avif)\n"),
            &Table::default(),
        )
        .unwrap();
        assert_eq!(found.destination.as_ref(), "img/cat.avif");
        assert_eq!(found.alt.as_ref(), "A cat");
    }

    #[test]
    fn a_picture_among_words_or_beside_another_stays_with_the_kit() {
        let table = Table::default();
        assert!(alone(&first_block("See ![cat](img/cat.avif)\n"), &table).is_none());
        assert!(alone(&first_block("![a](a.avif)![b](b.avif)\n"), &table).is_none());
        assert!(alone(&first_block("words\n"), &table).is_none());
        assert!(alone(&first_block("# ![cat](img/cat.avif)\n"), &table).is_none());
    }

    #[test]
    fn a_picture_that_is_a_link_or_a_reference_stays_with_the_kit() {
        let table = Table::default();
        let link = "[![cat](img/cat.avif)](https://example.org)\n";
        assert!(alone(&first_block(link), &table).is_none());
        let reference = "![cat][c]\n\n[c]: img/cat.avif\n";
        assert!(alone(&first_block(reference), &table).is_none());
    }

    #[test]
    fn a_figure_is_left_to_the_plugin_of_figures() {
        assert!(alone(&first_block("![E](math/e.tex)\n"), &with_formula()).is_none());
    }

    #[test]
    fn the_alternative_text_names_what_was_enlarged() {
        let picture = Enlarged::Picture {
            destination: "a.avif".to_owned(),
            alt: "A cat".into(),
        };
        assert_eq!(picture.alt().as_ref(), "A cat");
    }

    #[test]
    fn a_key_or_a_tap_opens_but_a_long_press_does_not() {
        use gpui_kit::{KeyboardClickEvent, TouchClickEvent};
        assert!(opens(&ClickEvent::Keyboard(KeyboardClickEvent::default())));
        assert!(opens(&ClickEvent::Touch(TouchClickEvent::default())));
        assert!(!opens(&ClickEvent::Touch(TouchClickEvent {
            long_press: true,
            ..TouchClickEvent::default()
        })));
    }
}
