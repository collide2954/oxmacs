use roxmltree::{Document, Node, ParsingOptions};

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::{Value, print};
use crate::vm::build::{full, make_map, make_vector};

const MODULE: &str = "ox/xml";
const DEEPEST: usize = 1000;

const ENTITIES: &[(&str, char)] = &[
    ("nbsp", '\u{a0}'),
    ("ensp", '\u{2002}'),
    ("emsp", '\u{2003}'),
    ("thinsp", '\u{2009}'),
    ("zwnj", '\u{200c}'),
    ("zwj", '\u{200d}'),
    ("shy", '\u{ad}'),
    ("ndash", '–'),
    ("mdash", '—'),
    ("hellip", '…'),
    ("lsquo", '‘'),
    ("rsquo", '’'),
    ("sbquo", '‚'),
    ("ldquo", '“'),
    ("rdquo", '”'),
    ("bdquo", '„'),
    ("laquo", '«'),
    ("raquo", '»'),
    ("lsaquo", '‹'),
    ("rsaquo", '›'),
    ("bull", '•'),
    ("middot", '·'),
    ("dagger", '†'),
    ("Dagger", '‡'),
    ("para", '¶'),
    ("sect", '§'),
    ("copy", '©'),
    ("reg", '®'),
    ("trade", '™'),
    ("deg", '°'),
    ("plusmn", '±'),
    ("times", '×'),
    ("divide", '÷'),
    ("frac12", '½'),
    ("frac14", '¼'),
    ("frac34", '¾'),
    ("sup1", '¹'),
    ("sup2", '²'),
    ("sup3", '³'),
    ("cent", '¢'),
    ("pound", '£'),
    ("euro", '€'),
    ("yen", '¥'),
    ("iexcl", '¡'),
    ("iquest", '¿'),
    ("prime", '′'),
    ("Prime", '″'),
    ("larr", '←'),
    ("rarr", '→'),
    ("uarr", '↑'),
    ("darr", '↓'),
    ("hearts", '♥'),
    ("Agrave", 'À'),
    ("Aacute", 'Á'),
    ("Acirc", 'Â'),
    ("Atilde", 'Ã'),
    ("Auml", 'Ä'),
    ("Aring", 'Å'),
    ("AElig", 'Æ'),
    ("Ccedil", 'Ç'),
    ("Egrave", 'È'),
    ("Eacute", 'É'),
    ("Ecirc", 'Ê'),
    ("Euml", 'Ë'),
    ("Igrave", 'Ì'),
    ("Iacute", 'Í'),
    ("Icirc", 'Î'),
    ("Iuml", 'Ï'),
    ("Ntilde", 'Ñ'),
    ("Ograve", 'Ò'),
    ("Oacute", 'Ó'),
    ("Ocirc", 'Ô'),
    ("Otilde", 'Õ'),
    ("Ouml", 'Ö'),
    ("Oslash", 'Ø'),
    ("Ugrave", 'Ù'),
    ("Uacute", 'Ú'),
    ("Ucirc", 'Û'),
    ("Uuml", 'Ü'),
    ("Yacute", 'Ý'),
    ("szlig", 'ß'),
    ("agrave", 'à'),
    ("aacute", 'á'),
    ("acirc", 'â'),
    ("atilde", 'ã'),
    ("auml", 'ä'),
    ("aring", 'å'),
    ("aelig", 'æ'),
    ("ccedil", 'ç'),
    ("egrave", 'è'),
    ("eacute", 'é'),
    ("ecirc", 'ê'),
    ("euml", 'ë'),
    ("igrave", 'ì'),
    ("iacute", 'í'),
    ("icirc", 'î'),
    ("iuml", 'ï'),
    ("ntilde", 'ñ'),
    ("ograve", 'ò'),
    ("oacute", 'ó'),
    ("ocirc", 'ô'),
    ("otilde", 'õ'),
    ("ouml", 'ö'),
    ("oslash", 'ø'),
    ("ugrave", 'ù'),
    ("uacute", 'ú'),
    ("ucirc", 'û'),
    ("uuml", 'ü'),
    ("yacute", 'ý'),
    ("yuml", 'ÿ'),
    ("oelig", 'œ'),
    ("OElig", 'Œ'),
    ("scaron", 'š'),
    ("Scaron", 'Š'),
    ("alpha", 'α'),
    ("beta", 'β'),
    ("gamma", 'γ'),
    ("delta", 'δ'),
    ("pi", 'π'),
    ("sigma", 'σ'),
    ("omega", 'ω'),
];

