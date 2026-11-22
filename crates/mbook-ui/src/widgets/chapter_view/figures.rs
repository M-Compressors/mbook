//! The diagrams and formulas of the text.
//!
//! A chapter shows a diagram or a formula with the syntax of a picture
//! (RFC 0001, "Diagram and formula resources"). The Markdown view of the Kit
//! knows pictures only, so the widget takes such a picture away from it with
//! two plugins:
//!
//! * [`BlockFigures`] takes a paragraph that holds nothing but one figure and
//!   draws it as a block in the middle of the column, a formula as a display
//!   formula;
//! * [`LineFigures`] takes any other figure and sets it in its line, a
//!   formula on the baseline of the text.
//!
//! Both read the [`Figures`] of the text, which the widget draws on the
//! background and puts into a [`Table`] the plugins share. A figure that
//! could not be drawn is shown by its alternative text.

use std::sync::{Arc, Mutex};

use gpui_kit::component::text::{
    InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext, MarkdownPlugin,
    markdown_ast::Node,
};
use gpui_kit::{
    App, InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, img, px,
};

use super::enlarge::{EnlargeHandler, Enlarged, opens};
use crate::entities::figure::{Figures, Setting};

/// The name of the nodes [`BlockFigures`] makes.
const BLOCK: &str = "mbook-figure";
/// The name of the nodes [`LineFigures`] makes.
const LINE: &str = "mbook-figure-in-line";

/// The figures of the text that is shown, where the parser of the Kit, which
/// runs on the background, and the widget both reach them.
#[derive(Clone, Default)]
pub(super) struct Table(Arc<Mutex<Arc<Figures>>>);

impl Table {
    /// The figures as they are now.
    pub(super) fn get(&self) -> Arc<Figures> {
        self.0
            .lock()
            .map_or_else(|_| Arc::default(), |figures| Arc::clone(&figures))
    }

    /// Replaces the figures.
    pub(super) fn set(&self, figures: Arc<Figures>) {
        if let Ok(mut held) = self.0.lock() {
            *held = figures;
        }
    }
}

/// What the plugins need to draw a figure in this frame.
#[derive(Clone)]
pub(super) struct Stage {
    /// The figures of the text.
    pub(super) table: Table,
    /// The width of the column of the text, in pixels.
    pub(super) column: f32,
    /// The height a figure may take, in a text that is shown a page at a
    /// time.
    pub(super) page: Option<f32>,
    /// The size of the text, in pixels.
    pub(super) text_size: f32,
    /// The gap under a paragraph, in pixels.
    pub(super) gap: f32,
    /// What a click on a figure that stands alone does; `None` when it
    /// does nothing.
    pub(super) enlarge: Option<EnlargeHandler>,
}

/// A figure the parser found in the text.
struct Shown {
    /// The destination as the Markdown parser reports it.
    destination: String,
    /// The alternative text.
    alt: SharedString,
    /// The title becomes a caption for a figure that stands alone.
    title: Option<SharedString>,
}

/// The figure an image node of the text shows, when it shows one.
fn shown(node: &Node, figures: &Figures) -> Option<Shown> {
    let (destination, alt, title) = match node {
        Node::Image(image) => (image.url.as_str(), &image.alt, image.title.as_deref()),
        Node::ImageReference(reference) => {
            let definition = figures.reference(&reference.identifier)?;
            (
                definition.destination.as_str(),
                &reference.alt,
                definition.title.as_deref(),
            )
        }
        _ => return None,
    };
    figures.shown_by(destination)?;
    Some(Shown {
        destination: destination.to_owned(),
        alt: alt.trim().to_owned().into(),
        title: title
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_owned)
            .map(Into::into),
    })
}

/// The node of a figure. Its text is the alternative text: it is what is
/// copied, searched and read aloud.
fn node_of(name: &'static str, shown: Shown, source: Option<&str>) -> MarkdownNode {
    let alt = shown.alt.clone();
    let text = match &shown.title {
        Some(title) if name == BLOCK && alt.is_empty() => title.clone(),
        Some(title) if name == BLOCK => format!("{alt}\n{title}").into(),
        _ => alt.clone(),
    };
    MarkdownNode::new(name, shown)
        .text(text)
        .markdown(source.unwrap_or_default().to_owned())
        .accessibility_label(alt)
}

