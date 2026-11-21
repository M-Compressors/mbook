//! Writing Markdown of the dialect of RFC 0001: what a front end uses to
//! turn the text, the links and the pictures of a source into the text of a
//! section.
//!
//! Escaping is a rule of the mbook dialect, not of Markdown in general: a
//! single tilde marks strikethrough and a bare `www.` starts a link. The
//! functions here are tested against `markdown` 1.0.0, the parser that
//! `mbook-core` reads chapters with.

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

/// What cannot stand in a link destination as it is.
const IN_DESTINATION: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'(')
    .add(b')')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'`');

/// Escapes text of a source so that it is read back as the same text.
///
/// Line feeds are kept. White space at the start and at the end of a line is
/// dropped: it would start a code block or force a line break.
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        escape_line(line.trim_matches([' ', '\t', '\r']), &mut out);
    }
    out
}

/// Escapes one line that has no white space around it.
fn escape_line(line: &str, out: &mut String) {
    // A number followed by `.` or `)` starts a list item.
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    let (number, rest) = line.split_at(digits);
    let starts_item = !number.is_empty() && rest.starts_with(['.', ')']);
    out.push_str(number);
    for (index, character) in rest.char_indices() {
        let at_start = index == 0;
        let special = match character {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '~' | '|' | '&' => true,
            // Headings, list items, rules and the underline of a heading.
            '#' | '+' | '-' | '=' => at_start && number.is_empty(),
            '.' | ')' => at_start && starts_item,
            _ => false,
        };
        if special {
            out.push('\\');
        }
        out.push(character);
    }
}

/// The name of a link target: the identifier an element has in its source,
/// with everything but letters, digits, `-`, `_` and `.` replaced by `-`.
pub(crate) fn anchor_name(id: &str) -> String {
    let safe = |c: char| c.is_alphanumeric() || matches!(c, '-' | '_' | '.');
    let name: String = id.chars().map(|c| if safe(c) { c } else { '-' }).collect();
    if name.is_empty() {
        "-".to_owned()
    } else {
        name
    }
}

/// The destination of a link into the book: the name of a section and,
/// when the link leads to a marked element of it, the identifier of that
/// element. An empty section name means the section the link is in.
pub(crate) fn destination(section: &str, id: Option<&str>) -> String {
    let mut out = utf8_percent_encode(section, IN_DESTINATION).to_string();
    if let Some(id) = id {
        out.push('#');
        out.push_str(&encoded_name(&anchor_name(id)));
    }
    out
}

/// A name as the fragment of a destination.
pub(crate) fn encoded_name(name: &str) -> String {
    utf8_percent_encode(name, IN_DESTINATION).to_string()
}

/// A path of the book as a destination.
pub(crate) fn encoded_path(path: &str) -> String {
    utf8_percent_encode(path, IN_DESTINATION).to_string()
}

/// An anchor definition for a name that is already safe: one line that
/// names the line it stands on.
pub(crate) fn definition(name: &str) -> String {
    format!("[{name}]: #{}", encoded_name(name))
}

/// The end of a link or a picture: its destination and its title.
pub(crate) fn target(destination: &str, title: Option<&str>) -> String {
    match title.filter(|title| !title.is_empty()) {
        Some(title) => format!("({destination} \"{}\")", escape_title(title)),
        None => format!("({destination})"),
    }
}

/// A title between double quotes.
fn escape_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for character in title.chars() {
        match character {
            '"' | '\\' => {
                out.push('\\');
                out.push(character);
            }
            // A title cannot hold an empty line.
            '\n' | '\r' => out.push(' '),
            _ => out.push(character),
        }
    }
    out
}

/// A picture: its alternative text, its destination and its title.
pub(crate) fn image(alt: &str, destination: &str, title: Option<&str>) -> String {
    let alt = escape(alt).replace('\n', " ");
    format!("![{alt}]{}", target(destination, title))
}

#[cfg(test)]
mod tests {
    use super::{anchor_name, definition, destination, escape, image, target};
    use markdown::mdast::Node;
    use markdown::{ParseOptions, to_mdast};

    /// The text a renderer shows for Markdown, with the kinds of the nodes
    /// that are neither a paragraph nor text.
    fn rendered(markdown: &str) -> (String, Vec<&'static str>) {
        fn visit(node: &Node, text: &mut String, other: &mut Vec<&'static str>) {
            match node {
                Node::Root(_) | Node::Paragraph(_) => {}
                Node::Text(node) => text.push_str(&node.value),
                Node::Heading(_) => other.push("heading"),
                Node::List(_) => other.push("list"),
                Node::Link(_) => other.push("link"),
                Node::Emphasis(_) | Node::Strong(_) | Node::Delete(_) => other.push("emphasis"),
                Node::Code(_) | Node::InlineCode(_) => other.push("code"),
                Node::Html(_) => other.push("html"),
                _ => other.push("other"),
            }
            for child in node.children().map(Vec::as_slice).unwrap_or_default() {
                visit(child, text, other);
            }
        }
        let root = to_mdast(markdown, &ParseOptions::gfm()).unwrap();
        let (mut text, mut other) = (String::new(), Vec::new());
        visit(&root, &mut text, &mut other);
        (text, other)
    }

