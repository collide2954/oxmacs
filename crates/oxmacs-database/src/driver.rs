use std::fmt;
use std::path::{Path, PathBuf};

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Table,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub declared: String,
    pub not_null: bool,
    pub default: Option<String>,
    pub primary_key: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKey {
    pub from: String,
    pub table: String,
    pub to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub name: String,
    pub kind: Kind,
    pub columns: Vec<Column>,
    pub foreign_keys: Vec<ForeignKey>,
    pub rowid: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    pub name: String,
    pub table: String,
    pub unique: bool,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schema {
    pub tables: Vec<Table>,
    pub indexes: Vec<Index>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Query {
    pub table: String,
    pub sort: Vec<(String, bool)>,
    pub filters: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Page {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub rowids: Vec<Option<i64>>,
}

pub trait Driver: Send + fmt::Debug {
    fn name(&self) -> String;

    fn schema(&self) -> Result<Schema, String>;

    fn count(&self, query: &Query) -> Result<u64, String>;

    fn rows(&self, query: &Query, offset: u64, limit: u64) -> Result<Page, String>;
}

#[derive(Debug)]
pub struct Sqlite {
    connection: Connection,
    path: Option<PathBuf>,
}

fn failed(error: impl fmt::Display) -> String {
    error.to_string()
}

pub fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn value(raw: ValueRef<'_>) -> Value {
    match raw {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(number) => Value::Integer(number),
        ValueRef::Real(number) => Value::Real(number),
        ValueRef::Text(bytes) => Value::Text(String::from_utf8_lossy(bytes).into_owned()),
        ValueRef::Blob(bytes) => Value::Blob(bytes.to_vec()),
    }
}

fn text_of(value: &Value) -> Option<String> {
    match value {
        Value::Null | Value::Blob(_) => None,
        Value::Integer(number) => Some(number.to_string()),
        Value::Real(number) => Some(number.to_string()),
        Value::Text(text) => Some(text.clone()),
    }
}

fn count_of(number: i64) -> u64 {
    u64::try_from(number).unwrap_or_default()
}

fn signed(number: u64) -> i64 {
    i64::try_from(number).unwrap_or(i64::MAX)
}

impl Sqlite {
    pub fn open(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Err(format!("{} does not exist", path.display()));
        }
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(path, flags).map_err(failed)?;
        Self::checked(connection, Some(path.to_owned()))
    }

    pub fn create(path: &Path) -> Result<Self, String> {
        if path.exists() {
            return Err(format!("{} already exists", path.display()));
        }
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(path, flags).map_err(failed)?;
        Self::checked(connection, Some(path.to_owned()))
    }

    pub fn memory() -> Result<Self, String> {
        Self::checked(Connection::open_in_memory().map_err(failed)?, None)
    }

    fn checked(connection: Connection, path: Option<PathBuf>) -> Result<Self, String> {
        let sqlite = Self { connection, path };
        sqlite
            .connection
            .query_row("SELECT count(*) FROM sqlite_schema", [], |_| Ok(()))
            .map_err(|error| format!("{}: {error}", sqlite.name()))?;
        Ok(sqlite)
    }

    fn strings(&self, sql: &str, name: &str) -> Result<Vec<Vec<Option<String>>>, String> {
        let mut statement = self.connection.prepare(sql).map_err(failed)?;
        let width = statement.column_count();
        let rows = statement
            .query_map([name], |row| {
                (0..width)
                    .map(|at| row.get_ref(at).map(|raw| text_of(&value(raw))))
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(failed)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(failed)
    }

    fn has_rowid(&self, table: &str) -> bool {
        self.connection
            .prepare(&format!("SELECT rowid FROM {} LIMIT 0", quote(table)))
            .is_ok()
    }

    fn table(&self, name: &str, kind: Kind) -> Result<Table, String> {
        let columns = self
            .strings(
                "SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info(?1)",
                name,
            )?
            .into_iter()
            .map(|fields| {
                let text = |at: usize| fields[at].clone().unwrap_or_default();
                Column {
                    name: text(0),
                    declared: text(1),
                    not_null: text(2) == "1",
                    default: fields[3].clone(),
                    primary_key: text(4) != "0",
                }
            })
            .collect();
        let foreign_keys = self
            .strings(
                "SELECT \"from\", \"table\", \"to\" FROM pragma_foreign_key_list(?1) ORDER BY id, seq",
                name,
            )?
            .into_iter()
            .map(|fields| ForeignKey {
                from: fields[0].clone().unwrap_or_default(),
                table: fields[1].clone().unwrap_or_default(),
                to: fields[2].clone(),
            })
            .collect();
        Ok(Table {
            name: name.to_owned(),
            kind,
            columns,
            foreign_keys,
            rowid: kind == Kind::Table && self.has_rowid(name),
        })
    }
}

fn select(query: &Query, what: &str, rowid: bool) -> String {
    let mut sql = format!("SELECT {what} FROM {}", quote(&query.table));
    if !query.filters.is_empty() {
        let conditions: Vec<String> = query
            .filters
            .iter()
            .map(|condition| format!("({condition})"))
            .collect();
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }
    let mut order: Vec<String> = query
        .sort
        .iter()
        .map(|(column, descending)| {
            format!(
                "{} {}",
                quote(column),
                if *descending { "DESC" } else { "ASC" }
            )
        })
        .collect();
    if rowid {
        order.push("rowid".to_owned());
    }
    if !order.is_empty() {
        sql.push_str(" ORDER BY ");
        sql.push_str(&order.join(", "));
    }
    sql
}

impl Driver for Sqlite {
    fn name(&self) -> String {
        self.path
            .as_ref()
            .map_or_else(|| "memory".to_owned(), |path| path.display().to_string())
    }

    fn schema(&self) -> Result<Schema, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT name, type FROM sqlite_schema WHERE type IN ('table', 'view') \
                 AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name COLLATE NOCASE",
            )
            .map_err(failed)?;
        let found = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(failed)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(failed)?;
        let tables = found
            .into_iter()
            .map(|(name, kind)| {
                let kind = if kind == "view" {
                    Kind::View
                } else {
                    Kind::Table
                };
                self.table(&name, kind)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT name, tbl_name FROM sqlite_schema WHERE type = 'index' \
                 AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name COLLATE NOCASE",
            )
            .map_err(failed)?;
        let named = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(failed)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(failed)?;
        let indexes = named
            .into_iter()
            .map(|(name, table)| {
                let unique = self
                    .strings("SELECT \"unique\", name FROM pragma_index_list(?1)", &table)?
                    .into_iter()
                    .any(|fields| {
                        fields[1].as_deref() == Some(&name) && fields[0].as_deref() == Some("1")
                    });
                let columns = self
                    .strings(
                        "SELECT name FROM pragma_index_info(?1) ORDER BY seqno",
                        &name,
                    )?
                    .into_iter()
                    .map(|fields| fields[0].clone().unwrap_or_default())
                    .collect();
                Ok(Index {
                    name,
                    table,
                    unique,
                    columns,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Schema { tables, indexes })
    }

    fn count(&self, query: &Query) -> Result<u64, String> {
        let unsorted = Query {
            sort: Vec::new(),
            ..query.clone()
        };
        let sql = select(&unsorted, "count(*)", false);
        self.connection
            .query_row(&sql, [], |row| row.get::<_, i64>(0))
            .map(count_of)
            .map_err(failed)
    }

    fn rows(&self, query: &Query, offset: u64, limit: u64) -> Result<Page, String> {
        let rowid = self.has_rowid(&query.table);
        let what = if rowid { "rowid, *" } else { "*" };
        let mut sql = select(query, what, rowid);
        sql.push_str(" LIMIT ?1 OFFSET ?2");
        let mut statement = self.connection.prepare(&sql).map_err(failed)?;
        let skip = usize::from(rowid);
        let columns: Vec<String> = statement
            .column_names()
            .into_iter()
            .skip(skip)
            .map(str::to_owned)
            .collect();
        let width = statement.column_count();
        let mut rows = statement
            .query([signed(limit), signed(offset)])
            .map_err(failed)?;
        let mut page = Page {
            columns,
            ..Page::default()
        };
        while let Some(row) = rows.next().map_err(failed)? {
            let mut values = Vec::with_capacity(width - skip);
            for at in skip..width {
                values.push(value(row.get_ref(at).map_err(failed)?));
            }
            page.rowids.push(if rowid {
                Some(row.get::<_, i64>(0).map_err(failed)?)
            } else {
                None
            });
            page.rows.push(values);
        }
        Ok(page)
    }
}
#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::{Path, PathBuf};

    pub(crate) const SETUP: &str = r#"
        CREATE TABLE artists (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
        CREATE TABLE albums (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL DEFAULT 'Untitled',
            artist_id INTEGER REFERENCES artists(id),
            cover BLOB,
            price REAL
        );
        CREATE INDEX albums_by_artist ON albums(artist_id);
        CREATE UNIQUE INDEX albums_by_title ON albums(title, artist_id);
        CREATE VIEW long_titles AS SELECT title FROM albums WHERE length(title) > 10;
        CREATE TABLE "odd ""name""" (x);
        CREATE TABLE tags (name TEXT PRIMARY KEY) WITHOUT ROWID;
        INSERT INTO artists (id, name) VALUES (1, 'AC/DC'), (2, 'Accept'), (3, 'Aerosmith');
        INSERT INTO albums (id, title, artist_id, cover, price) VALUES
            (1, 'For Those About To Rock', 1, x'89504E47', 9.99),
            (2, 'Balls to the Wall', 2, NULL, 8.5),
            (3, 'Restless and Wild', 2, NULL, NULL),
            (4, 'Let There Be Rock', 1, NULL, 7.0);
        INSERT INTO "odd ""name""" VALUES ('quoted');
        INSERT INTO tags VALUES ('rock'), ('metal');
    "#;

    pub(crate) fn database() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("music.db");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch(SETUP).unwrap();
        (dir, path)
    }

    pub(crate) fn query(table: &str) -> super::Query {
        super::Query {
            table: table.to_owned(),
            ..super::Query::default()
        }
    }

    pub(crate) fn exists(path: &Path) -> bool {
        path.exists()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::fixtures::{database, exists, query};
    use super::*;

    fn opened() -> (tempfile::TempDir, Sqlite) {
        let (dir, path) = database();
        let sqlite = Sqlite::open(&path).unwrap();
        (dir, sqlite)
    }

    fn names(schema: &Schema) -> Vec<(&str, Kind)> {
        schema
            .tables
            .iter()
            .map(|table| (table.name.as_str(), table.kind))
            .collect()
    }

    #[test]
    fn the_schema_lists_tables_and_views_by_name() {
        let (_dir, sqlite) = opened();

        let schema = sqlite.schema().unwrap();

        assert_eq!(
            names(&schema),
            [
                ("albums", Kind::Table),
                ("artists", Kind::Table),
                ("long_titles", Kind::View),
                ("odd \"name\"", Kind::Table),
                ("tags", Kind::Table),
            ]
        );
    }

    #[test]
    fn columns_describe_their_type_keys_and_defaults() {
        let (_dir, sqlite) = opened();

        let schema = sqlite.schema().unwrap();

        let albums = &schema.tables[0];
        assert_eq!(albums.columns.len(), 5);
        assert_eq!(
            albums.columns[0],
            Column {
                name: "id".to_owned(),
                declared: "INTEGER".to_owned(),
                not_null: false,
                default: None,
                primary_key: true,
            }
        );
        assert_eq!(
            albums.columns[1],
            Column {
                name: "title".to_owned(),
                declared: "TEXT".to_owned(),
                not_null: true,
                default: Some("'Untitled'".to_owned()),
                primary_key: false,
            }
        );
        assert!(albums.rowid);
        assert!(!schema.tables[4].rowid);
        assert!(!schema.tables[2].rowid);
    }

    #[test]
    fn foreign_keys_and_indexes_are_listed() {
        let (_dir, sqlite) = opened();

        let schema = sqlite.schema().unwrap();

        assert_eq!(
            schema.tables[0].foreign_keys,
            [ForeignKey {
                from: "artist_id".to_owned(),
                table: "artists".to_owned(),
                to: Some("id".to_owned()),
            }]
        );
        assert_eq!(
            schema.indexes,
            [
                Index {
                    name: "albums_by_artist".to_owned(),
                    table: "albums".to_owned(),
                    unique: false,
                    columns: vec!["artist_id".to_owned()],
                },
                Index {
                    name: "albums_by_title".to_owned(),
                    table: "albums".to_owned(),
                    unique: true,
                    columns: vec!["title".to_owned(), "artist_id".to_owned()],
                },
            ]
        );
    }

    #[test]
    fn rows_come_in_pages_with_their_rowids_and_types() {
        let (_dir, sqlite) = opened();

        let page = sqlite.rows(&query("albums"), 1, 2).unwrap();

        assert_eq!(page.columns, ["id", "title", "artist_id", "cover", "price"]);
        assert_eq!(page.rowids, [Some(2), Some(3)]);
        assert_eq!(
            page.rows,
            [
                vec![
                    Value::Integer(2),
                    Value::Text("Balls to the Wall".to_owned()),
                    Value::Integer(2),
                    Value::Null,
                    Value::Real(8.5),
                ],
                vec![
                    Value::Integer(3),
                    Value::Text("Restless and Wild".to_owned()),
                    Value::Integer(2),
                    Value::Null,
                    Value::Null,
                ],
            ]
        );
        let first = sqlite.rows(&query("albums"), 0, 1).unwrap();
        assert_eq!(first.rows[0][3], Value::Blob(vec![0x89, 0x50, 0x4E, 0x47]));
    }

    #[test]
    fn views_and_tables_without_rowids_have_none() {
        let (_dir, sqlite) = opened();

        let view = sqlite.rows(&query("long_titles"), 0, 10).unwrap();
        let tags = sqlite.rows(&query("tags"), 0, 10).unwrap();

        assert_eq!(view.columns, ["title"]);
        assert_eq!(view.rowids.len(), 4);
        assert!(view.rowids.iter().all(Option::is_none));
        assert_eq!(
            tags.rows,
            [
                vec![Value::Text("metal".to_owned())],
                vec![Value::Text("rock".to_owned())],
            ]
        );
    }

    #[test]
    fn counts_sorts_and_filters_shape_the_rows() {
        let (_dir, sqlite) = opened();
        let filtered = Query {
            table: "albums".to_owned(),
            sort: vec![("price".to_owned(), true)],
            filters: vec![
                "artist_id = 1 OR price IS NULL".to_owned(),
                "id > 0".to_owned(),
            ],
        };

        assert_eq!(sqlite.count(&query("albums")).unwrap(), 4);
        assert_eq!(sqlite.count(&filtered).unwrap(), 3);
        let page = sqlite.rows(&filtered, 0, 10).unwrap();
        assert_eq!(page.rowids, [Some(1), Some(4), Some(3)]);
    }

    #[test]
    fn odd_names_are_quoted() {
        let (_dir, sqlite) = opened();

        let page = sqlite.rows(&query("odd \"name\""), 0, 10).unwrap();

        assert_eq!(page.rows, [vec![Value::Text("quoted".to_owned())]]);
    }

    #[test]
    fn bad_conditions_report_what_sqlite_said() {
        let (_dir, sqlite) = opened();
        let broken = Query {
            filters: vec!["no_such_column = 1".to_owned()],
            ..query("albums")
        };

        let error = sqlite.rows(&broken, 0, 10).unwrap_err();

        assert!(error.contains("no_such_column"), "{error}");
    }

    #[test]
    fn opening_needs_an_existing_database() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.db");
        let junk = dir.path().join("junk.db");
        fs::write(&junk, "this is not a database at all, just text").unwrap();

        let error = Sqlite::open(&missing).unwrap_err();
        assert!(error.contains("missing.db"), "{error}");
        assert!(!exists(&missing));
        let error = Sqlite::open(&junk).unwrap_err();
        assert!(error.contains("not a database"), "{error}");
    }

    #[test]
    fn creating_makes_a_new_file_but_never_replaces_one() {
        let (dir, path) = database();
        let fresh = dir.path().join("fresh.db");

        let created = Sqlite::create(&fresh).unwrap();

        assert!(exists(&fresh));
        assert_eq!(created.schema().unwrap(), Schema::default());
        assert_eq!(created.name(), fresh.display().to_string());
        let error = Sqlite::create(&path).unwrap_err();
        assert!(error.contains("already exists"), "{error}");
    }

    #[test]
    fn memory_databases_start_empty() {
        let memory = Sqlite::memory().unwrap();

        assert_eq!(memory.schema().unwrap(), Schema::default());
        assert_eq!(memory.name(), "memory");
    }
}