/// The size a figure takes on the screen and how much it was scaled for it.
///
/// A figure is drawn for text of its own size; `scale` brings it to the size
/// of the text around it. It is made smaller, never larger, to fit the
/// column and the page.
pub(super) fn fitted(
    natural: (f32, f32),
    scale: f32,
    column: f32,
    page: Option<f32>,
) -> Option<(f32, f32, f32)> {
    let (width, height) = natural;
    let usable = |length: f32| length.is_finite() && length > 0.0;
    if !(usable(width) && usable(height) && usable(scale) && usable(column)) {
        return None;
    }
    let mut scale = scale.min(column / width);
    if let Some(page) = page.filter(|page| usable(*page)) {
        scale = scale.min(page / height);
    }
    Some((width * scale, height * scale, scale))
}

/// The alternative text of a figure that cannot be shown.
fn alternative(alt: &SharedString) -> impl IntoElement + use<> {
    div().italic().child(alt.clone())
}

/// Draws a paragraph that holds nothing but a figure.
pub(super) struct BlockFigures(pub(super) Stage);

impl MarkdownPlugin for BlockFigures {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        BLOCK
    }

    fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Paragraph(paragraph) = node else {
            return None;
        };
        let [only] = paragraph.children.as_slice() else {
            return None;
        };
        let shown = shown(only, &self.0.table.get())?;
        Some(node_of(BLOCK, shown, cx.node_source(node)))
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, _: &mut App) -> impl IntoElement {
        let stage = &self.0;
        let at = node.source_range().map_or(0, |range| range.start);
        let frame = div()
            .id((BLOCK, at))
            .test_support()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .pb(px(stage.gap));
        let Some(shown) = node.data::<Shown>() else {
            return frame;
        };
        let figures = stage.table.get();
        let drawn = figures
            .shown_by(&shown.destination)
            .and_then(|figure| figure.set(Setting::Block).ok())
            .and_then(|figure| {
                let size = fitted(
                    (figure.width, figure.height),
                    stage.text_size / figure.em,
                    stage.column,
                    stage.page.map(|height| {
                        if shown.title.is_some() {
                            (height - 2.0 * stage.text_size).max(1.0)
                        } else {
                            height
                        }
                    }),
                )?;
                Some((figure, size))
            });
        let body = if let Some((figure, (width, height, scale))) = drawn {
            let image = figure.image(scale * window.scale_factor());
            let mut picture = img(image)
                .id(("figure-enlarge", at))
                .flex_none()
                .w(px(width))
                .h(px(height));
            if let Some(enlarge) = stage.enlarge.clone() {
                let figure = Arc::clone(figure);
                let alt = shown.alt.clone();
                picture = picture.cursor_pointer().on_click(move |event, window, cx| {
                    if opens(event) {
                        cx.stop_propagation();
                        enlarge(
                            Enlarged::Figure {
                                figure: Arc::clone(&figure),
                                alt: alt.clone(),
                            },
                            window,
                            cx,
                        );
                    }
                });
            }
            let body = div()
                .id(("figure-image", at))
                .test_support()
                .w_full()
                .flex()
                .justify_center()
                .child(picture);
            if shown.alt.is_empty() {
                body
            } else {
                body.role(Role::Image).aria_label(shown.alt.clone())
            }
            .into_any_element()
        } else {
            alternative(&shown.alt).into_any_element()
        };
        frame.child(body).children(
            shown
                .title
                .clone()
                .map(|title| div().text_center().text_sm().pt_1().child(title)),
        )
    }
}

/// Sets a figure that stands in a line of text.
pub(super) struct LineFigures(pub(super) Stage);

impl MarkdownPlugin for LineFigures {
    fn name(&self) -> &str {
        LINE
    }

    fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let shown = shown(node, &self.0.table.get())?;
        Some(node_of(LINE, shown, cx.node_source(node)))
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        window: &mut Window,
        _: &mut App,
    ) -> Option<InlineElement> {
        let stage = &self.0;
        let shown = node.data::<Shown>()?;
        let figures = stage.table.get();
        let drawn = figures
            .shown_by(&shown.destination)
            .and_then(|figure| figure.set(Setting::Line).ok())
            .and_then(|figure| {
                let size = fitted(
                    (figure.width, figure.height),
                    context.font_size().as_f32() / figure.em,
                    stage.column,
                    stage.page,
                )?;
                Some((figure, size))
            });
        let Some((figure, (width, height, scale))) = drawn else {
            return Some(InlineElement::new(alternative(&shown.alt)));
        };
        let image = figure.image(scale * window.scale_factor());
        Some(
            InlineElement::new(img(image).w(px(width)).h(px(height)))
                .with_baseline(px(figure.baseline * scale)),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use markdown::ParseOptions;

    use super::*;
    use crate::entities::figure::{
        FigureKind, FigureSource, FigureSources, ImageReference, Palette,
    };

    fn figures() -> Figures {
        let sources = FigureSources {
            by_destination: HashMap::from([(
                "math/e.tex".to_owned(),
                FigureSource {
                    kind: FigureKind::Formula,
                    source: "E = mc^2".into(),
                },
            )]),
            by_reference: HashMap::from([(
                "e".to_owned(),
                ImageReference {
                    destination: "math/e.tex".to_owned(),
                    title: Some("Mass–energy equivalence".to_owned()),
                },
            )]),
        };
        Figures::draw(&sources, Palette::PAPER)
    }

    /// The first inline node of the first paragraph of `text`.
    fn first_inline(text: &str) -> Node {
        let root = markdown::to_mdast(text, &ParseOptions::gfm()).unwrap();
        let paragraph = root.children().unwrap().first().unwrap().clone();
        paragraph.children().unwrap().first().unwrap().clone()
    }

    #[test]
    fn a_picture_whose_destination_is_a_figure_is_a_figure() {
        let found = shown(&first_inline("![ Energy ](math/e.tex)\n"), &figures()).unwrap();
        assert_eq!(found.destination, "math/e.tex");
        assert_eq!(found.alt.as_ref(), "Energy");
    }

    #[test]
    fn a_reference_picture_is_a_figure_through_its_definition() {
        let node = first_inline("![Energy][E]\n\n[e]: math/e.tex\n");
        let found = shown(&node, &figures()).unwrap();
        assert_eq!(found.destination, "math/e.tex");
        assert_eq!(found.title.as_deref(), Some("Mass–energy equivalence"));
    }

    #[test]
    fn a_picture_of_the_book_and_a_word_are_no_figures() {
        let figures = figures();
        assert!(shown(&first_inline("![cat](img/cat.avif)\n"), &figures).is_none());
        assert!(shown(&first_inline("![x][other]\n\n[other]: a.avif\n"), &figures).is_none());
        assert!(shown(&first_inline("words\n"), &figures).is_none());
    }

    #[test]
    fn a_figure_takes_the_size_of_the_text_around_it() {
        let (width, height, scale) = fitted((40.0, 20.0), 1.5, 600.0, None).unwrap();
        assert!((width - 60.0).abs() < 0.01 && (height - 30.0).abs() < 0.01);
        assert!((scale - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn a_figure_wider_than_the_column_is_made_smaller() {
        let (width, height, scale) = fitted((1200.0, 300.0), 1.0, 600.0, None).unwrap();
        assert!((width - 600.0).abs() < 0.01 && (height - 150.0).abs() < 0.01);
        assert!((scale - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn a_figure_taller_than_the_page_is_made_smaller() {
        let (width, height, _) = fitted((300.0, 900.0), 1.0, 600.0, Some(300.0)).unwrap();
        assert!((width - 100.0).abs() < 0.01 && (height - 300.0).abs() < 0.01);
    }

    #[test]
    fn a_figure_is_never_made_larger_than_the_text_asks() {
        let (width, _, scale) = fitted((10.0, 10.0), 1.0, 600.0, Some(400.0)).unwrap();
        assert!((width - 10.0).abs() < 0.01 && (scale - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_figure_or_a_column_without_a_size_fits_nothing() {
        assert!(fitted((0.0, 10.0), 1.0, 600.0, None).is_none());
        assert!(fitted((10.0, 10.0), 1.0, 0.0, None).is_none());
        assert!(fitted((10.0, f32::NAN), 1.0, 600.0, None).is_none());
        assert!(fitted((10.0, 10.0), 0.0, 600.0, None).is_none());
        // A page that has no height yet does not limit the figure.
        assert!(fitted((10.0, 10.0), 1.0, 600.0, Some(0.0)).is_some());
    }

    #[test]
    fn the_table_hands_out_what_was_put_into_it() {
        let table = Table::default();
        assert!(table.get().is_empty());
        table.set(Arc::new(figures()));
        assert!(table.get().shown_by("math/e.tex").is_some());
        assert!(table.clone().get().shown_by("math/e.tex").is_some());
    }
}
