//! Diagrams in Mermaid notation, drawn with `merman` as SVG.

use merman::svg::SvgPipeline;
use merman::{
    MermaidEngine, OperationControl, ParseOptions, RenderOutput, RenderRequest, Renderer,
    SourcePresentationConfig, SvgRequest,
};
use serde_json::json;

use resvg::usvg;

use super::{Figure, FigureError, Palette, hex};

/// The size of the labels a diagram is drawn with, in pixels. A page scales
/// the drawing to the size of the text around it.
const EM: f32 = 16.0;

/// Colours that tell the parts of a pie, a timeline or a journey apart on a
/// light page and on a dark one.
const SERIES: [u32; 8] = [
    0x0046_50d8,
    0x002e_9e6b,
    0x00d9_8e04,
    0x00c2_410c,
    0x0079_56c9,
    0x000e_8fa8,
    0x00b8_3280,
    0x006b_7280,
];

/// The configuration that draws a diagram in the colours of a page.
///
/// Strict mode disables Mermaid callbacks, so a `click` runs nothing.
fn configuration(palette: Palette) -> serde_json::Value {
    let tone = |part: f32| hex(palette.mix(part));
    let (ink, page) = (hex(palette.ink), hex(palette.page));
    let mut variables = json!({
        "fontSize": format!("{EM}px"),
        // The gradient outline of the nodes of Mermaid 11 is too much beside
        // a page of text.
        "useGradient": false,
        "darkMode": palette.is_dark(),
        "background": page,
        "primaryColor": tone(0.06),
        "primaryBorderColor": tone(0.55),
        "primaryTextColor": ink,
        "secondaryColor": tone(0.12),
        "secondaryBorderColor": tone(0.45),
        "secondaryTextColor": ink,
        "tertiaryColor": tone(0.03),
        "tertiaryBorderColor": tone(0.35),
        "tertiaryTextColor": ink,
        "textColor": ink,
        "titleColor": ink,
        "lineColor": tone(0.7),
        "mainBkg": tone(0.06),
        "nodeBorder": tone(0.55),
        "edgeLabelBackground": page,
        "clusterBkg": tone(0.03),
        "clusterBorder": tone(0.3),
        "actorBkg": tone(0.06),
        "actorBorder": tone(0.55),
        "actorTextColor": ink,
        "actorLineColor": tone(0.5),
        "signalColor": tone(0.7),
        "signalTextColor": ink,
        "labelBoxBkgColor": tone(0.06),
        "labelBoxBorderColor": tone(0.45),
        "labelTextColor": ink,
        "loopTextColor": ink,
        "noteBkgColor": tone(0.1),
        "noteBorderColor": tone(0.4),
        "noteTextColor": ink,
        "activationBkgColor": tone(0.14),
        "activationBorderColor": tone(0.55),
        "pieStrokeColor": page,
        "pieOuterStrokeColor": page,
        "pieTitleTextColor": ink,
        "pieSectionTextColor": hex(0x00ff_ffff),
        "pieLegendTextColor": ink,
    });
    if let Some(variables) = variables.as_object_mut() {
        for (index, colour) in (1..).zip(SERIES) {
            variables.insert(format!("pie{index}"), hex(colour).into());
        }
    }
    json!({
        "theme": "base",
        "securityLevel": "strict",
        // `merman` measures labels with its own tables, not with the font of
        // the book, so the labels are drawn in the grotesque those tables
        // were made for.
        "fontFamily": "Helvetica Neue, Helvetica, Arial, sans-serif",
        "themeVariables": variables,
    })
}

