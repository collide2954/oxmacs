use crate::geometry::Rect;
use crate::geometry::Vec2;

const TOUCHING: f64 = 1e-9;
const PASSES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub time: f64,
    pub normal: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact<I = usize> {
    pub index: I,
    pub normal: Vec2,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Moved<I = usize> {
    pub rect: Rect,
    pub contacts: Vec<Contact<I>>,
}

impl<I> Moved<I> {
    pub fn on_floor(&self) -> bool {
        self.contacts.iter().any(|contact| contact.normal.y < 0.0)
    }

    pub fn on_ceiling(&self) -> bool {
        self.contacts.iter().any(|contact| contact.normal.y > 0.0)
    }

    pub fn on_wall(&self) -> bool {
        self.contacts.iter().any(|contact| contact.normal.x != 0.0)
    }
}

fn span(start: f64, delta: f64, low: f64, high: f64) -> Option<(f64, f64)> {
    if delta == 0.0 {
        return (start > low + TOUCHING && start < high - TOUCHING)
            .then_some((f64::NEG_INFINITY, f64::INFINITY));
    }
    let (mut near, far) = if delta > 0.0 {
        (low - start, high - start)
    } else {
        (high - start, low - start)
    };
    if near.abs() < TOUCHING {
        near = 0.0;
    }
    Some((near / delta, far / delta))
}

pub fn sweep(moving: Rect, delta: Vec2, solid: Rect) -> Option<Hit> {
    let (x_entry, x_exit) = span(moving.x, delta.x, solid.left() - moving.w, solid.right())?;
    let (y_entry, y_exit) = span(moving.y, delta.y, solid.top() - moving.h, solid.bottom())?;
    let entry = x_entry.max(y_entry);
    let exit = x_exit.min(y_exit);
    if entry >= exit || !(0.0..=1.0).contains(&entry) {
        return None;
    }
    let normal = if x_entry > y_entry {
        Vec2::new(-delta.x.signum(), 0.0)
    } else {
        Vec2::new(0.0, -delta.y.signum())
    };
    Some(Hit {
        time: entry,
        normal,
    })
}

pub fn slide(moving: Rect, delta: Vec2, solids: &[Rect]) -> Moved {
    let mut rect = moving;
    let mut left = delta;
    let mut contacts = Vec::new();
    for _ in 0..PASSES {
        if left == Vec2::ZERO {
            break;
        }
        let nearest = solids
            .iter()
            .enumerate()
            .filter_map(|(index, solid)| Some((index, sweep(rect, left, *solid)?)))
            .min_by(|(_, a), (_, b)| a.time.total_cmp(&b.time));
        let Some((index, hit)) = nearest else {
            rect = rect.translated(left);
            break;
        };
        let solid = solids[index];
        rect = rect.translated(left * hit.time);
        if hit.normal.x < 0.0 {
            rect.x = solid.left() - rect.w;
        } else if hit.normal.x > 0.0 {
            rect.x = solid.right();
        } else if hit.normal.y < 0.0 {
            rect.y = solid.top() - rect.h;
        } else {
            rect.y = solid.bottom();
        }
        contacts.push(Contact {
            index,
            normal: hit.normal,
        });
        left *= 1.0 - hit.time;
        if hit.normal.x == 0.0 {
            left.y = 0.0;
        } else {
            left.x = 0.0;
        }
    }
    Moved { rect, contacts }
}
