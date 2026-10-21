use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use iced::Task;
use iced::futures::channel::oneshot;
use oxmacs_core::applet::Payload;

use crate::driver::Driver;
use crate::service::{DbId, Reply, Request, answer};

type Job = (Request, oneshot::Sender<Reply>);

#[derive(Debug, Default, Clone)]
pub(crate) struct Workers(Arc<Mutex<HashMap<DbId, mpsc::Sender<Job>>>>);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn spawn() -> mpsc::Sender<Job> {
    let (sender, receiver) = mpsc::channel::<Job>();
    let started = thread::Builder::new()
        .name("oxmacs-database".to_owned())
        .spawn(move || {
            let mut driver: Option<Box<dyn Driver>> = None;
            for (request, reply) in receiver {
                let _ = reply.send(answer(&mut driver, request));
            }
        });
    drop(started);
    sender
}

impl Workers {
    pub(crate) fn run(&self, request: Request) -> Task<Payload> {
        let db = request.db();
        let closing = matches!(request, Request::Close { .. });
        let (sender, receiver) = oneshot::channel();
        {
            let mut workers = lock(&self.0);
            let worker = workers.entry(db).or_insert_with(spawn);
            if worker.send((request, sender)).is_err() {
                workers.remove(&db);
            }
            if closing {
                workers.remove(&db);
            }
        }
        Task::perform(receiver, move |reply| {
            Payload::new(reply.unwrap_or(Reply::Closed { db }))
        })
    }

    #[cfg(test)]
    pub(crate) fn running(&self) -> usize {
        lock(&self.0).len()
    }
}

#[cfg(test)]
mod tests {
    use iced::futures::StreamExt;
    use iced::futures::executor::block_on;
    use iced_test::runtime::Action;

    use super::*;
    use crate::driver::fixtures::database;
    use crate::service::Source;

    fn reply(task: Task<Payload>) -> Reply {
        let mut stream = iced_test::runtime::task::into_stream(task).unwrap();
        loop {
            if let Some(Action::Output(payload)) = block_on(stream.next()) {
                return payload.get::<Reply>().cloned().unwrap();
            }
        }
    }

    #[test]
    fn workers_answer_on_their_own_thread() {
        let (_dir, path) = database();
        let workers = Workers::default();
        let db = DbId(3);

        let opened = reply(workers.run(Request::Open {
            db,
            source: Source::File(path.clone()),
        }));
        let schema = reply(workers.run(Request::Schema { db }));

        assert_eq!(
            opened,
            Reply::Opened {
                db,
                name: Ok(path.display().to_string()),
            }
        );
        assert!(
            matches!(schema, Reply::Schema { schema: Ok(found), .. } if found.tables.len() == 5)
        );
        assert_eq!(workers.running(), 1);
    }

    #[test]
    fn closing_ends_the_worker() {
        let workers = Workers::default();
        let db = DbId(4);
        reply(workers.run(Request::Open {
            db,
            source: Source::Memory,
        }));

        let closed = reply(workers.run(Request::Close { db }));

        assert_eq!(closed, Reply::Closed { db });
        assert_eq!(workers.running(), 0);
    }
}
