const SMALLEST: f32 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(pub(in crate::editor) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    Rows,
    Columns,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tiling {
    Window(WindowId),
    Split {
        axis: Axis,
        parts: Vec<(f32, Tiling)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    const WHOLE: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    };
}

impl Tiling {
    pub fn windows(&self) -> Vec<WindowId> {
        match self {
            Self::Window(id) => vec![*id],
            Self::Split { parts, .. } => {
                parts.iter().flat_map(|(_, part)| part.windows()).collect()
            }
        }
    }

    fn contains(&self, target: WindowId) -> bool {
        match self {
            Self::Window(id) => *id == target,
            Self::Split { parts, .. } => parts.iter().any(|(_, part)| part.contains(target)),
        }
    }

    pub(in crate::editor) fn split(&mut self, target: WindowId, axis: Axis, new: WindowId) -> bool {
        match self {
            Self::Window(id) if *id == target => {
                *self = Self::Split {
                    axis,
                    parts: vec![(0.5, Self::Window(target)), (0.5, Self::Window(new))],
                };
                true
            }
            Self::Window(_) => false,
            Self::Split { axis: own, parts } => {
                if *own == axis
                    && let Some(index) = parts
                        .iter()
                        .position(|(_, part)| *part == Self::Window(target))
                {
                    let half = parts[index].0 / 2.0;
                    parts[index].0 = half;
                    parts.insert(index + 1, (half, Self::Window(new)));
                    return true;
                }
                parts
                    .iter_mut()
                    .any(|(_, part)| part.split(target, axis, new))
            }
        }
    }

    pub(in crate::editor) fn remove(&mut self, target: WindowId) -> Option<WindowId> {
        let Self::Split { parts, .. } = self else {
            return None;
        };
        if let Some(index) = parts
            .iter()
            .position(|(_, part)| *part == Self::Window(target))
        {
            let (weight, _) = parts.remove(index);
            let neighbor = index.saturating_sub(1).min(parts.len() - 1);
            parts[neighbor].0 += weight;
            let next = parts[neighbor].1.windows().first().copied();
            if parts.len() == 1 {
                *self = parts.remove(0).1;
            }
            return next;
        }
        parts.iter_mut().find_map(|(_, part)| part.remove(target))
    }

    pub fn rects(&self) -> Vec<(WindowId, Rect)> {
        let mut rects = Vec::new();
        self.place(Rect::WHOLE, &mut rects);
        rects
    }

    fn place(&self, area: Rect, rects: &mut Vec<(WindowId, Rect)>) {
        match self {
            Self::Window(id) => rects.push((*id, area)),
            Self::Split { axis, parts } => {
                let mut offset = 0.0;
                for (weight, part) in parts {
                    let piece = match axis {
                        Axis::Rows => Rect {
                            y: area.y + offset * area.height,
                            height: weight * area.height,
                            ..area
                        },
                        Axis::Columns => Rect {
                            x: area.x + offset * area.width,
                            width: weight * area.width,
                            ..area
                        },
                    };
                    part.place(piece, rects);
                    offset += weight;
                }
            }
        }
    }

    pub(in crate::editor) fn balance(&mut self) {
        if let Self::Split { parts, .. } = self {
            let share = 1.0 / count_as_f32(parts.len());
            for (weight, part) in parts {
                *weight = share;
                part.balance();
            }
        }
    }

    pub(in crate::editor) fn share(&self, target: WindowId, axis: Axis) -> Option<f32> {
        let Self::Split { axis: own, parts } = self else {
            return None;
        };
        let (weight, part) = parts.iter().find(|(_, part)| part.contains(target))?;
        part.share(target, axis)
            .or_else(|| (*own == axis).then_some(*weight))
    }

    pub(in crate::editor) fn resize(&mut self, target: WindowId, axis: Axis, delta: f32) -> bool {
        let Self::Split { axis: own, parts } = self else {
            return false;
        };
        let Some(index) = parts.iter().position(|(_, part)| part.contains(target)) else {
            return false;
        };
        if parts[index].1.resize(target, axis, delta) {
            return true;
        }
        if *own != axis || parts.len() < 2 {
            return false;
        }
        let neighbor = if index + 1 < parts.len() {
            index + 1
        } else {
            index - 1
        };
        let delta = delta
            .min(parts[neighbor].0 - SMALLEST)
            .max(SMALLEST - parts[index].0);
        parts[index].0 += delta;
        parts[neighbor].0 -= delta;
        true
    }

    pub(in crate::editor) fn set_weights(&mut self, windows: &[WindowId], weights: &[f32]) -> bool {
        let Self::Split { parts, .. } = self else {
            return false;
        };
        let leaves = parts.len() == windows.len()
            && parts
                .iter()
                .zip(windows)
                .all(|((_, part), window)| *part == Self::Window(*window));
        if leaves {
            for ((weight, _), given) in parts.iter_mut().zip(weights) {
                *weight = *given;
            }
            return true;
        }
        parts
            .iter_mut()
            .any(|(_, part)| part.set_weights(windows, weights))
    }

    pub fn boundary(&self, path: &[usize], index: usize) -> Option<f32> {
        let Self::Split { parts, .. } = self else {
            return None;
        };
        match path {
            [first, rest @ ..] => parts.get(*first)?.1.boundary(rest, index),
            [] => (index + 1 < parts.len())
                .then(|| parts[..=index].iter().map(|(weight, _)| weight).sum()),
        }
    }

    pub(in crate::editor) fn set_divider(&mut self, path: &[usize], index: usize, fraction: f32) {
        match (self, path) {
            (Self::Split { parts, .. }, [first, rest @ ..]) => {
                if let Some((_, part)) = parts.get_mut(*first) {
                    part.set_divider(rest, index, fraction);
                }
            }
            (Self::Split { parts, .. }, []) if index + 1 < parts.len() => {
                let before: f32 = parts[..index].iter().map(|(weight, _)| weight).sum();
                let pair = parts[index].0 + parts[index + 1].0;
                let first = (fraction - before).clamp(SMALLEST, pair - SMALLEST);
                parts[index].0 = first;
                parts[index + 1].0 = pair - first;
            }
            _ => {}
        }
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "window counts are tiny compared to the f32 precision limit"
)]
fn count_as_f32(count: usize) -> f32 {
    count as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: WindowId = WindowId(0);
    const B: WindowId = WindowId(1);
    const C: WindowId = WindowId(2);

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn rect_of(tiling: &Tiling, id: WindowId) -> Rect {
        tiling
            .rects()
            .into_iter()
            .find(|(window, _)| *window == id)
            .unwrap()
            .1
    }

    #[test]
    fn splitting_halves_the_window() {
        let mut tiling = Tiling::Window(A);

        assert!(tiling.split(A, Axis::Rows, B));

        assert_eq!(tiling.windows(), [A, B]);
        let below = rect_of(&tiling, B);
        assert!(close(below.y, 0.5) && close(below.height, 0.5));
    }

    #[test]
    fn splitting_along_the_same_axis_adds_a_sibling() {
        let mut tiling = Tiling::Window(A);
        tiling.split(A, Axis::Columns, B);

        tiling.split(B, Axis::Columns, C);

        assert_eq!(tiling.windows(), [A, B, C]);
        assert!(close(rect_of(&tiling, A).width, 0.5));
        assert!(close(rect_of(&tiling, C).width, 0.25));
        assert!(close(rect_of(&tiling, C).x, 0.75));
    }

    #[test]
    fn splitting_across_nests_a_new_split() {
        let mut tiling = Tiling::Window(A);
        tiling.split(A, Axis::Columns, B);

        tiling.split(B, Axis::Rows, C);

        let lower_right = rect_of(&tiling, C);
        assert!(close(lower_right.x, 0.5) && close(lower_right.y, 0.5));
    }

    #[test]
    fn removing_gives_the_space_to_a_neighbor_and_collapses() {
        let mut tiling = Tiling::Window(A);
        tiling.split(A, Axis::Rows, B);

        assert_eq!(tiling.remove(B), Some(A));

        assert_eq!(tiling, Tiling::Window(A));
        assert_eq!(tiling.remove(A), None);
    }

    #[test]
    fn balancing_evens_out_siblings() {
        let mut tiling = Tiling::Window(A);
        tiling.split(A, Axis::Columns, B);
        tiling.split(B, Axis::Columns, C);

        tiling.balance();

        for id in [A, B, C] {
            assert!(close(rect_of(&tiling, id).width, 1.0 / 3.0));
        }
    }

    #[test]
    fn resizing_trades_space_with_a_neighbor() {
        let mut tiling = Tiling::Window(A);
        tiling.split(A, Axis::Rows, B);

        assert!(tiling.resize(B, Axis::Rows, 0.1));
        assert!(close(rect_of(&tiling, B).height, 0.6));
        assert!(close(rect_of(&tiling, A).height, 0.4));

        assert!(!tiling.resize(B, Axis::Columns, 0.1));
        assert!(tiling.resize(A, Axis::Rows, -1.0));
        assert!(close(rect_of(&tiling, A).height, SMALLEST));
    }

    #[test]
    fn shares_come_from_the_nearest_split_along_the_axis() {
        let mut tiling = Tiling::Window(A);
        tiling.split(A, Axis::Columns, B);
        tiling.split(B, Axis::Rows, C);

        assert_eq!(tiling.share(C, Axis::Rows), Some(0.5));
        assert_eq!(tiling.share(C, Axis::Columns), Some(0.5));
        assert_eq!(Tiling::Window(A).share(A, Axis::Rows), None);
    }

    #[test]
    fn boundaries_sum_the_weights_before_them() {
        let mut tiling = Tiling::Window(A);
        tiling.split(A, Axis::Columns, B);
        tiling.split(B, Axis::Columns, C);

        assert_eq!(tiling.boundary(&[], 0), Some(0.5));
        assert_eq!(tiling.boundary(&[], 1), Some(0.75));
        assert_eq!(tiling.boundary(&[], 2), None);
        assert_eq!(tiling.boundary(&[0], 0), None);
    }

    #[test]
    fn dragging_a_divider_moves_the_boundary() {
        let mut tiling = Tiling::Window(A);
        tiling.split(A, Axis::Columns, B);
        tiling.split(B, Axis::Rows, C);

        tiling.set_divider(&[], 0, 0.3);
        assert!(close(rect_of(&tiling, A).width, 0.3));

        tiling.set_divider(&[1], 0, 0.75);
        assert!(close(rect_of(&tiling, B).height, 0.75));
    }
}
