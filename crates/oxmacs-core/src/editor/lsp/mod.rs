mod edits;
mod features;
mod glue;
mod protocol;
mod servers;

use std::env;
use std::mem;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

pub use protocol::Encoding;
pub use servers::{SERVERS, ServerSpec};

use crate::external::lsp::{Event, Launch};
use crate::text::BufferId;
use features::{Asked, Assist};

const RETRY: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Starting,
    Ready,
    Stopped(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Pending {
    Initialize,
    Shutdown,
    Feature(Asked),
}

#[derive(Debug, Clone)]
struct Document {
    buffer: BufferId,
    uri: String,
    version: i64,
    revision: u64,
}

#[derive(Debug)]
struct Client {
    id: ServerId,
    spec: ServerSpec,
    root: PathBuf,
    state: State,
    next_request: i64,
    pending: Vec<(i64, Pending)>,
    capabilities: Value,
    encoding: Encoding,
    documents: Vec<Document>,
    progress: Vec<(String, String)>,
    diagnostics: Vec<(String, Vec<Value>)>,
}

#[derive(Debug)]
enum Out {
    Send(ServerId, String),
    Start(ServerId, Launch),
    Stop(ServerId),
    Log(String),
    Tell(String),
}

#[derive(Debug)]
enum Outcome {
    Ready,
    Diagnostics {
        server: ServerId,
        path: PathBuf,
        items: Vec<Value>,
    },
    Answered {
        server: ServerId,
        asked: Asked,
        result: Value,
    },
    Unanswered(Asked),
    EditRequested {
        server: ServerId,
        request: Value,
        params: Value,
    },
}

#[derive(Debug, Clone, Copy)]
struct Visited<'a> {
    buffer: BufferId,
    language: &'a str,
    path: &'a Path,
    revision: u64,
}

