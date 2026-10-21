use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
#[cfg(test)]
use std::ops::Range;

use crate::driver::{Page, Query, Value};
use crate::service::DbId;
use crate::widget::clip;

pub(crate) const PAGE: u64 = 200;
pub(crate) const PAD: usize = 2;
const WIDEST: usize = 40;
const NARROWEST: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Cell {
    pub(crate) row: u64,
    pub(crate) column: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Loaded {
    pub(crate) rows: Vec<Vec<Value>>,
    pub(crate) rowids: Vec<Option<i64>>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Grid {
    pub(crate) db: DbId,
    pub(crate) query: Query,
    pub(crate) columns: Vec<String>,
    pub(crate) widths: Vec<usize>,
    pub(crate) total: Option<u64>,
    pub(crate) pages: BTreeMap<u64, Loaded>,
    pub(crate) pending: BTreeSet<u64>,
    pub(crate) error: Option<String>,
    pub(crate) cursor: Cell,
    pub(crate) anchor: Option<Cell>,
    pub(crate) top: u64,
    pub(crate) left: usize,
    pub(crate) shown_rows: usize,
    pub(crate) shown_width: usize,
}

pub(crate) fn shown(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_owned(),
        Value::Integer(number) => number.to_string(),
        Value::Real(number) => real(*number),
        Value::Text(text) => text
            .replace("\r\n", "↵")
            .replace(['\n', '\r'], "↵")
            .replace('\t', " "),
        Value::Blob(bytes) if bytes.len() == 1 => "1 byte".to_owned(),
        Value::Blob(bytes) => format!("{} bytes", bytes.len()),
    }
}

fn real(number: f64) -> String {
    if number.is_finite() && number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{number:.1}")
    } else {
        number.to_string()
    }
}

pub(crate) fn raw(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Text(text) => text.clone(),
        Value::Blob(bytes) => bytes.iter().fold(String::new(), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        }),
        Value::Integer(_) | Value::Real(_) => shown(value),
    }
}

fn index(number: u64) -> usize {
    usize::try_from(number).unwrap_or(usize::MAX)
}

fn wide(number: usize) -> u64 {
    u64::try_from(number).unwrap_or(u64::MAX)
}

impl Grid {
    pub(crate) fn new(db: DbId, query: Query) -> Self {
        Self {
            db,
            query,
            columns: Vec::new(),
            widths: Vec::new(),
            total: None,
            pages: BTreeMap::new(),
            pending: BTreeSet::new(),
            error: None,
            cursor: Cell::default(),
            anchor: None,
            top: 0,
            left: 0,
            shown_rows: 30,
            shown_width: 100,
        }
    }

    pub(crate) fn value(&self, row: u64, column: usize) -> Option<&Value> {
        self.pages
            .get(&(row / PAGE))?
            .rows
            .get(index(row % PAGE))?
            .get(column)
    }

    pub(crate) fn last_row(&self) -> u64 {
        match self.total {
            Some(total) => total.saturating_sub(1),
            None => self.pages.iter().next_back().map_or(0, |(page, loaded)| {
                (page * PAGE + wide(loaded.rows.len())).saturating_sub(1)
            }),
        }
    }

    pub(crate) fn width(&self, column: usize) -> usize {
        self.widths.get(column).copied().unwrap_or(NARROWEST)
    }

    fn fits(&self, left: usize, column: usize) -> bool {
        let used: usize = (left..=column).map(|at| self.width(at) + PAD).sum();
        used <= self.shown_width
    }

    fn reveal(&mut self) {
        let rows = wide(self.shown_rows.max(1));
        if self.cursor.row < self.top {
            self.top = self.cursor.row;
        } else if self.cursor.row >= self.top + rows {
            self.top = self.cursor.row + 1 - rows;
        }
        if self.cursor.column < self.left {
            self.left = self.cursor.column;
        }
        while self.left < self.cursor.column && !self.fits(self.left, self.cursor.column) {
            self.left += 1;
        }
    }

