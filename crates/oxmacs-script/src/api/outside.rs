use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ox_lang::sink::{Sink, Started};
use oxmacs_core::editor::Editor;
use oxmacs_core::external::process::Process;

#[derive(Clone)]
pub struct Waker(Arc<dyn Fn() + Send + Sync>);

impl Waker {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(wake))
    }

    pub fn wake(&self) {
        (self.0)();
    }
}

impl fmt::Debug for Waker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Waker")
    }
}

#[derive(Debug)]
enum Heard {
    Started(u64, Started),
    Ended(u64, Option<i32>),
    Printed(String),
}

#[derive(Debug, Default)]
pub(crate) struct Outside {
    ids: Mutex<Option<Arc<AtomicU64>>>,
    heard: Mutex<Vec<Heard>>,
    waker: Mutex<Option<Waker>>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Outside {
    pub(crate) fn number_processes(&self, ids: Arc<AtomicU64>) {
        *locked(&self.ids) = Some(ids);
    }

    pub(crate) fn wake_with(&self, waker: Waker) {
        *locked(&self.waker) = Some(waker);
    }

    fn wake(&self) {
        let waker = locked(&self.waker).clone();
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    pub(crate) fn deliver(&self, editor: &mut Editor, mut print: impl FnMut(&mut Editor, &str)) {
        let heard = std::mem::take(&mut *locked(&self.heard));
        let mut printed = String::new();
        for heard in heard {
            match heard {
                Heard::Printed(text) => printed.push_str(&text),
                Heard::Started(id, started) => {
                    if let Ok(process) = started.handle.downcast::<Process>() {
                        editor.track_ox_process(
                            id,
                            &started.program,
                            started.args,
                            started.directory,
                            Process::clone(&process),
                        );
                    }
                }
                Heard::Ended(id, code) => editor.ox_process_ended(id, code),
            }
        }
        if !printed.is_empty() {
            print(editor, &printed);
        }
    }
}

impl Sink for Outside {
    fn process_started(&self, process: Started) -> Option<u64> {
        let id = locked(&self.ids).as_ref()?.fetch_add(1, Ordering::Relaxed);
        locked(&self.heard).push(Heard::Started(id, process));
        Some(id)
    }

    fn process_ended(&self, id: u64, code: Option<i32>) {
        locked(&self.heard).push(Heard::Ended(id, code));
        self.wake();
    }

    fn work_ready(&self) {
        self.wake();
    }

    fn printed(&self, text: &str) {
        locked(&self.heard).push(Heard::Printed(text.to_owned()));
    }
}