/// Draws a diagram.
pub(super) fn draw(source: &str, palette: Palette) -> Result<Figure, FigureError> {
    let refused = |error: &dyn std::fmt::Display| FigureError::Diagram(error.to_string());
    let engine = MermaidEngine::new()
        .with_site_config(SourcePresentationConfig::from_value(configuration(palette)));
    let renderer = Renderer::new()
        .with_presentation_engine(engine)
        .with_parse_options(ParseOptions::strict());
    // The pipeline turns HTML labels into SVG text: rasterisers such as
    // `resvg` draw no `foreignObject`.
    let request = SvgRequest {
        pipeline: Some(SvgPipeline::resvg_safe()),
        ..SvgRequest::default()
    };
    let output = renderer
        .render(RenderRequest::svg(source, OperationControl::new(), request))
        .map_err(|error| refused(&error))?;
    let RenderOutput::Svg(Some(drawn)) = output else {
        return Err(FigureError::Diagram(
            "the source does not say which diagram it is".to_owned(),
        ));
    };
    let document = drawn.svg().to_owned();
    let size = usvg::Tree::from_str(&document, &usvg::Options::default())
        .map_err(|error| refused(&error))?
        .size();
    Ok(Figure {
        svg: document,
        width: size.width(),
        height: size.height(),
        baseline: size.height(),
        em: EM,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOW: &str =
        "flowchart TD\n  A[Start] --> B{Warm?}\n  B -->|yes| C[Go]\n  B -->|no| D[Stay]";

    #[test]
    fn a_flowchart_is_drawn_with_a_size() {
        let diagram = draw(FLOW, Palette::default()).unwrap();
        assert!(diagram.width > 50.0 && diagram.height > 50.0);
        assert!((diagram.baseline - diagram.height).abs() < f32::EPSILON);
        assert!((diagram.em - EM).abs() < f32::EPSILON);
        assert!(diagram.svg.contains("<svg"));
    }

    #[test]
    fn a_drawing_has_nothing_the_rasteriser_cannot_draw() {
        let diagram = draw(FLOW, Palette::default()).unwrap();
        assert!(!diagram.svg.contains("<foreignObject"));
        assert!(diagram.svg.contains("Start"));
    }

    #[test]
    fn a_remote_image_is_removed_from_the_diagram() {
        let source = "flowchart LR\n  A@{ img: \"https://example.com/image.png\", label: \"Start\" }\n  A --> B";
        let diagram = draw(source, Palette::default()).unwrap();
        assert!(!diagram.svg.contains("https://example.com/image.png"));
    }

    #[test]
    fn labels_outside_ascii_are_drawn() {
        let diagram = draw(
            "flowchart LR\n  A[Вокзал] --> B[Письмо]",
            Palette::default(),
        )
        .unwrap();
        assert!(diagram.svg.contains("Вокзал"));
    }

    #[test]
    fn other_kinds_of_diagram_are_drawn() {
        for source in [
            "sequenceDiagram\n  Alice->>Bob: Hello\n  Bob-->>Alice: Hi",
            "pie title Pets\n  \"Dogs\" : 4\n  \"Cats\" : 6",
            "stateDiagram-v2\n  [*] --> Still\n  Still --> Moving\n  Moving --> [*]",
            "classDiagram\n  Animal <|-- Duck\n  Animal : +int age",
        ] {
            let diagram = draw(source, Palette::default());
            assert!(diagram.is_ok(), "{source}: {diagram:?}");
        }
    }

    #[test]
    fn a_diagram_takes_the_colours_of_its_page() {
        let night = Palette {
            ink: 0x00b8_b8b8,
            page: 0,
        };
        let diagram = draw(FLOW, night).unwrap();
        assert!(diagram.svg.to_ascii_lowercase().contains("#b8b8b8"));
        assert_ne!(diagram.svg, draw(FLOW, Palette::default()).unwrap().svg);
    }

    #[test]
    fn a_source_that_is_no_diagram_says_why() {
        for source in ["flowch", "just words", "flowchart TD\n  A --"] {
            let refused = draw(source, Palette::default());
            assert!(
                matches!(&refused, Err(FigureError::Diagram(why)) if !why.is_empty()),
                "{source}: {refused:?}"
            );
        }
    }

    #[test]
    fn the_configuration_names_the_colours_of_the_page() {
        let configuration = configuration(Palette::default());
        let variables = &configuration["themeVariables"];
        assert_eq!(variables["textColor"], "#1b1b1b");
        assert_eq!(variables["background"], "#ffffff");
        assert_eq!(variables["pie1"], "#4650d8");
        assert_eq!(variables["pie8"], "#6b7280");
        assert_eq!(variables["darkMode"], false);
        assert_eq!(configuration["securityLevel"], "strict");
    }
}
