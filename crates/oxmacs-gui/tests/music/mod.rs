use std::f64::consts::TAU;
use std::fs;
use std::path::Path;

pub const RATE: u32 = 44_100;

const SPEED: f64 = 0.8;
const BEAT: f64 = 60.0 / 84.0;
const BAR: f64 = BEAT * 4.0;
const INTRO: f64 = 8.0;
const CYCLE: f64 = 40.0;

const CHORDS: [(f64, f64, [f64; 4]); 4] = [
    (38.0, 45.0, [53.0, 60.0, 64.0, 69.0]),
    (43.0, 38.0, [53.0, 59.0, 64.0, 69.0]),
    (36.0, 43.0, [52.0, 59.0, 62.0, 67.0]),
    (41.0, 36.0, [52.0, 57.0, 60.0, 67.0]),
];

const MELODY: [(f64, f64, f64); 24] = [
    (69.0, 0.0, 1.0),
    (72.0, 1.0, 1.0),
    (76.0, 2.0, 1.5),
    (74.0, 3.5, 0.5),
    (71.0, 4.0, 1.5),
    (74.0, 5.5, 0.5),
    (76.0, 6.0, 2.0),
    (74.0, 8.0, 1.0),
    (71.0, 9.0, 1.0),
    (67.0, 10.0, 2.0),
    (69.0, 12.0, 1.0),
    (72.0, 13.0, 1.0),
    (76.0, 14.0, 2.0),
    (77.0, 16.0, 1.5),
    (76.0, 17.5, 0.5),
    (74.0, 18.0, 1.0),
    (72.0, 19.0, 1.0),
    (71.0, 20.0, 2.0),
    (69.0, 22.0, 2.0),
    (67.0, 24.0, 1.0),
    (71.0, 25.0, 1.0),
    (74.0, 26.0, 2.0),
    (72.0, 28.0, 1.5),
    (67.0, 29.5, 1.75),
];

const PAD: usize = 0;
const BASS: usize = 16;
const LEAD: usize = 17;

fn hertz(midi: f64) -> f64 {
    440.0 * 2f64.powf((midi - 69.0) / 12.0)
}

fn soft_saw(phase: f64, harmonics: u32) -> f64 {
    let (first, cosine) = (TAU * phase).sin_cos();
    let (mut previous, mut current, mut sum) = (0.0, first, 0.0);
    for n in 1..=harmonics {
        sum += current / f64::from(n);
        let next = 2.0 * cosine * current - previous;
        previous = current;
        current = next;
    }
    sum
}

fn triangle(phase: f64) -> f64 {
    let p = TAU * phase;
    p.sin() - (3.0 * p).sin() / 9.0 + (5.0 * p).sin() / 25.0
}

struct Noise(u64);

impl Noise {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        f64::from(u32::try_from(self.0 >> 32).unwrap_or(0)) / f64::from(u32::MAX) * 2.0 - 1.0
    }
}

#[derive(Default)]
struct Lowpass {
    level: f64,
}

impl Lowpass {
    fn run(&mut self, input: f64, cutoff: f64) -> f64 {
        let k = 1.0 - (-TAU * cutoff / f64::from(RATE)).exp();
        self.level += k * (input - self.level);
        self.level
    }
}

struct Comb {
    buffer: Vec<f64>,
    at: usize,
    store: f64,
}

impl Comb {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length],
            at: 0,
            store: 0.0,
        }
    }

    fn run(&mut self, input: f64) -> f64 {
        let out = self.buffer[self.at];
        self.store = out * 0.65 + self.store * 0.35;
        self.buffer[self.at] = input + self.store * 0.93;
        self.at = (self.at + 1) % self.buffer.len();
        out
    }
}

struct Allpass {
    buffer: Vec<f64>,
    at: usize,
}

impl Allpass {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length],
            at: 0,
        }
    }

    fn run(&mut self, input: f64) -> f64 {
        let delayed = self.buffer[self.at];
        let out = delayed - input;
        self.buffer[self.at] = input + delayed * 0.5;
        self.at = (self.at + 1) % self.buffer.len();
        out
    }
}

struct Reverb {
    combs: Vec<Comb>,
    passes: Vec<Allpass>,
}

impl Reverb {
    fn new(spread: usize) -> Self {
        Self {
            combs: [1557, 1617, 1491, 1422, 1277, 1356]
                .into_iter()
                .map(|length| Comb::new(length + spread))
                .collect(),
            passes: [556, 441, 341]
                .into_iter()
                .map(|length| Allpass::new(length + spread))
                .collect(),
        }
    }

    fn run(&mut self, input: f64) -> f64 {
        let mut out: f64 = self.combs.iter_mut().map(|comb| comb.run(input)).sum();
        for pass in &mut self.passes {
            out = pass.run(out);
        }
        out * 0.05
    }
}

struct Echo {
    lines: [Vec<f64>; 2],
    at: usize,
    tone: [Lowpass; 2],
}

impl Echo {
    fn new(length: usize) -> Self {
        Self {
            lines: [vec![0.0; length], vec![0.0; length]],
            at: 0,
            tone: [Lowpass::default(), Lowpass::default()],
        }
    }

