mod edits;
mod features;
mod protocol;
mod servers;

use std::env;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

pub use protocol::Encoding;
pub use servers::{SERVERS, ServerSpec};

use crate::editor::{Editor, Effect};
use crate::external::diagnostics::{Diagnostic, Severity};
use crate::external::lsp::{self, Event, Launch};
use crate::text::{Buffer, BufferId};

const LOG: &str = "*language servers*";
const LOG_LINES: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerId(pub u64);

type Hints = (BufferId, u64, Vec<(usize, String)>);

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Starting,
    Ready,
    Stopped(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::editor) enum Pending {
    Initialize,
    Shutdown,
    Feature(features::Asked),
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
pub(in crate::editor) struct Clients {
    specs: Vec<ServerSpec>,
    running: Vec<Client>,
    next: u64,
    told: Vec<&'static str>,
    auto_mark: Option<(BufferId, u64)>,
    signature: Option<(BufferId, u64, usize)>,
    signature_shown: Option<String>,
    hints: Vec<Hints>,
    hints_asked: Option<(BufferId, u64)>,
    pub(in crate::editor) find: fn(&str) -> Option<PathBuf>,
}

fn on_path(program: &str) -> Option<PathBuf> {
    lsp::locate(program, env::var_os("PATH").as_deref())
}

impl Default for Clients {
    fn default() -> Self {
        Self {
            specs: Vec::new(),
            running: Vec::new(),
            next: 0,
            told: Vec::new(),
            auto_mark: None,
            signature: None,
            signature_shown: None,
            hints: Vec::new(),
            hints_asked: None,
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

fn severity(code: Option<u64>) -> Option<Severity> {
    match code {
        Some(1) | None => Some(Severity::Error),
        Some(2 | 3) => Some(Severity::Warning),
        _ => None,
    }
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
}

fn notification(method: &str, params: &Value) -> String {
    json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string()
}

fn response(id: &Value, result: &Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
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

impl Editor {
    pub fn add_language_servers(&mut self, specs: &[ServerSpec]) {
        for spec in specs {
            self.lsp.specs.retain(|known| known.name != spec.name);
            self.lsp.specs.push(*spec);
        }
    }

    fn spec_for(&self, language: &str) -> Option<ServerSpec> {
        self.lsp
            .specs
            .iter()
            .find(|spec| spec.language_id(language).is_some())
            .copied()
    }

    fn client_mut(&mut self, id: ServerId) -> Option<&mut Client> {
        self.lsp.running.iter_mut().find(|client| client.id == id)
    }

    fn send_to(&mut self, id: ServerId, message: String) {
        self.effects.push(Effect::ToServer(id, message));
    }

    pub(in crate::editor) fn sync_servers(&mut self) {
        if self.lsp.specs.is_empty() {
            return;
        }
        let mut seen = Vec::new();
        for index in 0..self.buffers.len() {
            let Some(language) = self.buffers.language(index) else {
                continue;
            };
            let Some(buffer) = self.buffers.get(index) else {
                continue;
            };
            let Some(path) = buffer.file().map(|file| file.path.clone()) else {
                continue;
            };
            let Some(spec) = self.spec_for(language.name()) else {
                continue;
            };
            seen.push(buffer.id());
            if self.settings.language_servers {
                self.sync_buffer(spec, language.name(), index, &path);
            }
        }
        self.close_documents(&seen);
    }

    fn sync_buffer(&mut self, spec: ServerSpec, language: &str, index: usize, path: &Path) {
        let root = root_of(&spec, path);
        let id = match self
            .lsp
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
        let Some(buffer) = self.buffers.get(index) else {
            return;
        };
        let (buffer_id, revision) = (buffer.id(), buffer.revision());
        let Some(client) = self.lsp.running.iter().find(|client| client.id == id) else {
            return;
        };
        let known = client.document(buffer_id).map(|document| document.revision);
        if client.state != State::Ready || known == Some(revision) {
            return;
        }
        let text = buffer.contents();
        let Some(client) = self.client_mut(id) else {
            return;
        };
        let message = if let Some(document) = client
            .documents
            .iter_mut()
            .find(|document| document.buffer == buffer_id)
        {
            document.version += 1;
            document.revision = revision;
            notification(
                "textDocument/didChange",
                &json!({
                    "textDocument": {"uri": document.uri, "version": document.version},
                    "contentChanges": [{"text": text}]
                }),
            )
        } else {
            let uri = protocol::uri(path);
            client.documents.push(Document {
                buffer: buffer_id,
                uri: uri.clone(),
                version: 1,
                revision,
            });
            let language_id = spec.language_id(language).unwrap_or_default();
            notification(
                "textDocument/didOpen",
                &json!({
                    "textDocument": {"uri": uri, "languageId": language_id, "version": 1, "text": text}
                }),
            )
        };
        self.send_to(id, message);
    }

    fn close_documents(&mut self, seen: &[BufferId]) {
        let mut closing = Vec::new();
        for client in &mut self.lsp.running {
            for document in client
                .documents
                .extract_if(.., |document| !seen.contains(&document.buffer))
            {
                if client.state == State::Ready {
                    closing.push((
                        client.id,
                        notification(
                            "textDocument/didClose",
                            &json!({"textDocument": {"uri": document.uri}}),
                        ),
                    ));
                }
            }
        }
        for (id, message) in closing {
            self.send_to(id, message);
        }
    }

    fn start_server(&mut self, spec: ServerSpec, root: &Path) -> Option<ServerId> {
        let find = self.lsp.find;
        let Some((program, args)) = spec
            .programs
            .iter()
            .find_map(|(program, args)| find(program).map(|found| (found, *args)))
        else {
            if !self.lsp.told.contains(&spec.name) {
                self.lsp.told.push(spec.name);
                self.show_message(if spec.runs {
                    format!(
                        "{} would add completion, docs and more here; M-x list-tools can install it",
                        spec.name
                    )
                } else {
                    format!(
                        "{} would add completion, docs and more here; {}",
                        spec.name, spec.install
                    )
                });
            }
            return None;
        };
        let id = ServerId(self.lsp.next);
        self.lsp.next += 1;
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
        self.lsp.running.push(client);
        self.effects.push(Effect::StartServer(
            id,
            Launch {
                program,
                args: args.iter().map(|arg| (*arg).to_owned()).collect(),
                directory: root.to_path_buf(),
            },
        ));
        self.send_to(id, initialize);
        self.server_log(id, &format!("starting in {}", root.display()));
        Some(id)
    }

    pub fn server_event(&mut self, id: ServerId, event: Event) {
        match event {
            Event::Message(body) => self.server_message(id, &body),
            Event::Log(line) => self.server_log(id, &line),
            Event::Exited(reason) => self.server_exited(id, reason),
        }
    }

    fn server_exited(&mut self, id: ServerId, reason: Option<String>) {
        let Some(client) = self.client_mut(id) else {
            return;
        };
        let expected = client
            .pending
            .iter()
            .any(|(_, pending)| *pending == Pending::Shutdown);
        client.state = State::Stopped(reason.clone());
        client.pending.clear();
        client.progress.clear();
        let name = client.spec.name;
        let note = reason.clone().unwrap_or_else(|| "exited".to_owned());
        self.server_log(id, &format!("stopped: {note}"));
        if !expected && let Some(reason) = reason {
            self.show_message(format!("{name} stopped: {reason}; M-x lsp-log shows why"));
        }
    }

    fn server_message(&mut self, id: ServerId, body: &str) {
        let Ok(message) = serde_json::from_str::<Value>(body) else {
            self.server_log(id, &format!("unreadable message: {body}"));
            return;
        };
        let method = message.get("method").and_then(Value::as_str);
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match (method, message.get("id")) {
            (Some(method), Some(request)) => {
                let request = request.clone();
                self.server_request(id, method, &request, &params);
            }
            (Some(method), None) => self.server_notification(id, method, &params),
            (None, Some(answered)) => {
                let Some(answered) = answered.as_i64() else {
                    return;
                };
                self.server_response(id, answered, &message);
            }
            (None, None) => {}
        }
    }

    fn server_request(&mut self, id: ServerId, method: &str, request: &Value, params: &Value) {
        let result = match method {
            "workspace/configuration" => {
                let count = params
                    .get("items")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                Value::Array(vec![Value::Null; count])
            }
            "workspace/workspaceFolders" => {
                let Some(root) = self.client_mut(id).map(|client| client.root.clone()) else {
                    return;
                };
                json!([{"uri": protocol::uri(&root), "name": root.display().to_string()}])
            }
            "workspace/applyEdit" => self.applied_edit(id, params),
            "window/workDoneProgress/create"
            | "client/registerCapability"
            | "client/unregisterCapability"
            | "window/showMessageRequest" => Value::Null,
            _ => {
                self.send_to(
                    id,
                    json!({"jsonrpc": "2.0", "id": request, "error": {"code": -32601, "message": format!("oxmacs doesn't handle {method}")}})
                        .to_string(),
                );
                return;
            }
        };
        self.send_to(id, response(request, &result));
    }

    fn server_notification(&mut self, id: ServerId, method: &str, params: &Value) {
        match method {
            "textDocument/publishDiagnostics" => self.published(id, params),
            "window/logMessage" => {
                let text = params.get("message").and_then(Value::as_str).unwrap_or("");
                self.server_log(id, text);
            }
            "window/showMessage" => {
                let text = params
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                self.server_log(id, &text);
                let urgent = params
                    .get("type")
                    .and_then(Value::as_u64)
                    .is_some_and(|kind| kind <= 2);
                if urgent && let Some(client) = self.client_mut(id) {
                    let name = client.spec.name;
                    self.show_message(format!("{name}: {text}"));
                }
            }
            "$/progress" => self.progressed(id, params),
            _ => {}
        }
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

    fn published(&mut self, id: ServerId, params: &Value) {
        let Some(path) = params
            .get("uri")
            .and_then(Value::as_str)
            .and_then(protocol::path)
        else {
            return;
        };
        let raw = params
            .get("diagnostics")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let uri = params["uri"].as_str().unwrap_or_default().to_owned();
        let Some(client) = self.client_mut(id) else {
            return;
        };
        client.diagnostics.retain(|(known, _)| *known != uri);
        client.diagnostics.push((uri, raw));
        let encoding = client.encoding;
        let buffer = self
            .buffers
            .position(|buffer| buffer.file().is_some_and(|file| file.path == path));
        let line_text = |line: usize| -> String {
            buffer
                .and_then(|index| self.buffers.get(index))
                .filter(|buffer| line < buffer.len_lines())
                .map(|buffer| buffer.line_text(line).into_owned())
                .unwrap_or_default()
        };
        let place = |position: &Value| {
            let line = number(&position["line"]);
            let units = number(&position["character"]);
            let text = line_text(line);
            let column = if text.is_empty() {
                units
            } else {
                encoding.to_column(&text, units)
            };
            (line + 1, column + 1)
        };
        let found: Vec<Diagnostic> = params
            .get("diagnostics")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| {
                let severity = severity(item.get("severity").and_then(Value::as_u64))?;
                let message = item.get("message").and_then(Value::as_str)?.to_owned();
                Some(Diagnostic {
                    path: path.clone(),
                    start: place(&item["range"]["start"]),
                    end: place(&item["range"]["end"]),
                    severity,
                    message,
                })
            })
            .collect();
        self.replace_diagnostics(&path, found);
    }

    fn server_response(&mut self, id: ServerId, answered: i64, message: &Value) {
        let Some(client) = self.client_mut(id) else {
            return;
        };
        let Some(at) = client
            .pending
            .iter()
            .position(|(known, _)| *known == answered)
        else {
            return;
        };
        let (_, pending) = client.pending.remove(at);
        if let Some(error) = message.get("error") {
            let text = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("failed")
                .to_owned();
            self.server_log(id, &format!("request failed: {text}"));
            return;
        }
        let result = message.get("result").cloned().unwrap_or(Value::Null);
        match pending {
            Pending::Initialize => self.initialized(id, &result),
            Pending::Shutdown => {
                self.send_to(id, notification("exit", &Value::Null));
                self.effects.push(Effect::StopServer(id));
            }
            Pending::Feature(asked) => self.answered(id, asked, &result),
        }
    }

    fn initialized(&mut self, id: ServerId, result: &Value) {
        let Some(client) = self.client_mut(id) else {
            return;
        };
        client.capabilities = result.get("capabilities").cloned().unwrap_or(Value::Null);
        client.encoding = Encoding::named(
            client
                .capabilities
                .get("positionEncoding")
                .and_then(Value::as_str),
        );
        client.state = State::Ready;
        self.send_to(id, notification("initialized", &json!({})));
        self.server_log(id, "ready");
        self.sync_servers();
    }

    pub(in crate::editor) fn server_saved(&mut self, buffer: BufferId) {
        let mut saved = Vec::new();
        for client in &self.lsp.running {
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
            self.send_to(id, message);
        }
    }

    pub(in crate::editor) fn serves(&self, language: &str, path: &Path) -> bool {
        self.lsp.running.iter().any(|client| {
            !matches!(client.state, State::Stopped(_))
                && client.spec.language_id(language).is_some()
                && path.starts_with(&client.root)
        })
    }

    pub(in crate::editor) fn server_specs(&self) -> Vec<ServerSpec> {
        self.lsp.specs.clone()
    }

    pub(in crate::editor) fn server_found(&self, spec: &ServerSpec) -> Option<PathBuf> {
        spec.programs
            .iter()
            .find_map(|(program, _)| (self.lsp.find)(program))
    }

    pub(in crate::editor) fn server_running(&self, spec: &ServerSpec) -> bool {
        self.lsp
            .running
            .iter()
            .any(|client| client.spec.name == spec.name && client.state == State::Ready)
    }

    pub(in crate::editor) fn server_label(&self, language: &str, path: &Path) -> Option<String> {
        self.lsp
            .running
            .iter()
            .rev()
            .find(|client| {
                client.spec.language_id(language).is_some() && path.starts_with(&client.root)
            })
            .map(Client::label)
    }

    pub fn shutdown_servers(&mut self) {
        let mut asked = Vec::new();
        for client in &mut self.lsp.running {
            if client.state == State::Ready {
                asked.push((
                    client.id,
                    client.request("shutdown", &Value::Null, Pending::Shutdown),
                ));
            }
        }
        for (id, message) in asked {
            self.send_to(id, message);
        }
    }

    pub(in crate::editor) fn restart_server(&mut self) {
        let buffer = self.buffers.current().id();
        let Some(at) = self
            .lsp
            .running
            .iter()
            .position(|client| client.document(buffer).is_some())
        else {
            self.show_message("No language server works on this buffer");
            return;
        };
        let client = self.lsp.running.remove(at);
        self.effects.push(Effect::StopServer(client.id));
        self.show_message(format!("Restarting {}", client.spec.name));
        self.sync_servers();
    }

    pub(in crate::editor) fn show_server_log(&mut self) {
        match self.buffer_named(LOG) {
            Some(id) => self.show_buffer(id),
            None => self.show_message("No language server has started"),
        }
    }

    fn server_log(&mut self, id: ServerId, text: &str) {
        let name = self
            .lsp
            .running
            .iter()
            .find(|client| client.id == id)
            .map_or("server", |client| client.spec.name);
        let line = format!("[{name}] {}\n", text.trim_end());
        let index = self
            .buffers
            .position(|buffer| buffer.name() == LOG)
            .unwrap_or_else(|| {
                let mut buffer = Buffer::new(LOG);
                buffer.set_read_only(true);
                self.buffers
                    .push_back_with(buffer, crate::editor::buffers::Mode::Text(None))
            });
        if let Some(buffer) = self.buffers.get_mut(index) {
            let end = buffer.len_chars();
            buffer.insert_at(end, &line);
            let excess = buffer.len_lines().saturating_sub(LOG_LINES);
            if excess > 0 {
                let cut = buffer.line_start(excess);
                buffer.delete_range(0..cut);
            }
        }
    }
}

#[cfg(test)]
mod tests;
