use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

use super::Heap;
use crate::error::Error;
use crate::portable::{Failure, Portable, Signal};
use crate::value::Value;
use crate::value::promise::Promise;

type Outcome = Arc<Mutex<Option<Result<Portable, Failure>>>>;

#[derive(Debug)]
pub(crate) struct Job {
    promise: Value,
    outcome: Outcome,
    thread: JoinHandle<()>,
    carried: Vec<(Value, Value)>,
}

#[derive(Debug)]
pub(crate) struct Finished {
    pub promise: Value,
    pub outcome: Result<Portable, Failure>,
    pub carried: Vec<(Value, Value)>,
}

impl Heap {
    pub(crate) fn signal(&self) -> &Arc<Signal> {
        &self.signal
    }

    pub fn background(
        &mut self,
        name: &str,
        work: impl FnOnce() -> Result<Portable, Failure> + Send + 'static,
    ) -> Result<Value, Error> {
        self.background_carrying(name, &[], work)
    }

    pub fn background_carrying(
        &mut self,
        name: &str,
        carried: &[(&str, Value)],
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
            carried: carried
                .iter()
                .map(|(key, value)| (Value::keyword(key), *value))
                .collect(),
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

    pub(crate) fn job_roots(&self) -> impl Iterator<Item = Value> + '_ {
        self.jobs.iter().flat_map(|job| {
            std::iter::once(job.promise).chain(job.carried.iter().map(|(_, value)| *value))
        })
    }

    pub(crate) fn finished_jobs(&mut self) -> Vec<Finished> {
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
            finished.push(Finished {
                promise: job.promise,
                outcome,
                carried: job.carried,
            });
        }
        finished
    }
}
