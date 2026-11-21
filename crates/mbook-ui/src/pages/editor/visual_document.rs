//! An editable projection of the Markdown source. The parser supplies byte
//! ranges, so formatting the view never serializes or rewrites the chapter.

use std::ops::Range;

use markdown::mdast::{Node, ReferenceKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Style {
    Heading,
    Bold,
    Italic,
    Strike,
    Code,
    Link,
    Image,
    Quote,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Open,
    Close,
    Replace,
}

struct Patch<'a> {
    source: Range<usize>,
    text: &'a str,
    kind: Kind,
}

struct Span {
    source: Range<usize>,
    display: Range<usize>,
    kind: Kind,
}

struct Mark {
    whole: Range<usize>,
    content: Range<usize>,
    style: Style,
}

#[derive(Default)]
pub(super) struct Document {
    pub(super) text: String,
    pub(super) styles: Vec<(Range<usize>, Style)>,
    spans: Vec<Span>,
    marks: Vec<Mark>,
}

impl Document {
    pub(super) fn parse(source: &str) -> Self {
        let mut patches = Vec::new();
        let mut marks = Vec::new();
        if let Ok(root) = markdown::to_mdast(source, &markdown::ParseOptions::gfm()) {
            collect(&root, source, &mut patches, &mut marks);
        }
        patches.sort_unstable_by_key(|patch| patch.source.start);
        let mut text = String::with_capacity(source.len());
        let mut spans = Vec::with_capacity(patches.len());
        let mut previous = 0;
        for patch in patches {
            if patch.source.start < previous {
                continue;
            }
            text.push_str(&source[previous..patch.source.start]);
            let start = text.len();
            text.push_str(patch.text);
            previous = patch.source.end;
            spans.push(Span {
                source: patch.source,
                display: start..text.len(),
                kind: patch.kind,
            });
        }
        text.push_str(&source[previous..]);
        let mut document = Self {
            text,
            spans,
            marks,
            styles: Vec::new(),
        };
        document.styles = document
            .marks
            .iter()
            .map(|mark| {
                let span = document.display_offset(mark.content.start)
                    ..document.display_offset(mark.content.end);
                (span, mark.style)
            })
            .filter(|(range, _)| !range.is_empty())
            .collect();
        document
    }

    pub(super) fn display_offset(&self, offset: usize) -> usize {
        let index = self.spans.partition_point(|span| span.source.end <= offset);
        if let Some(span) = self.spans.get(index)
            && span.source.start <= offset
        {
            return span.display.start;
        }
        index.checked_sub(1).map_or(offset, |previous| {
            let span = &self.spans[previous];
            span.display.end + offset - span.source.end
        })
    }

    fn source_offset(&self, offset: usize, after_hidden: bool) -> usize {
        let index = self.spans.partition_point(|span| {
            span.display.end < offset
                || (span.display.end == offset && (after_hidden || !span.display.is_empty()))
        });
        if let Some(span) = self.spans.get(index)
            && span.display.start <= offset
        {
            return span.source.start;
        }
        index.checked_sub(1).map_or(offset, |previous| {
            let span = &self.spans[previous];
            span.source.end + offset - span.display.end
        })
    }

    pub(super) fn source_range(&self, range: Range<usize>) -> Range<usize> {
        if range.is_empty() {
            let at = range.start;
            let before = self.source_offset(at, false);
            let after = self.source_offset(at, true);
            let offset = if self.spans.iter().any(|span| {
                span.display.is_empty() && span.display.start == at && span.kind == Kind::Open
            }) {
                after
            } else {
                before
            };
            return offset..offset;
        }
        self.source_offset(range.start, true)..self.source_offset(range.end, false)
    }

