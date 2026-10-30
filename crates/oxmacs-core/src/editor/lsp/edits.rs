use std::cmp::Reverse;
use std::path::PathBuf;

use oxmacs_files::uri;
use serde_json::Value;

use super::features::STALE;
use super::{Encoding, ServerId, number};
use crate::editor::Editor;
use crate::text::{Buffer, BufferId};

pub(super) fn position(buffer: &Buffer, encoding: Encoding, at: &Value) -> usize {
    let line = number(&at["line"]);
    if line >= buffer.len_lines() {
        return buffer.len_chars();
    }
    let text = buffer.line_text(line);
    let column = encoding.to_column(&text, number(&at["character"]));
    (buffer.line_start(line) + column).min(buffer.line_end(line))
}

pub(super) fn files(edit: &Value) -> Vec<(PathBuf, Vec<Value>)> {
    let mut found = Vec::new();
    if let Some(changes) = edit.get("documentChanges").and_then(Value::as_array) {
        for change in changes {
            let uri = change["textDocument"]["uri"].as_str();
            let edits = change.get("edits").and_then(Value::as_array);
            if let (Some(path), Some(edits)) = (uri.and_then(uri::to_path), edits) {
                found.push((path, edits.clone()));
            }
        }
    } else if let Some(changes) = edit.get("changes").and_then(Value::as_object) {
        for (uri, edits) in changes {
            if let (Some(path), Some(edits)) = (uri::to_path(uri), edits.as_array()) {
                found.push((path, edits.clone()));
            }
        }
    }
    found
}

impl Editor {
    pub(super) fn apply_text_edits(
        &mut self,
        index: usize,
        encoding: Encoding,
        edits: &[Value],
    ) -> usize {
        let Some(buffer) = self.buffers.get(index) else {
            return 0;
        };
        let mut ranges: Vec<(usize, usize, usize, String)> = edits
            .iter()
            .enumerate()
            .filter_map(|(order, edit)| {
                let range = edit.get("range")?;
                let text = edit.get("newText")?.as_str()?.to_owned();
                let start = position(buffer, encoding, &range["start"]);
                let end = position(buffer, encoding, &range["end"]).max(start);
                Some((start, end, order, text))
            })
            .collect();
        ranges.sort_by_key(|range| Reverse((range.0, range.1, range.2)));
        let Some(buffer) = self.buffers.get_mut(index) else {
            return 0;
        };
        let point = buffer.point();
        let kept = buffer.add_marker(point);
        for (start, end, _, text) in &ranges {
            buffer.delete_range(*start..*end);
            buffer.insert_at(*start, text);
        }
        let point = buffer.marker(kept).unwrap_or(point).min(buffer.len_chars());
        buffer.remove_marker(kept);
        buffer.set_point(point);
        ranges.len()
    }

    fn outdated(&self, id: ServerId, edit: &Value) -> bool {
        if self.lsp.client(id).is_none() {
            return false;
        }
        let versions: Vec<(PathBuf, Option<i64>)> = edit
            .get("documentChanges")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|change| {
                let path = uri::to_path(change["textDocument"]["uri"].as_str()?)?;
                Some((path, change["textDocument"]["version"].as_i64()))
            })
            .chain(files(edit).into_iter().map(|(path, _)| (path, None)))
            .collect();
        versions.iter().any(|(path, version)| {
            self.index_visiting(path)
                .and_then(|index| self.buffers.get(index))
                .is_some_and(|buffer| {
                    self.lsp
                        .version(id, buffer.id())
                        .is_some_and(|(revision, known)| {
                            revision != buffer.revision()
                                || version.is_some_and(|version| version != known)
                        })
                })
        })
    }

    pub(super) fn apply_workspace_edit(
        &mut self,
        id: ServerId,
        at: Option<(BufferId, u64)>,
        edit: &Value,
    ) -> Option<(usize, usize)> {
        if at.is_some_and(|at| self.changed_since(at)) || self.outdated(id, edit) {
            self.show_message(STALE);
            return None;
        }
        let encoding = self.lsp.encoding(id);
        let origin = self.buffers.current().id();
        let mut places = 0;
        let mut changed = 0;
        for (path, edits) in files(edit) {
            let index = self.index_visiting(&path).or_else(|| {
                self.open_file(&path);
                self.index_visiting(&path)
            });
            let Some(index) = index else {
                continue;
            };
            let applied = self.apply_text_edits(index, encoding, &edits);
            places += applied;
            changed += usize::from(applied > 0);
        }
        if let Some(index) = self.buffers.position(|buffer| buffer.id() == origin) {
            self.select_buffer(index);
        }
        Some((places, changed))
    }
}
