use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crate::graph::Knob;

#[derive(Debug, Clone)]
struct Entry {
    knob: Knob,
    value: Arc<AtomicU32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub name: Arc<str>,
    pub value: f32,
    pub default: f32,
    pub low: f32,
    pub high: f32,
}

#[derive(Debug, Default)]
pub struct Controls(Mutex<Vec<Entry>>);

impl Controls {
    fn entries(&self) -> std::sync::MutexGuard<'_, Vec<Entry>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn slot(&self, knob: &Knob) -> Arc<AtomicU32> {
        let mut entries = self.entries();
        if let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.knob.name == knob.name)
        {
            entry.knob = knob.clone();
            return Arc::clone(&entry.value);
        }
        let value = Arc::new(AtomicU32::new(knob.default.to_bits()));
        entries.push(Entry {
            knob: knob.clone(),
            value: Arc::clone(&value),
        });
        value
    }

    pub fn set(&self, name: &str, value: f32) {
        let mut entries = self.entries();
        match entries.iter().find(|entry| &*entry.knob.name == name) {
            Some(entry) => entry.value.store(value.to_bits(), Ordering::Relaxed),
            None => entries.push(Entry {
                knob: Knob {
                    name: name.into(),
                    default: value,
                    low: value.min(0.0),
                    high: value.max(1.0),
                },
                value: Arc::new(AtomicU32::new(value.to_bits())),
            }),
        }
    }

    pub fn get(&self, name: &str) -> Option<f32> {
        self.entries()
            .iter()
            .find(|entry| &*entry.knob.name == name)
            .map(|entry| f32::from_bits(entry.value.load(Ordering::Relaxed)))
    }

    pub fn list(&self) -> Vec<Shown> {
        self.entries()
            .iter()
            .map(|entry| Shown {
                name: Arc::clone(&entry.knob.name),
                value: f32::from_bits(entry.value.load(Ordering::Relaxed)),
                default: entry.knob.default,
                low: entry.knob.low,
                high: entry.knob.high,
            })
            .collect()
    }
}
