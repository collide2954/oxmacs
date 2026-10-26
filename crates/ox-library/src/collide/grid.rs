use std::collections::{BTreeSet, HashMap};
use std::hash::Hash;

use super::cell;
use crate::geometry::Rect;
use crate::geometry::Vec2;

#[derive(Clone, Debug)]
pub struct Grid<T> {
    size: f64,
    cells: HashMap<(i64, i64), Vec<T>>,
    rects: HashMap<T, Rect>,
}

impl<T: Copy + Eq + Hash + Ord> Grid<T> {
    pub fn new(size: f64) -> Self {
        Self {
            size: if size > 0.0 { size } else { 1.0 },
            cells: HashMap::new(),
            rects: HashMap::new(),
        }
    }

    fn covered(&self, rect: Rect) -> impl Iterator<Item = (i64, i64)> + use<T> {
        let size = self.size;
        let (left, right) = (cell(rect.left(), size), cell(rect.right(), size));
        let (top, bottom) = (cell(rect.top(), size), cell(rect.bottom(), size));
        (top..=bottom).flat_map(move |row| (left..=right).map(move |column| (column, row)))
    }

    pub fn len(&self) -> usize {
        self.rects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    pub fn clear(&mut self) {
        self.cells.clear();
        self.rects.clear();
    }

    pub fn rect(&self, id: T) -> Option<Rect> {
        self.rects.get(&id).copied()
    }

    pub fn insert(&mut self, id: T, rect: Rect) {
        self.remove(id);
        for key in self.covered(rect) {
            self.cells.entry(key).or_default().push(id);
        }
        self.rects.insert(id, rect);
    }

    pub fn remove(&mut self, id: T) -> Option<Rect> {
        let rect = self.rects.remove(&id)?;
        for key in self.covered(rect) {
            if let Some(ids) = self.cells.get_mut(&key) {
                ids.retain(|other| *other != id);
                if ids.is_empty() {
                    self.cells.remove(&key);
                }
            }
        }
        Some(rect)
    }

    fn matching(&self, area: Rect, keep: impl Fn(Rect) -> bool) -> Vec<T> {
        let mut found = BTreeSet::new();
        for key in self.covered(area) {
            for id in self.cells.get(&key).into_iter().flatten() {
                if keep(self.rects[id]) {
                    found.insert(*id);
                }
            }
        }
        found.into_iter().collect()
    }

    pub fn query(&self, area: Rect) -> Vec<T> {
        self.matching(area, |rect| rect.overlaps(area))
    }

    pub fn at(&self, point: Vec2) -> Vec<T> {
        self.matching(Rect::at(point, Vec2::ZERO), |rect| rect.contains(point))
    }

    pub fn pairs(&self) -> Vec<(T, T)> {
        let mut found = BTreeSet::new();
        for ids in self.cells.values() {
            for (at, first) in ids.iter().enumerate() {
                for second in &ids[at + 1..] {
                    if self.rects[first].overlaps(self.rects[second]) {
                        found.insert((*first.min(second), *first.max(second)));
                    }
                }
            }
        }
        found.into_iter().collect()
    }
}
