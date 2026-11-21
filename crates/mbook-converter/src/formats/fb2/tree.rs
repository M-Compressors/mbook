//! A small document tree built from the events of the XML parser.
//!
//! Elements and attributes are known by their local name: the prefix of a
//! namespace (`l:href`, `xlink:href`) is dropped, so that `href` is the same
//! attribute whatever the document calls its namespace. Text is kept as the
//! document has it, white space included, because it decides whether two
//! inline elements are separated. The base64 of a `binary` element never
//! enters the tree: it is gathered on the side, under the limits.

use crate::error::{Error, Result};
use crate::format::Format;
use mbook::Limits;
use quick_xml::XmlVersion;
use quick_xml::encoding::{decode, detect_encoding};
use quick_xml::escape::resolve_html5_entity;
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::reader::Reader;
use std::borrow::Cow;
use std::collections::HashSet;

/// How many elements may be open at once, the root included.
pub(super) const MAX_DEPTH: usize = 256;

/// What an element holds.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Node {
    /// A child element.
    Element(Element),
    /// A run of text, entities already replaced.
    Text(String),
}

/// An element: its local name, its attributes and its children in document
/// order.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Element {
    /// The name without a namespace prefix.
    pub(super) name: String,
    /// Attribute names without prefix, and their unescaped values.
    attributes: Vec<(String, String)>,
    /// Elements and text, as the document has them.
    pub(super) children: Vec<Node>,
}

impl Element {
    /// An element without attributes or children.
    fn new(name: String) -> Self {
        Self {
            name,
            attributes: Vec::new(),
            children: Vec::new(),
        }
    }

    /// The value of an attribute, by local name; the first one wins.
    pub(super) fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The child elements, in document order.
    pub(super) fn elements(&self) -> impl Iterator<Item = &Self> {
        self.children.iter().filter_map(|node| match node {
            Node::Element(element) => Some(element),
            Node::Text(_) => None,
        })
    }

    /// The child elements of a name.
    pub(super) fn named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Self> {
        self.elements().filter(move |element| element.name == name)
    }

    /// The first child element of a name.
    pub(super) fn child<'a>(&'a self, name: &'a str) -> Option<&'a Self> {
        self.named(name).next()
    }
}

/// A `binary` element: the identifier and the base64 text without white
/// space.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Binary {
    /// The `id`, trimmed.
    pub(super) id: String,
    /// The base64 characters.
    pub(super) data: Vec<u8>,
}

/// A parsed document.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Document {
    /// The root element.
    pub(super) root: Element,
    /// The pictures worth decoding: one for each identifier, the first
    /// wins.
    pub(super) binaries: Vec<Binary>,
}

/// A source error of this format.
fn bad(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Error {
    Error::source(Format::Fb2, error)
}

/// Turns bytes in any encoding the document declares into text.
///
/// A byte order mark or the declaration of the document tells the encoding;
/// without either it is UTF-8.
fn decode_document(source: &[u8]) -> Result<Cow<'_, str>> {
    let detected = detect_encoding(source);
    let (bom, encoding) = detected
        .as_ref()
        .map_or((0, None), |found| (found.bom_len(), Some(found.encoding())));
    let bytes = source.get(bom..).unwrap_or_default();
    if encoding.is_some_and(|encoding| !encoding.is_ascii_compatible()) {
        // UTF-16: the encoding is known from the first bytes.
        return encoding.map_or_else(
            || Ok(Cow::Borrowed("")),
            |encoding| decode(bytes, encoding).map_err(bad),
        );
    }
    // An encoding of the declaration, that is written in ASCII.
    let mut reader = Reader::from_reader(bytes);
    let declared = match reader.read_event() {
        Ok(Event::Decl(declaration)) => declaration.encoder(),
        _ => None,
    };
    match declared.filter(|declared| declared.is_ascii_compatible()) {
        Some(declared) => decode(bytes, declared).map_err(bad),
        None => std::str::from_utf8(bytes).map(Cow::Borrowed).map_err(bad),
    }
}

/// The element a tag opens, with its attributes.
fn element(start: &BytesStart<'_>) -> Result<Element> {
    let mut element = Element::new(start.local_name().as_ref().to_owned());
    for attribute in start.attributes() {
        let attribute = attribute.map_err(bad)?;
        let key = attribute.key.local_name().as_ref().to_owned();
        // `xmlns` and `xmlns:l` only declare prefixes.
        if key == "xmlns" || attribute.key.as_ref().starts_with("xmlns:") {
            continue;
        }
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(bad)?;
        element.attributes.push((key, value.into_owned()));
    }
    Ok(element)
}

