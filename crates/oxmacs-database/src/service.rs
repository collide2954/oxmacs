use std::collections::HashMap;
use std::path::PathBuf;

use crate::driver::{Driver, Page, Query, Schema, Sqlite};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DbId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    File(PathBuf),
    Create(PathBuf),
    Memory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Open {
        db: DbId,
        source: Source,
    },
    Schema {
        db: DbId,
    },
    Count {
        db: DbId,
        query: Query,
    },
    Rows {
        db: DbId,
        query: Query,
        offset: u64,
        limit: u64,
    },
    Close {
        db: DbId,
    },
}

impl Request {
    pub fn db(&self) -> DbId {
        match self {
            Self::Open { db, .. }
            | Self::Schema { db }
            | Self::Count { db, .. }
            | Self::Rows { db, .. }
            | Self::Close { db } => *db,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Opened {
        db: DbId,
        name: Result<String, String>,
    },
    Schema {
        db: DbId,
        schema: Result<Schema, String>,
    },
    Counted {
        db: DbId,
        query: Query,
        count: Result<u64, String>,
    },
    Rows {
        db: DbId,
        query: Query,
        offset: u64,
        page: Result<Page, String>,
    },
    Closed {
        db: DbId,
    },
}

fn connect(source: &Source) -> Result<Box<dyn Driver>, String> {
    let driver = match source {
        Source::File(path) => Sqlite::open(path)?,
        Source::Create(path) => Sqlite::create(path)?,
        Source::Memory => Sqlite::memory()?,
    };
    Ok(Box::new(driver))
}

const CLOSED: &str = "The database is closed";

pub fn answer(driver: &mut Option<Box<dyn Driver>>, request: Request) -> Reply {
    match request {
        Request::Open { db, source } => {
            let name = connect(&source).map(|opened| {
                let name = opened.name();
                *driver = Some(opened);
                name
            });
            Reply::Opened { db, name }
        }
        Request::Schema { db } => Reply::Schema {
            db,
            schema: driver
                .as_ref()
                .ok_or_else(|| CLOSED.to_owned())
                .and_then(|driver| driver.schema()),
        },
        Request::Count { db, query } => Reply::Counted {
            db,
            count: driver
                .as_ref()
                .ok_or_else(|| CLOSED.to_owned())
                .and_then(|driver| driver.count(&query)),
            query,
        },
        Request::Rows {
            db,
            query,
            offset,
            limit,
        } => Reply::Rows {
            db,
            page: driver
                .as_ref()
                .ok_or_else(|| CLOSED.to_owned())
                .and_then(|driver| driver.rows(&query, offset, limit)),
            query,
            offset,
        },
        Request::Close { db } => {
            *driver = None;
            Reply::Closed { db }
        }
    }
}

#[derive(Debug, Default)]
pub struct Service {
    drivers: HashMap<DbId, Option<Box<dyn Driver>>>,
}

impl Service {
    pub fn handle(&mut self, request: Request) -> Reply {
        let slot = self.drivers.entry(request.db()).or_default();
        answer(slot, request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::fixtures::{database, query};

    #[test]
    fn requests_open_read_and_close_a_database() {
        let (_dir, path) = database();
        let mut service = Service::default();
        let db = DbId(1);

        assert_eq!(
            service.handle(Request::Open {
                db,
                source: Source::File(path.clone()),
            }),
            Reply::Opened {
                db,
                name: Ok(path.display().to_string()),
            }
        );
        let Reply::Schema { schema, .. } = service.handle(Request::Schema { db }) else {
            panic!("expected a schema");
        };
        assert_eq!(schema.unwrap().tables.len(), 5);
        assert_eq!(
            service.handle(Request::Count {
                db,
                query: query("albums"),
            }),
            Reply::Counted {
                db,
                query: query("albums"),
                count: Ok(4),
            }
        );
        let Reply::Rows { offset, page, .. } = service.handle(Request::Rows {
            db,
            query: query("albums"),
            offset: 2,
            limit: 10,
        }) else {
            panic!("expected rows");
        };
        assert_eq!(offset, 2);
        assert_eq!(page.unwrap().rows.len(), 2);
        assert_eq!(service.handle(Request::Close { db }), Reply::Closed { db });
        assert_eq!(
            service.handle(Request::Schema { db }),
            Reply::Schema {
                db,
                schema: Err("The database is closed".to_owned()),
            }
        );
    }

    #[test]
    fn failed_opens_say_why() {
        let dir = tempfile::tempdir().unwrap();
        let mut service = Service::default();

        let reply = service.handle(Request::Open {
            db: DbId(7),
            source: Source::File(dir.path().join("missing.db")),
        });

        let Reply::Opened {
            name: Err(error), ..
        } = reply
        else {
            panic!("expected a failure");
        };
        assert!(error.contains("missing.db"), "{error}");
    }

    #[test]
    fn each_database_keeps_its_own_connection() {
        let (_dir, path) = database();
        let mut service = Service::default();
        service.handle(Request::Open {
            db: DbId(1),
            source: Source::Memory,
        });
        service.handle(Request::Open {
            db: DbId(2),
            source: Source::File(path),
        });

        let tables = |reply: Reply| match reply {
            Reply::Schema { schema, .. } => schema.unwrap().tables.len(),
            other => panic!("{other:?}"),
        };

        assert_eq!(tables(service.handle(Request::Schema { db: DbId(1) })), 0);
        assert_eq!(tables(service.handle(Request::Schema { db: DbId(2) })), 5);
    }
}
