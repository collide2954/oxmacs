use std::f64::consts::PI;
use std::sync::OnceLock;

use rayon::prelude::*;

const STEP: f64 = 0.002;
const EVERY: usize = 5;
const SPACING: f64 = STEP * 5.0;
const LONGEST: f64 = 8.0 * PI;
const FARTHEST: f64 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum End {
    Escapes(f64),
    Falls(f64),
}

impl End {
    fn angle(self) -> f64 {
        match self {
            Self::Escapes(angle) | Self::Falls(angle) => angle,
        }
    }

    fn same_kind(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Escapes(_), Self::Escapes(_)) | (Self::Falls(_), Self::Falls(_))
        )
    }
}

#[derive(Debug)]
pub(crate) struct Row {
    impact: f64,
    samples: Vec<f32>,
    last: f64,
    peak: usize,
    end: End,
}

fn narrow(value: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "inverse radii between 0 and 1 fit an f32"
    )]
    let narrowed = value as f32;
    narrowed
}

fn index_of(place: f64) -> usize {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "callers pass angles from 0 up to eight half turns"
    )]
    let index = place.max(0.0) as usize;
    index
}

fn real(index: usize) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "sample counts are far below 2^52"
    )]
    let real = index as f64;
    real
}

fn change([u, slope]: [f64; 2]) -> [f64; 2] {
    [slope, 1.5 * u * u - u]
}

fn nudged(state: [f64; 2], by: [f64; 2], step: f64) -> [f64; 2] {
    [state[0] + by[0] * step, state[1] + by[1] * step]
}

fn advanced(state: [f64; 2]) -> [f64; 2] {
    let first = change(state);
    let second = change(nudged(state, first, STEP / 2.0));
    let third = change(nudged(state, second, STEP / 2.0));
    let fourth = change(nudged(state, third, STEP));
    let blend = |index: usize| {
        (first[index] + 2.0 * second[index] + 2.0 * third[index] + fourth[index]) / 6.0
    };
    nudged(state, [blend(0), blend(1)], STEP)
}

fn integrate(impact: f64) -> Row {
    let (mut state, mut angle) = ([0.0_f64, 1.0 / impact], 0.0_f64);
    let mut samples = vec![0.0_f32];
    let mut steps = 0_usize;
    let end = loop {
        let before = state[0];
        let next = advanced(state);
        if next[0] <= 0.0 {
            break End::Escapes(angle + STEP * before / (before - next[0]));
        }
        state = next;
        angle += STEP;
        steps += 1;
        if steps.is_multiple_of(EVERY) {
            samples.push(narrow(state[0]));
        }
        if state[0] >= 1.0 || angle >= LONGEST {
            break End::Falls(angle);
        }
    };
    let peak = (0..samples.len()).fold(0, |best, index| {
        if samples[index] > samples[best] {
            index
        } else {
            best
        }
    });
    let last = match end {
        End::Escapes(_) => 0.0,
        End::Falls(_) => state[0],
    };
    Row {
        impact,
        samples,
        last,
        peak,
        end,
    }
}

impl Row {
    fn at(&self, angle: f64) -> Option<f64> {
        if !(0.0..=self.end.angle()).contains(&angle) {
            return None;
        }
        let place = angle / SPACING;
        let index = index_of(place);
        let final_sample = self.samples.len() - 1;
        if index < final_sample {
            let low = f64::from(self.samples[index]);
            let high = f64::from(self.samples[index + 1]);
            return Some(low + (high - low) * (place - real(index)));
        }
        let start = real(final_sample) * SPACING;
        let span = (self.end.angle() - start).max(1e-12);
        let from = f64::from(self.samples[final_sample]);
        Some(from + (self.last - from) * ((angle - start) / span).clamp(0.0, 1.0))
    }

    fn reach(&self, target: f64) -> Option<f64> {
        let rising = &self.samples[..=self.peak];
        if f64::from(rising[self.peak]) >= target {
            let index = rising.partition_point(|sample| f64::from(*sample) < target);
            if index == 0 {
                return Some(0.0);
            }
            let low = f64::from(rising[index - 1]);
            let high = f64::from(rising[index]);
            let fraction = (target - low) / (high - low).max(1e-12);
            return Some((real(index - 1) + fraction) * SPACING);
        }
        if matches!(self.end, End::Falls(_)) && self.last >= target {
            let final_sample = self.samples.len() - 1;
            let start = real(final_sample) * SPACING;
            let from = f64::from(self.samples[final_sample]);
            let fraction = (target - from) / (self.last - from).max(1e-12);
            return Some(start + (self.end.angle() - start) * fraction.clamp(0.0, 1.0));
        }
        None
    }
}

