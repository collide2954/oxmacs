use strum::{EnumString, IntoStaticStr};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum Direction {
    #[default]
    Forward,
    Reverse,
    PingPong,
    PingPongReverse,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub region: usize,
    pub seconds: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Animation {
    pub frames: Vec<Frame>,
    pub direction: Direction,
    pub repeat: Option<u32>,
}

impl Animation {
    pub fn new(frames: Vec<Frame>) -> Self {
        Self {
            frames,
            direction: Direction::Forward,
            repeat: None,
        }
    }

    pub fn uniform(regions: impl IntoIterator<Item = usize>, seconds: f64) -> Self {
        Self::new(
            regions
                .into_iter()
                .map(|region| Frame { region, seconds })
                .collect(),
        )
    }

    fn bounces(&self) -> bool {
        matches!(
            self.direction,
            Direction::PingPong | Direction::PingPongReverse
        )
    }

    fn pass(&self, index: u32) -> Vec<usize> {
        let forward = match self.direction {
            Direction::Forward => true,
            Direction::Reverse => false,
            Direction::PingPong => index % 2 == 0,
            Direction::PingPongReverse => index % 2 == 1,
        };
        let count = self.frames.len();
        let mut order: Vec<usize> = if forward {
            (0..count).collect()
        } else {
            (0..count).rev().collect()
        };
        if index > 0 && self.bounces() && !order.is_empty() {
            order.remove(0);
        }
        order
    }

    fn order(&self) -> Vec<usize> {
        match self.repeat {
            Some(times) => (0..times.max(1)).flat_map(|pass| self.pass(pass)).collect(),
            None if self.bounces() => {
                let mut order = self.pass(0);
                let mut back = self.pass(1);
                back.pop();
                order.extend(back);
                order
            }
            None => self.pass(0),
        }
    }

    fn total(&self, order: &[usize]) -> f64 {
        order.iter().map(|&at| self.frames[at].seconds).sum()
    }

    pub fn duration(&self) -> Option<f64> {
        self.repeat.map(|_| self.total(&self.order()))
    }

    pub fn finished(&self, seconds: f64) -> bool {
        self.duration().is_some_and(|duration| seconds >= duration)
    }

    pub fn frame_at(&self, seconds: f64) -> Option<usize> {
        let order = self.order();
        let last = *order.last()?;
        let total = self.total(&order);
        let mut left = match self.repeat {
            None if total > 0.0 => seconds.rem_euclid(total),
            _ => seconds,
        };
        for &at in &order {
            let frame = self.frames[at];
            if left < frame.seconds {
                return Some(frame.region);
            }
            left -= frame.seconds;
        }
        Some(self.frames[last].region)
    }
}
