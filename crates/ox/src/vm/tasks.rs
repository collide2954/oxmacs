use std::rc::Rc;
use std::time::{Duration, Instant};

use super::machine::{Fiber, Generator, Landing, Machine};
use super::{Flow, Runtime, Step, Stop, interrupted};
use crate::bytecode::Code;
use crate::error::{Error, ErrorKind};
use crate::span::Span;
use crate::task::{Report, TaskId};
use crate::value::Value;
use crate::value::promise::Promise;
use crate::vm::build::{full, raised};

const WAITING_SLICE: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub struct Task {
    id: TaskId,
    fiber: Fiber,
    promise: Option<Value>,
    resume: Option<Step>,
    waiting: Option<(Value, Landing)>,
}

impl Runtime {
    fn fiber_for(function: Value) -> Result<Fiber, Error> {
        let code = Self::call_code(function, &[], Span::new(0, 0))?;
        Ok(Fiber::new(Machine::new(&Rc::new(code))))
    }

    fn start(&mut self, fiber: Fiber, promise: Option<Value>) -> TaskId {
        self.next_task += 1;
        let id = TaskId(self.next_task);
        self.tasks.push_back(Task {
            id,
            fiber,
            promise,
            resume: None,
            waiting: None,
        });
        id
    }

    pub fn spawn(&mut self, function: Value, arguments: &[Value]) -> Result<TaskId, Error> {
        let code = Self::call_code(function, arguments, Span::new(0, 0))?;
        Ok(self.start(Fiber::new(Machine::new(&Rc::new(code))), None))
    }

    pub(super) fn spawn_promise(&mut self, function: Value) -> Result<Value, Error> {
        let fiber = Self::fiber_for(function)?;
        let promise = self
            .heap
            .promise(Promise::Pending { awaited: false })
            .map_err(full)?;
        self.start(fiber, Some(promise));
        Ok(promise)
    }

    pub(super) fn make_generator(&mut self, function: Value) -> Result<Value, Error> {
        let fiber = Self::fiber_for(function)?;
        self.heap
            .generator(Generator::Ready(Box::new(fiber.machine), Landing::Nowhere))
            .map_err(full)
    }

    pub fn promise(&mut self) -> Result<Value, Error> {
        self.heap
            .promise(Promise::Pending { awaited: false })
            .map_err(full)
    }

    pub fn settle(&mut self, promise: Value, outcome: Promise) -> bool {
        match self.heap.promise_of(promise) {
            Some(Promise::Pending { .. }) => self.heap.set_promise(promise, outcome),
            _ => false,
        }
    }

    pub fn cancel(&mut self, id: TaskId) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|task| task.id == id) {
            task.resume = Some(Err(interrupted()));
            return true;
        }
        let Some(index) = self.blocked.iter().position(|task| task.id == id) else {
            return false;
        };
        let mut task = self.blocked.remove(index);
        task.waiting = None;
        task.resume = Some(Err(interrupted()));
        self.tasks.push_back(task);
        true
    }

    pub fn run_tasks(&mut self, budget: Duration) -> Report {
        self.schedule(Instant::now() + budget);
        Report {
            finished: std::mem::take(&mut self.finished),
            unhandled: std::mem::take(&mut self.unhandled),
            pending: self.tasks.len() + self.blocked.len(),
        }
    }

    fn schedule(&mut self, deadline: Instant) {
        loop {
            self.wake();
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let waiting = u32::try_from(self.tasks.len()).unwrap_or(u32::MAX);
            let Some(mut task) = self.tasks.pop_front() else {
                break;
            };
            let turn = now + (deadline - now) / waiting;
            let first = task.resume.take();
            self.running = task.promise;
            let outcome = self.drive(&mut task.fiber, Some(turn), first);
            self.running = None;
            match outcome {
                Ok(Stop::Paused) => self.tasks.push_back(task),
                Ok(Stop::Blocked(promise, landing)) => {
                    task.waiting = Some((promise, landing));
                    self.blocked.push(task);
                }
                Ok(Stop::Finished(value)) => self.complete(&task, Ok(value)),
                Err(error) => self.complete(&task, Err(error)),
            }
        }
    }

    fn outcome_of(&self, promise: Value, landing: Landing) -> Option<Step> {
        match self.heap.promise_of(promise)? {
            Promise::Pending { .. } => None,
            Promise::Delivered(value) => Some(Ok(Flow::Land(landing, value))),
            Promise::Failed(value) => Some(Err(raised(&self.heap, value))),
        }
    }

    fn wake(&mut self) {
        let mut index = 0;
        while index < self.blocked.len() {
            let ready = self.blocked[index]
                .waiting
                .and_then(|(promise, landing)| self.outcome_of(promise, landing));
            match ready {
                Some(step) => {
                    let mut task = self.blocked.remove(index);
                    task.waiting = None;
                    task.resume = Some(step);
                    self.tasks.push_back(task);
                }
                None => index += 1,
            }
        }
    }

    fn complete(&mut self, task: &Task, result: Result<Value, Error>) {
        let Some(promise) = task.promise else {
            self.finished.push((task.id, result));
            return;
        };
        match result {
            Ok(value) => {
                self.heap.set_promise(promise, Promise::Delivered(value));
            }
            Err(error) => {
                let error = if error.kind == ErrorKind::Interrupt {
                    Error::new("The task was interrupted")
                } else {
                    error
                };
                let awaited = matches!(
                    self.heap.promise_of(promise),
                    Some(Promise::Pending { awaited: true })
                );
                let value = self.error_value(&error).unwrap_or(Value::Nil);
                self.heap.set_promise(promise, Promise::Failed(value));
                if !awaited {
                    self.unhandled.push(error);
                }
            }
        }
    }

    pub(super) fn wait_for(&mut self, promise: Value, landing: Landing) -> Step {
        loop {
            if let Some(step) = self.outcome_of(promise, landing) {
                return step;
            }
            self.wake();
            if self.tasks.is_empty() {
                return Err(Error::new("Nothing can deliver this promise"));
            }
            self.schedule(Instant::now() + WAITING_SLICE);
        }
    }

    pub(super) fn task_roots(&self, roots: &mut Vec<Value>, codes: &mut Vec<Rc<Code>>) {
        for task in self.tasks.iter().chain(&self.blocked) {
            task.fiber.roots(roots, codes);
            roots.extend(task.promise);
            roots.extend(task.waiting.map(|(promise, _)| promise));
            if let Some(Ok(Flow::Land(_, value))) = &task.resume {
                roots.push(*value);
            }
        }
        for (fiber, promise) in &self.parked {
            fiber.roots(roots, codes);
            roots.push(*promise);
        }
        for (_, result) in &self.finished {
            if let Ok(value) = result {
                roots.push(*value);
            }
        }
    }
}