#[derive(Debug)]
struct Orbits {
    rows: Vec<Row>,
}

fn impacts() -> Vec<f64> {
    let spans = [
        (0.01, 2.4, 240),
        (2.4, 2.59, 400),
        (2.59, 2.606, 1200),
        (2.606, 3.2, 500),
        (3.2, 12.0, 700),
        (12.0, FARTHEST, 400),
    ];
    spans
        .iter()
        .flat_map(|(from, to, count)| {
            (0..*count).map(move |index| from + (to - from) * real(index) / real(*count))
        })
        .chain([FARTHEST])
        .collect()
}

fn orbits() -> &'static Orbits {
    static ORBITS: OnceLock<Orbits> = OnceLock::new();
    ORBITS.get_or_init(|| Orbits {
        rows: impacts().into_par_iter().map(integrate).collect(),
    })
}

pub(crate) fn warm() {
    orbits();
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Path {
    Straight(f64),
    Weak(f64),
    Table {
        low: &'static Row,
        high: &'static Row,
        weight: f64,
    },
}

fn weak_end(impact: f64) -> f64 {
    PI + 2.0 / impact + 15.0 * PI / 16.0 / (impact * impact)
}

fn weak_at(impact: f64, angle: f64) -> f64 {
    angle.sin() / impact + (1.0 - angle.cos()).powi(2) / (2.0 * impact * impact)
}

impl Path {
    pub(crate) fn of(impact: f64, lensing: bool) -> Self {
        let impact = impact.max(1e-9);
        if !lensing {
            return Self::Straight(impact);
        }
        if impact >= FARTHEST {
            return Self::Weak(impact);
        }
        let rows = &orbits().rows;
        let index = rows
            .partition_point(|row| row.impact <= impact)
            .clamp(1, rows.len() - 1);
        let (low, high) = (&rows[index - 1], &rows[index]);
        let weight = ((impact - low.impact) / (high.impact - low.impact)).clamp(0.0, 1.0);
        if low.end.same_kind(high.end) {
            Self::Table { low, high, weight }
        } else {
            let nearest = if weight < 0.5 { low } else { high };
            Self::Table {
                low: nearest,
                high: nearest,
                weight: 0.0,
            }
        }
    }

    pub(crate) fn end(self) -> End {
        match self {
            Self::Straight(_) => End::Escapes(PI),
            Self::Weak(impact) => End::Escapes(weak_end(impact)),
            Self::Table { low, high, weight } => {
                let angle = low.end.angle() + (high.end.angle() - low.end.angle()) * weight;
                match low.end {
                    End::Escapes(_) => End::Escapes(angle),
                    End::Falls(_) => End::Falls(angle),
                }
            }
        }
    }

    pub(crate) fn at(self, angle: f64) -> Option<f64> {
        match self {
            Self::Straight(impact) => (0.0..=PI).contains(&angle).then(|| angle.sin() / impact),
            Self::Weak(impact) => (0.0..=weak_end(impact))
                .contains(&angle)
                .then(|| weak_at(impact, angle).max(0.0)),
            Self::Table { low, high, weight } => {
                let (below, above) = (low.at(angle)?, high.at(angle)?);
                Some(below + (above - below) * weight)
            }
        }
    }

    pub(crate) fn reach(self, target: f64) -> Option<f64> {
        match self {
            Self::Straight(impact) => (target * impact <= 1.0).then(|| (target * impact).asin()),
            Self::Weak(impact) => {
                if weak_at(impact, PI / 2.0) < target {
                    return None;
                }
                let (mut low, mut high) = (0.0, PI / 2.0);
                for _ in 0..40 {
                    let middle = f64::midpoint(low, high);
                    if weak_at(impact, middle) < target {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                Some(high)
            }
            Self::Table { low, high, weight } => {
                let (below, above) = (low.reach(target)?, high.reach(target)?);
                Some(below + (above - below) * weight)
            }
        }
    }
}

pub fn deflection(impact: f64) -> Option<f64> {
    match Path::of(impact, true).end() {
        End::Escapes(angle) => Some(angle - PI),
        End::Falls(_) => None,
    }
}
