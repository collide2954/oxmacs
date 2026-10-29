use std::cell::Cell;
use std::time::Instant;

use jiff::{SignedDuration, Timestamp};

#[derive(Debug, Clone, Copy)]
struct Pinned {
    started: Instant,
    wall: Timestamp,
    now: Instant,
}

thread_local! {
    static PINNED: Cell<Option<Pinned>> = const { Cell::new(None) };
}

fn pinned() -> Option<Pinned> {
    PINNED.get()
}

pub fn now() -> Instant {
    pinned().map_or_else(Instant::now, |pinned| pinned.now)
}

pub fn wall() -> Timestamp {
    pinned().map_or_else(Timestamp::now, |pinned| {
        let since = SignedDuration::try_from(pinned.now - pinned.started).unwrap_or_default();
        pinned.wall.checked_add(since).unwrap_or(pinned.wall)
    })
}

pub fn pin(now: Option<Instant>) {
    let pinned = now.map(|now| match pinned() {
        Some(before) => Pinned { now, ..before },
        None => Pinned {
            started: now,
            wall: Timestamp::now(),
            now,
        },
    });
    PINNED.set(pinned);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_pinned_clock_moves_only_when_told() {
        let start = Instant::now();
        pin(Some(start));
        let wall = wall();
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(now(), start);
        assert_eq!(super::wall(), wall);

        pin(Some(start + Duration::from_secs(2)));
        assert_eq!(now(), start + Duration::from_secs(2));
        assert_eq!((super::wall() - wall).get_seconds(), 2);

        pin(None);
        assert!(now() > start);
    }
}