    pub(crate) fn go(&mut self, cell: Cell) {
        self.cursor = Cell {
            row: cell.row.min(self.last_row()),
            column: cell.column.min(self.columns.len().saturating_sub(1)),
        };
        self.reveal();
    }

    pub(crate) fn clamp(&mut self) {
        let inside = Cell {
            row: self.cursor.row.min(self.last_row()),
            column: self.cursor.column.min(self.columns.len().saturating_sub(1)),
        };
        if inside != self.cursor {
            self.go(inside);
        }
    }

    pub(crate) fn shift(&mut self, rows: i64, columns: i64) {
        let column = isize::try_from(columns).unwrap_or(0);
        self.go(Cell {
            row: self.cursor.row.saturating_add_signed(rows),
            column: self.cursor.column.saturating_add_signed(column),
        });
    }

    pub(crate) fn next_cell(&mut self, forward: bool) {
        let Cell { row, column } = self.cursor;
        let last_column = self.columns.len().saturating_sub(1);
        let next = if forward {
            if column < last_column {
                Cell {
                    row,
                    column: column + 1,
                }
            } else if row < self.last_row() {
                Cell {
                    row: row + 1,
                    column: 0,
                }
            } else {
                self.cursor
            }
        } else if column > 0 {
            Cell {
                row,
                column: column - 1,
            }
        } else if row > 0 {
            Cell {
                row: row - 1,
                column: last_column,
            }
        } else {
            self.cursor
        };
        self.go(next);
    }

    pub(crate) fn scroll(&mut self, rows: i64, columns: i64) {
        let highest = (self.last_row() + 1).saturating_sub(wide(self.shown_rows));
        self.top = self.top.saturating_add_signed(rows).min(highest);
        let column = isize::try_from(columns).unwrap_or(0);
        self.left = self
            .left
            .saturating_add_signed(column)
            .min(self.columns.len().saturating_sub(1));
    }

    #[cfg(test)]
    pub(crate) fn visible_columns(&self) -> Range<usize> {
        let mut end = self.left;
        while end < self.columns.len() && (end == self.left || self.fits(self.left, end)) {
            end += 1;
        }
        self.left..end
    }

    pub(crate) fn wanted(&self) -> Vec<u64> {
        let start = self.top.saturating_sub(PAGE / 2);
        let mut end = self.top + wide(self.shown_rows) + PAGE / 2;
        if let Some(total) = self.total {
            end = end.min(total);
        }
        let end = end.max(1);
        (start / PAGE..=(end - 1) / PAGE)
            .filter(|page| !self.pages.contains_key(page) && !self.pending.contains(page))
            .collect()
    }

    pub(crate) fn asked(&mut self, page: u64) {
        self.pending.insert(page);
    }

    pub(crate) fn insert(&mut self, offset: u64, page: Page) {
        let number = offset / PAGE;
        self.pending.remove(&number);
        if self.columns != page.columns {
            self.columns.clone_from(&page.columns);
            self.widths = self
                .columns
                .iter()
                .enumerate()
                .map(|(at, name)| {
                    page.rows
                        .iter()
                        .filter_map(|row| row.get(at))
                        .map(|value| shown(value).chars().count())
                        .chain([name.chars().count()])
                        .max()
                        .unwrap_or(0)
                        .clamp(NARROWEST, WIDEST)
                })
                .collect();
        }
        if self.total.is_none() && wide(page.rows.len()) < PAGE {
            self.total = Some(offset + wide(page.rows.len()));
        }
        self.pages.insert(
            number,
            Loaded {
                rows: page.rows,
                rowids: page.rowids,
            },
        );
    }

    pub(crate) fn resize(&mut self, rows: usize, width: usize) {
        self.shown_rows = rows.max(1);
        self.shown_width = width.max(1);
        self.reveal();
    }

    pub(crate) fn reset(&mut self) {
        self.pages.clear();
        self.pending.clear();
        self.total = None;
        self.error = None;
    }

    fn requery(&mut self) {
        self.reset();
        self.anchor = None;
        self.cursor.row = 0;
        self.top = 0;
    }

