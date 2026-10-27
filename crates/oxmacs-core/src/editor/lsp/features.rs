use serde_json::{Value, json};

use super::edits::{files, position};
use super::{Encoding, ServerId, number, protocol};
use crate::command::Command;
use crate::editor::{Editor, Offer, Picked, Place};
use crate::text::BufferId;

const AUTO_PREFIX: usize = 3;
const MOST_ITEMS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::editor) enum Asked {
    Completion {
        buffer: BufferId,
        start: usize,
        explicit: bool,
    },
    Signature {
        buffer: BufferId,
    },
    Hover,
    Definition {
        origin: (BufferId, usize),
    },
    References {
        name: String,
    },
    Rename {
        name: String,
        at: (BufferId, u64),
    },
    Format {
        buffer: BufferId,
        revision: u64,
        save: bool,
    },
    Hints {
        buffer: BufferId,
        revision: u64,
    },
    CodeActions {
        at: (BufferId, u64),
    },
    Command,
}

pub(super) const STALE: &str =
    "The text changed while the server worked on it, so its edits were dropped";

type Hints = (BufferId, u64, Vec<(usize, String)>);

#[derive(Debug, Default)]
pub(super) struct Assist {
    auto_mark: Option<(BufferId, u64)>,
    signature: Option<(BufferId, u64, usize)>,
    signature_shown: Option<String>,
    hints: Vec<Hints>,
    hints_asked: Option<(BufferId, u64)>,
    formatting_saves: Vec<BufferId>,
}

fn word_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn text_of(contents: &Value) -> String {
    match contents {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(text_of)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
        Value::Object(_) => contents
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    }
}

fn plain(markdown: &str) -> String {
    let lines: Vec<&str> = markdown
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .collect();
    lines.join("\n").trim().to_owned()
}

fn locations(result: &Value) -> Vec<(String, Value)> {
    let items = match result {
        Value::Array(items) => items.clone(),
        Value::Null => Vec::new(),
        single => vec![single.clone()],
    };
    items
        .into_iter()
        .filter_map(|item| {
            let uri = item
                .get("targetUri")
                .or_else(|| item.get("uri"))?
                .as_str()?
                .to_owned();
            let range = item
                .get("targetSelectionRange")
                .or_else(|| item.get("range"))?
                .clone();
            Some((uri, range))
        })
        .collect()
}

fn titled(item: &Value) -> String {
    item.get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled action")
        .to_owned()
}

impl Editor {
    fn serving_here(&mut self, capability: &str, quiet: bool) -> Option<(ServerId, Value)> {
        self.sync_servers();
        let buffer = self.buffers.current();
        let (id, point) = (buffer.id(), buffer.point());
        let line = buffer.line_of(point);
        let text = buffer.line_text(line).into_owned();
        let column = point - buffer.line_start(line);
        let Some(server) = self.lsp.serving(id) else {
            if !quiet {
                self.show_message("No language server works on this buffer");
            }
            return None;
        };
        if !self.lsp.capable(server, capability) {
            if !quiet {
                let name = self.lsp.name(server);
                self.show_message(format!("{name} can't do that"));
            }
            return None;
        }
        let uri = self.lsp.uri(server, id)?;
        let character = self.lsp.encoding(server).to_units(&text, column);
        Some((
            server,
            json!({"textDocument": {"uri": uri}, "position": {"line": line, "character": character}}),
        ))
    }

    fn ask_server(&mut self, id: ServerId, method: &str, params: &Value, asked: Asked) {
        self.lsp.ask(id, method, params, asked);
        self.flush_lsp();
    }

    fn encoding_of(&self, id: ServerId) -> Encoding {
        self.lsp.encoding(id)
    }

    pub fn lsp_serves_here(&self) -> bool {
        self.lsp.serving(self.buffers.current().id()).is_some()
    }

    fn word_start(&self) -> usize {
        let buffer = self.buffers.current();
        let point = buffer.point();
        point
            - buffer
                .chars_before(point)
                .take_while(|c| word_char(*c))
                .count()
    }

    pub(in crate::editor) fn lsp_complete(&mut self, explicit: bool) {
        let Some((id, params)) = self.serving_here("completionProvider", !explicit) else {
            return;
        };
        let asked = Asked::Completion {
            buffer: self.buffers.current().id(),
            start: self.word_start(),
            explicit,
        };
        self.ask_server(id, "textDocument/completion", &params, asked);
    }

    fn trigger_typed(&self) -> bool {
        let buffer = self.buffers.current();
        let Some(typed) = buffer.chars_before(buffer.point()).next() else {
            return false;
        };
        self.lsp.triggered_by(buffer.id(), typed)
    }

