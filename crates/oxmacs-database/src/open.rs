use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::{Editor, Offer, Picked};
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;

use crate::driver::{Kind, Query};
use crate::service::{DbId, Reply, Request, Source};
use crate::text::Lines;
use crate::tree::{Folds, Group, Row, Shown, State, render};

#[derive(Debug)]
pub(crate) struct DatabaseState {
    pub(crate) tree_scope: ScopeId,
    pub(crate) grid_scope: ScopeId,
    pub(crate) next: u64,
    pub(crate) open: Vec<DbId>,
    pub(crate) recent: Vec<PathBuf>,
}

#[derive(Debug)]
pub(crate) struct TreeView {
    scope: ScopeId,
    pub(crate) db: DbId,
    pub(crate) path: Option<PathBuf>,
    pub(crate) state: State,
    pub(crate) counts: HashMap<String, u64>,
    pub(crate) folds: Folds,
    pub(crate) lines: Lines<Row>,
}

impl AppletBuffer for TreeView {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn line_style(&self, line: usize) -> LineStyle {
        self.lines.style(line)
    }

    fn pills(&self, line: usize) -> Vec<(Range<usize>, Highlight)> {
        self.lines.pills(line)
    }

    fn directory(&self) -> Option<&Path> {
        self.path.as_deref().and_then(Path::parent)
    }

    fn fixed_name(&self) -> bool {
        true
    }
}

const RECENT: usize = 20;
const SCRATCH: &str = "scratch database";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Spot {
    Group(Group),
    Table(String),
    Column(String, String),
    Index(String),
}

fn spot(state: &State, row: Row) -> Option<Spot> {
    let State::Ready(schema) = state else {
        return None;
    };
    Some(match row {
        Row::Info | Row::Blank => return None,
        Row::Group(group) => Spot::Group(group),
        Row::Table(index) => Spot::Table(schema.tables.get(index)?.name.clone()),
        Row::Column(index, at) => {
            let table = schema.tables.get(index)?;
            Spot::Column(table.name.clone(), table.columns.get(at)?.name.clone())
        }
        Row::Index(index) => Spot::Index(schema.indexes.get(index)?.name.clone()),
    })
}