    pub(crate) fn sort(&mut self, column: usize, add: bool) -> bool {
        let Some(name) = self.columns.get(column).cloned() else {
            return false;
        };
        let sort = &mut self.query.sort;
        let found = sort.iter().position(|(sorted, _)| *sorted == name);
        let next = match found.map(|at| sort[at].1) {
            None => Some(false),
            Some(false) => Some(true),
            Some(true) => None,
        };
        if add {
            match (found, next) {
                (Some(at), Some(descending)) => sort[at].1 = descending,
                (Some(at), None) => {
                    sort.remove(at);
                }
                (None, Some(descending)) => sort.push((name, descending)),
                (None, None) => {}
            }
        } else {
            *sort = next
                .map(|descending| (name, descending))
                .into_iter()
                .collect();
        }
        self.fit_headings();
        self.requery();
        true
    }

    pub(crate) fn sort_mark(&self, column: usize) -> Option<(bool, usize)> {
        let name = self.columns.get(column)?;
        let at = self
            .query
            .sort
            .iter()
            .position(|(sorted, _)| sorted == name)?;
        Some((self.query.sort[at].1, at + 1))
    }

    fn mark(&self, column: usize) -> String {
        let Some((descending, order)) = self.sort_mark(column) else {
            return String::new();
        };
        let arrow = if descending { '▾' } else { '▴' };
        if self.query.sort.len() > 1 {
            format!(" {arrow}{order}")
        } else {
            format!(" {arrow}")
        }
    }

    pub(crate) fn heading(&self, column: usize) -> String {
        let name = self.columns.get(column).map_or("", String::as_str);
        let mark = self.mark(column);
        let room = self.width(column).saturating_sub(mark.chars().count());
        format!("{}{mark}", clip(name, room))
    }

    fn fit_headings(&mut self) {
        let wanted: Vec<usize> = self
            .columns
            .iter()
            .enumerate()
            .map(|(column, name)| name.chars().count() + self.mark(column).chars().count())
            .collect();
        for (width, wanted) in self.widths.iter_mut().zip(wanted) {
            *width = (*width).max(wanted.min(WIDEST));
        }
    }