    pub(in crate::editor) fn lsp_wants_auto_complete(&self) -> bool {
        let buffer = self.buffers.current();
        let typed = buffer
            .chars_before(buffer.point())
            .take_while(|c| word_char(*c))
            .count();
        self.last_command == Some(Command::SelfInsertCommand)
            && self.minibuffer.is_none()
            && self.popup.is_none()
            && self.lsp_serves_here()
            && (typed >= AUTO_PREFIX || self.trigger_typed())
            && self.lsp.assist.auto_mark != Some((buffer.id(), buffer.revision()))
    }

    pub(in crate::editor) fn lsp_auto_complete(&mut self) {
        let buffer = self.buffers.current();
        self.lsp.assist.auto_mark = Some((buffer.id(), buffer.revision()));
        self.lsp_complete(false);
    }

    pub(in crate::editor) fn lsp_wants_signature(&self) -> bool {
        let buffer = self.buffers.current();
        self.lsp_serves_here()
            && self.lsp.assist.signature != Some((buffer.id(), buffer.revision(), buffer.point()))
    }

    pub(in crate::editor) fn completion_at_point(&mut self) {
        self.lsp_complete(true);
    }

    fn in_comment(&self) -> bool {
        let buffer = self.buffers.current();
        let point = buffer.point();
        let line = buffer.line_of(point);
        self.buffers
            .syntax(0)
            .and_then(|syntax| syntax.comment_start(buffer, line))
            .is_some_and(|column| buffer.line_start(line) + column < point)
    }

    pub fn wants_auto_complete(&self) -> bool {
        self.lsp_wants_auto_complete() && !self.in_comment()
    }

    pub fn auto_complete(&mut self) {
        if self.wants_auto_complete() {
            self.lsp_auto_complete();
        }
    }

    pub fn wants_signature(&self) -> bool {
        let quiet = self.minibuffer.is_none() && self.popup.is_none() && self.question.is_none();
        quiet && self.lsp_wants_signature()
    }

    pub fn show_signature(&mut self) {
        if self.wants_signature() {
            self.lsp_signature();
        }
    }

    fn lsp_hints(&mut self) {
        let buffer = self.buffers.current();
        let (id, revision, lines) = (buffer.id(), buffer.revision(), buffer.len_lines());
        if !self.settings.inlay_hints || self.lsp.assist.hints_asked == Some((id, revision)) {
            return;
        }
        let Some((server, params)) = self.serving_here("inlayHintProvider", true) else {
            return;
        };
        self.lsp.assist.hints_asked = Some((id, revision));
        let range = json!({
            "textDocument": params["textDocument"],
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": lines, "character": 0}}
        });
        self.ask_server(
            server,
            "textDocument/inlayHint",
            &range,
            Asked::Hints {
                buffer: id,
                revision,
            },
        );
    }

    pub(in crate::editor) fn hint_on_line(
        &self,
        buffer: BufferId,
        revision: u64,
        line: usize,
    ) -> Option<String> {
        self.lsp
            .assist
            .hints
            .iter()
            .find(|(known, at, _)| *known == buffer && *at == revision)?
            .2
            .iter()
            .find(|(number, _)| *number == line)
            .map(|(_, text)| text.clone())
    }

    fn hinted(&mut self, buffer: BufferId, revision: u64, result: &Value) {
        let mut lines: Vec<(usize, String)> = Vec::new();
        for hint in result.as_array().into_iter().flatten() {
            let label = match &hint["label"] {
                Value::String(text) => text.clone(),
                Value::Array(parts) => parts
                    .iter()
                    .filter_map(|part| part["value"].as_str())
                    .collect(),
                _ => continue,
            };
            let label = label.trim().to_owned();
            if label.is_empty() {
                continue;
            }
            let line = number(&hint["position"]["line"]);
            match lines.iter_mut().find(|(known, _)| *known == line) {
                Some((_, text)) => {
                    text.push_str("  ");
                    text.push_str(&label);
                }
                None => lines.push((line, label)),
            }
        }
        self.lsp
            .assist
            .hints
            .retain(|(known, _, _)| *known != buffer);
        self.lsp.assist.hints.push((buffer, revision, lines));
    }

    pub(in crate::editor) fn lsp_signature(&mut self) {
        self.lsp_hints();
        let buffer = self.buffers.current();
        self.lsp.assist.signature = Some((buffer.id(), buffer.revision(), buffer.point()));
        let buffer = buffer.id();
        if let Some((id, params)) = self.serving_here("signatureHelpProvider", true) {
            self.ask_server(
                id,
                "textDocument/signatureHelp",
                &params,
                Asked::Signature { buffer },
            );
        }
    }