    fn run(&mut self, input: f64) -> [f64; 2] {
        let out = [self.lines[0][self.at], self.lines[1][self.at]];
        let back = [
            self.tone[0].run(out[1], 2600.0),
            self.tone[1].run(out[0], 2600.0),
        ];
        self.lines[0][self.at] = input + back[0] * 0.42;
        self.lines[1][self.at] = back[1] * 0.42;
        self.at = (self.at + 1) % self.lines[0].len();
        out
    }
}

fn swell(time: f64, start: f64, length: f64, attack: f64, release: f64) -> f64 {
    let into = time - start;
    if into < 0.0 || into > length + release {
        return 0.0;
    }
    let rise = (into / attack).min(1.0);
    let fall = if into > length {
        1.0 - (into - length) / release
    } else {
        1.0
    };
    rise * fall
}

fn electric_piano(time: f64, frequency: f64) -> f64 {
    if !(0.0..5.0).contains(&time) {
        return 0.0;
    }
    let index = 0.1 + 0.5 * (-time * 6.0).exp();
    let modulator = (TAU * frequency * time).sin() * index;
    (TAU * frequency * time + modulator).sin() * (-time * 1.2).exp() * (time * 200.0).min(1.0)
}

fn thump(time: f64) -> f64 {
    if !(0.0..0.6).contains(&time) {
        return 0.0;
    }
    let sweep = 48.0 * time + 60.0 * (1.0 - (-time * 22.0).exp()) / 22.0;
    (TAU * sweep).sin() * (-time * 7.0).exp() * (time * 300.0).min(1.0)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "bars count up from zero"
)]
fn chord(bar: f64) -> (f64, f64, [f64; 4]) {
    CHORDS[bar.max(0.0) as usize % CHORDS.len()]
}

fn section(bar: f64) -> (bool, bool) {
    if bar < INTRO {
        return (false, false);
    }
    let into = (bar - INTRO) % CYCLE;
    (into < 32.0, (16.0..32.0).contains(&into))
}

struct Voice {
    note: f64,
    old: f64,
    changed: f64,
    slot: usize,
}

#[test]
#[ignore = "writes $OXMACS_MUSIC_SECONDS (or 120) seconds of the score to $OXMACS_MUSIC"]
fn score_preview() {
    let Ok(out) = std::env::var("OXMACS_MUSIC") else {
        return;
    };
    let seconds = std::env::var("OXMACS_MUSIC_SECONDS")
        .ok()
        .and_then(|seconds| seconds.parse().ok())
        .unwrap_or(120.0);
    vaporwave(seconds, Path::new(&out));
}