pub(crate) fn title(path: Option<&Path>) -> String {
    path.and_then(Path::file_name).map_or_else(
        || SCRATCH.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn place_of(path: Option<&Path>) -> String {
    path.map_or_else(|| "In memory".to_owned(), |path| path.display().to_string())
}

fn unique_name(editor: &Editor, path: Option<&Path>) -> String {
    let base = title(path);
    if editor.buffer_named(&base).is_none() {
        return base;
    }
    let parent = path
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut name = format!("{base} <{parent}>");
    let mut number = 2;
    while editor.buffer_named(&name).is_some() {
        name = format!("{base} <{number}>");
        number += 1;
    }
    name
}

pub(crate) fn tree_of(editor: &Editor, db: DbId) -> Option<BufferId> {
    editor.find_applet_buffer::<TreeView>(|view| view.db == db)
}

fn start(editor: &mut Editor, source: Source, path: Option<PathBuf>) {
    let Some((scope, db)) = editor.applet_state_mut::<DatabaseState>().map(|state| {
        state.next += 1;
        let db = DbId(state.next);
        state.open.push(db);
        if let Some(path) = &path {
            state.recent.retain(|known| known != path);
            state.recent.insert(0, path.clone());
            state.recent.truncate(RECENT);
        }
        (state.tree_scope, db)
    }) else {
        return;
    };
    let name = unique_name(editor, path.as_deref());
    let id = editor.add_applet_buffer_with(&name, |buffer| {
        buffer.set_read_only(true);
        TreeView {
            scope,
            db,
            path,
            state: State::Opening,
            counts: HashMap::new(),
            folds: Folds::default(),
            lines: Lines::default(),
        }
    });
    editor.show_buffer(id);
    redraw(editor, id, None);
    editor.request(Request::Open { db, source });
}

fn redraw(editor: &mut Editor, id: BufferId, focus: Option<Spot>) {
    let Some((buffer, view)) = editor.applet_buffer_mut::<TreeView>(id) else {
        return;
    };
    let line = buffer.line_of(buffer.point());
    let focus = focus.or_else(|| {
        view.lines
            .rows
            .get(line)
            .and_then(|row| spot(&view.state, *row))
    });
    let name = title(view.path.as_deref());
    let place = place_of(view.path.as_deref());
    view.lines = render(&Shown {
        name: &name,
        place: &place,
        state: &view.state,
        counts: &view.counts,
        folds: &view.folds,
    });
    buffer.set_text(&view.lines.text);
    let rows = &view.lines.rows;
    let found = focus.and_then(|focus| {
        rows.iter()
            .position(|row| spot(&view.state, *row).as_ref() == Some(&focus))
    });
    let target = found.unwrap_or_else(|| line.min(rows.len().saturating_sub(1)));
    let start = buffer.line_start(target);
    buffer.set_point(start);
}

fn first_table(state: &State) -> Option<Spot> {
    let State::Ready(schema) = state else {
        return None;
    };
    schema
        .tables
        .iter()
        .find(|table| table.kind == Kind::Table)
        .or_else(|| schema.tables.first())
        .map(|table| Spot::Table(table.name.clone()))
}

fn set_state(editor: &mut Editor, db: DbId, state: State) {
    let Some(id) = tree_of(editor, db) else {
        return;
    };
    let mut focus = None;
    if let Some((_, view)) = editor.applet_buffer_mut::<TreeView>(id) {
        if view.state == State::Opening {
            focus = first_table(&state);
        }
        view.state = state;
    }
    redraw(editor, id, focus);
}

pub(crate) fn open_file(editor: &mut Editor, path: &Path) -> bool {
    let path = editor.absolute(path);
    if let Some(id) =
        editor.find_applet_buffer::<TreeView>(|view| view.path.as_deref() == Some(&path))
    {
        editor.show_buffer(id);
        return true;
    }
    start(editor, Source::File(path.clone()), Some(path));
    true
}

fn ask_for_file(editor: &mut Editor) {
    editor.prompt_path("Open database: ", false, |editor, path| {
        open_file(editor, &path);
    });
}

pub(crate) fn prompt_open(editor: &mut Editor) {
    let recent = editor
        .applet_state::<DatabaseState>()
        .map(|state| state.recent.clone())
        .unwrap_or_default();
    if recent.is_empty() {
        ask_for_file(editor);
        return;
    }
    let offered = recent.clone();
    editor.prompt_pick(
        "Open database: ",
        move |_| {
            offered
                .iter()
                .map(|path| {
                    let folder = path
                        .parent()
                        .map(Path::display)
                        .map(|shown| shown.to_string());
                    Offer::noted(title(Some(path)), folder.unwrap_or_default())
                })
                .chain([Offer::from(ANOTHER)])
                .collect()
        },
        move |editor, picked| match picked {
            Picked::Offered(index) if index < recent.len() => {
                open_file(editor, &recent[index]);
            }
            Picked::Offered(_) => ask_for_file(editor),
            Picked::Typed(text) => {
                let path = editor.absolute(Path::new(&text));
                open_file(editor, &path);
            }
        },
    );
}

const ANOTHER: &str = "Another database file…";

pub(crate) fn prompt_create(editor: &mut Editor) {
    editor.prompt_path("Create database: ", false, |editor, path| {
        if path.exists() {
            editor.show_message(format!(
                "{} already exists; M-x database-open opens it",
                path.display()
            ));
            return;
        }
        start(editor, Source::Create(path.clone()), Some(path));
    });
}

pub(crate) fn scratch(editor: &mut Editor) {
    start(editor, Source::Memory, None);
}

fn current(editor: &Editor) -> Option<(BufferId, &TreeView)> {
    let id = editor.buffer().id();
    editor
        .applet_buffer::<TreeView>(id)
        .map(|(_, view)| (id, view))
}

fn item(row: Row) -> bool {
    matches!(row, Row::Group(_) | Row::Table(_) | Row::Index(_))
}

pub(crate) fn step(editor: &mut Editor, forward: bool) {
    let Some((_, view)) = current(editor) else {
        return;
    };
    let buffer = editor.buffer();
    let line = buffer.line_of(buffer.point());
    let rows = &view.lines.rows;
    let found = if forward {
        (line + 1..rows.len()).find(|at| item(rows[*at]))
    } else {
        (0..line).rev().find(|at| item(rows[*at]))
    };
    if let Some(found) = found {
        let buffer = editor.buffer_mut();
        let start = buffer.line_start(found);
        buffer.set_point(start);
    }
}

pub(crate) fn toggle(editor: &mut Editor) {
    let Some((id, view)) = current(editor) else {
        return;
    };
    let buffer = editor.buffer();
    let Some(row) = view.lines.rows.get(buffer.line_of(buffer.point())).copied() else {
        return;
    };
    let State::Ready(schema) = &view.state else {
        return;
    };
    let name = |index: usize| schema.tables.get(index).map(|table| table.name.clone());
    let change = match row {
        Row::Group(group) => Some((None, Some(group), Spot::Group(group))),
        Row::Table(index) => name(index).map(|name| (Some(name.clone()), None, Spot::Table(name))),
        Row::Column(index, _) => {
            name(index).map(|name| (Some(name.clone()), None, Spot::Table(name)))
        }
        Row::Info | Row::Blank | Row::Index(_) => None,
    };
    let Some((table, group, focus)) = change else {
        return;
    };
    if let Some((_, view)) = editor.applet_buffer_mut::<TreeView>(id) {
        if let Some(table) = table
            && !view.folds.expanded.remove(&table)
            && !matches!(row, Row::Column(..))
        {
            view.folds.expanded.insert(table);
        }
        if let Some(group) = group
            && !view.folds.closed.remove(&group)
        {
            view.folds.closed.insert(group);
        }
    }
    redraw(editor, id, Some(focus));
}

pub(crate) fn reload(editor: &mut Editor) {
    let Some((_, view)) = current(editor) else {
        return;
    };
    let db = view.db;
    editor.request(Request::Schema { db });
}

pub(crate) fn close_unused(editor: &mut Editor) {
    let open = editor
        .applet_state::<DatabaseState>()
        .map(|state| state.open.clone())
        .unwrap_or_default();
    for db in open {
        let tables = crate::table::grids(editor)
            .into_iter()
            .filter_map(|id| editor.applet_buffer::<crate::table::GridView>(id))
            .any(|(_, view)| view.grid.db == db);
        if tree_of(editor, db).is_some() || tables {
            continue;
        }
        if let Some(state) = editor.applet_state_mut::<DatabaseState>() {
            state.open.retain(|known| *known != db);
        }
        editor.request(Request::Close { db });
    }
}

pub(crate) fn replied(editor: &mut Editor, reply: &Reply) {
    match reply {
        Reply::Opened { db, name: Ok(_) } => editor.request(Request::Schema { db: *db }),
        Reply::Opened {
            db,
            name: Err(error),
        }
        | Reply::Schema {
            db,
            schema: Err(error),
        } => {
            set_state(editor, *db, State::Failed(error.clone()));
            editor.show_message(error.clone());
        }
        Reply::Schema {
            db,
            schema: Ok(schema),
        } => {
            for table in schema
                .tables
                .iter()
                .filter(|table| table.kind == Kind::Table)
            {
                editor.request(Request::Count {
                    db: *db,
                    query: Query {
                        table: table.name.clone(),
                        ..Query::default()
                    },
                });
            }
            set_state(editor, *db, State::Ready(schema.clone()));
        }
        Reply::Counted {
            db,
            query,
            count: Ok(count),
        } if query.sort.is_empty() && query.filters.is_empty() => {
            crate::table::replied(editor, reply);
            let Some(id) = tree_of(editor, *db) else {
                return;
            };
            if let Some((_, view)) = editor.applet_buffer_mut::<TreeView>(id) {
                view.counts.insert(query.table.clone(), *count);
            }
            redraw(editor, id, None);
        }
        Reply::Counted { .. } | Reply::Rows { .. } => crate::table::replied(editor, reply),
        Reply::Closed { .. } => {}
    }
}
#[cfg(test)]
mod tests {
    use std::fs;

    use oxmacs_core::editor::Editor;

    use crate::driver::fixtures::database;
    use crate::service::Request;
    use crate::testing::{at_point, installed, lines, press, pump, requests, type_text};

    fn opened() -> (tempfile::TempDir, Editor, crate::service::Service) {
        let (dir, path) = database();
        let mut editor = installed(dir.path());
        let mut service = crate::service::Service::default();
        assert!(editor.open_file(&path));
        pump(&mut editor, &mut service);
        (dir, editor, service)
    }

    #[test]
    fn visiting_a_database_file_shows_its_schema() {
        let (dir, editor, _service) = opened();

        assert_eq!(editor.buffer().name(), "music.db");
        assert!(editor.buffer().is_read_only());
        let shown = lines(&editor);
        assert_eq!(shown[1], dir.path().join("music.db").display().to_string());
        assert!(shown.contains(&"  ▸ albums      4".to_owned()), "{shown:?}");
        assert!(shown.contains(&"  ▸ artists     3".to_owned()), "{shown:?}");
        assert_eq!(at_point(&editor), "  ▸ albums      4");
        assert_eq!(editor.default_directory(), dir.path());
    }

    #[test]
    fn a_database_already_open_is_shown_again() {
        let (dir, mut editor, mut service) = opened();
        editor.open_file(&dir.path().join("music.db"));
        pump(&mut editor, &mut service);

        let trees = editor
            .buffer_names()
            .into_iter()
            .filter(|name| name.starts_with("music.db"))
            .count();
        assert_eq!(trees, 1);
    }

    #[test]
    fn files_that_are_not_databases_say_so() {
        let dir = tempfile::tempdir().unwrap();
        let junk = dir.path().join("junk.db");
        fs::write(&junk, "plain text, certainly not SQLite").unwrap();
        let mut editor = installed(dir.path());
        let mut service = crate::service::Service::default();

        editor.open_file(&junk);
        pump(&mut editor, &mut service);

        let shown = lines(&editor);
        assert!(shown[3].contains("not a database"), "{shown:?}");
        assert!(
            editor
                .message()
                .unwrap_or_default()
                .contains("not a database")
        );
    }

    #[test]
    fn scratch_databases_live_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = installed(dir.path());
        let mut service = crate::service::Service::default();

        press(&mut editor, "M-x");
        type_text(&mut editor, "database-scratch");
        press(&mut editor, "RET");
        pump(&mut editor, &mut service);

        assert_eq!(editor.buffer().name(), "scratch database");
        let shown = lines(&editor);
        assert_eq!(shown[1], "In memory");
        assert_eq!(shown[3], "No tables yet");
    }

    #[test]
    fn database_create_makes_a_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = installed(dir.path());
        let mut service = crate::service::Service::default();

        press(&mut editor, "M-x");
        type_text(&mut editor, "database-create");
        press(&mut editor, "RET");
        type_text(&mut editor, "fresh.db");
        press(&mut editor, "RET");
        pump(&mut editor, &mut service);

        assert!(dir.path().join("fresh.db").exists());
        assert_eq!(editor.buffer().name(), "fresh.db");
        assert_eq!(lines(&editor)[3], "No tables yet");
    }

    #[test]
    fn database_open_offers_recent_databases_first() {
        let (dir, mut editor, mut service) = opened();
        editor.show_buffer(editor.buffer_named("*scratch*").unwrap());

        press(&mut editor, "M-x");
        type_text(&mut editor, "database-open");
        press(&mut editor, "RET");
        let offered: Vec<String> = editor
            .completions()
            .into_iter()
            .map(|completion| completion.label)
            .collect();
        assert_eq!(offered, ["music.db", "Another database file…"]);
        press(&mut editor, "RET");
        pump(&mut editor, &mut service);

        assert_eq!(editor.buffer().name(), "music.db");
        let _ = dir;
    }

    #[test]
    fn database_open_without_recents_asks_for_a_file() {
        let (dir, path) = database();
        let mut editor = installed(dir.path());
        let mut service = crate::service::Service::default();

        press(&mut editor, "M-x");
        type_text(&mut editor, "database-open");
        press(&mut editor, "RET");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Open database: ");
        type_text(&mut editor, "music.db");
        press(&mut editor, "RET");
        pump(&mut editor, &mut service);

        assert_eq!(editor.buffer().name(), "music.db");
        let _ = path;
    }

    #[test]
    fn n_and_p_move_between_groups_tables_and_indexes() {
        let (_dir, mut editor, _service) = opened();

        for (keys, expected) in [
            ("n", "  ▸ artists     3"),
            ("n n", "  ▸ tags        2"),
            ("n", "▾ Views (1)"),
            ("p p p p p", "▾ Tables (4)"),
        ] {
            press(&mut editor, keys);
            assert_eq!(at_point(&editor), expected, "{keys}");
        }
    }

    #[test]
    fn tab_shows_a_tables_columns_and_folds_groups() {
        let (_dir, mut editor, _service) = opened();

        press(&mut editor, "TAB");
        assert_eq!(lines(&editor)[5], "      id         INTEGER  PK");
        assert_eq!(at_point(&editor), "  ▾ albums      4");

        press(&mut editor, "C-n C-n TAB");
        assert_eq!(at_point(&editor), "  ▸ albums      4");
        assert_eq!(lines(&editor)[5], "  ▸ artists     3");

        press(&mut editor, "p TAB");
        assert_eq!(at_point(&editor), "▸ Tables (4)");
        assert_eq!(lines(&editor)[4], "");
    }

    #[test]
    fn g_reads_the_schema_again() {
        let (dir, mut editor, mut service) = opened();
        let connection = rusqlite::Connection::open(dir.path().join("music.db")).unwrap();
        connection
            .execute_batch("CREATE TABLE genres (id INTEGER PRIMARY KEY)")
            .unwrap();

        press(&mut editor, "g");
        pump(&mut editor, &mut service);

        assert!(lines(&editor).contains(&"  ▸ genres      0".to_owned()));
        assert_eq!(at_point(&editor), "  ▸ albums      4");
    }

    #[test]
    fn killing_the_buffer_closes_the_connection() {
        let (_dir, mut editor, _service) = opened();

        press(&mut editor, "C-x k RET");

        assert!(
            requests(&mut editor)
                .iter()
                .any(|request| matches!(request, Request::Close { .. }))
        );
    }
}
