use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use ox_lang::Isolate;
use ox_lang::check::{Occurrence, Severity};
use ox_lang::describe::Kind;
use ox_lang::forms::Forms;
use ox_lang::ide::{self, Source};
use ox_lang::names;
use ox_lang::span::Span;
use oxmacs_files::units::Encoding;
use oxmacs_files::uri;
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
    pub plain: fn() -> Isolate,
    pub editor: fn() -> Isolate,
    pub roots: fn(&Path) -> Vec<PathBuf>,
}

impl Options {
    pub fn plain() -> Self {
        Self {
            flavor: |_| Flavor::Plain,
            plain: Isolate::new,
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
    forms: Forms,
    module: Option<String>,
    occurrences: Vec<Occurrence>,
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

fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect()
}

fn position(text: &str, byte: usize) -> Value {
    let byte = byte.min(text.len());
    let starts = line_starts(text);
    let line = starts.partition_point(|start| *start <= byte) - 1;
    let before = &text[starts[line]..byte];
    let character = Encoding::Utf16.to_units(before, before.chars().count());
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
    let units = usize::try_from(at["character"].as_u64().unwrap_or(0)).unwrap_or(0);
    let line = text[start..].split('\n').next().unwrap_or_default();
    let column = Encoding::Utf16.to_column(line, units);
    start + line.chars().take(column).map(char::len_utf8).sum::<usize>()
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
        let (plain, editor) = (self.options.plain, self.options.editor);
        let slot = match flavor {
            Flavor::Plain => &mut self.plain,
            Flavor::Editor => &mut self.editor,
        };
        let isolate = slot.get_or_insert_with(|| match flavor {
            Flavor::Plain => plain(),
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
        let isolate = self.isolate(path.as_deref());
        let (found, occurrences) = isolate.check_with_bindings(&name, &text);
        let forms = isolate.checked_forms(&name, &text);
        if let Some(document) = self.documents.get_mut(uri) {
            document.forms = forms;
            document.occurrences = occurrences;
        }
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
                let path = uri::to_path(&uri);
                let name = path
                    .as_ref()
                    .map_or_else(|| uri.clone(), |path| path.display().to_string());
                let text = params["textDocument"]["text"].as_str()?.to_owned();
                let forms = Forms::built_in();
                let module = path
                    .as_deref()
                    .and_then(|path| ide::module_name(&(self.options.roots)(path), path));
                self.documents.insert(
                    uri.clone(),
                    Document {
                        text,
                        path,
                        name,
                        forms,
                        module,
                        occurrences: Vec::new(),
                    },
                );
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

    fn with_file<T>(
        &mut self,
        uri: &str,
        use_it: impl FnOnce(&Isolate, &str, ide::File<'_>) -> T,
    ) -> Option<T> {
        let path = self.documents.get(uri)?.path.clone();
        self.isolate(path.as_deref());
        let document = self.documents.get(uri)?;
        let isolate = match path.as_deref().map_or(Flavor::Plain, self.options.flavor) {
            Flavor::Plain => self.plain.as_ref()?,
            Flavor::Editor => self.editor.as_ref()?,
        };
        let file = ide::File {
            name: &document.name,
            text: &document.text,
            forms: &document.forms,
        };
        Some(use_it(isolate, &document.module(), file))
    }
    fn lookup(&mut self, uri: &str, name: &str) -> Option<(String, Option<String>, Kind)> {
        let described = self.with_file(uri, |isolate, module, file| {
            ide::describe(isolate, module, file, name)
        })??;
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
        let form = text[..at].rfind("\n(").map_or(0, |found| found + 1);
        let prefix = names::typed_name(&text[form..at]).to_owned();
        let found = self.with_file(&uri, |isolate, module, file| {
            ide::completions(isolate, module, file, &prefix)
        })?;
        Some(Value::Array(
            found
                .into_iter()
                .map(|candidate| {
                    json!({
                        "label": candidate.name,
                        "kind": completion_kind(candidate.kind),
                        "detail": candidate.signature,
                        "documentation": candidate.doc,
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

    fn sources(&self) -> (Vec<&String>, Vec<Source<'_>>) {
        let mut uris: Vec<&String> = self.documents.keys().collect();
        uris.sort();
        let sources = uris
            .iter()
            .map(|uri| {
                let document = &self.documents[*uri];
                Source {
                    module: document.module.as_deref(),
                    occurrences: &document.occurrences,
                }
            })
            .collect();
        (uris, sources)
    }

    fn same_binding(&self, uri: &str, at: usize) -> Vec<(String, Occurrence)> {
        let (uris, sources) = self.sources();
        let Some(index) = uris.iter().position(|known| *known == uri) else {
            return Vec::new();
        };
        ide::same_binding(&sources, index, at)
            .into_iter()
            .map(|(found, occurrence)| (uris[found].clone(), occurrence))
            .collect()
    }

    fn located(&self, uri: &str, span: Span) -> Value {
        let text = self
            .documents
            .get(uri)
            .map_or("", |document| document.text.as_str());
        json!({"uri": uri, "range": range(text, span)})
    }

    fn definition(&mut self, params: &Value) -> Option<Value> {
        let (uri, text, at) = self.at(params)?;
        if let Some((found, occurrence)) = self
            .same_binding(&uri, at)
            .into_iter()
            .find(|(_, occurrence)| occurrence.defines)
        {
            return Some(self.located(&found, occurrence.span));
        }
        let name = Self::name_at(&text, at)?;
        let document = self.documents.get(&uri)?;
        let (module, path) = (document.module(), document.path.clone());
        let place = self
            .isolate(path.as_deref())
            .describe(&module, &name)?
            .place?;
        let file = Path::new(place.file());
        file.is_file().then(
            || json!({"uri": uri::from_path(file), "range": range(&place.source.text, place.span)}),
        )
    }

    fn references(&self, params: &Value) -> Option<Value> {
        let (uri, _, at) = self.at(params)?;
        Some(Value::Array(
            self.same_binding(&uri, at)
                .into_iter()
                .map(|(found, occurrence)| self.located(&found, occurrence.span))
                .collect(),
        ))
    }

    fn rename(&self, params: &Value) -> Option<Value> {
        let (uri, _, at) = self.at(params)?;
        let replacement = params["newName"].as_str()?;
        let mut changes: HashMap<String, Vec<Value>> = HashMap::new();
        for (found, occurrence) in self.same_binding(&uri, at) {
            let text = &self.documents.get(&found)?.text;
            changes.entry(found).or_default().push(
                json!({"range": range(text, ide::name_span(&occurrence)), "newText": replacement}),
            );
        }
        Some(json!({"changes": changes}))
    }
    fn format(&self, params: &Value) -> Option<Value> {
        let uri = params["textDocument"]["uri"].as_str()?;
        let document = self.documents.get(uri)?;
        let text = &document.text;
        let formatted = ox_lang::format::format_with(text, &document.forms).ok()?;
        if formatted == *text {
            return Some(json!([]));
        }
        let whole = Span::new(0, text.len());
        Some(json!([{"range": range(text, whole), "newText": formatted}]))
    }

    fn symbols(&self, params: &Value) -> Option<Value> {
        let uri = params["textDocument"]["uri"].as_str()?;
        let document = self.documents.get(uri)?;
        let text = &document.text;
        Some(Value::Array(
            names::definitions_with(text, &document.forms)
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