#[derive(Debug)]
pub(in crate::editor) struct Lsp {
    specs: Vec<ServerSpec>,
    running: Vec<Client>,
    next: u64,
    told: Vec<&'static str>,
    missing: Vec<(&'static str, Instant)>,
    roots: Vec<(PathBuf, &'static str, PathBuf)>,
    out: Vec<Out>,
    assist: Assist,
    pub(in crate::editor) find: fn(&str) -> Option<PathBuf>,
}

fn on_path(program: &str) -> Option<PathBuf> {
    oxmacs_process::which(program)
}

impl Default for Lsp {
    fn default() -> Self {
        Self {
            specs: Vec::new(),
            running: Vec::new(),
            next: 0,
            told: Vec::new(),
            missing: Vec::new(),
            roots: Vec::new(),
            out: Vec::new(),
            assist: Assist::default(),
            find: on_path,
        }
    }
}

fn root_of(spec: &ServerSpec, file: &Path) -> PathBuf {
    let folder = file.parent().unwrap_or(file);
    let marked = |markers: &[&str]| {
        folder
            .ancestors()
            .find(|ancestor| markers.iter().any(|marker| ancestor.join(marker).exists()))
            .map(Path::to_path_buf)
    };
    marked(spec.roots)
        .or_else(|| marked(&[".git"]))
        .unwrap_or_else(|| folder.to_path_buf())
}

fn number(value: &Value) -> usize {
    value
        .as_u64()
        .and_then(|number| usize::try_from(number).ok())
        .unwrap_or_default()
}

impl Client {
    fn request(&mut self, method: &str, params: &Value, pending: Pending) -> String {
        self.next_request += 1;
        let id = self.next_request;
        self.pending.push((id, pending));
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
    }

    fn label(&self) -> String {
        match &self.state {
            State::Starting => format!("{}…", self.spec.name),
            State::Stopped(_) => format!("{} stopped", self.spec.name),
            State::Ready => match self.progress.last() {
                Some((_, text)) => format!("{} {text}", self.spec.name),
                None => self.spec.name.to_owned(),
            },
        }
    }

    fn document(&self, buffer: BufferId) -> Option<&Document> {
        self.documents
            .iter()
            .find(|document| document.buffer == buffer)
    }

    fn capable(&self, capability: &str) -> bool {
        self.capabilities
            .get(capability)
            .is_some_and(|value| !value.is_null() && *value != Value::Bool(false))
    }
}

fn notification(method: &str, params: &Value) -> String {
    json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string()
}

fn response(id: &Value, result: &Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

fn closing(uri: &str) -> String {
    notification(
        "textDocument/didClose",
        &json!({"textDocument": {"uri": uri}}),
    )
}

fn initialize(root: &Path) -> Value {
    let name = root
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    json!({
        "processId": std::process::id(),
        "clientInfo": {"name": "oxmacs", "version": env!("CARGO_PKG_VERSION")},
        "rootUri": protocol::uri(root),
        "rootPath": root.display().to_string(),
        "workspaceFolders": [{"uri": protocol::uri(root), "name": name}],
        "capabilities": {
            "general": {"positionEncodings": ["utf-32", "utf-16"]},
            "window": {"workDoneProgress": true},
            "workspace": {
                "configuration": true,
                "workspaceFolders": true,
                "applyEdit": true,
                "workspaceEdit": {"documentChanges": true}
            },
            "textDocument": {
                "synchronization": {"didSave": true},
                "publishDiagnostics": {"relatedInformation": false},
                "completion": {
                    "completionItem": {"snippetSupport": false, "documentationFormat": ["plaintext"]},
                    "contextSupport": true
                },
                "hover": {"contentFormat": ["plaintext", "markdown"]},
                "signatureHelp": {
                    "signatureInformation": {
                        "documentationFormat": ["plaintext"],
                        "parameterInformation": {"labelOffsetSupport": true}
                    }
                },
                "definition": {},
                "references": {},
                "rename": {},
                "formatting": {},
                "codeAction": {
                    "codeActionLiteralSupport": {
                        "codeActionKind": {"valueSet": ["", "quickfix", "refactor", "refactor.extract", "refactor.inline", "refactor.rewrite", "source", "source.organizeImports"]}
                    }
                },
                "inlayHint": {}
            }
        }
    })
}

impl Lsp {
    fn add_specs(&mut self, specs: &[ServerSpec]) {
        for spec in specs {
            self.specs.retain(|known| known.name != spec.name);
            self.specs.push(*spec);
        }
    }

    fn spec_for(&self, language: &str) -> Option<ServerSpec> {
        self.specs
            .iter()
            .find(|spec| spec.language_id(language).is_some())
            .copied()
    }

    fn client(&self, id: ServerId) -> Option<&Client> {
        self.running.iter().find(|client| client.id == id)
    }

    fn client_mut(&mut self, id: ServerId) -> Option<&mut Client> {
        self.running.iter_mut().find(|client| client.id == id)
    }

    fn send(&mut self, id: ServerId, message: String) {
        self.out.push(Out::Send(id, message));
    }

    fn log(&mut self, id: ServerId, text: &str) {
        let name = self.client(id).map_or("server", |client| client.spec.name);
        self.out
            .push(Out::Log(format!("[{name}] {}\n", text.trim_end())));
    }

    fn take_out(&mut self) -> Vec<Out> {
        mem::take(&mut self.out)
    }

    fn sync(&mut self, visited: &[Visited<'_>], enabled: bool, text: &dyn Fn(BufferId) -> String) {
        if self.specs.is_empty() {
            return;
        }
        let mut seen = Vec::new();
        for file in visited {
            let Some(spec) = self.spec_for(file.language) else {
                continue;
            };
            seen.push(file.buffer);
            if enabled {
                self.sync_buffer(spec, file, text);
            }
        }
        self.close_documents(&seen);
    }

    fn root_for(&mut self, spec: &ServerSpec, path: &Path) -> PathBuf {
        if let Some((_, _, root)) = self
            .roots
            .iter()
            .find(|(file, name, _)| file == path && *name == spec.name)
        {
            return root.clone();
        }
        let root = root_of(spec, path);
        self.roots
            .push((path.to_path_buf(), spec.name, root.clone()));
        root
    }

    fn reconcile(&mut self, buffer: BufferId, owner: ServerId, uri: &str) {
        let mut closed = Vec::new();
        for client in &mut self.running {
            let (id, ready) = (client.id, client.state == State::Ready);
            for document in client.documents.extract_if(.., |document| {
                document.buffer == buffer && (id != owner || document.uri != uri)
            }) {
                if ready {
                    closed.push((id, closing(&document.uri)));
                }
            }
        }
        for (id, message) in closed {
            self.send(id, message);
        }
    }

    fn sync_buffer(
        &mut self,
        spec: ServerSpec,
        file: &Visited<'_>,
        text: &dyn Fn(BufferId) -> String,
    ) {
        let root = self.root_for(&spec, file.path);
        let id = match self
            .running
            .iter()
            .find(|client| client.spec.name == spec.name && client.root == root)
        {
            Some(client) => client.id,
            None => match self.start_server(spec, &root) {
                Some(id) => id,
                None => return,
            },
        };
        let uri = protocol::uri(file.path);
        self.reconcile(file.buffer, id, &uri);
        let Some(client) = self.client(id) else {
            return;
        };
        let known = client
            .document(file.buffer)
            .map(|document| document.revision);
        if client.state != State::Ready || known == Some(file.revision) {
            return;
        }
        let contents = text(file.buffer);
        let Some(client) = self.client_mut(id) else {
            return;
        };
        let message = if let Some(document) = client
            .documents
            .iter_mut()
            .find(|document| document.buffer == file.buffer)
        {
            document.version += 1;
            document.revision = file.revision;
            notification(
                "textDocument/didChange",
                &json!({
                    "textDocument": {"uri": document.uri, "version": document.version},
                    "contentChanges": [{"text": contents}]
                }),
            )
        } else {
            client.documents.push(Document {
                buffer: file.buffer,
                uri: uri.clone(),
                version: 1,
                revision: file.revision,
            });
            let language_id = spec.language_id(file.language).unwrap_or_default();
            notification(
                "textDocument/didOpen",
                &json!({
                    "textDocument": {"uri": uri, "languageId": language_id, "version": 1, "text": contents}
                }),
            )
        };
        self.send(id, message);
    }

    fn close_documents(&mut self, seen: &[BufferId]) {
        let mut closed = Vec::new();
        for client in &mut self.running {
            let (id, ready) = (client.id, client.state == State::Ready);
            for document in client
                .documents
                .extract_if(.., |document| !seen.contains(&document.buffer))
            {
                if ready {
                    closed.push((id, closing(&document.uri)));
                }
            }
        }
        for (id, message) in closed {
            self.send(id, message);
        }
    }

    fn start_server(&mut self, spec: ServerSpec, root: &Path) -> Option<ServerId> {
        let now = Instant::now();
        self.missing
            .retain(|(_, since)| now.duration_since(*since) < RETRY);
        if self.missing.iter().any(|(name, _)| *name == spec.name) {
            return None;
        }
        let find = self.find;
        let Some((program, args)) = spec
            .programs
            .iter()
            .find_map(|(program, args)| find(program).map(|found| (found, *args)))
        else {
            self.missing.push((spec.name, now));
            if !self.told.contains(&spec.name) {
                self.told.push(spec.name);
                self.out.push(Out::Tell(if spec.runs {
                    format!(
                        "{} would add completion, docs and more here; M-x list-tools can install it",
                        spec.name
                    )
                } else {
                    format!(
                        "{} would add completion, docs and more here; {}",
                        spec.name, spec.install
                    )
                }));
            }
            return None;
        };
        let id = ServerId(self.next);
        self.next += 1;
        let mut client = Client {
            id,
            spec,
            root: root.to_path_buf(),
            state: State::Starting,
            next_request: 0,
            pending: Vec::new(),
            capabilities: Value::Null,
            encoding: Encoding::Utf16,
            documents: Vec::new(),
            progress: Vec::new(),
            diagnostics: Vec::new(),
        };
        let initialize = client.request("initialize", &initialize(root), Pending::Initialize);
        self.running.push(client);
        self.out.push(Out::Start(
            id,
            Launch {
                program,
                args: args.iter().map(|arg| (*arg).to_owned()).collect(),
                directory: root.to_path_buf(),
            },
        ));
        self.send(id, initialize);
        self.log(id, &format!("starting in {}", root.display()));
        Some(id)
    }

    fn receive(&mut self, id: ServerId, event: Event) -> Vec<Outcome> {
        match event {
            Event::Message(body) => self.message(id, &body),
            Event::Log(line) => {
                self.log(id, &line);
                Vec::new()
            }
            Event::Exited(reason) => self.exited(id, reason),
        }
    }

    fn exited(&mut self, id: ServerId, reason: Option<String>) -> Vec<Outcome> {
        let Some(client) = self.client_mut(id) else {
            return Vec::new();
        };
        let expected = client
            .pending
            .iter()
            .any(|(_, pending)| *pending == Pending::Shutdown);
        let unanswered = client
            .pending
            .drain(..)
            .filter_map(|(_, pending)| match pending {
                Pending::Feature(asked) => Some(Outcome::Unanswered(asked)),
                Pending::Initialize | Pending::Shutdown => None,
            })
            .collect();
        client.state = State::Stopped(reason.clone());
        client.progress.clear();
        let name = client.spec.name;
        let note = reason.clone().unwrap_or_else(|| "exited".to_owned());
        self.log(id, &format!("stopped: {note}"));
        if !expected && let Some(reason) = reason {
            self.out.push(Out::Tell(format!(
                "{name} stopped: {reason}; M-x lsp-log shows why"
            )));
        }
        unanswered
    }

    fn message(&mut self, id: ServerId, body: &str) -> Vec<Outcome> {
        let Ok(message) = serde_json::from_str::<Value>(body) else {
            self.log(id, &format!("unreadable message: {body}"));
            return Vec::new();
        };
        let method = message.get("method").and_then(Value::as_str);
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match (method, message.get("id")) {
            (Some(method), Some(request)) => self.asked_of(id, method, request.clone(), params),
            (Some(method), None) => self.told_by(id, method, &params),
            (None, Some(answered)) => answered
                .as_i64()
                .map(|answered| self.answered_by(id, answered, &message))
                .unwrap_or_default(),
            (None, None) => Vec::new(),
        }
    }

    fn asked_of(
        &mut self,
        id: ServerId,
        method: &str,
        request: Value,
        params: Value,
    ) -> Vec<Outcome> {
        let result = match method {
            "workspace/configuration" => {
                let count = params
                    .get("items")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                Value::Array(vec![Value::Null; count])
            }
            "workspace/workspaceFolders" => {
                let Some(root) = self.client(id).map(|client| client.root.clone()) else {
                    return Vec::new();
                };
                json!([{"uri": protocol::uri(&root), "name": root.display().to_string()}])
            }
            "workspace/applyEdit" => {
                return vec![Outcome::EditRequested {
                    server: id,
                    request,
                    params,
                }];
            }
            "window/workDoneProgress/create"
            | "client/registerCapability"
            | "client/unregisterCapability"
            | "window/showMessageRequest" => Value::Null,
            _ => {
                self.send(
                    id,
                    json!({"jsonrpc": "2.0", "id": request, "error": {"code": -32601, "message": format!("oxmacs doesn't handle {method}")}})
                        .to_string(),
                );
                return Vec::new();
            }
        };
        self.reply(id, &request, &result);
        Vec::new()
    }

    fn told_by(&mut self, id: ServerId, method: &str, params: &Value) -> Vec<Outcome> {
        match method {
            "textDocument/publishDiagnostics" => return self.published(id, params),
            "window/logMessage" => {
                let text = params.get("message").and_then(Value::as_str).unwrap_or("");
                self.log(id, text);
            }
            "window/showMessage" => {
                let text = params
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                self.log(id, &text);
                let urgent = params
                    .get("type")
                    .and_then(Value::as_u64)
                    .is_some_and(|kind| kind <= 2);
                if urgent && let Some(client) = self.client(id) {
                    let name = client.spec.name;
                    self.out.push(Out::Tell(format!("{name}: {text}")));
                }
            }
            "$/progress" => self.progressed(id, params),
            _ => {}
        }
        Vec::new()
    }

    fn progressed(&mut self, id: ServerId, params: &Value) {
        let token = params
            .get("token")
            .map(ToString::to_string)
            .unwrap_or_default();
        let value = params.get("value").cloned().unwrap_or(Value::Null);
        let Some(client) = self.client_mut(id) else {
            return;
        };
        let kind = value.get("kind").and_then(Value::as_str).unwrap_or("");
        if kind == "end" {
            client.progress.retain(|(known, _)| *known != token);
            return;
        }
        let title = value
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                client
                    .progress
                    .iter()
                    .find(|(known, _)| *known == token)
                    .map(|(_, text)| text.split(" (").next().unwrap_or("").to_owned())
            })
            .unwrap_or_default();
        let text = match value.get("percentage").and_then(Value::as_u64) {
            Some(percent) => format!("{title} ({percent}%)"),
            None => title,
        };
        match client
            .progress
            .iter_mut()
            .find(|(known, _)| *known == token)
        {
            Some((_, shown)) => *shown = text,
            None => client.progress.push((token, text)),
        }
    }

    fn published(&mut self, id: ServerId, params: &Value) -> Vec<Outcome> {
        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
            return Vec::new();
        };
        let Some(path) = protocol::path(uri) else {
            return Vec::new();
        };
        let items = params
            .get("diagnostics")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let uri = uri.to_owned();
        let Some(client) = self.client_mut(id) else {
            return Vec::new();
        };
        client.diagnostics.retain(|(known, _)| *known != uri);
        client.diagnostics.push((uri, items.clone()));
        vec![Outcome::Diagnostics {
            server: id,
            path,
            items,
        }]
    }

    fn answered_by(&mut self, id: ServerId, answered: i64, message: &Value) -> Vec<Outcome> {
        let Some(client) = self.client_mut(id) else {
            return Vec::new();
        };
        let Some(at) = client
            .pending
            .iter()
            .position(|(known, _)| *known == answered)
        else {
            return Vec::new();
        };
        let (_, pending) = client.pending.remove(at);
        if let Some(error) = message.get("error") {
            let text = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("failed")
                .to_owned();
            self.log(id, &format!("request failed: {text}"));
            return match pending {
                Pending::Feature(asked) => vec![Outcome::Unanswered(asked)],
                Pending::Initialize | Pending::Shutdown => Vec::new(),
            };
        }
        let result = message.get("result").cloned().unwrap_or(Value::Null);
        match pending {
            Pending::Initialize => self.initialized(id, &result),
            Pending::Shutdown => {
                self.send(id, notification("exit", &Value::Null));
                self.out.push(Out::Stop(id));
                Vec::new()
            }
            Pending::Feature(asked) => vec![Outcome::Answered {
                server: id,
                asked,
                result,
            }],
        }
    }

    fn initialized(&mut self, id: ServerId, result: &Value) -> Vec<Outcome> {
        let Some(client) = self.client_mut(id) else {
            return Vec::new();
        };
        client.capabilities = result.get("capabilities").cloned().unwrap_or(Value::Null);
        client.encoding = Encoding::named(
            client
                .capabilities
                .get("positionEncoding")
                .and_then(Value::as_str),
        );
        client.state = State::Ready;
        self.send(id, notification("initialized", &json!({})));
        self.log(id, "ready");
        vec![Outcome::Ready]
    }

    fn ask(&mut self, id: ServerId, method: &str, params: &Value, asked: Asked) {
        let Some(client) = self.client_mut(id) else {
            return;
        };
        let message = client.request(method, params, Pending::Feature(asked));
        self.send(id, message);
    }

    fn reply(&mut self, id: ServerId, request: &Value, result: &Value) {
        self.send(id, response(request, result));
    }

    fn saved(&mut self, buffer: BufferId) {
        let mut saved = Vec::new();
        for client in &self.running {
            if client.state == State::Ready
                && let Some(document) = client.document(buffer)
            {
                saved.push((
                    client.id,
                    notification(
                        "textDocument/didSave",
                        &json!({"textDocument": {"uri": document.uri}}),
                    ),
                ));
            }
        }
        for (id, message) in saved {
            self.send(id, message);
        }
    }

    fn serving(&self, buffer: BufferId) -> Option<ServerId> {
        self.running
            .iter()
            .find(|client| client.state == State::Ready && client.document(buffer).is_some())
            .map(|client| client.id)
    }

    fn name(&self, id: ServerId) -> &'static str {
        self.client(id).map_or("server", |client| client.spec.name)
    }

