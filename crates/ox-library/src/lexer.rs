use std::collections::HashSet;
use std::sync::Arc;

use ox_lang::Isolate;
use ox_lang::build::{make_map, make_vector};
use ox_lang::error::Error;
use ox_lang::heap::Heap;
use ox_lang::native::{expected, field};
use ox_lang::value::Value;
use regex::{Captures, Regex};

const MODULE: &str = "ox/lexer";

#[derive(Debug)]
struct Block {
    opener: String,
    close: Regex,
    face: Value,
}

#[derive(Debug)]
struct Spec {
    token: Regex,
    blocks: Vec<Block>,
    keywords: HashSet<String>,
    constants: HashSet<String>,
    types: HashSet<String>,
}

fn regex(heap: &Heap, name: &str, value: Value) -> Result<Regex, Error> {
    if let Some(found) = heap.foreign_of::<Regex>(value) {
        return Ok(found.clone());
    }
    if let Some(found) = heap.shared_of::<Regex>(value) {
        return Ok((*found).clone());
    }
    let pattern = heap
        .text(value)
        .ok_or_else(|| expected(heap, name, "a regex", value))?;
    Regex::new(pattern).map_err(|error| Error::new(format!("{name}: {error}")))
}

fn words(heap: &Heap, spec: Value, key: &str) -> HashSet<String> {
    field(heap, spec, key)
        .and_then(|set| heap.elements(set))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|word| heap.text(word).map(str::to_owned))
        .collect()
}

fn compiled(heap: &Heap, value: Value) -> Result<Arc<Spec>, Error> {
    const NAME: &str = "lexer/spec";
    if let Some(spec) = heap.shared_of::<Spec>(value) {
        return Ok(spec);
    }
    if heap.map_of(value).is_none() {
        return Err(expected(
            heap,
            NAME,
            "a map of :token, :blocks, :keywords, :constants and :types",
            value,
        ));
    }
    let token = field(heap, value, "token")
        .ok_or_else(|| Error::new(format!("{NAME} needs a :token regex")))?;
    let mut blocks = Vec::new();
    for (opener, block) in field(heap, value, "blocks")
        .and_then(|blocks| heap.entries(blocks))
        .unwrap_or_default()
    {
        let opener = heap
            .text(opener)
            .ok_or_else(|| expected(heap, NAME, "block openers as strings", opener))?;
        let close = field(heap, block, "close")
            .ok_or_else(|| Error::new(format!("{NAME}'s block {opener} needs a :close regex")))?;
        blocks.push(Block {
            opener: opener.to_owned(),
            close: regex(heap, NAME, close)?,
            face: field(heap, block, "face").unwrap_or(Value::keyword("string")),
        });
    }
    Ok(Arc::new(Spec {
        token: regex(heap, NAME, token)?,
        blocks,
        keywords: words(heap, value, "keywords"),
        constants: words(heap, value, "constants"),
        types: words(heap, value, "types"),
    }))
}

type Span = (usize, usize, Value);

impl Spec {
    fn block(&self, opener: &str) -> Option<&Block> {
        self.blocks.iter().find(|block| block.opener == opener)
    }

    fn word_face(&self, word: &str) -> Option<Value> {
        let face = if self.keywords.contains(word) {
            "keyword"
        } else if self.constants.contains(word) {
            "constant"
        } else if self.types.contains(word) {
            "type"
        } else {
            return None;
        };
        Some(Value::keyword(face))
    }

    fn span_of(&self, found: &Captures<'_>, offset: usize) -> Option<Span> {
        let whole = found.get(0)?;
        let (start, end) = (offset + whole.start(), offset + whole.end());
        let group = |name: &str| found.name(name);
        let face = |name: &str| Value::keyword(name);
        if group("comment").is_some() {
            Some((start, end, face("comment")))
        } else if let Some(key) = group("key") {
            Some((offset + key.start(), offset + key.end(), face("attribute")))
        } else if group("string").is_some() {
            Some((start, end, face("string")))
        } else if group("attribute").is_some() {
            Some((start, end, face("attribute")))
        } else if group("macro").is_some() {
            Some((start, end, face("macro")))
        } else if group("number").is_some() {
            Some((start, end, face("constant")))
        } else if let Some(call) = group("call") {
            let shown = self.word_face(call.as_str()).unwrap_or(face("function"));
            Some((offset + call.start(), offset + call.end(), shown))
        } else {
            let word = group("word")?;
            Some((start, end, self.word_face(word.as_str())?))
        }
    }

    fn scan(&self, text: &str, offset: usize, spans: &mut Vec<Span>) -> Option<String> {
        let rest = &text[offset..];
        for found in self.token.captures_iter(rest) {
            let Some(opener) = found.name("block") else {
                spans.extend(self.span_of(&found, offset));
                continue;
            };
            let Some(block) = self.block(opener.as_str()) else {
                continue;
            };
            let whole = found.get(0).map_or(opener.range(), |whole| whole.range());
            let after = offset + whole.end;
            if let Some(closed) = block.close.find(&text[after..]) {
                let end = after + closed.end();
                spans.push((offset + whole.start, end, block.face));
                return self.scan(text, end, spans);
            }
            spans.push((offset + whole.start, text.len(), block.face));
            return Some(opener.as_str().to_owned());
        }
        None
    }

    fn lex(&self, text: &str, state: Option<&str>) -> (Vec<Span>, Option<String>) {
        let mut spans = Vec::new();
        let Some(block) = state.and_then(|opener| self.block(opener)) else {
            let state = self.scan(text, 0, &mut spans);
            return (spans, state);
        };
        if let Some(closed) = block.close.find(text) {
            spans.push((0, closed.end(), block.face));
            let state = self.scan(text, closed.end(), &mut spans);
            return (spans, state);
        }
        if !text.is_empty() {
            spans.push((0, text.len(), block.face));
        }
        (spans, Some(block.opener.clone()))
    }
}

fn chars(text: &str, byte: usize) -> Value {
    Value::Int(i64::try_from(text[..byte].chars().count()).unwrap_or(i64::MAX))
}

fn lex(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let spec = compiled(heap, arguments[0])?;
    let text = heap
        .text(arguments[1])
        .ok_or_else(|| expected(heap, "lexer/lex", "a line of text", arguments[1]))?
        .to_owned();
    let state = heap.text(arguments[2]).map(str::to_owned);
    let (spans, state) = spec.lex(&text, state.as_deref());
    let mut shown = Vec::new();
    for (start, end, face) in spans {
        shown.push(make_vector(
            heap,
            &[chars(&text, start), chars(&text, end), face],
        )?);
    }
    let spans = make_vector(heap, &shown)?;
    let state = match state {
        Some(opener) => heap.string(&opener)?,
        None => Value::Nil,
    };
    make_map(
        heap,
        &[
            Value::keyword("spans"),
            spans,
            Value::keyword("state"),
            state,
        ],
    )
}

pub(crate) fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "spec")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let spec = compiled(heap, arguments[0])?;
            heap.shared("lexer", spec).map_err(Error::from)
        });
    isolate
        .native(MODULE, "lex")
        .arity(3, Some(3))
        .function(lex);
}