/// The text a reference such as `&amp;`, `&#8212;` or `&nbsp;` stands for.
///
/// XML itself has five named entities. Files in the wild use those of HTML
/// as well, so they are accepted.
fn reference(reference: &BytesRef<'_>) -> Result<String> {
    if let Some(character) = reference.resolve_char_ref().map_err(bad)? {
        return Ok(character.to_string());
    }
    resolve_html5_entity(reference)
        .map(str::to_owned)
        .ok_or_else(|| bad(format!("unknown entity &{};", &**reference)))
}

/// The size of a picture, from the length of its base64 text.
fn decoded_size(length: usize) -> u64 {
    u64::try_from(length).map_or(u64::MAX, |length| length / 4 * 3)
}

/// Builds the tree while the events come in.
struct Builder<'a> {
    /// The limits of the book.
    limits: &'a Limits,
    /// The open elements, the root first.
    stack: Vec<Element>,
    /// The finished root.
    root: Option<Element>,
    /// The `binary` element being read.
    binary: Option<Binary>,
    /// The pictures read so far.
    binaries: Vec<Binary>,
    /// Identifiers of the pictures that were taken.
    seen: HashSet<String>,
    /// The size the finished pictures decode to.
    total: u64,
}

impl<'a> Builder<'a> {
    /// A builder with nothing read.
    fn new(limits: &'a Limits) -> Self {
        Self {
            limits,
            stack: Vec::new(),
            root: None,
            binary: None,
            binaries: Vec::new(),
            seen: HashSet::new(),
            total: 0,
        }
    }

    /// Checks the number of open elements, `extra` of them not yet pushed.
    fn check_depth(&self, extra: usize) -> Result<()> {
        if self.stack.len() + extra > MAX_DEPTH {
            return Err(Error::Limit(
                "the elements of the document are nested too deeply",
            ));
        }
        Ok(())
    }

    /// Tells whether a tag opens a `binary` element that is a picture.
    fn is_binary(&self, element: &Element) -> bool {
        element.name == "binary" && self.stack.len() == 1
    }

    /// Opens an element.
    fn start(&mut self, element: Element) -> Result<()> {
        if self.binary.is_some() {
            return Err(bad("an element inside a binary"));
        }
        if self.root.is_some() {
            return Err(bad("more than one root element"));
        }
        self.check_depth(1)?;
        if self.is_binary(&element) {
            let id = element.attr("id").unwrap_or_default().trim().to_owned();
            // Only a picture that will be used is gathered, so the size of
            // the others does not count.
            let counted = !id.is_empty() && self.seen.insert(id.clone());
            self.binary = Some(Binary {
                id: if counted { id } else { String::new() },
                data: Vec::new(),
            });
        }
        self.stack.push(element);
        Ok(())
    }

    /// Closes the open element.
    fn end(&mut self) -> Result<()> {
        let Some(element) = self.stack.pop() else {
            return Err(bad("an end tag without a start tag"));
        };
        if let Some(binary) = self.binary.take() {
            self.total = self.total.saturating_add(decoded_size(binary.data.len()));
            if !binary.id.is_empty() {
                self.binaries.push(binary);
            }
        }
        match self.stack.last_mut() {
            Some(parent) => parent.children.push(Node::Element(element)),
            None => self.root = Some(element),
        }
        Ok(())
    }

    /// Adds text to the open element.
    fn text(&mut self, text: &str) -> Result<()> {
        if let Some(binary) = &mut self.binary {
            if binary.id.is_empty() {
                return Ok(());
            }
            let added = text
                .bytes()
                .filter(|byte| !byte.is_ascii_whitespace())
                .count();
            let length = binary.data.len().saturating_add(added);
            let size = decoded_size(length);
            if size > self.limits.max_resource_bytes {
                return Err(Error::Limit("a picture is larger than the limit"));
            }
            if self.total.saturating_add(size) > self.limits.max_file_bytes {
                return Err(Error::Limit(
                    "the pictures together are larger than the limit",
                ));
            }
            binary
                .data
                .extend(text.bytes().filter(|byte| !byte.is_ascii_whitespace()));
            return Ok(());
        }
        match self.stack.last_mut() {
            Some(open) => match open.children.last_mut() {
                Some(Node::Text(last)) => last.push_str(text),
                _ => open.children.push(Node::Text(text.to_owned())),
            },
            None if text.trim().is_empty() => {}
            None => return Err(bad("text outside the root element")),
        }
        Ok(())
    }
}