    fn capable(&self, id: ServerId, capability: &str) -> bool {
        self.client(id)
            .is_some_and(|client| client.capable(capability))
    }

    fn encoding(&self, id: ServerId) -> Encoding {
        self.client(id)
            .map_or(Encoding::Utf16, |client| client.encoding)
    }

    fn uri(&self, id: ServerId, buffer: BufferId) -> Option<String> {
        self.client(id)?
            .document(buffer)
            .map(|document| document.uri.clone())
    }

    fn version(&self, id: ServerId, buffer: BufferId) -> Option<(u64, i64)> {
        self.client(id)?
            .document(buffer)
            .map(|document| (document.revision, document.version))
    }

    fn formatter(&self, buffer: BufferId) -> Option<(ServerId, String)> {
        self.running.iter().find_map(|client| {
            let document = client.document(buffer)?;
            (client.state == State::Ready && client.capable("documentFormattingProvider"))
                .then(|| (client.id, document.uri.clone()))
        })
    }

    fn triggered_by(&self, buffer: BufferId, typed: char) -> bool {
        let typed = typed.to_string();
        self.running.iter().any(|client| {
            client.document(buffer).is_some()
                && client.capabilities["completionProvider"]["triggerCharacters"]
                    .as_array()
                    .is_some_and(|triggers| {
                        triggers
                            .iter()
                            .any(|trigger| trigger.as_str() == Some(typed.as_str()))
                    })
        })
    }

