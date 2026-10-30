use std::f32::consts::{PI, SQRT_2, TAU};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::controls::Controls;
use crate::graph::{Color, Node, Pass, Sound, Stages, Wave};
use crate::sample::Sample;
use crate::{float, len_float, samples};

pub type Frame = [f32; 2];

#[derive(Debug, Clone, Copy)]
pub struct Context {
    pub rate: f32,
    pub pitch: f32,
    pub gate: bool,
    pub velocity: f32,
    pub input: Frame,
}

pub trait Unit: Send {
    fn next(&mut self, context: &Context) -> Frame;
}

pub type Boxed = Box<dyn Unit>;

#[derive(Debug, Clone, Copy)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut mixed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Self((mixed ^ (mixed >> 31)) | 1)
    }

    pub fn next_bits(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn unit(&mut self) -> f32 {
        float(self.next_bits() >> 40) / 16_777_216.0
    }
}

struct Constant(f32);

impl Unit for Constant {
    fn next(&mut self, _: &Context) -> Frame {
        [self.0; 2]
    }
}

struct Read(fn(&Context) -> Frame);

impl Unit for Read {
    fn next(&mut self, context: &Context) -> Frame {
        (self.0)(context)
    }
}

struct Control {
    value: Arc<AtomicU32>,
    current: f32,
    smoothing: f32,
}

impl Unit for Control {
    fn next(&mut self, _: &Context) -> Frame {
        let target = f32::from_bits(self.value.load(Ordering::Relaxed));
        self.current += (target - self.current) * self.smoothing;
        [self.current; 2]
    }
}

