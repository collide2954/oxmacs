use std::any::Any;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use num_bigint::BigInt;

use crate::symbol::Sym;
use crate::value::record::Family;

#[derive(Debug, Clone)]
pub enum Portable {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Char(char),
    Symbol(Sym),
    Keyword(Sym),
    String(Arc<str>),
    BigInt(BigInt),
    List(Vec<Portable>, Box<Portable>),
    Vector(Vec<Portable>),
    Map(Vec<(Portable, Portable)>),
    Set(Vec<Portable>),
    Record {
        module: Sym,
        name: Sym,
        family: Family,
        fields: Box<[Sym]>,
        values: Vec<Portable>,
    },
    Channel(Arc<Channel>),
    Foreign(Sym, Arc<dyn Any + Send + Sync>),
}

impl Portable {
    pub fn keyword(name: &str) -> Self {
        Self::Keyword(Sym::intern(name))
    }

    pub fn keyed<'a>(entries: impl IntoIterator<Item = (&'a str, Self)>) -> Self {
        Self::Map(
            entries
                .into_iter()
                .map(|(name, value)| (Self::keyword(name), value))
                .collect(),
        )
    }
}

#[derive(Debug, Default)]
pub struct Signal {
    raised: Mutex<bool>,
    condvar: Condvar,
}

impl Signal {
    pub(crate) fn notify(&self) {
        *self.raised.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.condvar.notify_all();
    }

    pub(crate) fn wait(&self, timeout: Duration) {
        let raised = self.raised.lock().unwrap_or_else(PoisonError::into_inner);
        let (mut raised, _) = self
            .condvar
            .wait_timeout_while(raised, timeout, |raised| !*raised)
            .unwrap_or_else(PoisonError::into_inner);
        *raised = false;
    }
}

#[derive(Debug, Default)]
struct State {
    items: VecDeque<Portable>,
    closed: bool,
    ended: bool,
    watchers: Vec<Weak<Signal>>,
}

#[derive(Debug)]
pub struct Channel {
    capacity: usize,
    state: Mutex<State>,
}

#[derive(Debug)]
pub enum Sent {
    Done,
    Full,
    Closed,
}

#[derive(Debug)]
pub enum Received {
    Item(Portable),
    Empty,
    Closed,
}

impl Channel {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            state: Mutex::default(),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn changed(state: &mut State) {
        state.watchers.retain(|watcher| match watcher.upgrade() {
            Some(signal) => {
                signal.notify();
                true
            }
            None => false,
        });
    }

    pub fn send(&self, item: Portable) -> Sent {
        let mut state = self.state();
        if state.closed {
            return Sent::Closed;
        }
        if state.items.len() >= self.capacity {
            return Sent::Full;
        }
        state.items.push_back(item);
        Self::changed(&mut state);
        Sent::Done
    }

    pub(crate) fn receive(&self) -> Received {
        let mut state = self.state();
        let Some(item) = state.items.pop_front() else {
            return if state.closed {
                state.ended = true;
                Received::Closed
            } else {
                Received::Empty
            };
        };
        Self::changed(&mut state);
        Received::Item(item)
    }

    pub fn close(&self) {
        let mut state = self.state();
        state.closed = true;
        Self::changed(&mut state);
    }

    pub(crate) fn done(&self) -> bool {
        self.state().ended
    }

    pub(crate) fn has_room(&self) -> bool {
        let state = self.state();
        state.closed || state.items.len() < self.capacity
    }

    pub(crate) fn watch(&self, signal: &Arc<Signal>) {
        let mut state = self.state();
        let known = state
            .watchers
            .iter()
            .any(|watcher| watcher.as_ptr() == Arc::as_ptr(signal));
        if !known {
            state.watchers.push(Arc::downgrade(signal));
        }
    }
}

#[derive(Debug)]
pub struct Failure {
    pub value: Option<Portable>,
    pub message: String,
}
