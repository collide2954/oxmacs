use std::path::{Path, PathBuf};

use serde_json::Value;

use super::features::Asked;
use super::{Out, Outcome, ServerId, ServerSpec, Visited, number};
use crate::editor::buffers::Mode;
use crate::editor::{Editor, Effect};
use crate::external::diagnostics::{Diagnostic, Severity};
use crate::external::lsp::Event;
use crate::text::{Buffer, BufferId};

const LOG: &str = "*language servers*";
const LOG_LINES: usize = 2000;

fn severity(code: Option<u64>) -> Option<Severity> {
    match code {
        Some(1) | None => Some(Severity::Error),
        Some(2 | 3) => Some(Severity::Warning),
        _ => None,
    }
}

impl Editor {
    pub fn add_language_servers(&mut self, specs: &[ServerSpec]) {
        self.lsp.add_specs(specs);
    }

    pub(super) fn flush_lsp(&mut self) {
        for out in self.lsp.take_out() {
            match out {
                Out::Send(id, message) => self.effects.push(Effect::ToServer(id, message)),
                Out::Start(id, launch) => self.effects.push(Effect::StartServer(id, launch)),
                Out::Stop(id) => self.effects.push(Effect::StopServer(id)),
                Out::Log(line) => self.log_server(&line),
                Out::Tell(text) => self.show_message(text),
            }
        }
    }

    pub(in crate::editor) fn sync_servers(&mut self) {
        let visited: Vec<Visited<'_>> = (0..self.buffers.len())
            .filter_map(|index| {
                let language = self.buffers.language(index)?;
                let buffer = self.buffers.get(index)?;
                Some(Visited {
                    buffer: buffer.id(),
                    language: language.name(),
                    path: &buffer.file()?.path,
                    revision: buffer.revision(),
                })
            })
            .collect();
        let buffers = &self.buffers;
        let text = |id: BufferId| {
            buffers
                .iter()
                .find(|buffer| buffer.id() == id)
                .map(Buffer::contents)
                .unwrap_or_default()
        };
        self.lsp
            .sync(&visited, self.settings.language_servers, &text);
        self.flush_lsp();
    }

    pub fn server_event(&mut self, id: ServerId, event: Event) {
        for outcome in self.lsp.receive(id, event) {
            self.follow(outcome);
        }
        self.flush_lsp();
    }

    fn follow(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Ready => self.sync_servers(),
            Outcome::Diagnostics {
                server,
                path,
                items,
            } => self.diagnosed(server, &path, &items),
            Outcome::Answered {
                server,
                asked,
                result,
            } => self.answered(server, asked, &result),
            Outcome::Unanswered(asked) => {
                if let Asked::Format {
                    buffer, save: true, ..
                } = asked
                {
                    self.finish_formatting_save(buffer);
                }
            }
            Outcome::EditRequested {
                server,
                request,
                params,
            } => {
                let result = self.applied_edit(server, &params);
                self.lsp.reply(server, &request, &result);
            }
        }
    }

    fn diagnosed(&mut self, server: ServerId, path: &Path, items: &[Value]) {
        let encoding = self.lsp.encoding(server);
        let buffer = self.index_visiting(path);
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
        let found: Vec<Diagnostic> = items
            .iter()
            .filter_map(|item| {
                let severity = severity(item.get("severity").and_then(Value::as_u64))?;
                let message = item.get("message").and_then(Value::as_str)?.to_owned();
                Some(Diagnostic {
                    path: path.to_path_buf(),
                    start: place(&item["range"]["start"]),
                    end: place(&item["range"]["end"]),
                    severity,
                    message,
                })
            })
            .collect();
        self.replace_diagnostics(path, found);
    }

    pub(in crate::editor) fn server_saved(&mut self, buffer: BufferId) {
        self.lsp.saved(buffer);
        self.flush_lsp();
    }

    pub(in crate::editor) fn serves(&self, language: &str, path: &Path) -> bool {
        self.lsp.serves(language, path)
    }

    pub(in crate::editor) fn server_specs(&self) -> Vec<ServerSpec> {
        self.lsp.specs.clone()
    }

    pub(in crate::editor) fn server_found(&self, spec: &ServerSpec) -> Option<PathBuf> {
        self.lsp.found(spec)
    }

    pub(in crate::editor) fn server_running(&self, spec: &ServerSpec) -> bool {
        self.lsp.runs(spec)
    }

    pub(in crate::editor) fn server_label(&self, language: &str, path: &Path) -> Option<String> {
        self.lsp.label(language, path)
    }

    pub fn shutdown_servers(&mut self) {
        self.lsp.shutdown();
        self.flush_lsp();
    }

    pub(in crate::editor) fn retry_servers(&mut self) {
        self.lsp.retry();
    }

    pub(in crate::editor) fn restart_server(&mut self) {
        let buffer = self.buffers.current().id();
        match self.lsp.restart(buffer) {
            Some(name) => {
                self.flush_lsp();
                self.show_message(format!("Restarting {name}"));
                self.sync_servers();
            }
            None => self.show_message("No language server works on this buffer"),
        }
    }

    pub(in crate::editor) fn show_server_log(&mut self) {
        match self.buffer_named(LOG) {
            Some(id) => self.show_buffer(id),
            None => self.show_message("No language server has started"),
        }
    }

    fn log_server(&mut self, line: &str) {
        let index = self
            .buffers
            .position(|buffer| buffer.name() == LOG)
            .unwrap_or_else(|| {
                let mut buffer = Buffer::new(LOG);
                buffer.set_read_only(true);
                self.buffers.push_back_with(buffer, Mode::Text(None))
            });
        if let Some(buffer) = self.buffers.get_mut(index) {
            let end = buffer.len_chars();
            buffer.insert_at(end, line);
            let excess = buffer.len_lines().saturating_sub(LOG_LINES);
            if excess > 0 {
                let cut = buffer.line_start(excess);
                buffer.delete_range(0..cut);
            }
        }
    }
}
