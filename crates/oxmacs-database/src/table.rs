use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::editor::{Editor, Tiling, WindowId};
use oxmacs_core::text::BufferId;

use crate::driver::Query;
use crate::grid::{Cell, Grid, PAGE};
use crate::open::{DatabaseState, TreeView};
use crate::service::{DbId, Reply, Request};
use crate::text::thousands;
use crate::tree::{Row, State};

#[derive(Debug)]
pub(crate) struct GridView {
    scope: ScopeId,
    pub(crate) grid: Grid,
    pub(crate) counting: bool,
}

impl AppletBuffer for GridView {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn position(&self) -> Option<String> {
        let total = self.grid.total.map_or_else(|| "…".to_owned(), thousands);
        Some(format!("{}/{total}", thousands(self.grid.cursor.row + 1)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Motion {
    Up,
    Down,
    Left,
    Right,
    Next,
    Previous,
    RowStart,
    RowEnd,
    First,
    Last,
    PageDown,
    PageUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridViewport {
    pub buffer: BufferId,
    pub rows: usize,
    pub width: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridClick {
    pub buffer: BufferId,
    pub row: u64,
    pub column: usize,
    pub extend: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridScroll {
    pub buffer: BufferId,
    pub rows: i64,
    pub columns: i64,
}

const TREE_SHARE: f32 = 0.28;

pub(crate) fn grids(editor: &Editor) -> Vec<BufferId> {
    editor.applet_buffers::<GridView>()
}

fn current(editor: &Editor) -> Option<BufferId> {
    let id = editor.buffer().id();
    editor.applet_buffer::<GridView>(id).map(|_| id)
}

fn load(editor: &mut Editor, id: BufferId) {
    let Some((_, view)) = editor.applet_buffer_mut::<GridView>(id) else {
        return;
    };
    let db = view.grid.db;
    let query = view.grid.query.clone();
    let wanted = view.grid.wanted();
    for page in &wanted {
        view.grid.asked(*page);
    }
    let count = view.grid.total.is_none() && !view.counting;
    if count {
        view.counting = true;
    }
    for page in wanted {
        editor.request(Request::Rows {
            db,
            query: query.clone(),
            offset: page * PAGE,
            limit: PAGE,
        });
    }
    if count {
        editor.request(Request::Count { db, query });
    }
}

fn change(editor: &mut Editor, id: BufferId, work: impl FnOnce(&mut Grid)) {
    if let Some((_, view)) = editor.applet_buffer_mut::<GridView>(id) {
        work(&mut view.grid);
    }
    load(editor, id);
}

pub(crate) fn open_from_tree(editor: &mut Editor) {
    let id = editor.buffer().id();
    let Some((buffer, view)) = editor.applet_buffer::<TreeView>(id) else {
        return;
    };
    let line = buffer.line_of(buffer.point());
    let State::Ready(schema) = &view.state else {
        return;
    };
    let name = |index: usize| schema.tables.get(index).map(|table| table.name.clone());
    let picked = match view.lines.rows.get(line) {
        Some(Row::Table(index)) => name(*index).map(|table| (table, 0)),
        Some(Row::Column(index, at)) => name(*index).map(|table| (table, *at)),
        Some(Row::Index(index)) => schema
            .indexes
            .get(*index)
            .map(|found| (found.table.clone(), 0)),
        Some(Row::Group(_)) => {
            crate::open::toggle(editor);
            return;
        }
        _ => None,
    };
    let Some((table, column)) = picked else {
        return;
    };
    let db = view.db;
    let title = crate::open::title(view.path.as_deref());
    open_table(editor, db, &title, &table, column);
}

fn window_with_table(editor: &Editor, db: DbId) -> Option<WindowId> {
    editor.tiling().windows().into_iter().find(|window| {
        editor
            .pane_of(*window)
            .and_then(|pane| pane.applet_buffer::<GridView>())
            .is_some_and(|view| view.grid.db == db)
    })
}

fn open_table(editor: &mut Editor, db: DbId, title: &str, table: &str, column: usize) {
    let existing = editor.find_applet_buffer::<GridView>(|view| {
        view.grid.db == db && view.grid.query.table == table
    });
    let id = if let Some(id) = existing {
        id
    } else {
        let Some(scope) = editor
            .applet_state::<DatabaseState>()
            .map(|state| state.grid_scope)
        else {
            return;
        };
        let query = Query {
            table: table.to_owned(),
            ..Query::default()
        };
        editor.add_applet_buffer_with(&format!("{title}: {table}"), |buffer| {
            buffer.set_read_only(true);
            GridView {
                scope,
                grid: Grid::new(db, query),
                counting: false,
            }
        })
    };
    if let Some(window) = window_with_table(editor, db) {
        editor.select_window(window);
        editor.show_buffer(id);
    } else {
        editor.show_buffer_beside(id);
        if matches!(editor.tiling(), Tiling::Split { parts, .. } if parts.len() == 2) {
            editor.drag_divider(&[], 0, TREE_SHARE);
        }
    }
    if let Some((_, view)) = editor.applet_buffer_mut::<GridView>(id) {
        view.grid.cursor.column = column;
    }
    change(editor, id, |grid| {
        if !grid.columns.is_empty() {
            grid.go(grid.cursor);
        }
    });
}

pub(crate) fn motion(editor: &mut Editor, motion: Motion) {
    let Some(id) = current(editor) else {
        return;
    };
    change(editor, id, |grid| {
        let rows = i64::try_from(grid.shown_rows).unwrap_or(1);
        let Cell { row, column } = grid.cursor;
        match motion {
            Motion::Up => grid.shift(-1, 0),
            Motion::Down => grid.shift(1, 0),
            Motion::Left => grid.shift(0, -1),
            Motion::Right => grid.shift(0, 1),
            Motion::Next => grid.next_cell(true),
            Motion::Previous => grid.next_cell(false),
            Motion::RowStart => grid.go(Cell { row, column: 0 }),
            Motion::RowEnd => grid.go(Cell {
                row,
                column: usize::MAX,
            }),
            Motion::First => grid.go(Cell { row: 0, column }),
            Motion::Last => grid.go(Cell {
                row: u64::MAX,
                column,
            }),
            Motion::PageDown => grid.shift(rows, 0),
            Motion::PageUp => grid.shift(-rows, 0),
        }
    });
}

pub(crate) fn select(editor: &mut Editor) {
    let Some(id) = current(editor) else {
        return;
    };
    change(editor, id, |grid| grid.anchor = Some(grid.cursor));
    editor.show_message("Mark set");
}

pub(crate) fn unselect(editor: &mut Editor) {
    let Some(id) = current(editor) else {
        return;
    };
    change(editor, id, |grid| grid.anchor = None);
    editor.show_message("Quit");
}

pub(crate) fn copy(editor: &mut Editor) {
    let Some(id) = current(editor) else {
        return;
    };
    let Some((_, view)) = editor.applet_buffer_mut::<GridView>(id) else {
        return;
    };
    let text = view.grid.copied();
    view.grid.anchor = None;
    editor.copy_text(text);
}

fn requery(editor: &mut Editor, id: BufferId, work: impl FnOnce(&mut Grid) -> bool) {
    let Some((_, view)) = editor.applet_buffer_mut::<GridView>(id) else {
        return;
    };
    if work(&mut view.grid) {
        view.counting = false;
        load(editor, id);
    }
}

pub(crate) fn sort(editor: &mut Editor, add: bool) {
    let Some(id) = current(editor) else {
        return;
    };
    requery(editor, id, |grid| grid.sort(grid.cursor.column, add));
}

pub(crate) fn filter_cell(editor: &mut Editor, negate: bool) {
    let Some(id) = current(editor) else {
        return;
    };
    let condition = editor
        .applet_buffer::<GridView>(id)
        .and_then(|(_, view)| view.grid.condition(negate));
    let Some(condition) = condition else {
        editor.show_message("This row has not loaded yet");
        return;
    };
    requery(editor, id, |grid| grid.filter(&condition));
}

pub(crate) fn refresh(editor: &mut Editor) {
    let Some(id) = current(editor) else {
        return;
    };
    if let Some((_, view)) = editor.applet_buffer_mut::<GridView>(id) {
        view.grid.reset();
        view.counting = false;
    }
    load(editor, id);
}

pub(crate) fn viewport(editor: &mut Editor, event: &GridViewport) {
    change(editor, event.buffer, |grid| {
        grid.resize(event.rows, event.width);
    });
}

pub(crate) fn clicked(editor: &mut Editor, event: &GridClick) {
    change(editor, event.buffer, |grid| {
        if event.extend {
            grid.anchor.get_or_insert(grid.cursor);
        } else {
            grid.anchor = None;
        }
        grid.go(Cell {
            row: event.row,
            column: event.column,
        });
    });
}

pub(crate) fn scrolled(editor: &mut Editor, event: &GridScroll) {
    change(editor, event.buffer, |grid| {
        grid.scroll(event.rows, event.columns);
    });
}

fn matching(editor: &Editor, db: DbId, query: &Query) -> Vec<BufferId> {
    grids(editor)
        .into_iter()
        .filter(|id| {
            editor
                .applet_buffer::<GridView>(*id)
                .is_some_and(|(_, view)| view.grid.db == db && &view.grid.query == query)
        })
        .collect()
}

pub(crate) fn replied(editor: &mut Editor, reply: &Reply) {
    match reply {
        Reply::Counted { db, query, count } => {
            for id in matching(editor, *db, query) {
                if let Some((_, view)) = editor.applet_buffer_mut::<GridView>(id) {
                    view.counting = false;
                    match count {
                        Ok(count) => {
                            view.grid.total = Some(*count);
                            if !view.grid.columns.is_empty() {
                                view.grid.clamp();
                            }
                        }
                        Err(error) => view.grid.error = Some(error.clone()),
                    }
                }
                load(editor, id);
            }
        }
        Reply::Rows {
            db,
            query,
            offset,
            page,
        } => {
            for id in matching(editor, *db, query) {
                let Some((_, view)) = editor.applet_buffer_mut::<GridView>(id) else {
                    continue;
                };
                match page {
                    Ok(page) => {
                        view.grid.insert(*offset, page.clone());
                        if view.grid.total.is_some() {
                            view.grid.clamp();
                        }
                    }
                    Err(error) => {
                        view.grid.pending.remove(&(offset / PAGE));
                        view.grid.error = Some(error.clone());
                        editor.show_message(error.clone());
                    }
                }
            }
        }
        Reply::Opened { .. } | Reply::Schema { .. } | Reply::Closed { .. } => {}
    }
}
#[cfg(test)]
mod tests {
    use std::path::Path;

    use oxmacs_core::applet::Payload;
    use oxmacs_core::editor::{Editor, Effect};

    use super::*;
    use crate::driver::Value;
    use crate::driver::fixtures::database;
    use crate::service::{Request, Service};
    use crate::testing::{installed, press, pump, requests};

    fn with_numbers(path: &Path) {
        let connection = rusqlite::Connection::open(path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE numbers (n INTEGER PRIMARY KEY, label TEXT);
                 WITH RECURSIVE count(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM count WHERE n < 1000)
                 INSERT INTO numbers SELECT n, 'row ' || n FROM count;",
            )
            .unwrap();
    }

    fn opened() -> (tempfile::TempDir, Editor, Service) {
        let (dir, path) = database();
        with_numbers(&path);
        let mut editor = installed(dir.path());
        let mut service = Service::default();
        editor.open_file(&path);
        pump(&mut editor, &mut service);
        (dir, editor, service)
    }

    fn view(editor: &Editor) -> &Grid {
        let id = editor.buffer().id();
        &editor.applet_buffer::<GridView>(id).unwrap().1.grid
    }

    fn table(editor: &mut Editor, service: &mut Service, keys: &str) {
        if !keys.is_empty() {
            press(editor, keys);
        }
        press(editor, "RET");
        pump(editor, service);
    }

    #[test]
    fn return_on_a_table_opens_its_rows_beside_the_tree() {
        let (_dir, mut editor, mut service) = opened();

        table(&mut editor, &mut service, "");

        assert_eq!(editor.buffer().name(), "music.db: albums");
        assert_eq!(editor.tiling().windows().len(), 2);
        let grid = view(&editor);
        assert_eq!(grid.columns, ["id", "title", "artist_id", "cover", "price"]);
        assert_eq!(grid.total, Some(4));
        assert_eq!(
            grid.value(0, 1),
            Some(&Value::Text("For Those About To Rock".to_owned()))
        );
        assert_eq!(editor.pane().status().position.as_deref(), Some("1/4"));
    }

    #[test]
    fn another_table_reuses_the_table_window() {
        let (_dir, mut editor, mut service) = opened();
        table(&mut editor, &mut service, "");
        press(&mut editor, "C-x o");

        table(&mut editor, &mut service, "n");

        assert_eq!(editor.buffer().name(), "music.db: artists");
        assert_eq!(editor.tiling().windows().len(), 2);
    }

    #[test]
    fn return_on_a_column_starts_at_that_column() {
        let (_dir, mut editor, mut service) = opened();

        table(&mut editor, &mut service, "TAB C-n C-n");

        assert_eq!(view(&editor).cursor, Cell { row: 0, column: 1 });
    }

    #[test]
    fn keys_move_the_cursor_through_the_rows() {
        let (_dir, mut editor, mut service) = opened();
        table(&mut editor, &mut service, "n n");
        assert_eq!(editor.buffer().name(), "music.db: numbers");

        for (keys, row, column) in [
            ("C-n", 1, 0),
            ("<down> C-f", 2, 1),
            ("TAB", 3, 0),
            ("S-TAB", 2, 1),
            ("C-a", 2, 0),
            ("C-e", 2, 1),
            ("C-p <up>", 0, 1),
            ("<left>", 0, 0),
            ("<right>", 0, 1),
            ("M->", 999, 1),
            ("M-<", 0, 1),
        ] {
            press(&mut editor, keys);
            pump(&mut editor, &mut service);
            assert_eq!(view(&editor).cursor, Cell { row, column }, "{keys}");
        }
    }

    #[test]
    fn the_last_rows_load_when_the_cursor_gets_there() {
        let (_dir, mut editor, mut service) = opened();
        table(&mut editor, &mut service, "n n");

        press(&mut editor, "M->");
        pump(&mut editor, &mut service);

        assert_eq!(
            view(&editor).value(999, 1),
            Some(&Value::Text("row 1000".to_owned()))
        );
        assert_eq!(
            editor.pane().status().position.as_deref(),
            Some("1,000/1,000")
        );
    }

    #[test]
    fn page_keys_move_by_a_windowful() {
        let (_dir, mut editor, mut service) = opened();
        table(&mut editor, &mut service, "n n");
        let buffer = editor.buffer().id();
        editor.deliver(&Payload::new(GridViewport {
            buffer,
            rows: 20,
            width: 80,
        }));

        press(&mut editor, "C-v");
        assert_eq!(view(&editor).cursor.row, 20);
        press(&mut editor, "<next> M-v");
        assert_eq!(view(&editor).cursor.row, 20);
        press(&mut editor, "<prior>");
        assert_eq!(view(&editor).cursor.row, 0);
    }

    #[test]
    fn m_w_copies_the_cell_or_the_selection() {
        let (_dir, mut editor, mut service) = opened();
        table(&mut editor, &mut service, "");

        press(&mut editor, "C-f M-w");
        press(&mut editor, "C-SPC C-n C-b M-w");

        let copied: Vec<String> = editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::WriteClipboard(text) => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(
            copied,
            [
                "For Those About To Rock",
                "1\tFor Those About To Rock\n2\tBalls to the Wall",
            ]
        );
        assert_eq!(view(&editor).anchor, None);
    }

    #[test]
    fn c_g_drops_the_selection() {
        let (_dir, mut editor, mut service) = opened();
        table(&mut editor, &mut service, "");

        press(&mut editor, "C-SPC C-n C-g");

        assert_eq!(view(&editor).anchor, None);
    }

    #[test]
    fn g_reads_the_rows_again() {
        let (dir, mut editor, mut service) = opened();
        table(&mut editor, &mut service, "");
        let connection = rusqlite::Connection::open(dir.path().join("music.db")).unwrap();
        connection
            .execute_batch("UPDATE albums SET title = 'Changed' WHERE id = 1")
            .unwrap();

        press(&mut editor, "g");
        pump(&mut editor, &mut service);

        assert_eq!(
            view(&editor).value(0, 1),
            Some(&Value::Text("Changed".to_owned()))
        );
    }

    #[test]
    fn clicks_move_the_cursor_and_shift_clicks_select() {
        let (_dir, mut editor, mut service) = opened();
        table(&mut editor, &mut service, "");
        let buffer = editor.buffer().id();

        editor.deliver(&Payload::new(GridClick {
            buffer,
            row: 2,
            column: 1,
            extend: false,
        }));
        editor.deliver(&Payload::new(GridClick {
            buffer,
            row: 3,
            column: 2,
            extend: true,
        }));

        let grid = view(&editor);
        assert_eq!(grid.cursor, Cell { row: 3, column: 2 });
        assert_eq!(grid.anchor, Some(Cell { row: 2, column: 1 }));
    }

    #[test]
    fn the_wheel_scrolls_and_loads_rows() {
        let (_dir, mut editor, mut service) = opened();
        table(&mut editor, &mut service, "n n");
        let buffer = editor.buffer().id();

        editor.deliver(&Payload::new(GridScroll {
            buffer,
            rows: 600,
            columns: 0,
        }));
        pump(&mut editor, &mut service);

        let grid = view(&editor);
        assert_eq!(grid.top, 600);
        assert_eq!(grid.value(610, 0), Some(&Value::Integer(611)));
    }

    fn titles(editor: &Editor) -> Vec<String> {
        let grid = view(editor);
        (0..grid.total.unwrap_or(0))
            .filter_map(|row| match grid.value(row, 1) {
                Some(Value::Text(title)) => Some(title.clone()),
                _ => None,
            })
            .collect()
    }

    fn sorts(editor: &Editor) -> Vec<(String, bool)> {
        view(editor).query.sort.clone()
    }

    #[test]
    fn s_sorts_by_the_cursor_column_up_down_and_off() {
        let (_dir, mut editor, mut service) = opened();
        table(&mut editor, &mut service, "");

        press(&mut editor, "C-n C-f s");
        pump(&mut editor, &mut service);
        assert_eq!(
            titles(&editor),
            [
                "Balls to the Wall",
                "For Those About To Rock",
                "Let There Be Rock",
                "Restless and Wild"
            ]
        );
        assert_eq!(view(&editor).cursor, Cell { row: 0, column: 1 });
        assert_eq!(editor.pane().status().position.as_deref(), Some("1/4"));

        press(&mut editor, "s");
        pump(&mut editor, &mut service);
        assert_eq!(titles(&editor)[0], "Restless and Wild");

        press(&mut editor, "s");
        pump(&mut editor, &mut service);
        assert_eq!(titles(&editor)[0], "For Those About To Rock");
        assert_eq!(sorts(&editor), []);
    }

    #[test]
    fn shift_s_sorts_by_a_further_column() {
        let (_dir, mut editor, mut service) = opened();
        table(&mut editor, &mut service, "");

        press(&mut editor, "C-f C-f s C-b S");
        pump(&mut editor, &mut service);
        assert_eq!(
            sorts(&editor),
            [("artist_id".to_owned(), false), ("title".to_owned(), false)]
        );
        assert_eq!(
            titles(&editor),
            [
                "For Those About To Rock",
                "Let There Be Rock",
                "Balls to the Wall",
                "Restless and Wild"
            ]
        );

        press(&mut editor, "S");
        pump(&mut editor, &mut service);
        assert_eq!(
            titles(&editor),
            [
                "Let There Be Rock",
                "For Those About To Rock",
                "Restless and Wild",
                "Balls to the Wall"
            ]
        );
    }

    #[test]
    fn equals_keeps_the_rows_with_this_cells_value() {
        let (_dir, mut editor, mut service) = opened();
        table(&mut editor, &mut service, "");

        press(&mut editor, "C-f C-f =");
        pump(&mut editor, &mut service);

        assert_eq!(view(&editor).query.filters, ["artist_id = 1"]);
        assert_eq!(
            titles(&editor),
            ["For Those About To Rock", "Let There Be Rock"]
        );
        assert_eq!(editor.pane().status().position.as_deref(), Some("1/2"));
    }

    #[test]
    fn bang_keeps_the_rows_that_differ_including_nulls() {
        let (_dir, mut editor, mut service) = opened();
        table(&mut editor, &mut service, "");

        press(&mut editor, "C-n C-e !");
        pump(&mut editor, &mut service);
        assert_eq!(
            titles(&editor),
            [
                "For Those About To Rock",
                "Restless and Wild",
                "Let There Be Rock"
            ]
        );

        press(&mut editor, "C-n !");
        pump(&mut editor, &mut service);
        assert_eq!(
            view(&editor).query.filters,
            ["price IS NOT 8.5", "price IS NOT NULL"]
        );
        assert_eq!(editor.pane().status().position.as_deref(), Some("1/2"));
    }

    #[test]
    fn filtering_a_row_not_loaded_yet_says_so() {
        let (_dir, mut editor, mut service) = opened();
        table(&mut editor, &mut service, "n n");

        press(&mut editor, "M-> =");

        assert_eq!(editor.message(), Some("This row has not loaded yet"));
        assert_eq!(view(&editor).query.filters, Vec::<String>::new());
    }

    #[test]
    fn closing_the_tree_and_its_tables_closes_the_connection() {
        let (_dir, mut editor, mut service) = opened();
        table(&mut editor, &mut service, "");

        press(&mut editor, "C-x k RET");
        assert!(
            !requests(&mut editor)
                .iter()
                .any(|request| matches!(request, Request::Close { .. }))
        );
        press(&mut editor, "C-x o C-x k RET");

        assert!(
            requests(&mut editor)
                .iter()
                .any(|request| matches!(request, Request::Close { .. }))
        );
        assert_eq!(grids(&editor), []);
    }
}
