use std::rc::Rc;
use std::time::{Duration, Instant};

use super::{Machine, Runtime};
use crate::error::Error;
use crate::span::Span;
use crate::task::{Report, TaskId};
use crate::value::Value;

#[derive(Debug)]
pub struct Task {
    pub(super) id: TaskId,
    pub(super) machine: Machine,
    pub(super) cancelled: bool,
}

impl Runtime {
    pub fn spawn(&mut self, function: Value, arguments: &[Value]) -> Result<TaskId, Error> {
        let code = Self::call_code(function, arguments, Span::new(0, 0))?;
        self.next_task += 1;
        let id = TaskId(self.next_task);
        self.tasks.push_back(Task {
            id,
            machine: Machine::new(&Rc::new(code)),
            cancelled: false,
        });
        Ok(id)
    }

    pub fn cancel(&mut self, id: TaskId) -> bool {
        match self.tasks.iter_mut().find(|task| task.id == id) {
            Some(task) => {
                task.cancelled = true;
                true
            }
            None => false,
        }
    }

    pub fn run_tasks(&mut self, budget: Duration) -> Report {
        let deadline = Instant::now() + budget;
        let mut report = Report::default();
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let waiting = u32::try_from(self.tasks.len()).unwrap_or(u32::MAX);
            let Some(mut task) = self.tasks.pop_front() else {
                break;
            };
            let turn = now + (deadline - now) / waiting;
            let interrupt = std::mem::take(&mut task.cancelled);
            self.running = Some(task.id);
            let outcome = self.drive(&mut task.machine, Some(turn), interrupt);
            self.running = None;
            match outcome {
                Ok(Some(value)) => report.finished.push((task.id, Ok(value))),
                Ok(None) => self.tasks.push_back(task),
                Err(error) => report.finished.push((task.id, Err(error))),
            }
        }
        report.pending = self.tasks.len();
        report
    }
}