fn blep(phase: f32, step: f32) -> f32 {
    if phase < step {
        let t = phase / step;
        t + t - t * t - 1.0
    } else if phase > 1.0 - step {
        let t = (phase - 1.0) / step;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

struct Oscillator {
    wave: Wave,
    frequency: Boxed,
    width: Boxed,
    phase: f32,
}

impl Unit for Oscillator {
    fn next(&mut self, context: &Context) -> Frame {
        let hertz = self.frequency.next(context)[0];
        let width = self.width.next(context)[0].clamp(0.01, 0.99);
        let step = (hertz / context.rate).clamp(-0.5, 0.5);
        let smallest = step.abs().max(1e-6);
        let phase = self.phase;
        let value = match self.wave {
            Wave::Sine => (phase * TAU).sin(),
            Wave::Saw => 2.0 * phase - 1.0 - blep(phase, smallest),
            Wave::Square => {
                let raw = if phase < width { 1.0 } else { -1.0 };
                raw + blep(phase, smallest) - blep((phase - width).rem_euclid(1.0), smallest)
            }
            Wave::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
        };
        self.phase = (phase + step).rem_euclid(1.0);
        [value; 2]
    }
}

struct Noise {
    color: Color,
    rng: Rng,
    pink: [f32; 3],
    brown: f32,
}

impl Unit for Noise {
    fn next(&mut self, _: &Context) -> Frame {
        let white = self.rng.unit() * 2.0 - 1.0;
        let value = match self.color {
            Color::White => white,
            Color::Pink => {
                let [a, b, c] = &mut self.pink;
                *a = 0.997_65 * *a + white * 0.099_046;
                *b = 0.963 * *b + white * 0.296_516_4;
                *c = 0.57 * *c + white * 1.052_691_3;
                (*a + *b + *c + white * 0.1848) * 0.22
            }
            Color::Brown => {
                self.brown = (self.brown + white * 0.02) / 1.02;
                self.brown * 3.5
            }
        };
        [value; 2]
    }
}

struct Mix(Vec<Boxed>);

impl Unit for Mix {
    fn next(&mut self, context: &Context) -> Frame {
        self.0.iter_mut().fold([0.0; 2], |sum, unit| {
            let [left, right] = unit.next(context);
            [sum[0] + left, sum[1] + right]
        })
    }
}

struct Product(Vec<Boxed>);

impl Unit for Product {
    fn next(&mut self, context: &Context) -> Frame {
        self.0.iter_mut().fold([1.0; 2], |product, unit| {
            let [left, right] = unit.next(context);
            [product[0] * left, product[1] * right]
        })
    }
}

struct Range {
    input: Boxed,
    low: Boxed,
    high: Boxed,
}

impl Unit for Range {
    fn next(&mut self, context: &Context) -> Frame {
        let input = self.input.next(context);
        let low = self.low.next(context)[0];
        let high = self.high.next(context)[0];
        input.map(|value| value.mul_add(0.5, 0.5).mul_add(high - low, low))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Release,
}

struct Envelope {
    input: Boxed,
    stages: Stages,
    attack: f32,
    decay: f32,
    release: f32,
    level: f32,
    stage: Stage,
    held: bool,
}

fn falloff(seconds: f32, rate: f32) -> f32 {
    (-6.9 / (seconds.max(0.001) * rate)).exp()
}

impl Unit for Envelope {
    fn next(&mut self, context: &Context) -> Frame {
        if context.gate && !self.held {
            self.stage = Stage::Attack;
        } else if !context.gate && self.held {
            self.stage = Stage::Release;
        }
        self.held = context.gate;
        match self.stage {
            Stage::Idle => {}
            Stage::Attack => {
                self.level += self.attack;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                let sustain = self.stages.sustain;
                self.level = sustain + (self.level - sustain) * self.decay;
            }
            Stage::Release => {
                self.level *= self.release;
                if self.level < 1e-5 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        let input = self.input.next(context);
        input.map(|value| value * self.level)
    }
}

struct Filter {
    pass: Pass,
    input: Boxed,
    cutoff: Boxed,
    resonance: Boxed,
    last: (f32, f32),
    shape: [f32; 4],
    state: [[f32; 2]; 2],
}

impl Unit for Filter {
    fn next(&mut self, context: &Context) -> Frame {
        let input = self.input.next(context);
        let cutoff = self.cutoff.next(context)[0].clamp(10.0, context.rate * 0.49);
        let resonance = self.resonance.next(context)[0].clamp(0.0, 0.98);
        if (cutoff, resonance) != self.last {
            self.last = (cutoff, resonance);
            let g = (PI * cutoff / context.rate).tan();
            let k = 2.0 - 2.0 * resonance;
            let a1 = 1.0 / (1.0 + g * (g + k));
            self.shape = [a1, g * a1, g * g * a1, k];
        }
        let [a1, a2, a3, k] = self.shape;
        let mut output = [0.0; 2];
        for (channel, value) in input.into_iter().enumerate() {
            let [first, second] = self.state[channel];
            let v3 = value - second;
            let v1 = a1 * first + a2 * v3;
            let v2 = second + a2 * first + a3 * v3;
            self.state[channel] = [2.0 * v1 - first, 2.0 * v2 - second];
            output[channel] = match self.pass {
                Pass::Low => v2,
                Pass::Band => v1,
                Pass::High => value - k * v1 - v2,
                Pass::Notch => value - k * v1,
            };
        }
        output
    }
}

struct Drive {
    input: Boxed,
    amount: Boxed,
}

impl Unit for Drive {
    fn next(&mut self, context: &Context) -> Frame {
        let input = self.input.next(context);
        let amount = self.amount.next(context)[0].max(0.0);
        if amount <= 0.0 {
            return input;
        }
        let gain = 1.0 + 9.0 * amount;
        let level = gain.tanh();
        input.map(|value| (value * gain).tanh() / level)
    }
}

struct Pan {
    input: Boxed,
    position: Boxed,
}

impl Unit for Pan {
    fn next(&mut self, context: &Context) -> Frame {
        let [left, right] = self.input.next(context);
        let middle = f32::midpoint(left, right);
        let angle = (self.position.next(context)[0].clamp(-1.0, 1.0) + 1.0) * PI * 0.25;
        [middle * angle.cos() * SQRT_2, middle * angle.sin() * SQRT_2]
    }
}

struct Delay {
    input: Boxed,
    feedback: Boxed,
    mix: Boxed,
    crossed: bool,
    lines: [Vec<f32>; 2],
    at: usize,
}

impl Unit for Delay {
    fn next(&mut self, context: &Context) -> Frame {
        let input = self.input.next(context);
        let feedback = self.feedback.next(context)[0].clamp(0.0, 0.98);
        let mix = self.mix.next(context)[0].clamp(0.0, 1.0);
        let read = [self.lines[0][self.at], self.lines[1][self.at]];
        let written = if self.crossed {
            [
                f32::midpoint(input[0], input[1]) + feedback * read[1],
                feedback * read[0],
            ]
        } else {
            [input[0] + feedback * read[0], input[1] + feedback * read[1]]
        };
        self.lines[0][self.at] = written[0];
        self.lines[1][self.at] = written[1];
        self.at = (self.at + 1) % self.lines[0].len();
        [
            input[0] * (1.0 - mix) + read[0] * mix,
            input[1] * (1.0 - mix) + read[1] * mix,
        ]
    }
}

struct Comb {
    line: Vec<f32>,
    at: usize,
    kept: f32,
}

impl Comb {
    fn next(&mut self, input: f32, feedback: f32, damping: f32) -> f32 {
        let output = self.line[self.at];
        self.kept = output * (1.0 - damping) + self.kept * damping;
        self.line[self.at] = input + self.kept * feedback;
        self.at = (self.at + 1) % self.line.len();
        output
    }
}

struct Allpass {
    line: Vec<f32>,
    at: usize,
}

impl Allpass {
    fn next(&mut self, input: f32) -> f32 {
        let delayed = self.line[self.at];
        self.line[self.at] = input + delayed * 0.5;
        self.at = (self.at + 1) % self.line.len();
        delayed - input
    }
}

const COMBS: [f32; 8] = [
    1116.0, 1188.0, 1277.0, 1356.0, 1422.0, 1491.0, 1557.0, 1617.0,
];
const ALLPASSES: [f32; 4] = [556.0, 441.0, 341.0, 225.0];
const SPREAD: f32 = 23.0;

struct Reverb {
    input: Boxed,
    size: Boxed,
    damping: Boxed,
    mix: Boxed,
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<Allpass>; 2],
}

impl Reverb {
    fn tuned(rate: f32, length: f32) -> usize {
        samples(length * rate / 44_100.0).max(1)
    }

    fn new(rate: f32, input: Boxed, size: Boxed, damping: Boxed, mix: Boxed) -> Self {
        let combs = [0.0, SPREAD].map(|spread| {
            COMBS
                .iter()
                .map(|length| Comb {
                    line: vec![0.0; Self::tuned(rate, length + spread)],
                    at: 0,
                    kept: 0.0,
                })
                .collect()
        });
        let allpasses = [0.0, SPREAD].map(|spread| {
            ALLPASSES
                .iter()
                .map(|length| Allpass {
                    line: vec![0.0; Self::tuned(rate, length + spread)],
                    at: 0,
                })
                .collect()
        });
        Self {
            input,
            size,
            damping,
            mix,
            combs,
            allpasses,
        }
    }
}

impl Unit for Reverb {
    fn next(&mut self, context: &Context) -> Frame {
        let input = self.input.next(context);
        let feedback = 0.7 + 0.28 * self.size.next(context)[0].clamp(0.0, 1.0);
        let damping = self.damping.next(context)[0].clamp(0.0, 1.0) * 0.4;
        let mix = self.mix.next(context)[0].clamp(0.0, 1.0);
        let fed = (input[0] + input[1]) * 0.015;
        let mut output = [0.0; 2];
        for channel in 0..2 {
            let mut wet: f32 = self.combs[channel]
                .iter_mut()
                .map(|comb| comb.next(fed, feedback, damping))
                .sum();
            for allpass in &mut self.allpasses[channel] {
                wet = allpass.next(wet);
            }
            output[channel] = input[channel] * (1.0 - mix) + wet * 3.0 * mix;
        }
        output
    }
}

const CHORUS_BASE: f32 = 0.012;

struct Chorus {
    input: Boxed,
    mix: Boxed,
    speed: f32,
    depth: f32,
    lines: [Vec<f32>; 2],
    at: usize,
    phase: f32,
}

impl Unit for Chorus {
    fn next(&mut self, context: &Context) -> Frame {
        let input = self.input.next(context);
        let mix = self.mix.next(context)[0].clamp(0.0, 1.0);
        let length = self.lines[0].len();
        let mut output = [0.0; 2];
        for channel in 0..2 {
            self.lines[channel][self.at] = input[channel];
            let offset = if channel == 0 { 0.0 } else { 0.25 };
            let swing = (TAU * (self.phase + offset)).sin();
            let delay = (CHORUS_BASE + self.depth * 0.5 * (1.0 + swing)) * context.rate;
            let back = delay.clamp(1.0, len_float(length) - 2.0);
            let whole = samples(back.floor());
            let fraction = back - back.floor();
            let newer = self.lines[channel][(self.at + length - whole) % length];
            let older = self.lines[channel][(self.at + length - whole - 1) % length];
            let wet = newer + (older - newer) * fraction;
            output[channel] = input[channel] * (1.0 - mix) + wet * mix;
        }
        self.at = (self.at + 1) % length;
        self.phase = (self.phase + self.speed / context.rate).rem_euclid(1.0);
        output
    }
}

struct Sampler {
    sample: Sample,
    root: Option<f32>,
    looping: bool,
    position: f32,
    step: f32,
}

impl Unit for Sampler {
    fn next(&mut self, context: &Context) -> Frame {
        let length = len_float(self.sample.frames.len());
        if length <= 0.0 {
            return [0.0; 2];
        }
        if self.position >= length {
            if !self.looping {
                return [0.0; 2];
            }
            self.position = self.position.rem_euclid(length);
        }
        let frame = self.sample.at(self.position);
        let ratio = self.root.map_or(1.0, |root| context.pitch / root);
        self.position += self.step * ratio;
        frame
    }
}

struct Grain {
    at: f32,
    step: f32,
    age: f32,
    pan: [f32; 2],
}

struct Grains {
    sample: Sample,
    position: Boxed,
    size: f32,
    every: f32,
    spread: f32,
    root: Option<f32>,
    step: f32,
    level: f32,
    wait: f32,
    rng: Rng,
    playing: Vec<Grain>,
}

impl Unit for Grains {
    fn next(&mut self, context: &Context) -> Frame {
        let length = len_float(self.sample.frames.len());
        let position = self.position.next(context)[0].clamp(0.0, 1.0);
        self.wait -= 1.0;
        if self.wait <= 0.0 && length > 0.0 {
            self.wait += self.every;
            let jitter = (self.rng.unit() * 2.0 - 1.0) * self.spread;
            let ratio = self.root.map_or(1.0, |root| context.pitch / root);
            let angle = (self.rng.unit() * 1.2 - 0.6 + 1.0) * PI * 0.25;
            self.playing.push(Grain {
                at: (position + jitter).clamp(0.0, 1.0) * length,
                step: self.step * ratio,
                age: 0.0,
                pan: [angle.cos() * SQRT_2, angle.sin() * SQRT_2],
            });
        }
        let mut output = [0.0; 2];
        for grain in &mut self.playing {
            let window = (PI * grain.age / self.size).sin().powi(2);
            let [left, right] = self.sample.at(grain.at);
            let middle = f32::midpoint(left, right) * window * self.level;
            output[0] += middle * grain.pan[0];
            output[1] += middle * grain.pan[1];
            grain.at += grain.step;
            grain.age += 1.0;
        }
        let size = self.size;
        self.playing.retain(|grain| grain.age < size);
        output
    }
}

fn pitch(context: &Context) -> Frame {
    [context.pitch; 2]
}

fn gate(context: &Context) -> Frame {
    [if context.gate { 1.0 } else { 0.0 }; 2]
}

fn velocity(context: &Context) -> Frame {
    [context.velocity; 2]
}

fn input(context: &Context) -> Frame {
    context.input
}

#[derive(Debug)]
pub struct Compiler<'a> {
    pub controls: &'a Controls,
    pub rate: f32,
    pub rng: Rng,
}

impl Compiler<'_> {
    #[expect(
        clippy::too_many_lines,
        reason = "one arm for each kind of node keeps the graph's whole vocabulary in one place"
    )]
    pub fn compile(&mut self, node: &Sound) -> Boxed {
        let rate = self.rate;
        match &**node {
            Node::Constant(value) => Box::new(Constant(*value)),
            Node::Pitch => Box::new(Read(pitch)),
            Node::Gate => Box::new(Read(gate)),
            Node::Velocity => Box::new(Read(velocity)),
            Node::Input => Box::new(Read(input)),
            Node::Control(knob) => {
                let value = self.controls.slot(knob);
                let current = f32::from_bits(value.load(Ordering::Relaxed));
                Box::new(Control {
                    value,
                    current,
                    smoothing: 1.0 - (-1.0 / (0.01 * rate)).exp(),
                })
            }
            Node::Oscillator {
                wave,
                frequency,
                width,
            } => Box::new(Oscillator {
                wave: *wave,
                phase: if *wave == Wave::Sine {
                    0.0
                } else {
                    self.rng.unit()
                },
                frequency: self.compile(frequency),
                width: self.compile(width),
            }),
            Node::Noise(color) => Box::new(Noise {
                color: *color,
                rng: Rng::new(self.rng.next_bits()),
                pink: [0.0; 3],
                brown: 0.0,
            }),
            Node::Mix(sounds) => Box::new(Mix(sounds
                .iter()
                .map(|sound| self.compile(sound))
                .collect())),
            Node::Product(sounds) => Box::new(Product(
                sounds.iter().map(|sound| self.compile(sound)).collect(),
            )),
            Node::Range { input, low, high } => Box::new(Range {
                input: self.compile(input),
                low: self.compile(low),
                high: self.compile(high),
            }),
            Node::Envelope { input, stages } => Box::new(Envelope {
                input: self.compile(input),
                stages: *stages,
                attack: 1.0 / (stages.attack.max(0.002) * rate),
                decay: falloff(stages.decay, rate),
                release: falloff(stages.release, rate),
                level: 0.0,
                stage: Stage::Idle,
                held: false,
            }),
            Node::Filter {
                pass,
                input,
                cutoff,
                resonance,
            } => Box::new(Filter {
                pass: *pass,
                input: self.compile(input),
                cutoff: self.compile(cutoff),
                resonance: self.compile(resonance),
                last: (f32::NAN, f32::NAN),
                shape: [0.0; 4],
                state: [[0.0; 2]; 2],
            }),
            Node::Drive { input, amount } => Box::new(Drive {
                input: self.compile(input),
                amount: self.compile(amount),
            }),
            Node::Pan { input, position } => Box::new(Pan {
                input: self.compile(input),
                position: self.compile(position),
            }),
            Node::Delay {
                input,
                seconds,
                feedback,
                mix,
                crossed,
            } => {
                let length = samples(seconds * rate).max(1);
                Box::new(Delay {
                    input: self.compile(input),
                    feedback: self.compile(feedback),
                    mix: self.compile(mix),
                    crossed: *crossed,
                    lines: [vec![0.0; length], vec![0.0; length]],
                    at: 0,
                })
            }
            Node::Reverb {
                input,
                size,
                damping,
                mix,
            } => {
                let input = self.compile(input);
                let size = self.compile(size);
                let damping = self.compile(damping);
                let mix = self.compile(mix);
                Box::new(Reverb::new(rate, input, size, damping, mix))
            }
            Node::Sampler {
                sample,
                root,
                looping,
                start,
            } => {
                let step = float(u64::from(sample.rate)) / rate;
                Box::new(Sampler {
                    position: start * float(u64::from(sample.rate)),
                    sample: sample.clone(),
                    root: *root,
                    looping: *looping,
                    step,
                })
            }
            Node::Grains {
                sample,
                size,
                density,
                position,
                spread,
                root,
            } => {
                let size_samples = (size * rate).max(16.0);
                Box::new(Grains {
                    position: self.compile(position),
                    size: size_samples,
                    every: rate / density.max(0.1),
                    spread: *spread,
                    root: *root,
                    step: float(u64::from(sample.rate)) / rate,
                    level: 1.0 / (size * density).max(1.0).sqrt(),
                    wait: 0.0,
                    rng: Rng::new(self.rng.next_bits()),
                    playing: Vec::new(),
                    sample: sample.clone(),
                })
            }
            Node::Chorus {
                input,
                rate: speed,
                depth,
                mix,
            } => {
                let length = samples((CHORUS_BASE + depth) * rate) + 4;
                Box::new(Chorus {
                    input: self.compile(input),
                    mix: self.compile(mix),
                    speed: *speed,
                    depth: *depth,
                    lines: [vec![0.0; length], vec![0.0; length]],
                    at: 0,
                    phase: 0.0,
                })
            }
        }
    }
}