    pub(in crate::editor) fn lsp_describe(&mut self) {
        if let Some((id, params)) = self.serving_here("hoverProvider", false) {
            self.ask_server(id, "textDocument/hover", &params, Asked::Hover);
        }
    }

    pub(in crate::editor) fn lsp_find_definition(&mut self) {
        let origin = self.current_location();
        if let Some((id, params)) = self.serving_here("definitionProvider", false) {
            self.ask_server(
                id,
                "textDocument/definition",
                &params,
                Asked::Definition { origin },
            );
        }
    }

    fn symbol_here(&self) -> String {
        let buffer = self.buffers.current();
        let point = buffer.point();
        let before: Vec<char> = buffer
            .chars_before(point)
            .take_while(|c| word_char(*c))
            .collect();
        let mut name: String = before.into_iter().rev().collect();
        name.extend(buffer.chars_after(point).take_while(|c| word_char(*c)));
        name
    }

    pub(in crate::editor) fn lsp_find_references(&mut self) {
        let name = self.symbol_here();
        if let Some((id, mut params)) = self.serving_here("referencesProvider", false) {
            params["context"] = json!({"includeDeclaration": true});
            self.ask_server(
                id,
                "textDocument/references",
                &params,
                Asked::References { name },
            );
        }
    }

    pub(in crate::editor) fn lsp_rename(&mut self) {
        let name = self.symbol_here();
        let Some((id, params)) = self.serving_here("renameProvider", false) else {
            return;
        };
        let prompt = if name.is_empty() {
            "Rename to: ".to_owned()
        } else {
            format!("Rename {name} to: ")
        };
        let initial = name.clone();
        self.prompt_with_text(&prompt, &initial, move |editor, wanted| {
            let wanted = wanted.trim().to_owned();
            if wanted.is_empty() || wanted == name {
                return;
            }
            let mut params = params;
            params["newName"] = json!(wanted);
            let at = editor.revision_here();
            editor.ask_server(
                id,
                "textDocument/rename",
                &params,
                Asked::Rename { name, at },
            );
        });
    }

    fn revision_here(&self) -> (BufferId, u64) {
        let buffer = self.buffers.current();
        (buffer.id(), buffer.revision())
    }

    pub(super) fn changed_since(&self, (buffer, revision): (BufferId, u64)) -> bool {
        self.buffers
            .iter()
            .any(|known| known.id() == buffer && known.revision() != revision)
    }

    pub(in crate::editor) fn lsp_formats(&self, index: usize) -> bool {
        let Some(buffer) = self.buffers.get(index).map(crate::text::Buffer::id) else {
            return false;
        };
        self.lsp.formatter(buffer).is_some()
    }

    pub(in crate::editor) fn lsp_format(&mut self, index: usize, save: bool) -> bool {
        self.sync_servers();
        let Some((buffer, revision)) = self
            .buffers
            .get(index)
            .map(|buffer| (buffer.id(), buffer.revision()))
        else {
            return false;
        };
        let Some((id, uri)) = self.lsp.formatter(buffer) else {
            return false;
        };
        let params = json!({
            "textDocument": {"uri": uri},
            "options": {"tabSize": self.indent_size_of(buffer).unwrap_or_else(|| self.tab_width_of(buffer)), "insertSpaces": !self.indent_tabs_of(buffer)}
        });
        self.ask_server(
            id,
            "textDocument/formatting",
            &params,
            Asked::Format {
                buffer,
                revision,
                save,
            },
        );
        true
    }

    pub(in crate::editor) fn format_then_save(&mut self, index: usize) -> bool {
        let Some(buffer) = self.buffers.get(index).map(crate::text::Buffer::id) else {
            return false;
        };
        if self.lsp.assist.formatting_saves.contains(&buffer) {
            return true;
        }
        if !self.lsp_format(index, true) {
            return false;
        }
        self.lsp.assist.formatting_saves.push(buffer);
        true
    }

    pub(in crate::editor) fn finish_formatting_save(&mut self, buffer: BufferId) -> bool {
        if !self.lsp.assist.formatting_saves.contains(&buffer) {
            return true;
        }
        self.lsp
            .assist
            .formatting_saves
            .retain(|known| *known != buffer);
        self.buffers
            .position(|known| known.id() == buffer)
            .is_none_or(|index| self.write_buffer_at(index, false))
    }

    pub(in crate::editor) fn finish_formatting_saves(&mut self) -> bool {
        let mut written = true;
        for buffer in self.lsp.assist.formatting_saves.clone() {
            written &= self.finish_formatting_save(buffer);
        }
        written
    }

