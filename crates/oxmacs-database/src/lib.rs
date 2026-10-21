mod driver;
mod grid;
mod open;
mod service;
mod table;
mod text;
mod tree;
mod widget;
mod worker;

use iced::{Element, Task};
use oxmacs_core::applet::{Applet, Payload};
use oxmacs_core::command::{Command, CommandId, CommandSpec, Scope, ScopeId};
use oxmacs_core::editor::{Editor, Pane, WindowId};
use oxmacs_gui::applet::{Context, GuiApplet};

pub use driver::{
    Column, Driver, ForeignKey, Index, Kind, Page, Query, Schema, Sqlite, Table, Value,
};
pub use service::{DbId, Reply, Request, Service, Source, answer};
pub use table::{GridClick, GridScroll, GridViewport};

use open::DatabaseState;
use table::Motion;

struct Spec {
    name: &'static str,
    summary: &'static str,
    help: Option<&'static str>,
    keys: &'static [&'static str],
    run: fn(&mut Editor),
}

const GLOBAL: &[Spec] = &[
    Spec {
        name: "database-open",
        summary: "Open a SQLite database",
        help: Some(
            "Recently opened databases come first; Another database file… asks for any file. Visiting a .db, .sqlite, .sqlite3 or .db3 file opens it the same way. The database shows its tables, views and indexes: n and p move between them, TAB shows a table's columns or folds a group, and g reads the schema again.",
        ),
        keys: &[],
        run: open::prompt_open,
    },
    Spec {
        name: "database-create",
        summary: "Create a new SQLite database file and open it",
        help: None,
        keys: &[],
        run: open::prompt_create,
    },
    Spec {
        name: "database-scratch",
        summary: "Open a throwaway database that lives in memory",
        help: None,
        keys: &[],
        run: open::scratch,
    },
];

const TREE: &[Spec] = &[
    Spec {
        name: "database-next",
        summary: "Move to the next group, table or index",
        help: None,
        keys: &["n"],
        run: |editor| open::step(editor, true),
    },
    Spec {
        name: "database-previous",
        summary: "Move to the previous group, table or index",
        help: None,
        keys: &["p"],
        run: |editor| open::step(editor, false),
    },
    Spec {
        name: "database-toggle",
        summary: "Show or hide a table's columns, or fold a group",
        help: None,
        keys: &["TAB"],
        run: open::toggle,
    },
    Spec {
        name: "database-reload",
        summary: "Read the schema again",
        help: None,
        keys: &["g"],
        run: open::reload,
    },
    Spec {
        name: "database-show-table",
        summary: "Show the rows of the table on this line",
        help: None,
        keys: &["RET"],
        run: table::open_from_tree,
    },
];

const GRID: &[Spec] = &[
    Spec {
        name: "database-next-row",
        summary: "Move to the next row",
        help: None,
        keys: &["C-n", "<down>"],
        run: |editor| table::motion(editor, Motion::Down),
    },
    Spec {
        name: "database-previous-row",
        summary: "Move to the previous row",
        help: None,
        keys: &["C-p", "<up>"],
        run: |editor| table::motion(editor, Motion::Up),
    },
    Spec {
        name: "database-next-column",
        summary: "Move to the next column",
        help: None,
        keys: &["C-f", "<right>"],
        run: |editor| table::motion(editor, Motion::Right),
    },
    Spec {
        name: "database-previous-column",
        summary: "Move to the previous column",
        help: None,
        keys: &["C-b", "<left>"],
        run: |editor| table::motion(editor, Motion::Left),
    },
    Spec {
        name: "database-next-cell",
        summary: "Move to the next cell, wrapping to the next row",
        help: None,
        keys: &["TAB"],
        run: |editor| table::motion(editor, Motion::Next),
    },
    Spec {
        name: "database-previous-cell",
        summary: "Move to the previous cell, wrapping to the previous row",
        help: None,
        keys: &["S-TAB"],
        run: |editor| table::motion(editor, Motion::Previous),
    },
    Spec {
        name: "database-first-column",
        summary: "Move to the first column",
        help: None,
        keys: &["C-a", "<home>"],
        run: |editor| table::motion(editor, Motion::RowStart),
    },
    Spec {
        name: "database-last-column",
        summary: "Move to the last column",
        help: None,
        keys: &["C-e", "<end>"],
        run: |editor| table::motion(editor, Motion::RowEnd),
    },
    Spec {
        name: "database-first-row",
        summary: "Move to the first row",
        help: None,
        keys: &["M-<", "C-<home>"],
        run: |editor| table::motion(editor, Motion::First),
    },
    Spec {
        name: "database-last-row",
        summary: "Move to the last row",
        help: None,
        keys: &["M->", "C-<end>"],
        run: |editor| table::motion(editor, Motion::Last),
    },
    Spec {
        name: "database-page-down",
        summary: "Move down by a windowful of rows",
        help: None,
        keys: &["C-v", "<next>"],
        run: |editor| table::motion(editor, Motion::PageDown),
    },
    Spec {
        name: "database-page-up",
        summary: "Move up by a windowful of rows",
        help: None,
        keys: &["M-v", "<prior>"],
        run: |editor| table::motion(editor, Motion::PageUp),
    },
    Spec {
        name: "database-select",
        summary: "Start selecting cells from here",
        help: None,
        keys: &["C-SPC"],
        run: table::select,
    },
    Spec {
        name: "database-unselect",
        summary: "Stop selecting cells",
        help: None,
        keys: &["C-g"],
        run: table::unselect,
    },
    Spec {
        name: "database-copy",
        summary: "Copy the selected cells, or this one, as tab-separated text",
        help: None,
        keys: &["M-w"],
        run: table::copy,
    },
    Spec {
        name: "database-refresh",
        summary: "Read the rows again",
        help: None,
        keys: &["g"],
        run: table::refresh,
    },
    Spec {
        name: "database-sort",
        summary: "Sort by this column alone, cycling up, down and off",
        help: None,
        keys: &["s"],
        run: |editor| table::sort(editor, false),
    },
    Spec {
        name: "database-sort-also",
        summary: "Sort by this column after the others, cycling up, down and off",
        help: None,
        keys: &["S"],
        run: |editor| table::sort(editor, true),
    },
    Spec {
        name: "database-filter-equal",
        summary: "Keep only the rows with this cell's value",
        help: None,
        keys: &["="],
        run: |editor| table::filter_cell(editor, false),
    },
    Spec {
        name: "database-filter-different",
        summary: "Keep only the rows without this cell's value",
        help: None,
        keys: &["!"],
        run: |editor| table::filter_cell(editor, true),
    },
];