    pub(crate) fn copied(&self) -> String {
        let anchor = self.anchor.unwrap_or(self.cursor);
        let rows = anchor.row.min(self.cursor.row)..=anchor.row.max(self.cursor.row);
        let columns = anchor.column.min(self.cursor.column)..=anchor.column.max(self.cursor.column);
        rows.map(|row| {
            columns
                .clone()
                .map(|column| self.value(row, column).map(raw).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Grid {
        Grid::new(
            DbId(1),
            Query {
                table: "albums".to_owned(),
                ..Query::default()
            },
        )
    }

    fn page(columns: &[&str], rows: Vec<Vec<Value>>) -> Page {
        Page {
            columns: columns.iter().map(|name| (*name).to_owned()).collect(),
            rowids: (0..rows.len())
                .map(|at| Some(i64::try_from(at).unwrap() + 1))
                .collect(),
            rows,
        }
    }

    fn numbers(count: u64, offset: u64) -> Page {
        page(
            &["n", "label"],
            (offset..offset + count)
                .map(|n| {
                    vec![
                        Value::Integer(i64::try_from(n).unwrap()),
                        Value::Text(format!("row {n}")),
                    ]
                })
                .collect(),
        )
    }

    fn sized(total: u64) -> Grid {
        let mut grid = grid();
        grid.total = Some(total);
        grid.insert(0, numbers(PAGE.min(total), 0));
        grid.resize(10, 100);
        grid
    }

    #[test]
    fn a_fresh_grid_wants_its_first_page() {
        let grid = grid();

        assert_eq!(grid.wanted(), [0]);
    }

    #[test]
    fn pages_bring_columns_values_and_fitted_widths() {
        let mut grid = grid();

        grid.insert(
            0,
            page(
                &["id", "title"],
                vec![
                    vec![
                        Value::Integer(1),
                        Value::Text("For Those About To Rock".to_owned()),
                    ],
                    vec![Value::Integer(22), Value::Null],
                ],
            ),
        );

        assert_eq!(grid.columns, ["id", "title"]);
        assert_eq!(grid.value(1, 0), Some(&Value::Integer(22)));
        assert_eq!(grid.value(5, 0), None);
        assert_eq!(grid.widths, [4, 23]);
        assert_eq!(grid.wanted(), Vec::<u64>::new());
    }

    #[test]
    fn widths_are_capped_for_long_values() {
        let mut grid = grid();

        grid.insert(0, page(&["notes"], vec![vec![Value::Text("x".repeat(90))]]));

        assert_eq!(grid.widths, [40]);
    }

    #[test]
    fn moving_stays_in_the_table_and_keeps_the_cursor_in_view() {
        let mut grid = sized(100);

        grid.shift(15, 0);
        assert_eq!((grid.cursor.row, grid.top), (15, 6));
        grid.shift(-20, 5);
        assert_eq!((grid.cursor.row, grid.cursor.column, grid.top), (0, 1, 0));
        grid.go(Cell {
            row: 500,
            column: 0,
        });
        assert_eq!((grid.cursor.row, grid.top), (99, 90));
        assert_eq!(grid.last_row(), 99);
    }

    #[test]
    fn tab_moves_along_the_row_and_wraps() {
        let mut grid = sized(100);

        grid.next_cell(true);
        assert_eq!(grid.cursor, Cell { row: 0, column: 1 });
        grid.next_cell(true);
        assert_eq!(grid.cursor, Cell { row: 1, column: 0 });
        grid.next_cell(false);
        assert_eq!(grid.cursor, Cell { row: 0, column: 1 });
        grid.go(Cell::default());
        grid.next_cell(false);
        assert_eq!(grid.cursor, Cell::default());
    }

    #[test]
    fn wide_tables_scroll_sideways_to_the_cursor() {
        let mut grid = sized(10);
        grid.columns = (0..4).map(|n| format!("c{n}")).collect();
        grid.widths = vec![10, 10, 10, 10];
        grid.resize(10, 25);

        assert_eq!(grid.visible_columns(), 0..2);
        grid.go(Cell { row: 0, column: 3 });
        assert_eq!(grid.left, 2);
        assert_eq!(grid.visible_columns(), 2..4);
        grid.go(Cell { row: 0, column: 0 });
        assert_eq!(grid.left, 0);
    }

    #[test]
    fn pages_near_the_view_are_wanted_once() {
        let mut grid = sized(1000);
        grid.resize(30, 100);

        grid.go(Cell {
            row: 450,
            column: 0,
        });
        assert_eq!(grid.wanted(), [1, 2]);
        grid.asked(1);
        grid.asked(2);
        assert_eq!(grid.wanted(), Vec::<u64>::new());
        grid.insert(200, numbers(200, 200));
        assert_eq!(grid.value(250, 1), Some(&Value::Text("row 250".to_owned())));
        assert!(!grid.pending.contains(&1));
    }

    #[test]
    fn the_wheel_scrolls_the_view_without_moving_the_cursor() {
        let mut grid = sized(100);

        grid.scroll(7, 0);
        assert_eq!((grid.top, grid.cursor.row), (7, 0));
        grid.scroll(-50, 0);
        assert_eq!(grid.top, 0);
        grid.scroll(500, 0);
        assert_eq!(grid.top, 90);
    }

    #[test]
    fn clamping_keeps_the_view_unless_the_cursor_falls_outside() {
        let mut grid = sized(100);
        grid.scroll(40, 0);

        grid.clamp();
        assert_eq!((grid.top, grid.cursor.row), (40, 0));

        grid.cursor = Cell {
            row: 500,
            column: 9,
        };
        grid.clamp();
        assert_eq!(grid.cursor, Cell { row: 99, column: 1 });
        assert_eq!(grid.top, 90);
    }

    #[test]
    fn selections_copy_as_tab_separated_text() {
        let mut grid = sized(100);
        grid.go(Cell { row: 1, column: 0 });
        assert_eq!(grid.copied(), "1");

        grid.anchor = Some(grid.cursor);
        grid.go(Cell { row: 2, column: 1 });

        assert_eq!(grid.copied(), "1\trow 1\n2\trow 2");
    }

    #[test]
    fn reset_forgets_rows_but_keeps_the_place() {
        let mut grid = sized(100);
        grid.go(Cell { row: 5, column: 1 });

        grid.reset();

        assert_eq!(grid.pages.len(), 0);
        assert_eq!(grid.total, None);
        assert_eq!(grid.cursor, Cell { row: 5, column: 1 });
        assert_eq!(grid.wanted(), [0]);
    }

    fn sorts(grid: &Grid) -> Vec<(&str, bool)> {
        grid.query
            .sort
            .iter()
            .map(|(name, descending)| (name.as_str(), *descending))
            .collect()
    }

    #[test]
    fn s_cycles_up_down_and_off_from_the_first_row() {
        let mut grid = sized(100);
        grid.go(Cell { row: 40, column: 1 });
        grid.anchor = Some(Cell { row: 30, column: 0 });
        grid.left = 1;

        assert!(grid.sort(1, false));
        assert_eq!(sorts(&grid), [("label", false)]);
        assert_eq!(grid.cursor, Cell { row: 0, column: 1 });
        assert_eq!((grid.top, grid.left, grid.total), (0, 1, None));
        assert_eq!(grid.anchor, None);
        assert!(grid.pages.is_empty());
        assert_eq!(grid.wanted(), [0]);
        grid.sort(1, false);
        assert_eq!(sorts(&grid), [("label", true)]);
        grid.sort(1, false);
        assert_eq!(sorts(&grid), []);
        assert!(!grid.sort(7, false));
    }

    #[test]
    fn s_on_another_column_replaces_the_sort() {
        let mut grid = sized(100);
        grid.sort(0, false);
        grid.sort(1, true);

        grid.sort(1, false);
        assert_eq!(sorts(&grid), [("label", true)]);
        grid.sort(0, false);
        assert_eq!(sorts(&grid), [("n", false)]);
    }

    #[test]
    fn shift_s_adds_a_further_key_and_cycles_it_in_place() {
        let mut grid = sized(100);
        grid.sort(1, false);

        grid.sort(0, true);
        assert_eq!(sorts(&grid), [("label", false), ("n", false)]);
        grid.sort(1, true);
        assert_eq!(sorts(&grid), [("label", true), ("n", false)]);
        grid.sort(1, true);
        assert_eq!(sorts(&grid), [("n", false)]);
    }

    #[test]
    fn sort_marks_number_several_sorts() {
        let mut grid = sized(100);
        assert_eq!(grid.heading(1), "label");

        grid.sort(1, false);
        assert_eq!(grid.sort_mark(1), Some((false, 1)));
        assert_eq!(grid.heading(1), "label ▴");
        grid.sort(0, true);
        grid.sort(0, true);
        assert_eq!(grid.sort_mark(0), Some((true, 2)));
        assert_eq!(grid.sort_mark(5), None);
        assert_eq!(grid.heading(0), "n ▾2");
        assert_eq!(grid.heading(1), "label ▴1");
    }

    #[test]
    fn values_show_compactly_and_copy_whole() {
        let text = Value::Text("two\nlines".to_owned());
        for (value, expected) in [
            (Value::Null, "NULL"),
            (Value::Integer(-3), "-3"),
            (Value::Real(7.0), "7.0"),
            (Value::Real(0.25), "0.25"),
            (text.clone(), "two↵lines"),
            (Value::Blob(vec![1, 2, 3]), "3 bytes"),
        ] {
            assert_eq!(shown(&value), expected);
        }
        assert_eq!(raw(&text), "two\nlines");
        assert_eq!(raw(&Value::Null), "");
        assert_eq!(raw(&Value::Blob(vec![0xAB, 0x01])), "ab01");
    }
}