    pub(in crate::editor) fn lsp_code_actions(&mut self) {
        let Some((id, params)) = self.serving_here("codeActionProvider", false) else {
            return;
        };
        let buffer = self.buffers.current();
        let line = buffer.line_of(buffer.point());
        let uri = params["textDocument"]["uri"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        let around: Vec<Value> = self
            .lsp
            .published_at(id, &uri)
            .into_iter()
            .filter(|diagnostic| {
                let first = number(&diagnostic["range"]["start"]["line"]);
                let last = number(&diagnostic["range"]["end"]["line"]);
                (first..=last).contains(&line)
            })
            .collect();
        let position = params["position"].clone();
        let asked = json!({
            "textDocument": {"uri": uri},
            "range": {"start": position, "end": position},
            "context": {"diagnostics": around}
        });
        let at = self.revision_here();
        self.ask_server(
            id,
            "textDocument/codeAction",
            &asked,
            Asked::CodeActions { at },
        );
    }

    pub(super) fn answered(&mut self, id: ServerId, asked: Asked, result: &Value) {
        match asked {
            Asked::Completion {
                buffer,
                start,
                explicit,
            } => self.completed(id, buffer, start, explicit, result),
            Asked::Signature { buffer } => self.signed(buffer, result),
            Asked::Hover => self.hovered(result),
            Asked::Definition { origin } => self.defined(id, origin, result),
            Asked::References { name } => self.referenced(id, &name, result),
            Asked::Rename { name, at } => {
                if let Some((places, changed)) = self.apply_workspace_edit(id, Some(at), result) {
                    self.show_message(if places == 0 {
                        format!("Nothing to rename for {name}")
                    } else {
                        format!(
                            "Renamed {name} in {places} places in {changed} files; C-x s saves them"
                        )
                    });
                }
            }
            Asked::Format {
                buffer,
                revision,
                save,
            } => self.formatted(id, (buffer, revision), save, result),
            Asked::Hints { buffer, revision } => self.hinted(buffer, revision, result),
            Asked::CodeActions { at } => self.offer_actions(id, at, result),
            Asked::Command => {}
        }
    }

    fn completed(
        &mut self,
        id: ServerId,
        buffer: BufferId,
        start: usize,
        explicit: bool,
        result: &Value,
    ) {
        let current = self.buffers.current();
        let point = current.point();
        if current.id() != buffer
            || self.minibuffer.is_some()
            || start > point
            || current.line_of(start) != current.line_of(point)
        {
            return;
        }
        let items = result
            .get("items")
            .and_then(Value::as_array)
            .or_else(|| result.as_array())
            .cloned()
            .unwrap_or_default();
        let mut items: Vec<&Value> = items.iter().take(MOST_ITEMS * 5).collect();
        items.sort_by_key(|item| {
            item.get("sortText")
                .or_else(|| item.get("label"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        });
        let encoding = self.encoding_of(id);
        let edited_start = items
            .first()
            .and_then(|item| item["textEdit"]["range"]["start"].as_object().map(|_| item))
            .map(|item| {
                position(
                    self.buffers.current(),
                    encoding,
                    &item["textEdit"]["range"]["start"],
                )
            })
            .filter(|at| {
                *at <= point
                    && self.buffers.current().line_of(*at) == self.buffers.current().line_of(point)
            });
        let mut candidates: Vec<String> = Vec::new();
        for item in items {
            let text = item["textEdit"]["newText"]
                .as_str()
                .or_else(|| item["insertText"].as_str())
                .or_else(|| item["label"].as_str())
                .unwrap_or_default()
                .to_owned();
            if !text.is_empty() && !candidates.contains(&text) {
                candidates.push(text);
            }
            if candidates.len() >= MOST_ITEMS {
                break;
            }
        }
        self.complete_at(edited_start.unwrap_or(start), candidates, explicit);
    }

    fn signed(&mut self, buffer: BufferId, result: &Value) {
        if self.buffers.current().id() != buffer {
            return;
        }
        let active = result.get("activeSignature").map_or(0, number);
        let label = result["signatures"]
            .get(active)
            .or_else(|| result["signatures"].get(0))
            .and_then(|signature| signature["label"].as_str())
            .map(str::to_owned);
        let ours = self.message.is_none() || self.message == self.lsp.assist.signature_shown;
        match label {
            Some(label) if ours => {
                self.show_message(label.clone());
                self.lsp.assist.signature_shown = Some(label);
            }
            None if ours && self.lsp.assist.signature_shown.is_some() => {
                self.message = None;
                self.lsp.assist.signature_shown = None;
            }
            _ => {}
        }
    }

    fn hovered(&mut self, result: &Value) {
        let text = plain(&text_of(&result["contents"]));
        if text.is_empty() {
            self.show_message("No documentation here");
            return;
        }
        self.show_doc(&text);
    }

    fn places(&mut self, id: ServerId, found: &[(String, Value)]) -> Vec<Place> {
        let encoding = self.encoding_of(id);
        found
            .iter()
            .filter_map(|(uri, range)| {
                let path = protocol::path(uri)?;
                let line = number(&range["start"]["line"]);
                let units = number(&range["start"]["character"]);
                let column = self
                    .index_visiting(&path)
                    .and_then(|index| self.buffers.get(index))
                    .filter(|buffer| line < buffer.len_lines())
                    .map_or(units, |buffer| {
                        encoding.to_column(&buffer.line_text(line), units)
                    });
                Some(Place {
                    path,
                    line: line + 1,
                    column: column + 1,
                })
            })
            .collect()
    }

    fn defined(&mut self, id: ServerId, origin: (BufferId, usize), result: &Value) {
        let found = locations(result);
        let places = self.places(id, &found);
        match places.as_slice() {
            [] => self.show_message("No definition found"),
            [place] => {
                if self.open_file(&place.path) {
                    self.remember_location(origin);
                    self.go_to_line(place.line, place.column);
                }
            }
            _ => {
                self.remember_location(origin);
                let count = places.len();
                self.show_locations(&format!("{count} definitions"), places);
            }
        }
    }

    fn referenced(&mut self, id: ServerId, name: &str, result: &Value) {
        let found = locations(result);
        let places = self.places(id, &found);
        if places.is_empty() {
            self.show_message(format!("No references to {name}"));
            return;
        }
        let count = places.len();
        let noun = if count == 1 {
            "reference"
        } else {
            "references"
        };
        self.show_locations(&format!("{count} {noun} to {name}"), places);
    }

    fn formatted(&mut self, id: ServerId, at: (BufferId, u64), save: bool, result: &Value) {
        let buffer = at.0;
        if save && !self.lsp.assist.formatting_saves.contains(&buffer) {
            return;
        }
        let stale = self.changed_since(at);
        if !stale && let Some(index) = self.buffers.position(|known| known.id() == buffer) {
            let edits = result.as_array().cloned().unwrap_or_default();
            let encoding = self.encoding_of(id);
            self.apply_text_edits(index, encoding, &edits);
        }
        if save {
            self.finish_formatting_save(buffer);
        }
        if stale {
            self.show_message(STALE);
        }
    }

    fn offer_actions(&mut self, id: ServerId, at: (BufferId, u64), result: &Value) {
        let actions = result.as_array().cloned().unwrap_or_default();
        if actions.is_empty() {
            self.show_message("No code actions here");
            return;
        }
        let offers: Vec<Offer> = actions
            .iter()
            .map(|action| match action.get("kind").and_then(Value::as_str) {
                Some(kind) if !kind.is_empty() => Offer::noted(titled(action), kind),
                _ => Offer::from(titled(action)),
            })
            .collect();
        self.prompt_pick(
            "Code action: ",
            move |_| offers.clone(),
            move |editor, picked| {
                let chosen = match picked {
                    Picked::Offered(index) => actions.get(index).cloned(),
                    Picked::Typed(title) => actions
                        .iter()
                        .find(|action| titled(action) == title)
                        .cloned(),
                };
                if let Some(action) = chosen {
                    editor.run_action(id, at, &action);
                }
            },
        );
    }

    fn run_action(&mut self, id: ServerId, at: (BufferId, u64), action: &Value) {
        if let Some(edit) = action.get("edit") {
            if self.apply_workspace_edit(id, Some(at), edit).is_none() {
                return;
            }
            self.sync_servers();
        }
        let command = match action.get("command") {
            Some(Value::String(_)) => Some(action.clone()),
            Some(command @ Value::Object(_)) => Some(command.clone()),
            _ => None,
        };
        if let Some(command) = command {
            let params = json!({
                "command": command["command"],
                "arguments": command.get("arguments").cloned().unwrap_or(json!([]))
            });
            self.ask_server(id, "workspace/executeCommand", &params, Asked::Command);
        }
    }

    pub(super) fn applied_edit(&mut self, id: ServerId, params: &Value) -> Value {
        match self.apply_workspace_edit(id, None, &params["edit"]) {
            Some((places, _)) => {
                json!({"applied": places > 0 || !files(&params["edit"]).is_empty()})
            }
            None => json!({"applied": false, "failureReason": STALE}),
        }
    }
}