#[expect(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "one pass over every sample, counting bars and beats from time"
)]
pub fn vaporwave(seconds: f64, out: &Path) {
    let rate = f64::from(RATE);
    let count = (seconds * rate).ceil() as usize;
    let step = SPEED / rate;
    let mut noise = Noise(0x9e37_79b9_7f4a_7c15);
    let mut pad_filters: [[Lowpass; 2]; 2] = Default::default();
    let [mut clap_high, mut clap_low, mut hat_low, mut hiss_low] = <[Lowpass; 4]>::default();
    let mut clap_tone: [Lowpass; 2] = Default::default();
    let mut lows: [Lowpass; 2] = Default::default();
    let mut highs: [Lowpass; 2] = Default::default();
    let mut rooms = [Reverb::new(0), Reverb::new(23)];
    let mut echo = Echo::new((0.75 * BEAT / SPEED * rate).round() as usize);
    let mut phases = vec![0.0; LEAD + MELODY.len()];
    let mut voices: Vec<Voice> = CHORDS[0]
        .2
        .iter()
        .enumerate()
        .map(|(which, note)| Voice {
            note: *note,
            old: *note,
            changed: -10.0,
            slot: which * 4,
        })
        .collect();
    let mut samples = Vec::with_capacity(count * 2);
    for index in 0..count {
        let real = index as f64 / rate;
        let time = real * SPEED;
        let bar = (time / BAR).floor();
        let in_bar = time - bar * BAR;
        let beat = in_bar / BEAT;
        let wobble = 1.0 + 0.0035 * (TAU * 0.55 * real).sin() + 0.002 * (TAU * 0.13 * real).sin();
        let (drumming, singing) = section(bar);
        let drums = if drumming { 1.0 } else { 0.0 };
        let intro = (time / (INTRO * BAR)).min(1.0);
        let (root, fifth, notes) = chord(bar);

        let chorus = 0.05 + 0.03 * (TAU * 0.3 * real).sin();
        let mut pad = [0.0; 2];
        for (voice, note) in voices.iter_mut().zip(notes) {
            if (voice.note - note).abs() > 0.1 {
                voice.old = voice.note;
                voice.note = note;
                voice.changed = time;
                voice.slot ^= 2;
            }
            let since = time - voice.changed;
            let rising = (since / 0.5).min(1.0);
            let falling = (1.0 - since / 0.9).max(0.0);
            for (slot, pitch, level) in [
                (voice.slot, voice.note, rising),
                (voice.slot ^ 2, voice.old, falling),
            ] {
                if level <= 0.0 {
                    continue;
                }
                for (side, detune) in [(0, -chorus), (1, chorus)] {
                    let at = PAD + slot + side;
                    phases[at] += hertz(pitch + detune) * wobble * step;
                    pad[side] += soft_saw(phases[at], 7) * level * 0.03;
                }
            }
        }
        let cutoff = (400.0 + 900.0 * intro) * (1.0 + 0.25 * (TAU * 0.05 * real).sin());
        for (side, filters) in pad_filters.iter_mut().enumerate() {
            let once = filters[0].run(pad[side], cutoff);
            pad[side] = filters[1].run(once, cutoff);
        }

        let mut keys = 0.0;
        for start in [bar, bar - 1.0, bar - 2.0] {
            if start >= 4.0 {
                let (_, _, struck) = chord(start);
                for (at, velocity) in [(0.0, 0.6), (2.5, 0.42)] {
                    for (voice, note) in struck.iter().enumerate() {
                        let since = time - start * BAR - at * BEAT - voice as f64 * 0.012;
                        keys +=
                            electric_piano(since, hertz(note + 12.0) * wobble) * velocity * 0.022;
                    }
                }
            }
        }

        let mut bass = 0.0;
        if time >= BAR * 4.0 {
            for (start, length, note) in [(0.0, 2.25, root), (2.5, 1.25, fifth)] {
                let level = swell(in_bar, start * BEAT, length * BEAT, 0.02, 0.15);
                if level > 0.0 {
                    phases[BASS] += hertz(note) * wobble * step;
                    let p = TAU * phases[BASS];
                    bass +=
                        (p.sin() + 0.4 * (2.0 * p).sin() + 0.15 * (3.0 * p).sin()) * level * 0.13;
                }
            }
        }

        let white = noise.next();
        let kick = drums
            * (thump(in_bar) + thump(in_bar - 2.0 * BEAT) + 0.4 * thump(in_bar - 2.75 * BEAT));
        let band = clap_high.run(white, 2500.0) - clap_low.run(white, 400.0);
        let since_clap = (beat % 2.0 - 1.0) * BEAT;
        let struck = if (0.0..0.6).contains(&since_clap) {
            (band * (-since_clap * 10.0).exp() * 0.5
                + 0.3 * (TAU * 180.0 * since_clap).sin() * (-since_clap * 20.0).exp())
                * (since_clap / 0.006).min(1.0)
                * drums
        } else {
            0.0
        };
        let once = clap_tone[0].run(struck, 3000.0);
        let clap = clap_tone[1].run(once, 3000.0);
        let eighth = (beat * 2.0).fract() * BEAT / 2.0;
        let accent = if (beat * 2.0).floor() as u64 % 2 == 1 {
            1.0
        } else {
            0.6
        };
        let hat =
            (white - hat_low.run(white, 5000.0)) * (-eighth * 60.0).exp() * 0.01 * accent * drums;

        let mut lead = 0.0;
        if singing {
            let phrase = (bar - INTRO) % 8.0;
            let along = time - (bar - phrase) * BAR;
            for (which, (note, start, length)) in MELODY.iter().enumerate() {
                let level = swell(along, start * BEAT, length * BEAT, 0.08, 0.4);
                if level <= 0.0 {
                    continue;
                }
                let held = along - start * BEAT;
                let vibrato =
                    1.0 + 0.004 * (TAU * 5.0 * held).sin() * ((held - 0.3) / 0.4).clamp(0.0, 1.0);
                let slot = LEAD + which;
                phases[slot] += hertz(*note) * wobble * vibrato * step;
                lead += (triangle(phases[slot]) + 0.25 * soft_saw(phases[slot], 5)) * level * 0.08;
            }
        }

        let duck = 1.0 - 0.25 * drums * (-(beat % 2.0) * BEAT * 4.0).exp();
        let hiss = hiss_low.run(white, 6000.0) * 0.0015;
        let reverb_send = (pad[0] + pad[1]) * 0.3 + keys * 0.6 + lead * 0.7 + clap * 0.3;
        let echoed = echo.run(keys * 0.3 + lead * 0.5);
        let dry = keys * 0.8 + bass + kick * 0.3 + clap * 0.06 + hat + lead * 0.8 + hiss;
        for (side, room) in rooms.iter_mut().enumerate() {
            let wet = room.run(reverb_send);
            let mixed = (pad[side] * duck + dry + wet * 1.5 + echoed[side] * 0.5) * intro.sqrt();
            let rumble = lows[side].run(mixed, 35.0);
            let cassette = highs[side].run(mixed - rumble, 9000.0);
            samples.push((cassette * 1.1).tanh() * 0.8);
        }
    }
    let peak = samples
        .iter()
        .fold(0.0f64, |most, sample| most.max(sample.abs()));
    let gain = if peak > 0.0 { 0.72 / peak } else { 1.0 };
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    let data = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 4).to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for sample in samples {
        let value = (sample * gain * f64::from(i16::MAX)).round() as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    fs::write(out, bytes).unwrap();
}
