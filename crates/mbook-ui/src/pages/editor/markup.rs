//! The Markdown the editor writes itself: the heading of a new chapter, a
//! picture, and the way from one file of the book to another.

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

/// Words as Markdown that is read back as the same words: what would start
/// inline syntax is escaped, and the words stay on one line.
///
/// It is for the places the editor writes words into: after the marks of a
/// heading and between the brackets of a picture. There only inline syntax
/// can begin, and `CommonMark` lets a backslash escape any ASCII punctuation.
pub(super) fn inline_text(words: &str) -> String {
    let words = words.trim();
    let mut text = String::with_capacity(words.len() + 8);
    for character in words.chars() {
        match character {
            '\n' | '\r' | '\t' => text.push(' '),
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '&' | '~' | '#' | '|' => {
                text.push('\\');
                text.push(character);
            }
            _ => text.push(character),
        }
    }
    text
}

/// The text a new chapter starts with: its title as a heading of the first
/// level, or nothing for a chapter without a title.
pub(super) fn chapter_start(title: &str) -> String {
    let title = inline_text(title);
    if title.is_empty() {
        String::new()
    } else {
        format!("# {title}\n\n")
    }
}

/// A picture of the book as the chapter `chapter` names it, with the
/// description read to those who cannot see it.
pub(super) fn picture_markdown(chapter: &str, picture: &str, description: &str) -> String {
    format!(
        "![{}]({})",
        inline_text(description),
        relative_book_path(chapter, picture)
    )
}

/// The way from the file `from` to the file `to` of the book, as a link
/// destination: relative, and percent-encoded.
pub(super) fn relative_book_path(from: &str, to: &str) -> String {
    const ESCAPED: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'/')
        .remove(b'.')
        .remove(b'-')
        .remove(b'_');
    let from = from.split('/').collect::<Vec<_>>();
    let to = to.split('/').collect::<Vec<_>>();
    let folders = &from[..from.len() - 1];
    let common = folders
        .iter()
        .zip(&to[..to.len() - 1])
        .take_while(|(left, right)| left == right)
        .count();
    let directory = "../".repeat(folders.len() - common);
    let path = to[common..].join("/");
    let encoded = utf8_percent_encode(&path, ESCAPED);
    format!("{directory}{encoded}")
}

/// A place of the book as a link destination from the chapter `from`: the
/// way to its chapter, and the name of the place after `#`.
pub(super) fn place_destination(from: &str, chapter: &str, anchor: Option<&str>) -> String {
    let mut destination = relative_book_path(from, chapter);
    if let Some(anchor) = anchor {
        const NAME: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_');
        destination.push('#');
        destination.extend(utf8_percent_encode(anchor, NAME));
    }
    destination
}

#[cfg(test)]
mod tests {
    use markdown::ParseOptions;
    use markdown::mdast::Node;

    use super::{chapter_start, inline_text, picture_markdown, place_destination};

    /// Words that hold everything Markdown gives a meaning to inside a line.
    const AWKWARD: [&str; 8] = [
        "Plain words",
        "*Stars* and _lines_ and `ticks`",
        "A [link](x.md) and ![a picture](p.avif)",
        "<b>bold</b> &amp; more",
        "C# and F# #",
        "~~struck~~ a | b",
        "back\\slash",
        "Яблоки: «поздно», 100%!",
    ];

    fn first(text: &str) -> Node {
        let root = markdown::to_mdast(text, &ParseOptions::gfm()).unwrap();
        root.children().unwrap()[0].clone()
    }

    #[test]
    fn escaped_words_are_read_back_as_the_heading_they_were() {
        for words in AWKWARD {
            let Node::Heading(heading) = first(&chapter_start(words)) else {
                panic!("{words}: not a heading");
            };
            assert_eq!(heading.depth, 1);
            assert_eq!(
                heading.children,
                [Node::Text(markdown::mdast::Text {
                    value: words.to_owned(),
                    position: heading.children[0].position().cloned(),
                })],
                "{words}"
            );
        }
    }

    #[test]
    fn escaped_words_are_read_back_as_the_description_of_the_picture() {
        for words in AWKWARD {
            let Node::Paragraph(paragraph) = first(&picture_markdown("a.md", "p.avif", words))
            else {
                panic!("{words}: not a paragraph");
            };
            let [Node::Image(image)] = paragraph.children.as_slice() else {
                panic!("{words}: not one picture");
            };
            assert_eq!((image.alt.as_str(), image.url.as_str()), (words, "p.avif"));
        }
    }

    #[test]
    fn words_are_kept_on_one_line_without_their_edges() {
        assert_eq!(
            inline_text("  two\nlines\tand a tab "),
            "two lines and a tab"
        );
    }

    #[test]
    fn a_chapter_without_a_title_starts_empty() {
        assert_eq!(chapter_start("  "), "");
        assert_eq!(chapter_start("The pond"), "# The pond\n\n");
    }

    #[test]
    fn a_book_picture_uses_an_encoded_path_relative_to_the_chapter() {
        assert_eq!(
            picture_markdown("part/intro/ch.md", "img/cover #1.avif", "The cover"),
            "![The cover](../../img/cover%20%231.avif)"
        );
        assert_eq!(picture_markdown("a.md", "p.avif", ""), "![](p.avif)");
    }

    #[test]
    fn a_place_is_the_way_to_its_chapter_and_its_name() {
        assert_eq!(place_destination("part/a.md", "part/b.md", None), "b.md");
        assert_eq!(
            place_destination("part/a.md", "notes/end.md", Some("сноска 1")),
            "../notes/end.md#%D1%81%D0%BD%D0%BE%D1%81%D0%BA%D0%B0%201"
        );
    }
}
