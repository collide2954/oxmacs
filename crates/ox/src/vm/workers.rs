use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

use super::Runtime;
use crate::error::{Error, ErrorKind};
use crate::isolate::work;
use crate::portable::{Failure, Portable};
use crate::symbol::Sym;
use crate::value::Value;
use crate::value::print;
use crate::value::promise::Promise;
use crate::vm::build::full;

type Outcome = Arc<Mutex<Option<Result<Portable, Failure>>>>;

#[derive(Debug)]
pub struct Worker {
    pub(super) promise: Value,
    outcome: Outcome,
    thread: JoinHandle<()>,
}

impl Runtime {
    fn name_of(&self, value: Value, what: &str) -> Result<String, Error> {
        match value {
            Value::Symbol(name) => Ok(name.name().to_owned()),
            _ => self.heap.text(value).map(str::to_owned).ok_or_else(|| {
                Error::new(format!(
                    "worker expects {what}, got {}",
                    print::write(&self.heap, value)
                ))
                .of(ErrorKind::Type)
            }),
        }
    }

    pub(super) fn start_worker(&mut self, arguments: &[Value]) -> Result<Value, Error> {
        let [module, function, rest @ ..] = arguments else {
            return Err(
                Error::new("worker takes a module and a function name").of(ErrorKind::Arity)
            );
        };
        let module = self.name_of(*module, "a module name")?;
        let function = Sym::intern(&self.name_of(*function, "a function name")?);
        let arguments = rest
            .iter()
            .map(|argument| self.portable(*argument, "to a worker"))
            .collect::<Result<Vec<_>, Error>>()?;
        let promise = self
            .heap
            .promise(Promise::Pending { awaited: false })
            .map_err(full)?;
        let outcome: Outcome = Arc::default();
        let slot = Arc::clone(&outcome);
        let signal = Arc::clone(&self.signal);
        let roots = self.roots.clone();
        let setup = self.worker_setup;
        let thread = thread::Builder::new()
            .name(format!("ox worker {module}"))
            .spawn(move || {
                let result = work(&roots, setup, &module, function, &arguments);
                *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(result);
                signal.notify();
            })
            .map_err(|error| Error::new(format!("The worker could not start: {error}")))?;
        self.workers.push(Worker {
            promise,
            outcome,
            thread,
        });
        Ok(promise)
    }

    pub(super) fn poll_workers(&mut self) {
        let mut index = 0;
        while index < self.workers.len() {
            let worker = &self.workers[index];
            let finished = worker.thread.is_finished();
            let taken = worker
                .outcome
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            let outcome = match taken {
                Some(outcome) => outcome,
                None if finished => Err(Failure {
                    value: None,
                    message: "The worker stopped unexpectedly".to_owned(),
                }),
                None => {
                    index += 1;
                    continue;
                }
            };
            let worker = self.workers.remove(index);
            self.finish_worker(worker.promise, outcome);
        }
    }

    fn finish_worker(&mut self, promise: Value, outcome: Result<Portable, Failure>) {
        match outcome {
            Ok(portable) => match self.rebuild(&portable) {
                Ok(value) => {
                    self.heap.set_promise(promise, Promise::Delivered(value));
                }
                Err(error) => self.fail_promise(promise, error, None),
            },
            Err(failure) => {
                let value = failure
                    .value
                    .and_then(|portable| self.rebuild(&portable).ok());
                self.fail_promise(promise, Error::new(failure.message), value);
            }
        }
    }
}
