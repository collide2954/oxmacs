use super::Runtime;
use crate::error::{Error, ErrorKind};
use crate::isolate::work;
use crate::portable::{Failure, Portable};
use crate::symbol::Sym;
use crate::value::Value;
use crate::value::print;
use crate::value::promise::Promise;

impl Runtime {
    fn name_of(&self, value: Value, what: &str) -> Result<String, Error> {
        match value {
            Value::Symbol(name) => Ok(name.name().to_owned()),
            _ => self.heap.text(value).map(str::to_owned).ok_or_else(|| {
                Error::new(format!(
                    "worker expects {what}, got {}",
                    print::brief(&self.heap, value)
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
        let roots = self.roots.clone();
        let sources = std::sync::Arc::clone(&self.sources);
        let setup = self.worker_setup;
        let name = format!("ox worker {module}");
        self.heap.background(&name, move || {
            work(&roots, sources, setup, &module, function, &arguments)
        })
    }

    pub(super) fn poll_jobs(&mut self) {
        for (promise, outcome) in self.heap.finished_jobs() {
            self.finish_job(promise, outcome);
        }
    }

    fn finish_job(&mut self, promise: Value, outcome: Result<Portable, Failure>) {
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