const XML_ENTITIES: &[&str] = &["amp", "lt", "gt", "quot", "apos"];

fn html_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let name_end = after
            .find(|character: char| !character.is_ascii_alphanumeric())
            .unwrap_or(after.len());
        let name = &after[..name_end];
        let closed = after[name_end..].starts_with(';');
        if !closed || name.is_empty() || XML_ENTITIES.contains(&name) {
            out.push('&');
            rest = after;
            continue;
        }
        let character = ENTITIES
            .iter()
            .find(|(known, _)| *known == name)
            .map_or('\u{fffd}', |(_, character)| *character);
        out.push(character);
        rest = &after[name_end + 1..];
    }
    out.push_str(rest);
    out
}

fn element(heap: &mut Heap, node: Node<'_, '_>, depth: usize, html: bool) -> Result<Value, Error> {
    if depth > DEEPEST {
        return Err(Error::new(format!(
            "xml/parse can't read elements nested more than {DEEPEST} deep"
        )));
    }
    let tag = heap.string(node.tag_name().name()).map_err(full)?;
    let mut attrs = Vec::new();
    for attribute in node.attributes() {
        attrs.push(Value::keyword(attribute.name()));
        attrs.push(heap.string(attribute.value()).map_err(full)?);
    }
    let attrs = make_map(heap, &attrs)?;
    let mut children = Vec::new();
    for child in node.children() {
        if child.is_element() {
            children.push(element(heap, child, depth + 1, html)?);
        } else if child.is_text()
            && let Some(text) = child.text()
            && (html || !text.trim().is_empty())
        {
            children.push(heap.string(text).map_err(full)?);
        }
    }
    let children = make_vector(heap, &children)?;
    make_map(
        heap,
        &[
            Value::keyword("tag"),
            tag,
            Value::keyword("attrs"),
            attrs,
            Value::keyword("children"),
            children,
        ],
    )
}

fn parse(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let Some(text) = heap.text(arguments[0]).map(str::to_owned) else {
        return Err(Error::new(format!(
            "xml/parse expects a string, got {}",
            print::brief(heap, arguments[0])
        ))
        .of(ErrorKind::Type));
    };
    let html = match &arguments[1..] {
        [] => false,
        [Value::Keyword(option), on] if option.name() == "html" => on.truthy(),
        _ => return Err(Error::new("xml/parse takes :html")),
    };
    let text = if html { html_entities(&text) } else { text };
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    let document = Document::parse_with_options(&text, options)
        .map_err(|error| Error::new(format!("xml/parse can't read this: {error}")))?;
    element(heap, document.root_element(), 0, html)
}

fn named(heap: &Heap, key: Value, name: &str) -> bool {
    match key {
        Value::Keyword(found) | Value::Symbol(found) => found.name() == name,
        _ => heap.text(key) == Some(name),
    }
}

fn field(heap: &Heap, map: Value, name: &str) -> Option<Value> {
    heap.entries(map)?
        .into_iter()
        .find(|(key, _)| named(heap, *key, name))
        .map(|(_, value)| value)
}

fn is_element(heap: &Heap, value: Value) -> bool {
    field(heap, value, "tag").is_some()
}

fn children(heap: &Heap, element: Value) -> Vec<Value> {
    field(heap, element, "children")
        .and_then(|children| heap.elements(children))
        .unwrap_or_default()
}

fn plain(heap: &Heap, value: Value) -> Option<String> {
    match value {
        Value::Int(_) | Value::Float(_) | Value::Bool(_) => Some(print::display(heap, value)),
        Value::Keyword(name) | Value::Symbol(name) => Some(name.name().to_owned()),
        _ => heap.text(value).map(str::to_owned),
    }
}

fn not_element(heap: &Heap, native: &str, value: Value) -> Error {
    Error::new(format!(
        "{native} expects an element, got {}",
        print::brief(heap, value)
    ))
    .of(ErrorKind::Type)
}

fn along(heap: &Heap, native: &str, arguments: &[Value]) -> Result<Vec<Value>, Error> {
    let start = arguments[0];
    if matches!(start, Value::Nil) {
        return Ok(Vec::new());
    }
    if !is_element(heap, start) {
        return Err(not_element(heap, native, start));
    }
    let Some(path) = heap.text(arguments[1]) else {
        return Err(Error::new(format!(
            "{native} expects a path such as \"channel/item\", got {}",
            print::brief(heap, arguments[1])
        ))
        .of(ErrorKind::Type));
    };
    let mut found = vec![start];
    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        found = found
            .into_iter()
            .flat_map(|element| children(heap, element))
            .filter(|child| {
                field(heap, *child, "tag")
                    .and_then(|tag| plain(heap, tag))
                    .as_deref()
                    == Some(segment)
            })
            .collect();
    }
    Ok(found)
}

