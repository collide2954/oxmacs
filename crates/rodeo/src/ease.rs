use std::f64::consts::PI;

use strum::{EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumString, IntoStaticStr, EnumIter)]
#[strum(serialize_all = "kebab-case")]
pub enum Ease {
    Linear,
    SineIn,
    SineOut,
    SineInOut,
    QuadIn,
    QuadOut,
    QuadInOut,
    CubicIn,
    CubicOut,
    CubicInOut,
    QuartIn,
    QuartOut,
    QuartInOut,
    QuintIn,
    QuintOut,
    QuintInOut,
    ExpoIn,
    ExpoOut,
    ExpoInOut,
    CircIn,
    CircOut,
    CircInOut,
    BackIn,
    BackOut,
    BackInOut,
    ElasticIn,
    ElasticOut,
    ElasticInOut,
    BounceIn,
    BounceOut,
    BounceInOut,
}

const BACK: f64 = 1.701_58;

impl Ease {
    pub fn all() -> impl Iterator<Item = Self> {
        Self::iter()
    }

    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn between(self, from: f64, to: f64, t: f64) -> f64 {
        (to - from).mul_add(self.apply(t), from)
    }

    pub fn apply(self, t: f64) -> f64 {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        match self {
            Self::Linear => t,
            Self::SineIn => 1.0 - (t * PI / 2.0).cos(),
            Self::SineOut => (t * PI / 2.0).sin(),
            Self::SineInOut => -((PI * t).cos() - 1.0) / 2.0,
            Self::QuadIn => power_in(t, 2),
            Self::QuadOut => power_out(t, 2),
            Self::QuadInOut => power_in_out(t, 2),
            Self::CubicIn => power_in(t, 3),
            Self::CubicOut => power_out(t, 3),
            Self::CubicInOut => power_in_out(t, 3),
            Self::QuartIn => power_in(t, 4),
            Self::QuartOut => power_out(t, 4),
            Self::QuartInOut => power_in_out(t, 4),
            Self::QuintIn => power_in(t, 5),
            Self::QuintOut => power_out(t, 5),
            Self::QuintInOut => power_in_out(t, 5),
            Self::ExpoIn => ends(t).unwrap_or_else(|| 2f64.powf(10.0 * t - 10.0)),
            Self::ExpoOut => ends(t).unwrap_or_else(|| 1.0 - 2f64.powf(-10.0 * t)),
            Self::ExpoInOut => ends(t).unwrap_or_else(|| {
                if t < 0.5 {
                    2f64.powf(20.0 * t - 10.0) / 2.0
                } else {
                    (2.0 - 2f64.powf(-20.0 * t + 10.0)) / 2.0
                }
            }),
            Self::CircIn => 1.0 - (1.0 - t * t).sqrt(),
            Self::CircOut => (1.0 - (t - 1.0).powi(2)).sqrt(),
            Self::CircInOut => {
                if t < 0.5 {
                    (1.0 - (1.0 - (2.0 * t).powi(2)).sqrt()) / 2.0
                } else {
                    f64::midpoint((1.0 - (-2.0 * t + 2.0).powi(2)).sqrt(), 1.0)
                }
            }
            Self::BackIn => (BACK + 1.0) * t.powi(3) - BACK * t * t,
            Self::BackOut => 1.0 + (BACK + 1.0) * (t - 1.0).powi(3) + BACK * (t - 1.0).powi(2),
            Self::BackInOut => {
                let overshoot = BACK * 1.525;
                if t < 0.5 {
                    (2.0 * t).powi(2) * ((overshoot + 1.0) * 2.0 * t - overshoot) / 2.0
                } else {
                    f64::midpoint(
                        (2.0 * t - 2.0).powi(2) * ((overshoot + 1.0) * (t * 2.0 - 2.0) + overshoot),
                        2.0,
                    )
                }
            }
            Self::ElasticIn => ends(t).unwrap_or_else(|| {
                -(2f64.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * (2.0 * PI / 3.0)).sin()
            }),
            Self::ElasticOut => ends(t).unwrap_or_else(|| {
                2f64.powf(-10.0 * t) * ((t * 10.0 - 0.75) * (2.0 * PI / 3.0)).sin() + 1.0
            }),
            Self::ElasticInOut => ends(t).unwrap_or_else(|| {
                let wave = ((20.0 * t - 11.125) * (2.0 * PI / 4.5)).sin();
                if t < 0.5 {
                    -(2f64.powf(20.0 * t - 10.0) * wave) / 2.0
                } else {
                    2f64.powf(-20.0 * t + 10.0) * wave / 2.0 + 1.0
                }
            }),
            Self::BounceIn => 1.0 - bounce_out(1.0 - t),
            Self::BounceOut => bounce_out(t),
            Self::BounceInOut => {
                if t < 0.5 {
                    (1.0 - bounce_out(1.0 - 2.0 * t)) / 2.0
                } else {
                    f64::midpoint(1.0, bounce_out(2.0 * t - 1.0))
                }
            }
        }
    }
}

fn ends(t: f64) -> Option<f64> {
    (t <= 0.0 || t >= 1.0).then_some(t)
}

fn power_in(t: f64, power: i32) -> f64 {
    t.powi(power)
}

fn power_out(t: f64, power: i32) -> f64 {
    1.0 - (1.0 - t).powi(power)
}

fn power_in_out(t: f64, power: i32) -> f64 {
    if t < 0.5 {
        2f64.powi(power - 1) * t.powi(power)
    } else {
        1.0 - (-2.0 * t + 2.0).powi(power) / 2.0
    }
}

fn bounce_out(t: f64) -> f64 {
    const SPRING: f64 = 7.5625;
    const WIDTH: f64 = 2.75;
    let (shift, lift) = if t < 1.0 / WIDTH {
        (0.0, 0.0)
    } else if t < 2.0 / WIDTH {
        (1.5 / WIDTH, 0.75)
    } else if t < 2.5 / WIDTH {
        (2.25 / WIDTH, 0.9375)
    } else {
        (2.625 / WIDTH, 0.984_375)
    };
    let t = t - shift;
    SPRING * t * t + lift
}