    /// Map a visual splice onto the original source and preserve delimiters
    /// around the text on either side of the selection.
    pub(super) fn splice(
        &self,
        source: &str,
        range: Range<usize>,
        replacement: &str,
    ) -> (Range<usize>, String, usize) {
        let mut range = self.source_range(range);
        let mut closing = Vec::new();
        let mut opening = Vec::new();
        if !range.is_empty() {
            for mark in self.marks.iter().rev() {
                if mark.style == Style::Heading || mark.whole == mark.content {
                    continue;
                }
                if replacement.is_empty() && range == mark.content {
                    range = mark.whole.clone();
                } else if range.start == mark.content.start && range.end > mark.content.end {
                    range.start = mark.whole.start;
                } else if range.end == mark.content.end && range.start < mark.content.start {
                    range.end = mark.whole.end;
                } else if mark.content.contains(&range.start) && range.end > mark.content.end {
                    closing.push(mark.content.end..mark.whole.end);
                } else if range.start < mark.content.start && mark.content.contains(&range.end) {
                    opening.push(mark.whole.start..mark.content.start);
                }
            }
        }
        closing.sort_unstable_by_key(|range| range.start);
        opening.sort_unstable_by_key(|range| range.start);
        let mut text = String::new();
        for span in closing {
            text.push_str(&source[span]);
        }
        if self.marks.iter().any(|mark| {
            mark.style == Style::Code
                && mark.content.start <= range.start
                && range.end <= mark.content.end
        }) {
            text.push_str(replacement);
        } else if replacement == "\n"
            && !self.marks.iter().any(|mark| {
                !matches!(mark.style, Style::Heading | Style::Quote)
                    && mark.content.start < range.start
                    && range.end < mark.content.end
            })
        {
            text.push_str(&paragraph_break(source, range.start));
        } else {
            escape_prose(replacement, &mut text);
        }
        let caret = range.start + text.len();
        for span in opening {
            text.push_str(&source[span]);
        }
        (range, text, caret)
    }

    pub(super) fn toggle(
        &self,
        source: &str,
        selection: Range<usize>,
        format: Format,
    ) -> Option<(Range<usize>, String, Range<usize>)> {
        let range = self.source_range(selection);
        let (prefix, suffix, style) = format.options();
        if suffix.is_empty() {
            if let Some(mark) = self.marks.iter().find(|mark| {
                mark.style == Style::Heading
                    && mark.content.start <= range.start
                    && range.start <= mark.content.end
            }) {
                let old_prefix = &source[mark.whole.start..mark.content.start];
                let prefix = if old_prefix == prefix { "" } else { prefix };
                let content = &source[mark.content.clone()];
                let offset = mark.whole.start + prefix.len() + range.start - mark.content.start;
                return Some((
                    mark.whole.clone(),
                    format!("{prefix}{content}"),
                    offset..offset,
                ));
            }
            let start = source[..range.start].rfind('\n').map_or(0, |at| at + 1);
            let end = source[range.start..]
                .find('\n')
                .map_or(source.len(), |at| range.start + at);
            let line = &source[start..end];
            let marker = line.split_once(' ').map(|(word, _)| word).filter(|word| {
                matches!(*word, "-" | "+" | "*" | ">")
                    || (!word.is_empty() && word.len() <= 6 && word.bytes().all(|b| b == b'#'))
                    || word.strip_suffix(['.', ')']).is_some_and(|number| {
                        !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())
                    })
            });
            let end = marker.map_or(start, |marker| start + marker.len() + 1);
            let prefix = if &source[start..end] == prefix {
                ""
            } else {
                prefix
            };
            let offset = (range.start - start).saturating_sub(end - start) + start + prefix.len();
            return Some((start..end, prefix.to_owned(), offset..offset));
        }
        if let Some(mark) = self.marks.iter().rev().find(|mark| {
            mark.style == style
                && mark.content.start <= range.start
                && range.end <= mark.content.end
        }) {
            let text = source[mark.content.clone()].to_owned();
            let removed = mark.content.start - mark.whole.start;
            return Some((
                mark.whole.clone(),
                text,
                range.start - removed..range.end - removed,
            ));
        }
        let selected = &source[range.clone()];
        if selected.is_empty() || selected.contains('\n') || selected.trim() != selected {
            return None;
        }
        let start = range.start + prefix.len();
        let end = start + selected.len();
        Some((range, format!("{prefix}{selected}{suffix}"), start..end))
    }
}

