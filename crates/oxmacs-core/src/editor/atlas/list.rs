use std::path::PathBuf;

use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::text::Buffer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::editor) enum ListKind {
    Backlinks(String),
    Agenda(Option<String>),
}

impl ListKind {
    fn name(&self) -> &'static str {
        match self {
            Self::Backlinks(_) => "*Backlinks*",
            Self::Agenda(_) => "*Agenda*",
        }
    }

    fn same(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Debug)]
pub(in crate::editor) struct AtlasList {
    pub(super) kind: ListKind,
    pub(super) rows: Vec<(usize, PathBuf, usize)>,
    pub(super) styles: Vec<LineStyle>,
}

impl AtlasList {
    pub(in crate::editor) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }
}

impl Editor {
    pub(super) fn atlas_list_index(&self, kind: &ListKind) -> Option<usize> {
        self.buffers
            .find_mode(|mode| matches!(mode, Mode::AtlasList(list) if list.kind.same(kind)))
    }

    pub(super) fn atlas_list_kind(&self, index: usize) -> Option<ListKind> {
        match self.buffers.mode(index)? {
            Mode::AtlasList(list) => Some(list.kind.clone()),
            _ => None,
        }
    }

    pub(super) fn show_atlas_list(&mut self, kind: &ListKind) {
        if let Some(index) = self.atlas_list_index(kind) {
            if let Some(Mode::AtlasList(list)) = self.buffers.mode_mut(index) {
                list.kind = kind.clone();
            }
        } else {
            let mut buffer = Buffer::with_text(kind.name(), "");
            buffer.set_read_only(true);
            let list = AtlasList {
                kind: kind.clone(),
                rows: Vec::new(),
                styles: Vec::new(),
            };
            self.beside(|editor| {
                editor.add_buffer_with(buffer, Mode::AtlasList(Box::new(list)));
            });
        }
        self.render_atlas_list(kind);
        self.show_beside(|editor| editor.atlas_list_index(kind));
    }

    pub(in crate::editor) fn refresh_atlas_list(&mut self) {
        if let Some(kind) = self.atlas_list_kind(0) {
            self.render_atlas_list(&kind);
        }
    }

    pub(super) fn render_atlas_list(&mut self, kind: &ListKind) {
        match kind {
            ListKind::Backlinks(_) => self.render_backlinks(),
            ListKind::Agenda(_) => self.render_agenda(),
        }
    }

    pub(super) fn atlas_row(&self) -> Option<(PathBuf, usize)> {
        let line = self.point_line();
        match self.buffers.mode(0)? {
            Mode::AtlasList(list) => list
                .rows
                .iter()
                .find(|(row, ..)| *row == line)
                .map(|(_, path, target)| (path.clone(), *target)),
            _ => None,
        }
    }

    pub(in crate::editor) fn atlas_visit(&mut self) {
        let Some((path, target)) = self.atlas_row() else {
            self.show_message("Nothing to visit on this line");
            return;
        };
        let origin = self.current_location();
        if self.open_file(&path) {
            self.remember_location(origin);
            self.go_to_line(target + 1, 1);
        }
    }
}
