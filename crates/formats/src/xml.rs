use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::{BytesStart, Event};

use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
    pub text: Option<String>,
}

impl Element {
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter(move |c| c.name == name)
    }

    pub fn child_mut(&mut self, name: &str) -> Option<&mut Element> {
        self.children.iter_mut().find(|c| c.name == name)
    }

    pub fn set_attr(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        match self.attrs.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = value,
            None => self.attrs.push((key.to_owned(), value)),
        }
    }

    pub fn remove_attr(&mut self, key: &str) {
        self.attrs.retain(|(k, _)| k != key);
    }

    pub fn new(name: &str, attrs: &[(&str, String)]) -> Self {
        Self {
            name: name.to_owned(),
            attrs: attrs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
            ..Self::default()
        }
    }
}

pub fn write_element(element: &Element, out: &mut String) {
    out.push('<');
    out.push_str(&element.name);
    for (k, v) in &element.attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        escape_into(v, out);
        out.push('"');
    }
    out.push('>');
    if let Some(text) = &element.text {
        escape_into(text, out);
    }
    for child in &element.children {
        write_element(child, out);
    }
    out.push_str("</");
    out.push_str(&element.name);
    out.push_str(">\n");
}

fn escape_into(value: &str, out: &mut String) {
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub declaration: Option<String>,
    pub root: Element,
}

pub fn parse(input: &str) -> Result<Document, Error> {
    let mut reader = Reader::from_str(input);
    let mut declaration = None;
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;

    loop {
        let pos = reader.buffer_position();
        let event = reader.read_event().map_err(|source| Error::Xml {
            position: reader.error_position(),
            source,
        })?;
        match event {
            Event::Decl(decl) => {
                declaration = Some((*decl).to_owned());
            }
            Event::Start(start) => stack.push(element_from_start(&start, pos)?),
            Event::Empty(start) => {
                let element = element_from_start(&start, pos)?;
                attach(&mut stack, &mut root, element, pos)?;
            }
            Event::End(_) => {
                let element = stack.pop().ok_or(Error::Malformed {
                    position: pos,
                    message: "unexpected end tag".into(),
                })?;
                attach(&mut stack, &mut root, element, pos)?;
            }
            Event::Text(text) => {
                push_text(&mut stack, &unescape_at(&text, pos)?);
            }
            Event::GeneralRef(reference) => {
                let raw = format!("&{};", &*reference);
                push_text(&mut stack, &unescape_at(&raw, pos)?);
            }
            Event::CData(data) => push_text(&mut stack, &data),
            Event::Eof => break,
            Event::Comment(_) | Event::PI(_) | Event::DocType(_) => {}
        }
    }

    if !stack.is_empty() {
        return Err(Error::Malformed {
            position: reader.buffer_position(),
            message: format!("unclosed element <{}>", stack[stack.len() - 1].name),
        });
    }
    let root = root.ok_or(Error::Malformed {
        position: 0,
        message: "document has no root element".into(),
    })?;
    Ok(Document { declaration, root })
}

fn element_from_start(start: &BytesStart<'_>, pos: u64) -> Result<Element, Error> {
    let name = start.name().as_ref().to_owned();
    let mut attrs = Vec::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|e| Error::Malformed {
            position: pos,
            message: format!("bad attribute in <{name}>: {e}"),
        })?;
        let key = attr.key.as_ref().to_owned();
        let value = unescape_at(&attr.value, pos)?.into_owned();
        attrs.push((key, value));
    }
    Ok(Element {
        name,
        attrs,
        ..Element::default()
    })
}

fn unescape_at(raw: &str, pos: u64) -> Result<std::borrow::Cow<'_, str>, Error> {
    unescape(raw).map_err(|e| Error::Malformed {
        position: pos,
        message: format!("bad escape: {e}"),
    })
}

fn push_text(stack: &mut [Element], text: &str) {
    if let Some(parent) = stack.last_mut() {
        parent.text.get_or_insert_with(String::new).push_str(text);
    }
}

fn attach(
    stack: &mut [Element],
    root: &mut Option<Element>,
    mut element: Element,
    pos: u64,
) -> Result<(), Error> {
    if element.text.as_deref().is_some_and(|t| t.trim().is_empty()) {
        element.text = None;
    }
    if let Some(parent) = stack.last_mut() {
        parent.children.push(element);
        Ok(())
    } else if root.is_none() {
        *root = Some(element);
        Ok(())
    } else {
        Err(Error::Malformed {
            position: pos,
            message: "multiple root elements".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_attributes_in_order_and_unescapes() {
        let doc = parse(
            r#"<?xml version="1.0"?><A z="1" a="&quot;x&quot; &amp; &lt;y>"><B/><C k="v"></C></A>"#,
        )
        .unwrap();
        assert_eq!(doc.declaration.as_deref(), Some(r#"xml version="1.0""#));
        assert_eq!(doc.root.name, "A");
        assert_eq!(doc.root.attrs[0], ("z".into(), "1".into()));
        assert_eq!(doc.root.attr("a"), Some(r#""x" & <y>"#));
        let names: Vec<_> = doc.root.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["B", "C"]);
        assert_eq!(doc.root.child("C").unwrap().attr("k"), Some("v"));
    }

    #[test]
    fn keeps_text_and_drops_whitespace() {
        let doc = parse("<A>\n  <B>hi &amp; bye</B><C>&amp; &amp;</C>\n</A>").unwrap();
        assert_eq!(doc.root.text, None);
        assert_eq!(doc.root.children[0].text.as_deref(), Some("hi & bye"));
        assert_eq!(doc.root.children[1].text.as_deref(), Some("& &"));
    }

    #[test]
    fn a_stored_element_reads_back() {
        let input = "<A k=\"a &amp; &lt;b&gt; &quot;c&quot; 'd'\"><B></B>\n<C x=\"1\"><D></D>\n</C>\n</A>\n";
        let element = parse(input).unwrap().root;
        let mut stored = String::new();
        write_element(&element, &mut stored);
        assert_eq!(parse(&stored).unwrap().root, element);
    }

    #[test]
    fn set_and_remove_attributes_keep_order() {
        let mut e = Element::new("E", &[("A", "1".into()), ("B", "2".into())]);
        e.set_attr("A", "9");
        e.set_attr("C", "3");
        e.remove_attr("B");
        assert_eq!(
            e.attrs,
            [("A".into(), "9".into()), ("C".into(), "3".into())]
        );
    }

    #[test]
    fn rejects_unclosed_and_mismatched() {
        assert!(parse("<A><B></B>").is_err());
        assert!(parse("<A></B>").is_err());
        assert!(parse("").is_err());
    }
}