fn register(editor: &mut Editor, scope: ScopeId, specs: &[Spec]) -> Vec<CommandId> {
    let mut added = Vec::new();
    for spec in specs {
        let command = editor.add_command(CommandSpec {
            name: spec.name,
            summary: spec.summary,
            help: spec.help,
            scope,
            run: spec.run,
        });
        for keys in spec.keys {
            editor.bind(scope, keys, command).ok();
        }
        added.push(command);
    }
    added
}

#[derive(Debug, Default)]
pub struct Database {
    workers: worker::Workers,
}

impl Applet for Database {
    fn name(&self) -> &'static str {
        "database"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn opt_in(&self) -> bool {
        true
    }

    fn install(&self, editor: &mut Editor) {
        let tree_scope = editor.add_scope("database schema");
        let grid_scope = editor.add_scope("database table");
        editor.set_applet_state(DatabaseState {
            tree_scope,
            grid_scope,
            next: 0,
            open: Vec::new(),
            recent: Vec::new(),
        });
        let global = register(editor, Scope::Global.into(), GLOBAL);
        register(editor, tree_scope, TREE);
        register(editor, grid_scope, GRID);
        editor.bind(tree_scope, "q", Command::QuitWindow).ok();
        editor.bind(grid_scope, "q", Command::QuitWindow).ok();
        editor.add_file_opener(&["db", "sqlite", "sqlite3", "db3"], open::open_file);
        editor.on_event(open::replied);
        editor.on_event(table::viewport);
        editor.on_event(table::clicked);
        editor.on_event(table::scrolled);
        editor.on_command(open::close_unused);
        if let Some(command) = global.first() {
            editor.set_splash_entry(*command, "Open a database");
        }
    }
}

impl GuiApplet for Database {
    fn perform(&mut self, payload: &Payload) -> Option<Task<Payload>> {
        let request = payload.get::<Request>()?.clone();
        Some(self.workers.run(request))
    }

    fn body<'a>(
        &'a self,
        _editor: &'a Editor,
        pane: Pane<'a>,
        _window: WindowId,
        context: &Context,
    ) -> Option<Element<'a, Payload>> {
        let view = pane.applet_buffer::<table::GridView>()?;
        Some(
            widget::GridWidget::new(
                &view.grid,
                pane.buffer().id(),
                context.faces,
                context.font,
                context.size,
                pane.is_selected(),
            )
            .into(),
        )
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::path::Path;

    use oxmacs_core::applet::Payload;
    use oxmacs_core::editor::{Editor, Effect};
    use oxmacs_core::input::KeySequence;

    use crate::Database;
    use crate::service::{Request, Service};

    pub(crate) fn installed(directory: &Path) -> Editor {
        let mut editor = Editor::new();
        editor.install(&Database::default());
        editor.set_home_directory(None);
        editor.set_working_directory(directory.to_owned());
        editor
    }

    pub(crate) fn press(editor: &mut Editor, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
    }

    pub(crate) fn type_text(editor: &mut Editor, text: &str) {
        for character in text.chars() {
            let notation = match character {
                ' ' => "SPC".to_owned(),
                other => other.to_string(),
            };
            press(editor, &notation);
        }
    }

    pub(crate) fn requests(editor: &mut Editor) -> Vec<Request> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Applet(payload) => payload.get::<Request>().cloned(),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn pump(editor: &mut Editor, service: &mut Service) {
        loop {
            let asked = requests(editor);
            if asked.is_empty() {
                return;
            }
            for request in asked {
                let reply = service.handle(request);
                editor.deliver(&Payload::new(reply));
            }
        }
    }

    pub(crate) fn lines(editor: &Editor) -> Vec<String> {
        editor
            .buffer()
            .contents()
            .split('\n')
            .map(str::to_owned)
            .collect()
    }

    pub(crate) fn at_point(editor: &Editor) -> String {
        let buffer = editor.buffer();
        let line = buffer.line_of(buffer.point());
        buffer.text_in(buffer.line_start(line)..buffer.line_end(line))
    }
}