    #[test]
    fn escaped_text_is_read_back_as_plain_text() {
        let lines = [
            "# not a heading",
            "- not a list item",
            "+ nor this",
            "1. not a numbered item",
            "12) nor this",
            "> not a quote",
            "*not emphasis* and _nor this_ and ~not struck~",
            "`not code` and <b>not html</b> and <!-- no comment -->",
            "[not a link](dest) and ![not a picture](dest)",
            "[^1] is not a footnote",
            "a | b | c",
            "AT&amp;T and &copy; stay as written",
            "a backslash \\ and a \\* pair",
            "---",
            "===",
            "***",
            "1.",
            "2024. A year at the start of a line",
            "    four spaces in front",
            "ends with two spaces  ",
            "[fig]: #fig",
        ];
        for line in lines {
            let (text, other) = rendered(&escape(line));
            assert_eq!((text.as_str(), other), (line.trim(), Vec::new()));
        }
    }

    #[test]
    fn escaped_lines_stay_separate_lines_of_one_paragraph() {
        let (text, other) = rendered(&escape("One\n===\nTwo\n---\n# Three"));
        assert_eq!(
            (text.as_str(), other),
            ("One\n===\nTwo\n---\n# Three", Vec::new())
        );
    }

    #[test]
    fn an_empty_line_still_ends_a_paragraph() {
        let escaped = escape("One.\n\nTwo.");
        assert_eq!(escaped, "One.\n\nTwo.");
    }

    #[test]
    fn text_without_markup_is_not_changed() {
        let text = "Мастер и Маргарита, 1967. «Никогда…» — it costs $5 (five).";
        assert_eq!(escape(text), text);
    }

    #[test]
    fn a_name_keeps_letters_digits_and_three_marks() {
        assert_eq!(anchor_name("fig_1.2-a"), "fig_1.2-a");
        assert_eq!(anchor_name("рис 1/2"), "рис-1-2");
        assert_eq!(anchor_name(""), "-");
    }

    #[test]
    fn a_destination_names_a_section_and_an_element() {
        assert_eq!(destination("ch 1.xhtml", None), "ch%201.xhtml");
        assert_eq!(destination("ch1.xhtml", Some("note 1")), "ch1.xhtml#note-1");
        assert_eq!(destination("", Some("top")), "#top");
    }

    #[test]
    fn a_definition_is_an_anchor_definition_of_the_format() {
        let book = &mut mbook_core::Book::new("T", "en").unwrap();
        let text = format!("{}\nA line.\n", definition("рис-1"));
        let chapter = book.add_chapter("a.md", text).unwrap();
        let names: Vec<_> = chapter
            .extra_anchors()
            .iter()
            .map(|anchor| anchor.name.as_str())
            .collect();
        assert_eq!(names, ["рис-1"]);
    }

    #[test]
    fn a_title_is_quoted_and_escaped() {
        assert_eq!(target("a.md", None), "(a.md)");
        assert_eq!(target("a.md", Some("")), "(a.md)");
        assert_eq!(
            target("a.md", Some("A \"quoted\" \\ title\nin two lines")),
            r#"(a.md "A \"quoted\" \\ title in two lines")"#
        );
    }

    #[test]
    fn a_picture_is_read_back_with_its_parts() {
        let markdown = image("A [bracketed]\nalt", "img/0001.avif", Some("Fig. \"1\""));
        let root = to_mdast(&markdown, &ParseOptions::gfm()).unwrap();
        let Some(Node::Paragraph(paragraph)) = root.children().and_then(|nodes| nodes.first())
        else {
            panic!("a picture is a paragraph");
        };
        let Some(Node::Image(picture)) = paragraph.children.first() else {
            panic!("the paragraph holds a picture");
        };
        assert_eq!(
            (
                picture.alt.as_str(),
                picture.url.as_str(),
                picture.title.as_deref()
            ),
            ("A [bracketed] alt", "img/0001.avif", Some("Fig. \"1\""))
        );
    }

    #[test]
    fn the_helpers_compose_into_marks_links_and_pictures() {
        // What `draft` tells a front end to write.
        let mark = format!("{}\n\n", definition(&anchor_name("note 1")));
        let link = format!(
            "[the *note*]{}",
            target(&destination("ch1", Some("note 1")), None)
        );
        let picture = image("A cat", &super::encoded_path("images/cat 1.png"), None);
        assert_eq!(mark, "[note-1]: #note-1\n\n");
        assert_eq!(link, "[the *note*](ch1#note-1)");
        assert_eq!(picture, "![A cat](images/cat%201.png)");
    }
}
