use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

use super::Heap;
use crate::error::Error;
use crate::portable::{Failure, Portable, Signal};
use crate::symbol::Sym;
use crate::value::Value;
use crate::value::promise::Promise;

type Outcome = Arc<Mutex<Option<Result<Portable, Failure>>>>;

#[derive(Debug)]
pub(crate) struct Job {
    promise: Value,
    outcome: Outcome,
    thread: JoinHandle<()>,
}

fn portable(json: serde_json::Value) -> Portable {
    match json {
        serde_json::Value::Null => Portable::Nil,
        serde_json::Value::Bool(flag) => Portable::Bool(flag),
        serde_json::Value::Number(number) => number.as_i64().map_or_else(
            || Portable::Float(number.as_f64().unwrap_or(f64::NAN)),
            Portable::Int,
        ),
        serde_json::Value::String(text) => Portable::String(text.into()),
        serde_json::Value::Array(items) => {
            Portable::Vector(items.into_iter().map(portable).collect())
        }
        serde_json::Value::Object(entries) => Portable::Map(
            entries
                .into_iter()
                .map(|(key, value)| (Portable::Keyword(Sym::intern(&key)), portable(value)))
                .collect(),
        ),
    }
}

impl Heap {
    pub(crate) fn signal(&self) -> &Arc<Signal> {
        &self.signal
    }

    pub fn background_json(
        &mut self,
        name: &str,
        work: impl FnOnce() -> Result<serde_json::Value, String> + Send + 'static,
    ) -> Result<Value, Error> {
        self.background(name, move || {
            work().map(portable).map_err(|message| Failure {
                value: None,
                message,
            })
        })
    }

    pub fn background(
        &mut self,
        name: &str,
        work: impl FnOnce() -> Result<Portable, Failure> + Send + 'static,
    ) -> Result<Value, Error> {
        let promise = self.promise(Promise::Pending { awaited: false })?;
        let outcome: Outcome = Arc::default();
        let slot = Arc::clone(&outcome);
        let signal = Arc::clone(&self.signal);
        let thread = thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                let result = work();
                *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(result);
                signal.notify();
            })
            .map_err(|error| Error::new(format!("{name} could not start: {error}")))?;
        self.jobs.push(Job {
            promise,
            outcome,
            thread,
        });
        Ok(promise)
    }

    pub(crate) fn job_count(&self) -> usize {
        self.jobs.len()
    }

    pub(crate) fn awaited_jobs(&self) -> bool {
        self.jobs.iter().any(|job| {
            matches!(
                self.promise_of(job.promise),
                Some(Promise::Pending { awaited: true })
            )
        })
    }

    pub(crate) fn job_promises(&self) -> impl Iterator<Item = Value> + '_ {
        self.jobs.iter().map(|job| job.promise)
    }

    pub(crate) fn finished_jobs(&mut self) -> Vec<(Value, Result<Portable, Failure>)> {
        let mut finished = Vec::new();
        let mut index = 0;
        while index < self.jobs.len() {
            let job = &self.jobs[index];
            let ended = job.thread.is_finished();
            let taken = job
                .outcome
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            let outcome = match taken {
                Some(outcome) => outcome,
                None if ended => Err(Failure {
                    value: None,
                    message: "The work stopped unexpectedly".to_owned(),
                }),
                None => {
                    index += 1;
                    continue;
                }
            };
            let job = self.jobs.remove(index);
            finished.push((job.promise, outcome));
        }
        finished
    }
}
