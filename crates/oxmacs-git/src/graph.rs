#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct GraphRow {
    pub(crate) lane: usize,
    pub(crate) above: Vec<usize>,
    pub(crate) below: Vec<usize>,
    pub(crate) through: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Lanes<T> {
    expected: Vec<Option<T>>,
}

impl<T> Default for Lanes<T> {
    fn default() -> Self {
        Self {
            expected: Vec::new(),
        }
    }
}

impl GraphRow {
    pub(crate) fn width(&self) -> usize {
        self.above
            .iter()
            .chain(&self.below)
            .chain(&self.through)
            .copied()
            .chain([self.lane])
            .max()
            .map_or(0, |widest| widest + 1)
    }
}

impl<T: PartialEq + Clone> Lanes<T> {
    pub(crate) fn place(&mut self, id: &T, parents: &[T]) -> GraphRow {
        let above: Vec<usize> = self.positions(id).collect();
        let through: Vec<usize> = self
            .expected
            .iter()
            .enumerate()
            .filter(|(at, expected)| expected.is_some() && !above.contains(at))
            .map(|(at, _)| at)
            .collect();
        for at in &above {
            self.expected[*at] = None;
        }
        let lane = above.first().copied().unwrap_or_else(|| self.free());
        let mut below = Vec::with_capacity(parents.len());
        for (number, parent) in parents.iter().enumerate() {
            let target = if number == 0 {
                lane
            } else if let Some(existing) = self.positions(parent).next() {
                existing
            } else {
                self.free()
            };
            self.expected[target] = Some(parent.clone());
            below.push(target);
        }
        while self.expected.last().is_some_and(Option::is_none) {
            self.expected.pop();
        }
        GraphRow {
            lane,
            above,
            below,
            through,
        }
    }

    fn positions<'a>(&'a self, id: &'a T) -> impl Iterator<Item = usize> + 'a {
        self.expected
            .iter()
            .enumerate()
            .filter(move |(_, expected)| expected.as_ref() == Some(id))
            .map(|(at, _)| at)
    }

    fn free(&mut self) -> usize {
        if let Some(at) = self.expected.iter().position(Option::is_none) {
            return at;
        }
        self.expected.push(None);
        self.expected.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph<T: PartialEq + Clone>(commits: &[(T, Vec<T>)]) -> Vec<GraphRow> {
        let mut lanes = Lanes::default();
        commits
            .iter()
            .map(|(id, parents)| lanes.place(id, parents))
            .collect()
    }

    fn commits(spec: &[(&'static str, &[&'static str])]) -> Vec<(&'static str, Vec<&'static str>)> {
        spec.iter()
            .map(|(id, parents)| (*id, parents.to_vec()))
            .collect()
    }

    fn row(lane: usize, above: &[usize], below: &[usize], through: &[usize]) -> GraphRow {
        GraphRow {
            lane,
            above: above.to_vec(),
            below: below.to_vec(),
            through: through.to_vec(),
        }
    }

    #[test]
    fn a_straight_history_keeps_one_lane() {
        let rows = graph(&commits(&[("c", &["b"]), ("b", &["a"]), ("a", &[])]));

        assert_eq!(
            rows,
            [
                row(0, &[], &[0], &[]),
                row(0, &[0], &[0], &[]),
                row(0, &[0], &[], &[]),
            ]
        );
    }

    #[test]
    fn a_merge_opens_a_lane_that_joins_back_at_the_fork() {
        let rows = graph(&commits(&[
            ("m", &["b", "f"]),
            ("f", &["a"]),
            ("b", &["a"]),
            ("a", &[]),
        ]));

        assert_eq!(
            rows,
            [
                row(0, &[], &[0, 1], &[]),
                row(1, &[1], &[1], &[0]),
                row(0, &[0], &[0], &[1]),
                row(0, &[0, 1], &[], &[]),
            ]
        );
    }

    #[test]
    fn a_second_parent_already_in_a_lane_joins_it() {
        let rows = graph(&commits(&[
            ("t", &["f"]),
            ("m", &["b", "f"]),
            ("f", &["a"]),
            ("b", &["a"]),
            ("a", &[]),
        ]));

        assert_eq!(rows[0], row(0, &[], &[0], &[]));
        assert_eq!(rows[1], row(1, &[], &[1, 0], &[0]));
        assert_eq!(rows[2], row(0, &[0], &[0], &[1]));
    }

    #[test]
    fn free_lanes_are_reused() {
        let rows = graph(&commits(&[
            ("x", &["y"]),
            ("z", &[]),
            ("w", &["v"]),
            ("y", &[]),
            ("v", &[]),
        ]));

        assert_eq!(rows[1], row(1, &[], &[], &[0]));
        assert_eq!(rows[2], row(1, &[], &[1], &[0]));
        assert_eq!(rows[3], row(0, &[0], &[], &[1]));
        assert_eq!(rows[4], row(1, &[1], &[], &[]));
    }

    #[test]
    fn a_new_tip_takes_the_leftmost_free_lane() {
        let rows = graph(&commits(&[
            ("m", &["x", "f"]),
            ("x", &[]),
            ("t", &["f"]),
            ("f", &[]),
        ]));

        assert_eq!(rows[1], row(0, &[0], &[], &[1]));
        assert_eq!(rows[2], row(0, &[], &[0], &[1]));
        assert_eq!(rows[3], row(0, &[0, 1], &[], &[]));
    }

    #[test]
    fn rows_are_as_wide_as_the_lanes_they_touch() {
        assert_eq!(row(0, &[0], &[0, 2], &[1]).width(), 3);
        assert_eq!(row(1, &[], &[], &[]).width(), 2);
    }
}
