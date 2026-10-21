use std::collections::{HashMap, HashSet};

use oxmacs_core::theme::Highlight;

use crate::driver::{Kind, Schema, Table};
use crate::text::{Line, Lines, thousands};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Group {
    Tables,
    Views,
    Indexes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    Info,
    Blank,
    Group(Group),
    Table(usize),
    Column(usize, usize),
    Index(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum State {
    Opening,
    Failed(String),
    Ready(Schema),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Folds {
    pub(crate) expanded: HashSet<String>,
    pub(crate) closed: HashSet<Group>,
}

pub(crate) struct Shown<'a> {
    pub(crate) name: &'a str,
    pub(crate) place: &'a str,
    pub(crate) state: &'a State,
    pub(crate) counts: &'a HashMap<String, u64>,
    pub(crate) folds: &'a Folds,
}

const GAP: usize = 2;

fn heading(out: &mut Lines<Row>, group: Group, count: usize, folds: &Folds) {
    let title = match group {
        Group::Tables => "Tables",
        Group::Views => "Views",
        Group::Indexes => "Indexes",
    };
    let mut line = Line::default();
    let open = !folds.closed.contains(&group);
    line.push(if open { "▾ " } else { "▸ " }, Some(Highlight::Comment));
    line.push(title, Some(Highlight::Heading));
    line.push(&format!(" ({count})"), Some(Highlight::Comment));
    out.add(Row::Group(group), line);
}

fn longest<'a>(names: impl Iterator<Item = &'a str>) -> usize {
    names.map(|name| name.chars().count()).max().unwrap_or(0)
}

fn tables(out: &mut Lines<Row>, schema: &Schema, kind: Kind, shown: &Shown<'_>) {
    let listed: Vec<(usize, &Table)> = schema
        .tables
        .iter()
        .enumerate()
        .filter(|(_, table)| table.kind == kind)
        .collect();
    let width = longest(listed.iter().map(|(_, table)| table.name.as_str()));
    let counted = |table: &Table| {
        (table.kind == Kind::Table)
            .then(|| shown.counts.get(&table.name).copied())
            .flatten()
    };
    let digits = listed
        .iter()
        .filter_map(|(_, table)| counted(table))
        .map(|count| thousands(count).len())
        .max()
        .unwrap_or(0);
    for (index, table) in listed {
        let expanded = shown.folds.expanded.contains(&table.name);
        let mut line = Line::default();
        line.push("  ", None);
        line.push(if expanded { "▾ " } else { "▸ " }, Some(Highlight::Comment));
        line.push(&table.name, None);
        if let Some(count) = counted(table) {
            line.pad(4 + width + GAP);
            let shown = thousands(count);
            line.pad(line.width() + digits - shown.len());
            line.push(&shown, Some(Highlight::Comment));
        }
        out.add(Row::Table(index), line);
        if expanded {
            columns(out, index, table);
        }
    }
}

fn columns(out: &mut Lines<Row>, index: usize, table: &Table) {
    let names = longest(table.columns.iter().map(|column| column.name.as_str()));
    let types = longest(table.columns.iter().map(|column| column.declared.as_str()));
    for (at, column) in table.columns.iter().enumerate() {
        let mut line = Line::default();
        line.push("      ", None);
        line.push(&column.name, None);
        let mut flags: Vec<(String, Option<Highlight>, bool)> = Vec::new();
        if column.primary_key {
            flags.push(("PK".to_owned(), Some(Highlight::Constant), true));
        }
        if column.not_null && !column.primary_key {
            flags.push(("not null".to_owned(), Some(Highlight::Comment), false));
        }
        if let Some(default) = &column.default {
            flags.push((format!("= {default}"), Some(Highlight::Comment), false));
        }
        if let Some(key) = table
            .foreign_keys
            .iter()
            .find(|key| key.from == column.name)
        {
            let target = key
                .to
                .as_deref()
                .map_or_else(|| key.table.clone(), |to| format!("{}.{to}", key.table));
            flags.push((format!("→ {target}"), Some(Highlight::Link), false));
        }
        if !column.declared.is_empty() || !flags.is_empty() {
            line.pad(6 + names + GAP);
            line.push(&column.declared, Some(Highlight::Type));
        }
        for (position, (flag, face, pill)) in flags.into_iter().enumerate() {
            if position == 0 {
                line.pad(6 + names + GAP + types + GAP);
            } else {
                line.push("  ", None);
            }
            match (pill, face) {
                (true, Some(face)) => line.pill(&flag, face),
                _ => {
                    line.push(&flag, face);
                }
            }
        }
        out.add(Row::Column(index, at), line);
    }
}

fn indexes(out: &mut Lines<Row>, schema: &Schema) {
    let width = longest(schema.indexes.iter().map(|index| index.name.as_str()));
    for (at, index) in schema.indexes.iter().enumerate() {
        let mut line = Line::default();
        line.push("    ", None);
        line.push(&index.name, None);
        line.pad(4 + width + GAP);
        line.push(
            &format!("{} ({})", index.table, index.columns.join(", ")),
            Some(Highlight::Comment),
        );
        if index.unique {
            line.push("  ", None);
            line.pill("unique", Highlight::Constant);
        }
        out.add(Row::Index(at), line);
    }
}

pub(crate) fn render(shown: &Shown<'_>) -> Lines<Row> {
    let mut out = Lines::default();
    let mut line = Line::default();
    line.push(shown.name, Some(Highlight::Heading));
    line.push("  SQLite", Some(Highlight::Comment));
    out.add(Row::Info, line);
    let mut line = Line::default();
    line.push(shown.place, Some(Highlight::Comment));
    out.add(Row::Info, line);
    out.add(Row::Blank, Line::default());
    let schema = match shown.state {
        State::Opening => {
            let mut line = Line::default();
            line.push("Opening…", Some(Highlight::Comment));
            out.add(Row::Info, line);
            return out;
        }
        State::Failed(error) => {
            let mut line = Line::default();
            line.push(error, Some(Highlight::Error));
            out.add(Row::Info, line);
            return out;
        }
        State::Ready(schema) => schema,
    };
    if schema.tables.is_empty() {
        let mut line = Line::default();
        line.push("No tables yet", Some(Highlight::Comment));
        out.add(Row::Info, line);
        return out;
    }
    let count = |kind| {
        schema
            .tables
            .iter()
            .filter(|table| table.kind == kind)
            .count()
    };
    let groups = [
        (Group::Tables, count(Kind::Table)),
        (Group::Views, count(Kind::View)),
        (Group::Indexes, schema.indexes.len()),
    ];
    let mut first = true;
    for (group, count) in groups {
        if count == 0 {
            continue;
        }
        if !first {
            out.add(Row::Blank, Line::default());
        }
        first = false;
        heading(&mut out, group, count, shown.folds);
        if shown.folds.closed.contains(&group) {
            continue;
        }
        match group {
            Group::Tables => tables(&mut out, schema, Kind::Table, shown),
            Group::Views => tables(&mut out, schema, Kind::View, shown),
            Group::Indexes => indexes(&mut out, schema),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::fixtures::database;
    use crate::driver::{Driver, Sqlite};

    fn schema() -> Schema {
        let (_dir, path) = database();
        Sqlite::open(&path).unwrap().schema().unwrap()
    }

    fn counts() -> HashMap<String, u64> {
        [("albums", 4), ("artists", 3_503), ("tags", 2)]
            .into_iter()
            .map(|(name, count)| (name.to_owned(), count))
            .collect()
    }

    fn shown<'a>(
        state: &'a State,
        counts: &'a HashMap<String, u64>,
        folds: &'a Folds,
    ) -> Lines<Row> {
        render(&Shown {
            name: "music.db",
            place: "C:/data/music.db",
            state,
            counts,
            folds,
        })
    }

    fn text(lines: &Lines<Row>) -> Vec<&str> {
        lines.text.split('\n').collect()
    }

    #[test]
    fn the_tree_lists_tables_views_and_indexes() {
        let state = State::Ready(schema());
        let counts = counts();

        let lines = shown(&state, &counts, &Folds::default());

        assert_eq!(
            text(&lines),
            [
                "music.db  SQLite",
                "C:/data/music.db",
                "",
                "▾ Tables (4)",
                "  ▸ albums          4",
                "  ▸ artists     3,503",
                "  ▸ odd \"name\"",
                "  ▸ tags            2",
                "",
                "▾ Views (1)",
                "  ▸ long_titles",
                "",
                "▾ Indexes (2)",
                "    albums_by_artist  albums (artist_id)",
                "    albums_by_title   albums (title, artist_id)  unique",
            ]
        );
        assert_eq!(lines.rows[4], Row::Table(0));
        assert_eq!(lines.rows[10], Row::Table(2));
        assert_eq!(lines.rows[13], Row::Index(0));
        assert_eq!(lines.pills(14), [(49..55, Highlight::Constant)]);
    }

    #[test]
    fn expanded_tables_show_their_columns() {
        let state = State::Ready(schema());
        let counts = counts();
        let mut folds = Folds::default();
        folds.expanded.insert("albums".to_owned());

        let lines = shown(&state, &counts, &folds);

        assert_eq!(
            text(&lines)[4..10],
            [
                "  ▾ albums          4",
                "      id         INTEGER  PK",
                "      title      TEXT     not null  = 'Untitled'",
                "      artist_id  INTEGER  → artists.id",
                "      cover      BLOB",
                "      price      REAL",
            ]
        );
        assert_eq!(lines.rows[7], Row::Column(0, 2));
        assert_eq!(lines.pills(5), [(26..28, Highlight::Constant)]);
        assert!(lines.style(7).spans.contains(&(26..38, Highlight::Link)));
    }

    #[test]
    fn closed_groups_fold_away() {
        let state = State::Ready(schema());
        let counts = counts();
        let mut folds = Folds::default();
        folds.closed.insert(Group::Tables);
        folds.closed.insert(Group::Indexes);

        let lines = shown(&state, &counts, &folds);

        assert_eq!(
            text(&lines)[3..],
            [
                "▸ Tables (4)",
                "",
                "▾ Views (1)",
                "  ▸ long_titles",
                "",
                "▸ Indexes (2)"
            ]
        );
    }

    #[test]
    fn opening_and_failures_say_so() {
        let counts = HashMap::new();
        let folds = Folds::default();

        let opening = shown(&State::Opening, &counts, &folds);
        let failed = shown(
            &State::Failed("file is not a database".to_owned()),
            &counts,
            &folds,
        );

        assert_eq!(text(&opening)[3], "Opening…");
        assert_eq!(text(&failed)[3], "file is not a database");
        assert_eq!(failed.style(3).spans, [(0..22, Highlight::Error)]);
    }

    #[test]
    fn empty_databases_offer_a_hint() {
        let state = State::Ready(Schema::default());
        let counts = HashMap::new();

        let lines = shown(&state, &counts, &Folds::default());

        assert_eq!(text(&lines)[3], "No tables yet");
    }

    #[test]
    fn views_are_not_counted() {
        let state = State::Ready(schema());
        let mut counts = counts();
        counts.insert("long_titles".to_owned(), 9);

        let lines = shown(&state, &counts, &Folds::default());

        assert_eq!(text(&lines)[10], "  ▸ long_titles");
    }
}
