use std::path::PathBuf;

use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::Page;
use oxmacs_core::text::BufferId;

use crate::notes::Notebook;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Month {
    pub(crate) selected: jiff::civil::Date,
    pub(crate) week: bool,
    pub(crate) filter: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ListKind {
    Backlinks(String),
    Agenda(Option<String>),
    Calendar(Month),
}

impl ListKind {
    fn name(&self) -> &'static str {
        match self {
            Self::Backlinks(_) => "*Backlinks*",
            Self::Agenda(_) => "*Agenda*",
            Self::Calendar(_) => "*Calendar*",
        }
    }

    fn same(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

pub(crate) type Rows = Vec<(usize, PathBuf, usize)>;

#[derive(Debug)]
pub(crate) struct ListView {
    scope: ScopeId,
    pub(crate) kind: ListKind,
    pub(crate) rows: Rows,
    styles: Vec<LineStyle>,
    width: Option<usize>,
}

impl AppletBuffer for ListView {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }
}

impl Notebook<'_> {
    pub(crate) fn list_id(&self, kind: &ListKind) -> Option<BufferId> {
        self.find_applet_buffer::<ListView>(|view| view.kind.same(kind))
    }

    pub(crate) fn list_kind(&self, id: BufferId) -> Option<ListKind> {
        self.applet_buffer::<ListView>(id)
            .map(|(_, view)| view.kind.clone())
    }

    pub(crate) fn current_kind(&self) -> Option<ListKind> {
        self.list_kind(self.buffer().id())
    }

    pub(crate) fn set_kind(&mut self, id: BufferId, kind: ListKind) {
        if let Some((_, view)) = self.applet_buffer_mut::<ListView>(id) {
            view.kind = kind;
        }
    }

    pub(crate) fn show_list(&mut self, kind: &ListKind) {
        let id = if let Some(id) = self.list_id(kind) {
            self.set_kind(id, kind.clone());
            id
        } else {
            let scope = if matches!(kind, ListKind::Calendar(_)) {
                self.atlas.calendar_scope
            } else {
                self.atlas.list_scope
            };
            let kind = kind.clone();
            self.add_applet_buffer_with(kind.name(), |buffer| {
                buffer.set_read_only(true);
                ListView {
                    scope,
                    kind,
                    rows: Vec::new(),
                    styles: Vec::new(),
                    width: None,
                }
            })
        };
        self.show_buffer_beside(id);
        self.render_list(kind);
    }

    pub(crate) fn refresh_list(&mut self) {
        if let Some(kind) = self.current_kind() {
            self.render_list(&kind);
        }
    }

    pub(crate) fn render_list(&mut self, kind: &ListKind) {
        match kind {
            ListKind::Backlinks(_) => self.render_backlinks(),
            ListKind::Agenda(_) => self.render_agenda(),
            ListKind::Calendar(_) => self.render_calendar(),
        }
    }

    pub(crate) fn fill_list(&mut self, id: BufferId, page: Page, rows: Rows) {
        let width = self.buffer_width(id);
        if let Some((buffer, view)) = self.applet_buffer_mut::<ListView>(id) {
            page.fill(buffer);
            view.rows = rows;
            view.styles = page.into_parts().1;
            view.width = width;
        }
    }

    pub(crate) fn refit_lists(&mut self) {
        for id in self.applet_buffers::<ListView>() {
            let width = self.buffer_width(id);
            let stale = self
                .applet_buffer::<ListView>(id)
                .filter(|(_, view)| width.is_some() && view.width != width)
                .map(|(_, view)| view.kind.clone());
            if let Some(kind) = stale {
                self.render_list(&kind);
            }
        }
    }

    pub(crate) fn row(&self) -> Option<(PathBuf, usize)> {
        let buffer = self.buffer();
        let line = buffer.line_of(buffer.point());
        let (_, view) = self.applet_buffer::<ListView>(buffer.id())?;
        view.rows
            .iter()
            .find(|(row, ..)| *row == line)
            .map(|(_, path, target)| (path.clone(), *target))
    }

    pub(crate) fn visit(&mut self) {
        let Some((path, target)) = self.row() else {
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
