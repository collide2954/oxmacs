use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use ox::Isolate;
use ox::check::Severity;
use ox::describe::Kind;
use ox::names::{self, Definition};
use ox::span::Span;
use serde_json::{Value, json};

type Failure = Box<dyn Error + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    Plain,
    Editor,
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub flavor: fn(&Path) -> Flavor,
    pub editor: fn() -> Isolate,
    pub roots: fn(&Path) -> Vec<PathBuf>,
}

impl Options {
    pub fn plain() -> Self {
        Self {
            flavor: |_| Flavor::Plain,
            editor: Isolate::new,
            roots: |path| path.parent().map(Path::to_path_buf).into_iter().collect(),
        }
    }
}

#[derive(Debug)]
struct Document {
    text: String,
    path: Option<PathBuf>,
    name: String,
}

impl Document {
    fn module(&self) -> String {
        format!("#check:{}", self.name)
    }
}

struct Server<'a> {
    options: &'a Options,
    documents: HashMap<String, Document>,
    plain: Option<Isolate>,
    editor: Option<Isolate>,
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = (bytes[at] == b'%' && at + 2 < bytes.len())
            .then(|| u8::from_str_radix(&text[at + 1..at + 3], 16).ok())
            .flatten();
        if let Some(byte) = escaped {
            out.push(byte);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = decode(uri.strip_prefix("file://")?);
    let drive = rest.len() > 2 && rest.as_bytes()[2] == b':' && rest.starts_with('/');
    Some(PathBuf::from(if drive { &rest[1..] } else { &rest[..] }))
}

fn uri_of(path: &Path) -> String {
    let text = path.display().to_string().replace('\\', "/");
    let encoded: String = text
        .chars()
        .map(|character| match character {
            ' ' => "%20".to_owned(),
            '#' => "%23".to_owned(),
            '%' => "%25".to_owned(),
            other => other.to_string(),
        })
        .collect();
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect()
}

fn position(text: &str, byte: usize) -> Value {
    let byte = byte.min(text.len());
    let starts = line_starts(text);
    let line = starts.partition_point(|start| *start <= byte) - 1;
    let character: usize = text[starts[line]..byte].chars().map(char::len_utf16).sum();
    json!({"line": line, "character": character})
}

fn range(text: &str, span: Span) -> Value {
    json!({
        "start": position(text, span.start as usize),
        "end": position(text, span.end as usize)
    })
}

fn offset(text: &str, at: &Value) -> usize {
    let starts = line_starts(text);
    let line = usize::try_from(at["line"].as_u64().unwrap_or(0)).unwrap_or(usize::MAX);
    let Some(start) = starts.get(line).copied() else {
        return text.len();
    };
    let mut wanted = usize::try_from(at["character"].as_u64().unwrap_or(0)).unwrap_or(0);
    let mut byte = start;
    for character in text[start..].chars() {
        if wanted == 0 || character == '\n' {
            break;
        }
        wanted = wanted.saturating_sub(character.len_utf16());
        byte += character.len_utf8();
    }
    byte
}

fn completion_kind(kind: Kind) -> u8 {
    match kind {
        Kind::Function | Kind::Native => 3,
        Kind::Macro | Kind::Special => 14,
        Kind::Record => 7,
        Kind::Value => 6,
    }
}

fn symbol_kind(kind: Kind) -> u8 {
    match kind {
        Kind::Record => 23,
        Kind::Value => 13,
        _ => 12,
    }
}

fn markdown(signature: &str, doc: Option<&str>) -> String {
    match doc {
        Some(doc) => format!("```ox\n{signature}\n```\n\n{doc}"),
        None => format!("```ox\n{signature}\n```"),
    }
}

fn typed(before: &str) -> &str {
    let start = before
        .char_indices()
        .rev()
        .take_while(|(_, character)| {
            !character.is_whitespace() && !"()[]{}\";'`~@#".contains(*character)
        })
        .last()
        .map_or(before.len(), |(at, _)| at);
    &before[start..]
}

impl<'a> Server<'a> {
    fn new(options: &'a Options) -> Self {
        Self {
            options,
            documents: HashMap::new(),
            plain: None,
            editor: None,
        }
    }

    fn isolate(&mut self, path: Option<&Path>) -> &mut Isolate {
        let flavor = path.map_or(Flavor::Plain, self.options.flavor);
        let roots = path.map(self.options.roots).unwrap_or_default();
        let editor = self.options.editor;
        let slot = match flavor {
            Flavor::Plain => &mut self.plain,
            Flavor::Editor => &mut self.editor,
        };
        let isolate = slot.get_or_insert_with(|| match flavor {
            Flavor::Plain => Isolate::new(),
            Flavor::Editor => editor(),
        });
        for root in roots {
            isolate.add_root(root);
        }
        isolate
    }

