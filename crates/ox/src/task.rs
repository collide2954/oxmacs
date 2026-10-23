use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::error::Error;
use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(pub(crate) u64);

#[derive(Debug, Default)]
pub struct Report {
    pub finished: Vec<(TaskId, Result<Value, Error>)>,
    pub unhandled: Vec<Error>,
    pub pending: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wake {
    Soon,
    At(Instant),
    Never,
}

#[derive(Debug, Clone, Default)]
pub struct Interrupter(Arc<AtomicBool>);

impl Interrupter {
    pub fn interrupt(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub(crate) fn take(&self) -> bool {
        self.0.swap(false, Ordering::Relaxed)
    }
}
