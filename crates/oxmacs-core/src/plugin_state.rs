use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;

#[derive(Default)]
pub struct States(HashMap<TypeId, Box<dyn Any + Send>>);

impl fmt::Debug for States {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("States")
            .field("count", &self.0.len())
            .finish()
    }
}

impl States {
    pub fn get<T: Any + Send>(&self) -> Option<&T> {
        self.0.get(&TypeId::of::<T>())?.downcast_ref()
    }

    pub fn get_mut<T: Any + Send>(&mut self) -> Option<&mut T> {
        self.0.get_mut(&TypeId::of::<T>())?.downcast_mut()
    }

    pub fn set<T: Any + Send>(&mut self, state: T) {
        self.0.insert(TypeId::of::<T>(), Box::new(state));
    }

    pub fn take<T: Any + Send>(&mut self) -> Option<T> {
        self.0
            .remove(&TypeId::of::<T>())?
            .downcast()
            .ok()
            .map(|state| *state)
    }
}