    fn diagnostics(&mut self, uri: &str) -> Option<Notification> {
        let document = self.documents.get(uri)?;
        let (name, text, path) = (
            document.name.clone(),
            document.text.clone(),
            document.path.clone(),
        );
        let found = self.isolate(path.as_deref()).check(&name, &text);
        let shown: Vec<Value> = found
            .iter()
            .map(|diagnostic| {
                json!({
                    "range": range(&text, diagnostic.span),
                    "severity": if diagnostic.severity == Severity::Error { 1 } else { 2 },
                    "code": diagnostic.code,
                    "source": "ox",
                    "message": diagnostic.message,
                })
            })
            .collect();
        Some(Notification::new(
            "textDocument/publishDiagnostics".to_owned(),
            json!({"uri": uri, "diagnostics": shown}),
        ))
    }

    fn notify(&mut self, notification: &Notification) -> Option<Notification> {
        let params = &notification.params;
        let uri = params["textDocument"]["uri"].as_str()?.to_owned();
        match notification.method.as_str() {
            "textDocument/didOpen" => {
                let path = path_of(&uri);
                let name = path
                    .as_ref()
                    .map_or_else(|| uri.clone(), |path| path.display().to_string());
                let text = params["textDocument"]["text"].as_str()?.to_owned();
                self.documents
                    .insert(uri.clone(), Document { text, path, name });
                self.diagnostics(&uri)
            }
            "textDocument/didChange" => {
                let text = params["contentChanges"].as_array()?.last()?["text"]
                    .as_str()?
                    .to_owned();
                self.documents.get_mut(&uri)?.text = text;
                self.diagnostics(&uri)
            }
            "textDocument/didClose" => {
                self.documents.remove(&uri);
                Some(Notification::new(
                    "textDocument/publishDiagnostics".to_owned(),
                    json!({"uri": uri, "diagnostics": []}),
                ))
            }
            _ => None,
        }
    }

    fn lookup(&mut self, uri: &str, name: &str) -> Option<(String, Option<String>, Kind)> {
        let document = self.documents.get(uri)?;
        if let Some(found) = names::definitions(&document.text)
            .into_iter()
            .find(|definition| definition.name == name)
        {
            return Some((found.signature, found.doc, found.kind));
        }
        let (module, path) = (document.module(), document.path.clone());
        let described = self.isolate(path.as_deref()).describe(&module, name)?;
        Some((described.signature, described.doc, described.kind))
    }

    fn at(&self, params: &Value) -> Option<(String, String, usize)> {
        let uri = params["textDocument"]["uri"].as_str()?.to_owned();
        let document = self.documents.get(&uri)?;
        let at = offset(&document.text, &params["position"]);
        Some((uri, document.text.clone(), at))
    }

    fn name_at(text: &str, at: usize) -> Option<String> {
        let start = text[..at].rfind('\n').map_or(0, |found| found + 1);
        let end = text[at..].find('\n').map_or(text.len(), |found| at + found);
        let column = text[start..at].chars().count();
        names::symbol_at(&text[start..end], column)
    }

    fn complete(&mut self, params: &Value) -> Option<Value> {
        let (uri, text, at) = self.at(params)?;
        let prefix = typed(&text[..at]).to_owned();
        let document = self.documents.get(&uri)?;
        let (module, path) = (document.module(), document.path.clone());
        let mut found: Vec<(String, Option<String>, Kind, Option<String>)> =
            names::definitions(&text)
                .into_iter()
                .filter(|definition| definition.name.starts_with(&prefix))
                .map(|definition| {
                    (
                        definition.name,
                        Some(definition.signature),
                        definition.kind,
                        definition.doc,
                    )
                })
                .collect();
        let isolate = self.isolate(path.as_deref());
        for name in isolate.completions(&module, &prefix) {
            if found.iter().any(|(known, ..)| *known == name) {
                continue;
            }
            let described = isolate.describe(&module, &name);
            found.push(match described {
                Some(described) => (
                    name,
                    Some(described.signature),
                    described.kind,
                    described.doc,
                ),
                None => (name, None, Kind::Value, None),
            });
        }
        Some(Value::Array(
            found
                .into_iter()
                .map(|(label, signature, kind, doc)| {
                    json!({
                        "label": label,
                        "kind": completion_kind(kind),
                        "detail": signature,
                        "documentation": doc,
                    })
                })
                .collect(),
        ))
    }

    fn hover(&mut self, params: &Value) -> Option<Value> {
        let (uri, text, at) = self.at(params)?;
        let name = Self::name_at(&text, at)?;
        let (signature, doc, _) = self.lookup(&uri, &name)?;
        Some(
            json!({"contents": {"kind": "markdown", "value": markdown(&signature, doc.as_deref())}}),
        )
    }

    fn signature_help(&mut self, params: &Value) -> Option<Value> {
        let (uri, text, at) = self.at(params)?;
        let head = names::call_head(&text[..at])?;
        let (signature, doc, kind) = self.lookup(&uri, &head)?;
        (kind != Kind::Value).then(|| {
            json!({
                "signatures": [{"label": signature, "documentation": doc}],
                "activeSignature": 0,
            })
        })
    }