fn gathered(heap: &Heap, value: Value, out: &mut String) {
    if let Some(text) = heap.text(value) {
        out.push_str(text);
        return;
    }
    for child in children(heap, value) {
        gathered(heap, child, out);
    }
}

fn text(heap: &mut Heap, value: Value) -> Result<Value, Error> {
    if matches!(value, Value::Nil) {
        return Ok(Value::Nil);
    }
    if heap.text(value).is_none() && !is_element(heap, value) {
        return Err(Error::new(format!(
            "xml/text expects an element or a string, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type));
    }
    let mut out = String::new();
    gathered(heap, value, &mut out);
    heap.string(&out).map_err(full)
}

fn attr(heap: &Heap, element: Value, name: Value) -> Result<Value, Error> {
    if matches!(element, Value::Nil) {
        return Ok(Value::Nil);
    }
    if !is_element(heap, element) {
        return Err(not_element(heap, "xml/attr", element));
    }
    let name = match name {
        Value::Keyword(found) | Value::Symbol(found) => found.name().to_owned(),
        _ => heap.text(name).map(str::to_owned).ok_or_else(|| {
            Error::new(format!(
                "xml/attr expects an attribute name, got {}",
                print::brief(heap, name)
            ))
            .of(ErrorKind::Type)
        })?,
    };
    Ok(field(heap, element, "attrs")
        .and_then(|attrs| field(heap, attrs, &name))
        .unwrap_or(Value::Nil))
}

fn escaped(text: &str, quotes: bool, out: &mut String) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if quotes => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|character| character.is_whitespace() || "<>&\"'/=".contains(character))
}

fn write(heap: &Heap, value: Value, out: &mut String) -> Result<(), Error> {
    if let Some(text) = plain(heap, value) {
        escaped(&text, false, out);
        return Ok(());
    }
    let Some(tag) = field(heap, value, "tag") else {
        return Err(Error::new(format!(
            "xml/write can't write {}",
            print::brief(heap, value)
        )));
    };
    let tag = plain(heap, tag)
        .filter(|tag| valid_name(tag))
        .ok_or_else(|| {
            Error::new(format!(
                "xml/write needs a tag name such as \"item\", got {}",
                print::brief(heap, tag)
            ))
        })?;
    out.push('<');
    out.push_str(&tag);
    let attrs = field(heap, value, "attrs").and_then(|attrs| heap.entries(attrs));
    for (key, item) in attrs.unwrap_or_default() {
        let name = plain(heap, key)
            .filter(|name| valid_name(name))
            .ok_or_else(|| {
                Error::new(format!(
                    "xml/write needs attribute names such as \"href\", got {}",
                    print::brief(heap, key)
                ))
            })?;
        let item = plain(heap, item).ok_or_else(|| {
            Error::new(format!(
                "xml/write needs text or numbers as attribute values, got {}",
                print::brief(heap, item)
            ))
        })?;
        out.push(' ');
        out.push_str(&name);
        out.push_str("=\"");
        escaped(&item, true, out);
        out.push('"');
    }
    let children = children(heap, value);
    if children.is_empty() {
        out.push_str("/>");
        return Ok(());
    }
    out.push('>');
    for child in children {
        write(heap, child, out)?;
    }
    out.push_str("</");
    out.push_str(&tag);
    out.push('>');
    Ok(())
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native_in(MODULE, "parse", 1, Some(3), parse);
    isolate.define_native_in(MODULE, "find", 2, Some(2), |heap, arguments| {
        let found = along(heap, "xml/find", arguments)?;
        Ok(found.first().copied().unwrap_or(Value::Nil))
    });
    isolate.define_native_in(MODULE, "find-all", 2, Some(2), |heap, arguments| {
        let found = along(heap, "xml/find-all", arguments)?;
        make_vector(heap, &found)
    });
    isolate.define_native_in(MODULE, "text", 1, Some(1), |heap, arguments| {
        text(heap, arguments[0])
    });
    isolate.define_native_in(MODULE, "attr", 2, Some(2), |heap, arguments| {
        attr(heap, arguments[0], arguments[1])
    });
    isolate.define_native_in(MODULE, "write", 1, Some(1), |heap, arguments| {
        let mut out = String::new();
        write(heap, arguments[0], &mut out)?;
        heap.string(&out).map_err(full)
    });
}
