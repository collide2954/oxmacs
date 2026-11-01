use std::cell::Cell;
use std::panic::{self, AssertUnwindSafe};

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

pub fn recovering() -> bool {
    DEPTH.get() > 0
}

pub fn recover<T>(work: impl FnOnce() -> T) -> Result<T, String> {
    DEPTH.set(DEPTH.get() + 1);
    let outcome = panic::catch_unwind(AssertUnwindSafe(work));
    DEPTH.set(DEPTH.get().saturating_sub(1));
    outcome.map_err(|payload| {
        payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic".to_owned())
    })
}