/// Reads a document into a tree.
///
/// # Errors
///
/// The text is not well-formed XML ([`Error::Source`]), or the elements are
/// nested more deeply than [`MAX_DEPTH`] or a picture is too large
/// ([`Error::Limit`]).
pub(super) fn parse(source: &[u8], limits: &Limits) -> Result<Document> {
    let text = decode_document(source)?;
    let mut reader = Reader::from_str(&text);
    let mut builder = Builder::new(limits);
    loop {
        match reader.read_event().map_err(bad)? {
            Event::Start(start) => builder.start(element(&start)?)?,
            Event::Empty(start) => {
                builder.start(element(&start)?)?;
                builder.end()?;
            }
            Event::End(_) => builder.end()?,
            Event::Text(text) => builder.text(&text.xml10_content())?,
            Event::CData(data) => builder.text(&data.xml10_content())?,
            Event::GeneralRef(name) => builder.text(&reference(&name)?)?,
            Event::Eof => break,
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
    }
    if !builder.stack.is_empty() {
        return Err(bad("an element is not closed"));
    }
    let root = builder
        .root
        .ok_or_else(|| bad("the document has no root element"))?;
    Ok(Document {
        root,
        binaries: builder.binaries,
    })
}

#[cfg(test)]
mod tests {
    use super::{Document, MAX_DEPTH, Node, parse};
    use crate::error::Error;
    use mbook::Limits;

    fn read(xml: &str) -> Result<Document, Error> {
        parse(xml.as_bytes(), &Limits::default())
    }

    #[test]
    fn elements_and_text_keep_the_order_of_the_document() -> Result<(), Error> {
        let document = read("<a>x<b/>y<c>z</c></a>")?;
        let kinds: Vec<String> = document
            .root
            .children
            .iter()
            .map(|node| match node {
                Node::Text(text) => format!("t:{text}"),
                Node::Element(element) => format!("e:{}", element.name),
            })
            .collect();
        assert_eq!(kinds, ["t:x", "e:b", "t:y", "e:c"]);
        Ok(())
    }

    #[test]
    fn prefixes_of_names_and_attributes_are_dropped() -> Result<(), Error> {
        let document =
            read(r#"<f:a xmlns:f="u" xmlns:q="v" q:href="1" href="2" plain="3"><f:b/></f:a>"#)?;
        assert_eq!(document.root.name, "a");
        assert_eq!(document.root.attr("href"), Some("1"));
        assert_eq!(document.root.attr("plain"), Some("3"));
        assert_eq!(document.root.attr("q"), None);
        assert_eq!(document.root.attr("xmlns"), None);
        assert!(document.root.child("b").is_some());
        assert_eq!(document.root.child("none"), None);
        assert_eq!(document.root.named("b").count(), 1);
        Ok(())
    }

    #[test]
    fn references_and_cdata_become_text() -> Result<(), Error> {
        let document = read(
            "<a t=\"&lt;&#8212;&#x2014;\">&amp;&lt;&gt;&quot;&apos;&#8212;&#x2014;&#x41;<![CDATA[<b>]]></a>",
        )?;
        assert_eq!(
            document.root.children,
            [Node::Text("&<>\"'\u{2014}\u{2014}A<b>".to_owned())]
        );
        assert_eq!(document.root.attr("t"), Some("<\u{2014}\u{2014}"));
        Ok(())
    }

    #[test]
    fn a_reference_that_stands_for_nothing_is_refused() {
        for xml in [
            "<a>&nosuch;</a>",
            "<a>&#0;</a>",
            "<a>&#xD800;</a>",
            "<a>&#zz;</a>",
        ] {
            assert!(matches!(read(xml), Err(Error::Source { .. })), "{xml}");
        }
    }

    #[test]
    fn broken_markup_is_refused() {
        for xml in [
            "<a><b></a>",
            "<a>",
            "",
            "<a/><b/>",
            "<a/>text",
            "text<a/>",
            "</a>",
            "<a x=\"1\" x=\"2\"/>",
            "<a x=\"&nope;\"/>",
        ] {
            assert!(matches!(read(xml), Err(Error::Source { .. })), "{xml}");
        }
    }

    #[test]
    fn white_space_outside_the_root_is_ignored() -> Result<(), Error> {
        let document = read("<?xml version=\"1.0\"?>\n<!-- c -->\n<a/>\n")?;
        assert_eq!(document.root.name, "a");
        Ok(())
    }

    #[test]
    fn nesting_is_bounded() -> Result<(), Error> {
        let nested = |depth: usize| format!("{}{}", "<a>".repeat(depth), "</a>".repeat(depth));
        read(&nested(MAX_DEPTH))?;
        assert!(matches!(read(&nested(MAX_DEPTH + 1)), Err(Error::Limit(_))));
        let empty = format!(
            "{}<a/>{}",
            "<a>".repeat(MAX_DEPTH),
            "</a>".repeat(MAX_DEPTH)
        );
        assert!(matches!(read(&empty), Err(Error::Limit(_))));
        Ok(())
    }

    #[test]
    fn a_binary_is_gathered_without_white_space_and_stays_out_of_the_tree() -> Result<(), Error> {
        let document = read(
            "<a><binary id=\" p \">QU\n JD</binary><binary id=\"p\">XX</binary><binary>XX</binary><b/></a>",
        )?;
        assert_eq!(document.binaries.len(), 1);
        assert_eq!(document.binaries[0].id, "p");
        assert_eq!(document.binaries[0].data, b"QUJD");
        assert_eq!(document.root.named("binary").count(), 3);
        assert!(
            document
                .root
                .named("binary")
                .all(|binary| binary.children.is_empty())
        );
        Ok(())
    }

    #[test]
    fn a_binary_beyond_the_root_is_an_ordinary_element() -> Result<(), Error> {
        let document = read("<a><b><binary id=\"p\">QUJD</binary></b></a>")?;
        assert_eq!(document.binaries.len(), 0);
        Ok(())
    }

    #[test]
    fn an_element_inside_a_binary_is_refused() {
        assert!(matches!(
            read("<a><binary id=\"p\"><b/></binary></a>"),
            Err(Error::Source { .. })
        ));
    }

    #[test]
    fn pictures_are_bounded_one_by_one_and_together() {
        let xml = "<a><binary id=\"p\">QUJDQUJD</binary><binary id=\"q\">QUJDQUJD</binary></a>";
        let mut limits = Limits::default();
        limits.max_resource_bytes = 5;
        assert!(matches!(
            parse(xml.as_bytes(), &limits),
            Err(Error::Limit(_))
        ));
        let mut limits = Limits::default();
        limits.max_file_bytes = 9;
        assert!(matches!(
            parse(xml.as_bytes(), &limits),
            Err(Error::Limit(_))
        ));
        limits.max_file_bytes = 12;
        assert!(parse(xml.as_bytes(), &limits).is_ok());
    }

    #[test]
    fn a_picture_that_is_not_used_does_not_count() {
        let xml = "<a><binary id=\"p\">QUJD</binary><binary id=\"p\">QUJDQUJDQUJD</binary><binary>QUJDQUJDQUJD</binary></a>";
        let mut limits = Limits::default();
        limits.max_resource_bytes = 3;
        assert!(parse(xml.as_bytes(), &limits).is_ok());
    }

    #[test]
    fn the_encoding_of_the_declaration_is_applied() -> Result<(), Error> {
        let mut bytes = b"<?xml version=\"1.0\" encoding=\"koi8-r\"?><a>".to_vec();
        bytes.extend_from_slice(&[0xf0, 0xd2, 0xc9, 0xd7, 0xc5, 0xd4]);
        bytes.extend_from_slice(b"</a>");
        let document = parse(&bytes, &Limits::default())?;
        assert_eq!(
            document.root.children,
            [Node::Text(
                "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}".to_owned()
            )]
        );
        Ok(())
    }

    #[test]
    fn utf_16_and_a_byte_order_mark_are_recognised() -> Result<(), Error> {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-16\"?><a>\u{43f}</a>";
        let little: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain(xml.encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        let big: Vec<u8> = [0xfe, 0xff]
            .into_iter()
            .chain(xml.encode_utf16().flat_map(u16::to_be_bytes))
            .collect();
        let bare: Vec<u8> = xml.encode_utf16().flat_map(u16::to_le_bytes).collect();
        for bytes in [little, big, bare] {
            let document = parse(&bytes, &Limits::default())?;
            assert_eq!(document.root.children, [Node::Text("\u{43f}".to_owned())]);
        }
        let mut utf8 = vec![0xef, 0xbb, 0xbf];
        utf8.extend_from_slice("<a>\u{43f}</a>".as_bytes());
        let document = parse(&utf8, &Limits::default())?;
        assert_eq!(document.root.children, [Node::Text("\u{43f}".to_owned())]);
        Ok(())
    }

    #[test]
    fn bytes_that_do_not_fit_their_encoding_are_refused() {
        let limits = Limits::default();
        assert!(matches!(
            parse(b"<a>\xff</a>", &limits),
            Err(Error::Source { .. })
        ));
        assert!(matches!(
            parse(b"\xff\xfe<\x00a", &limits),
            Err(Error::Source { .. })
        ));
        assert!(matches!(
            parse(
                b"<?xml version=\"1.0\" encoding=\"utf-8\"?><a>\xff</a>",
                &limits
            ),
            Err(Error::Source { .. })
        ));
    }

    #[test]
    fn a_declared_encoding_that_is_unknown_means_utf_8() -> Result<(), Error> {
        let document = read("<?xml version=\"1.0\" encoding=\"nope\"?><a>x</a>")?;
        assert_eq!(document.root.children, [Node::Text("x".to_owned())]);
        Ok(())
    }
}