/// Continues a list or quotation when Enter is pressed inside it.
fn paragraph_break(source: &str, offset: usize) -> String {
    let start = source[..offset].rfind('\n').map_or(0, |at| at + 1);
    let line = source[start..].split('\n').next().unwrap_or_default();
    let trimmed = line.trim_start_matches([' ', '\t']);
    if let Some((marker, body)) = trimmed.split_once(' ')
        && !body.is_empty()
    {
        let indent = &line[..line.len() - trimmed.len()];
        if matches!(marker, "-" | "+" | "*" | ">") {
            return format!("\n{indent}{marker} ");
        }
        if let Some(number) = marker.strip_suffix(['.', ')'])
            && let Ok(number) = number.parse::<u32>()
            && let Some(separator) = marker.chars().last()
        {
            return format!("\n{indent}{}{separator} ", number.saturating_add(1));
        }
    }
    "\n\n".to_owned()
}

/// The change made by one text input event, in bytes of the old and new
/// buffers. Match Unicode scalars, so both ends are always slice boundaries.
pub(super) fn changed_range(before: &str, after: &str) -> (Range<usize>, Range<usize>) {
    let start: usize = before
        .chars()
        .zip(after.chars())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum();
    let suffix: usize = before[start..]
        .chars()
        .rev()
        .zip(after[start..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum();
    (start..before.len() - suffix, start..after.len() - suffix)
}

#[derive(Clone, Copy)]
pub(super) enum Format {
    Paragraph,
    Heading,
    Subheading,
    Bold,
    Italic,
    Strike,
    Code,
    Bullet,
    Ordered,
    Quote,
}

impl Format {
    const fn options(self) -> (&'static str, &'static str, Style) {
        match self {
            Self::Paragraph => ("", "", Style::Heading),
            Self::Heading => ("# ", "", Style::Heading),
            Self::Subheading => ("## ", "", Style::Heading),
            Self::Bold => ("**", "**", Style::Bold),
            Self::Italic => ("*", "*", Style::Italic),
            Self::Strike => ("~~", "~~", Style::Strike),
            Self::Code => ("`", "`", Style::Code),
            Self::Bullet => ("- ", "", Style::Heading),
            Self::Ordered => ("1. ", "", Style::Heading),
            Self::Quote => ("> ", "", Style::Heading),
        }
    }
}

fn escape_prose(text: &str, out: &mut String) {
    for ch in text.chars() {
        if matches!(
            ch,
            '\\' | '*' | '_' | '~' | '`' | '[' | ']' | '<' | '>' | '#' | '&'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
}

fn collect<'a>(node: &Node, source: &'a str, patches: &mut Vec<Patch<'a>>, marks: &mut Vec<Mark>) {
    let children = node.children().map_or(&[][..], Vec::as_slice);
    if let Some(position) = node.position() {
        let whole = position.start.offset..position.end.offset;
        collect_mark(node, children, source, whole.clone(), patches, marks);
        match node {
            Node::Text(_) => {
                // Map a visible escaped punctuation byte back to its entire
                // source token so deleting it also removes the escape.
                let start = whole
                    .start
                    .checked_sub(1)
                    .filter(|&before| {
                        source.as_bytes()[before] == b'\\'
                            && source
                                .as_bytes()
                                .get(whole.start)
                                .is_some_and(u8::is_ascii_punctuation)
                    })
                    .unwrap_or(whole.start);
                let bytes = &source.as_bytes()[start..whole.end];
                let mut i = 0;
                while i + 1 < bytes.len() {
                    if bytes[i] == b'\\' && bytes[i + 1].is_ascii_punctuation() {
                        let at = start + i;
                        patches.push(Patch {
                            source: at..at + 2,
                            text: &source[at + 1..at + 2],
                            kind: Kind::Replace,
                        });
                        i += 1;
                    }
                    i += 1;
                }
            }
            Node::ListItem(item) => {
                if let Some(mark) = source[whole.clone()].chars().next()
                    && matches!(mark, '-' | '+' | '*')
                {
                    patches.push(Patch {
                        source: whole.start..whole.start + 1,
                        text: "•",
                        kind: Kind::Replace,
                    });
                }
                if let Some(checked) = item.checked
                    && let Some(relative) = source[whole.clone()].find('[')
                {
                    let at = whole.start + relative;
                    patches.push(Patch {
                        source: at..at + 3,
                        text: if checked { "☑" } else { "☐" },
                        kind: Kind::Replace,
                    });
                }
            }
            Node::Blockquote(_) => {
                let mut start = whole.start;
                for line in source[whole.clone()].split_inclusive('\n') {
                    let inside = line.trim_start_matches([' ', '\t']);
                    if inside.starts_with('>') {
                        let at = start + line.len() - inside.len();
                        patches.push(Patch {
                            source: at..at + 1,
                            text: "│",
                            kind: Kind::Replace,
                        });
                    }
                    start += line.len();
                }
            }
            _ => {}
        }
    }
    for child in children {
        collect(child, source, patches, marks);
    }
}

fn collect_mark(
    node: &Node,
    children: &[Node],
    source: &str,
    whole: Range<usize>,
    patches: &mut Vec<Patch<'_>>,
    marks: &mut Vec<Mark>,
) {
    let style = match node {
        Node::Heading(_) => Some(Style::Heading),
        Node::Strong(_) => Some(Style::Bold),
        Node::Emphasis(_) => Some(Style::Italic),
        Node::Delete(_) => Some(Style::Strike),
        Node::Link(_) | Node::LinkReference(_) => Some(Style::Link),
        Node::Image(_) | Node::ImageReference(_) => Some(Style::Image),
        Node::Blockquote(_) => Some(Style::Quote),
        Node::InlineCode(_) | Node::Code(_) => Some(Style::Code),
        _ => None,
    };
    if let Some(style) = style {
        let child_range = || {
            children
                .first()
                .and_then(Node::position)
                .zip(children.last().and_then(Node::position))
                .map_or_else(
                    || whole.clone(),
                    |(first, last)| first.start.offset..last.end.offset,
                )
        };
        let content = match node {
            Node::Heading(_) if children.is_empty() => whole.end..whole.end,
            Node::Heading(_)
            | Node::Strong(_)
            | Node::Emphasis(_)
            | Node::Delete(_)
            | Node::Link(_) => child_range(),
            Node::InlineCode(_) => {
                let n = source[whole.clone()]
                    .bytes()
                    .take_while(|&b| b == b'`')
                    .count();
                whole.start + n..whole.end - n
            }
            Node::LinkReference(reference) if reference.reference_kind == ReferenceKind::Full => {
                child_range()
            }
            Node::ImageReference(reference) if reference.reference_kind != ReferenceKind::Full => {
                whole.clone()
            }
            Node::Image(_) | Node::ImageReference(_) => {
                image_label(&source[whole.clone()], whole.start)
                    .filter(|label| !label.is_empty())
                    .unwrap_or_else(|| whole.clone())
            }
            Node::Code(_) => {
                fenced_body(&source[whole.clone()], whole.start).unwrap_or_else(|| whole.clone())
            }
            _ => whole.clone(),
        };
        if whole.start < content.start {
            patches.push(Patch {
                source: whole.start..content.start,
                text: "",
                kind: Kind::Open,
            });
        }
        if content.end < whole.end {
            patches.push(Patch {
                source: content.end..whole.end,
                text: "",
                kind: Kind::Close,
            });
        }
        marks.push(Mark {
            whole,
            content,
            style,
        });
    }
}

/// Finds the alternative text inside a node already identified as an image.
fn image_label(raw: &str, start: usize) -> Option<Range<usize>> {
    let inside = raw.strip_prefix("![")?;
    let mut depth = 0;
    let mut escaped = false;
    for (offset, ch) in inside.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '[' => depth += 1,
            ']' if depth == 0 => return Some(start + 2..start + 2 + offset),
            ']' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// Finds the code between the opening and closing lines of a fenced block.
fn fenced_body(raw: &str, start: usize) -> Option<Range<usize>> {
    let (first, rest) = raw.split_once('\n')?;
    let marker = first.trim_start().chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let length = first
        .trim_start()
        .chars()
        .take_while(|&ch| ch == marker)
        .count();
    let end = rest.rsplit_once('\n').map_or(rest.len(), |(body, last)| {
        let last = last.trim();
        if last.len() >= length && last.chars().all(|ch| ch == marker) {
            body.len()
        } else {
            rest.len()
        }
    });
    let body = start + first.len() + 1;
    (end > 0).then_some(body..body + end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(source: &str, range: Range<usize>, replacement: &str) -> String {
        let document = Document::parse(source);
        let (range, text, _) = document.splice(source, range, replacement);
        let mut edited = source.to_owned();
        edited.replace_range(range, &text);
        edited
    }

    #[test]
    fn prose_uses_formatted_text_and_keeps_block_markers_readable() {
        let document =
            Document::parse("# Title\n\n**Bold** and *italic*.\n\n- [x] Done\n\n> Quote");
        assert_eq!(
            document.text,
            "Title\n\nBold and italic.\n\n• ☑ Done\n\n│ Quote"
        );
        assert!(document.styles.contains(&(0..5, Style::Heading)));
        assert!(document.styles.contains(&(7..11, Style::Bold)));
    }

    #[test]
    fn typing_in_formatted_text_preserves_the_link_and_unicode() {
        let source = "**Café** and [the book](ch02.md#heading).";
        assert_eq!(
            edit(source, 3..5, "ё"),
            "**Cafё** and [the book](ch02.md#heading)."
        );
    }

    #[test]
    fn replacing_across_a_link_only_removes_the_selected_label() {
        assert_eq!(
            edit("Hi [bookmark](#book) there", 0..7, ""),
            "[mark](#book) there"
        );
    }

    #[test]
    fn removing_all_of_a_nested_mark_removes_its_delimiters() {
        assert_eq!(edit("***word*** later", 0..4, ""), " later");
    }

    #[test]
    fn deleting_escaped_punctuation_removes_the_escape() {
        assert_eq!(edit(r"one \* two", 4..5, ""), "one  two");
    }

    #[test]
    fn visual_paste_escapes_markup_in_prose() {
        let source = edit("", 0..0, "<hello> * world");
        assert_eq!(Document::parse(&source).text, "<hello> * world");
        assert_eq!(source, r"\<hello\> \* world");
    }

    #[test]
    fn editing_an_image_label_keeps_its_path_and_caption() {
        let source = "![The [lake]](img/lake.avif \"Caption\")";
        let document = Document::parse(source);
        assert_eq!(document.text, "The [lake]");
        assert_eq!(
            edit(source, 5..9, "shore"),
            "![The [shore]](img/lake.avif \"Caption\")"
        );
    }

    #[test]
    fn code_fences_are_hidden_and_pasting_inside_them_is_literal() {
        let source = "```rust\na + b\n```";
        assert_eq!(Document::parse(source).text, "a + b");
        assert_eq!(edit(source, 2..3, "*"), "```rust\na * b\n```");
    }

    #[test]
    fn reference_shorthand_stays_visible_when_the_label_is_the_target() {
        let source = "[linked][] and [label][linked]\n\n[linked]: https://example.com";
        assert_eq!(
            Document::parse(source).text,
            "[linked][] and label\n\n[linked]: https://example.com"
        );
    }

    #[test]
    fn a_heading_can_be_changed_back_to_prose() {
        let source = "## Headline";
        let (range, replacement, caret) = Document::parse(source)
            .toggle(source, 2..2, Format::Paragraph)
            .unwrap();
        assert_eq!(
            (range, replacement, caret),
            (0..11, "Headline".to_owned(), 2..2)
        );
    }

    #[test]
    fn a_line_break_inside_a_numbered_list_adds_the_next_item() {
        assert_eq!(edit("3. First", 8..8, "\n"), "3. First\n4. ");
    }

    #[test]
    fn a_line_break_in_prose_starts_a_markdown_paragraph() {
        assert_eq!(edit("First", 5..5, "\n"), "First\n\n");
    }

    #[test]
    fn a_diff_does_not_split_a_multibyte_character() {
        assert_eq!(changed_range("роза", "луза"), (0..4, 0..4));
    }
}
