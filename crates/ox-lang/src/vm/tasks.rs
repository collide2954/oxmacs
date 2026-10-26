use std::any::Any;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::machine::{Fiber, Generator, Landing, Machine};
use super::{Flow, Runtime, Step, Stop, Wait, interrupted};
use crate::bytecode::Code;
use crate::error::{Error, ErrorKind};
use crate::portable::{Received, Sent};
use crate::span::Span;
use crate::task::{Report, TaskId, Wake};
use crate::value::Value;
use crate::value::promise::Promise;
use crate::vm::build::raised;

const WAITING_SLICE: Duration = Duration::from_millis(10);
const OUTSIDE_WAIT: Duration = Duration::from_millis(50);

#[derive(Debug)]
pub struct Task {
    id: TaskId,
    fiber: Fiber,
    promise: Option<Value>,
    resume: Option<Step>,
    waiting: Option<(Wait, Landing)>,
}

fn closed() -> Error {
    Error::new("This channel is closed")
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

    pub(crate) fn spawn_promise(&mut self, function: Value) -> Result<Value, Error> {
        let fiber = Self::fiber_for(function)?;
        let promise = self.heap.promise(Promise::Pending { awaited: false })?;
        self.start(fiber, Some(promise));
        Ok(promise)
    }

    pub(crate) fn make_generator(&mut self, function: Value) -> Result<Value, Error> {
        let fiber = Self::fiber_for(function)?;
        self.heap
            .generator(Generator::Ready(Box::new(fiber.machine), Landing::Nowhere))
            .map_err(Error::from)
    }

    pub fn promise(&mut self) -> Result<Value, Error> {
        self.heap
            .promise(Promise::Pending { awaited: false })
            .map_err(Error::from)
    }

    pub(crate) fn settle(&mut self, promise: Value, outcome: Promise) -> bool {
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

    pub fn runnable(&self) -> usize {
        self.tasks.len()
    }

    pub fn pending(&self) -> usize {
        self.tasks.len() + self.blocked.len() + self.heap.job_count()
    }

    pub fn next_wake(&self) -> Wake {
        if !self.tasks.is_empty() || !self.parked.is_empty() || self.heap.awaited_jobs() {
            return Wake::Soon;
        }
        let mut earliest: Option<Instant> = None;
        for (wait, _) in self.blocked.iter().filter_map(|task| task.waiting.as_ref()) {
            match wait {
                Wait::Until(deadline) => {
                    earliest = Some(earliest.map_or(*deadline, |known| known.min(*deadline)));
                }
                Wait::Receive(_) | Wait::Send(..) if self.shared(wait) => return Wake::Soon,
                Wait::Promise(_) | Wait::Receive(_) | Wait::Send(..) => {}
            }
        }
        earliest.map_or(Wake::Never, Wake::At)
    }

    pub fn run_tasks(&mut self, budget: Duration, host: &mut dyn Any) -> Report {
        self.schedule(Instant::now() + budget, host);
        Report {
            finished: std::mem::take(&mut self.finished),
            unhandled: std::mem::take(&mut self.unhandled),
            pending: self.pending(),
        }
    }

    fn schedule(&mut self, deadline: Instant, host: &mut dyn Any) {
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
            self.heap.set_running_task(Some(task.id));
            let outcome = self.drive(&mut task.fiber, Some(turn), first, host);
            self.heap.set_running_task(None);
            self.running = None;
            match outcome {
                Ok(Stop::Paused) => self.tasks.push_back(task),
                Ok(Stop::Blocked(wait, landing)) => {
                    task.waiting = Some((wait, landing));
                    self.blocked.push(task);
                }
                Ok(Stop::Finished(value)) => self.complete(&task, Ok(value)),
                Err(error) => self.complete(&task, Err(error)),
            }
        }
    }

    pub(super) fn attempt(&mut self, wait: &Wait, landing: Landing) -> Option<Step> {
        match wait {
            Wait::Until(deadline) => {
                (Instant::now() >= *deadline).then_some(Ok(Flow::Land(landing, Value::Nil)))
            }
            Wait::Promise(promise) => match self.heap.promise_of(*promise)? {
                Promise::Pending { .. } => None,
                Promise::Delivered(value) => Some(Ok(Flow::Land(landing, value))),
                Promise::Failed(value) => Some(Err(raised(&self.heap, value))),
            },
            Wait::Receive(channel) => {
                let channel = Arc::clone(self.heap.channel_of(*channel)?);
                match channel.receive() {
                    Received::Item(item) => {
                        Some(self.rebuild(&item).map(|value| Flow::Land(landing, value)))
                    }
                    Received::Closed => Some(Ok(Flow::Land(landing, Value::Nil))),
                    Received::Empty => None,
                }
            }
            Wait::Send(channel, item) => {
                let channel = self.heap.channel_of(*channel)?;
                if !channel.has_room() {
                    return None;
                }
                match channel.send(item.clone()) {
                    Sent::Done => Some(Ok(Flow::Land(landing, Value::Nil))),
                    Sent::Closed => Some(Err(closed())),
                    Sent::Full => None,
                }
            }
        }
    }

    fn wake(&mut self) {
        self.poll_jobs();
        let mut index = 0;
        while index < self.blocked.len() {
            let Some((wait, landing)) = self.blocked[index].waiting.take() else {
                index += 1;
                continue;
            };
            if let Some(step) = self.attempt(&wait, landing) {
                let mut task = self.blocked.remove(index);
                task.resume = Some(step);
                self.tasks.push_back(task);
            } else {
                self.blocked[index].waiting = Some((wait, landing));
                index += 1;
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
                self.fail_promise(promise, error, None);
            }
        }
    }

    pub(super) fn fail_promise(&mut self, promise: Value, error: Error, value: Option<Value>) {
        let awaited = matches!(
            self.heap.promise_of(promise),
            Some(Promise::Pending { awaited: true })
        );
        let value = value
            .or_else(|| self.error_value(&error).ok())
            .unwrap_or(Value::Nil);
        self.heap.set_promise(promise, Promise::Failed(value));
        if !awaited {
            self.unhandled.push(error);
        }
    }

    fn shared(&self, wait: &Wait) -> bool {
        match wait {
            Wait::Promise(_) => false,
            Wait::Until(_) => true,
            Wait::Receive(channel) | Wait::Send(channel, _) => self
                .heap
                .channel_of(*channel)
                .is_some_and(|channel| Arc::strong_count(channel) > 1),
        }
    }

    fn outside_help(&self, wait: &Wait) -> bool {
        self.heap.job_count() > 0
            || self.shared(wait)
            || self.blocked.iter().any(|task| {
                task.waiting
                    .as_ref()
                    .is_some_and(|(waiting, _)| self.shared(waiting))
            })
    }

    pub(super) fn wait_for(&mut self, wait: &Wait, landing: Landing, host: &mut dyn Any) -> Step {
        loop {
            self.wake();
            if let Some(step) = self.attempt(wait, landing) {
                return step;
            }
            if self.interrupter.take() {
                return Err(interrupted());
            }
            if let Some(error) = self.over_budget() {
                return Err(error);
            }
            if !self.tasks.is_empty() {
                self.schedule(Instant::now() + WAITING_SLICE, host);
                continue;
            }
            if !self.outside_help(wait) {
                return Err(Error::new(match wait {
                    Wait::Promise(_) => "Nothing can deliver this promise",
                    Wait::Receive(_) => "Nothing can send to this channel",
                    Wait::Send(..) => "Nothing can receive from this channel",
                    Wait::Until(_) => "Nothing can wake this timer",
                }));
            }
            self.blocked_on(wait);
            let deadline = std::iter::once(wait)
                .chain(
                    self.blocked
                        .iter()
                        .filter_map(|task| task.waiting.as_ref().map(|(waiting, _)| waiting)),
                )
                .filter_map(|waiting| match waiting {
                    Wait::Until(deadline) => Some(*deadline),
                    _ => None,
                })
                .min();
            let deadline = deadline
                .into_iter()
                .chain(self.budget.map(|(deadline, _)| deadline))
                .min();
            let timeout = deadline.map_or(OUTSIDE_WAIT, |deadline| {
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(OUTSIDE_WAIT)
            });
            self.heap.signal().wait(timeout);
        }
    }

    pub(super) fn task_roots(&self, roots: &mut Vec<Value>, codes: &mut Vec<Rc<Code>>) {
        for task in self.tasks.iter().chain(&self.blocked) {
            task.fiber.roots(roots, codes);
            roots.extend(task.promise);
            roots.extend(task.waiting.as_ref().map(|(wait, _)| wait.value()));
            if let Some(Ok(Flow::Land(_, value))) = &task.resume {
                roots.push(*value);
            }
        }
        for (fiber, wait) in &self.parked {
            fiber.roots(roots, codes);
            roots.push(wait.value());
        }
        for (_, result) in &self.finished {
            if let Ok(value) = result {
                roots.push(*value);
            }
        }
        roots.extend(self.stand_ins.values().copied());
    }
}