    fn definition(&mut self, params: &Value) -> Option<Value> {
        let (uri, text, at) = self.at(params)?;
        let name = Self::name_at(&text, at)?;
        let wanted = names::short(&name);
        let mut ordered: Vec<&String> = vec![&uri];
        ordered.extend(self.documents.keys().filter(|other| **other != uri));
        for candidate in ordered {
            let document = &self.documents[candidate];
            if let Some(found) = names::definitions(&document.text)
                .into_iter()
                .find(|definition: &Definition| definition.name == wanted)
            {
                return Some(json!({"uri": candidate, "range": range(&document.text, found.span)}));
            }
        }
        let document = self.documents.get(&uri)?;
        let (module, path) = (document.module(), document.path.clone());
        let place = self
            .isolate(path.as_deref())
            .describe(&module, &name)?
            .place?;
        let file = Path::new(place.file());
        file.is_file()
            .then(|| json!({"uri": uri_of(file), "range": range(&place.source.text, place.span)}))
    }

    fn occurrences(&self, name: &str) -> Vec<(String, Span, String)> {
        let mut uris: Vec<&String> = self.documents.keys().collect();
        uris.sort();
        uris.into_iter()
            .flat_map(|uri| {
                let text = &self.documents[uri].text;
                names::occurrences(text, name)
                    .into_iter()
                    .map(move |span| (uri.clone(), span, text.clone()))
            })
            .collect()
    }

    fn references(&self, params: &Value) -> Option<Value> {
        let (_, text, at) = self.at(params)?;
        let name = Self::name_at(&text, at)?;
        Some(Value::Array(
            self.occurrences(&name)
                .into_iter()
                .map(|(uri, span, text)| json!({"uri": uri, "range": range(&text, span)}))
                .collect(),
        ))
    }

    fn rename(&self, params: &Value) -> Option<Value> {
        let (_, text, at) = self.at(params)?;
        let name = Self::name_at(&text, at)?;
        let wanted = names::short(&name).to_owned();
        let replacement = params["newName"].as_str()?;
        let mut changes: HashMap<String, Vec<Value>> = HashMap::new();
        for (uri, span, text) in self.occurrences(&name) {
            let end = span.end as usize;
            let edited = Span::new(end.saturating_sub(wanted.len()), end);
            changes
                .entry(uri)
                .or_default()
                .push(json!({"range": range(&text, edited), "newText": replacement}));
        }
        Some(json!({"changes": changes}))
    }

    fn format(&self, params: &Value) -> Option<Value> {
        let uri = params["textDocument"]["uri"].as_str()?;
        let text = &self.documents.get(uri)?.text;
        let formatted = ox::format::format(text).ok()?;
        if formatted == *text {
            return Some(json!([]));
        }
        let whole = Span::new(0, text.len());
        Some(json!([{"range": range(text, whole), "newText": formatted}]))
    }

    fn symbols(&self, params: &Value) -> Option<Value> {
        let uri = params["textDocument"]["uri"].as_str()?;
        let text = &self.documents.get(uri)?.text;
        Some(Value::Array(
            names::definitions(text)
                .into_iter()
                .map(|definition| {
                    json!({
                        "name": definition.name,
                        "detail": definition.signature,
                        "kind": symbol_kind(definition.kind),
                        "range": range(text, definition.span),
                        "selectionRange": range(text, definition.name_span),
                    })
                })
                .collect(),
        ))
    }

    fn answer(&mut self, request: &Request) -> Response {
        let params = &request.params;
        let result = match request.method.as_str() {
            "textDocument/completion" => self.complete(params),
            "textDocument/hover" => self.hover(params),
            "textDocument/signatureHelp" => self.signature_help(params),
            "textDocument/definition" => self.definition(params),
            "textDocument/references" => self.references(params),
            "textDocument/rename" => self.rename(params),
            "textDocument/formatting" => self.format(params),
            "textDocument/documentSymbol" => self.symbols(params),
            _ => {
                return Response::new_err(
                    request.id.clone(),
                    ErrorCode::MethodNotFound as i32,
                    format!("ox lsp doesn't handle {}", request.method),
                );
            }
        };
        Response::new_ok(request.id.clone(), result.unwrap_or(Value::Null))
    }
}

fn capabilities() -> Value {
    json!({
        "capabilities": {
            "textDocumentSync": 1,
            "completionProvider": {"triggerCharacters": ["/", "("]},
            "hoverProvider": true,
            "signatureHelpProvider": {"triggerCharacters": ["(", " "]},
            "definitionProvider": true,
            "referencesProvider": true,
            "renameProvider": true,
            "documentFormattingProvider": true,
            "documentSymbolProvider": true,
        },
        "serverInfo": {"name": "ox lsp", "version": env!("CARGO_PKG_VERSION")},
    })
}

pub fn run(connection: &Connection, options: &Options) -> Result<(), Failure> {
    connection.initialize(capabilities())?;
    let mut server = Server::new(options);
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    return Ok(());
                }
                let response = server.answer(&request);
                connection.sender.send(response.into())?;
            }
            Message::Notification(notification) => {
                if let Some(out) = server.notify(&notification) {
                    connection.sender.send(out.into())?;
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

pub fn serve(options: &Options) -> Result<(), Failure> {
    let (connection, threads) = Connection::stdio();
    run(&connection, options)?;
    drop(connection);
    threads.join()?;
    Ok(())
}