    fn published_at(&self, id: ServerId, uri: &str) -> Vec<Value> {
        self.client(id)
            .and_then(|client| client.diagnostics.iter().find(|(known, _)| *known == uri))
            .map(|(_, found)| found.clone())
            .unwrap_or_default()
    }

    fn serves(&self, language: &str, path: &Path) -> bool {
        self.running.iter().any(|client| {
            !matches!(client.state, State::Stopped(_))
                && client.spec.language_id(language).is_some()
                && path.starts_with(&client.root)
        })
    }

    fn found(&self, spec: &ServerSpec) -> Option<PathBuf> {
        spec.programs
            .iter()
            .find_map(|(program, _)| (self.find)(program))
    }

    fn runs(&self, spec: &ServerSpec) -> bool {
        self.running
            .iter()
            .any(|client| client.spec.name == spec.name && client.state == State::Ready)
    }

    fn label(&self, language: &str, path: &Path) -> Option<String> {
        self.running
            .iter()
            .rev()
            .find(|client| {
                client.spec.language_id(language).is_some() && path.starts_with(&client.root)
            })
            .map(Client::label)
    }

    fn shutdown(&mut self) {
        let mut asked = Vec::new();
        for client in &mut self.running {
            if client.state == State::Ready {
                asked.push((
                    client.id,
                    client.request("shutdown", &Value::Null, Pending::Shutdown),
                ));
            }
        }
        for (id, message) in asked {
            self.send(id, message);
        }
    }

    fn retry(&mut self) {
        self.missing.clear();
    }

    fn restart(&mut self, buffer: BufferId) -> Option<&'static str> {
        self.retry();
        let at = self
            .running
            .iter()
            .position(|client| client.document(buffer).is_some())?;
        let client = self.running.remove(at);
        self.out.push(Out::Stop(client.id));
        Some(client.spec.name)
    }
}

#[cfg(test)]
mod tests;
