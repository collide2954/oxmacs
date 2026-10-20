use std::f32::consts::TAU;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::text::{self, Paragraph as _, Text};
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Widget, mouse};
use iced::alignment::Vertical;
use iced::{Element, Font, Length, Pixels, Point, Rectangle, Size, Theme};
use oxmacs_core::editor::SplashAnimation;
use oxmacs_core::theme::{Color, Highlight, Theme as Faces};

use crate::style;
use crate::units::{floor_to_usize, to_f32};

pub const ROWS: usize = 9;
const TOP: usize = 2;
const SAMPLE: &str = "MMMMMMMMMMMMMMMM";
const LETTERS: [[&str; 5]; 6] = [
    [
        " ██████  ",
        "██    ██ ",
        "██    ██ ",
        "██    ██ ",
        " ██████  ",
    ],
    ["██   ██ ", " ██ ██  ", "  ███   ", " ██ ██  ", "██   ██ "],
    [
        "███    ███ ",
        "████  ████ ",
        "██ ████ ██ ",
        "██  ██  ██ ",
        "██      ██ ",
    ],
    [" █████  ", "██   ██ ", "███████ ", "██   ██ ", "██   ██ "],
    [" ██████ ", "██      ", "██      ", "██      ", " ██████ "],
    ["███████", "██     ", "███████", "     ██", "███████"],
];
const SHIMMER_PERIOD: f32 = 4.5;
const SHIMMER_SWEEP: f32 = 1.6;
const TRAVEL: f32 = 1.2;
const SCATTER: f32 = 0.6;
const SPARKLES_FROM: f32 = 2.0;
const SPARKLE_DENSITY: f32 = 0.08;
const FADE: f32 = 6.0;
const BLOCK: char = '█';
const SUB_X: f32 = 2.0;
const SUB_Y: f32 = 4.0;
const SPACING: f32 = 28.0;
const CLEAR: f32 = 8.0;
const BENDS: usize = 3;
const ROOM: f32 = 16.0;
const REACH: f32 = 4.0;
const TAIL: usize = 5;

const FORK_CYCLE: f32 = 32.0;
const SPIN: f32 = 0.8;
const GROOVE: f32 = 2.4;
const HELIX_TURN: f32 = 18.0;
const HELIX_HEIGHT: f32 = 3.5;
const DAUGHTER_HEIGHT: f32 = 2.5;
const UNZIP: f32 = 16.0;
const DIVERGE: f32 = 9.0;
const SPREAD: f32 = 50.0;
const LEAD_LAG: f32 = 10.0;
const FRAGMENT: f32 = 20.0;
const PRIMER: f32 = 3.0;
const OPENING: f32 = 8.0;
const LAG_SPEED: f32 = 1.4;
const RUNG_EVERY: usize = 2;
const FEED: usize = 6;
type Point2 = (f32, f32);

fn along(from: Point2, to: Point2, progress: f32) -> Point2 {
    (
        from.0 + (to.0 - from.0) * progress,
        from.1 + (to.1 - from.1) * progress,
    )
}

fn smooth(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}

fn lagging(column: f32, fork: f32) -> Option<bool> {
    let right = ((column / FRAGMENT).floor() + 1.0) * FRAGMENT;
    let started = fork - right - OPENING;
    if started <= 0.0 {
        return None;
    }
    let grown = (started * LAG_SPEED).min(FRAGMENT - PRIMER - 1.0);
    if column >= right - PRIMER {
        Some(true)
    } else if column >= right - PRIMER - grown {
        Some(false)
    } else {
        None
    }
}

fn lagging_front(fork: f32) -> Option<f32> {
    let right = ((fork - OPENING) / FRAGMENT).floor() * FRAGMENT;
    let started = fork - right - OPENING;
    let grown = started * LAG_SPEED;
    (started > 0.0 && grown < FRAGMENT - PRIMER - 1.0).then_some(right - PRIMER - grown)
}

fn bezier(start: Point2, bend: Point2, end: Point2, progress: f32) -> Point2 {
    along(
        along(start, bend, progress),
        along(bend, end, progress),
        progress,
    )
}

fn gap(from: Point2, to: Point2) -> f32 {
    ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt()
}

fn cell_gap(from: Point2, to: Point2) -> f32 {
    ((to.0 - from.0).powi(2) + ((to.1 - from.1) * 2.0).powi(2)).sqrt()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pixel {
    pub x: f32,
    pub y: f32,
    pub color: Color,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Art {
    pub cells: Vec<Vec<Option<Glyph>>>,
    pub pixels: Vec<Pixel>,
}

#[derive(Debug, Clone, Copy)]
struct Neuron {
    at: Point2,
    size: f32,
    color: Color,
}

#[derive(Debug, Clone, Copy)]
struct Axon {
    from: usize,
    to: usize,
    start: Point2,
    bend: Point2,
    end: Point2,
    progress: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph {
    pub ch: char,
    pub color: Color,
}

pub fn logo() -> Vec<Vec<bool>> {
    (0..5)
        .map(|row| {
            LETTERS
                .iter()
                .flat_map(|letter| letter[row].chars())
                .map(|c| c != ' ')
                .collect()
        })
        .collect()
}

pub fn still(animation: SplashAnimation) -> f32 {
    match animation {
        SplashAnimation::Plasma => SHIMMER_PERIOD + SHIMMER_SWEEP / 2.0,
        SplashAnimation::Wave => 1.0,
        SplashAnimation::Assemble => 6.0,
        SplashAnimation::Neurons => 2.4,
        SplashAnimation::Dna => FORK_CYCLE * 0.8,
    }
}

fn palette(faces: &Faces) -> [Color; 4] {
    [
        faces.highlight(Highlight::Heading),
        faces.highlight(Highlight::Link),
        faces.highlight(Highlight::String),
        faces.highlight(Highlight::Function),
    ]
}

fn cyclic(colors: &[Color], position: f32) -> Color {
    let scaled = position.rem_euclid(1.0) * to_f32(colors.len());
    let index = floor_to_usize(scaled) % colors.len();
    colors[index].mix(colors[(index + 1) % colors.len()], scaled.fract())
}

pub fn plasma(column: f32, row: f32, seconds: f32) -> f32 {
    let row = row * 2.0;
    let across = (column * 0.11 + seconds * 0.9).sin();
    let down = (row * 0.17 - seconds * 0.7).sin();
    let diagonal = ((column + row) * 0.07 + seconds * 0.5).sin();
    let dx = column - 26.0 - 20.0 * (seconds * 0.23).sin();
    let dy = row - 9.0 - 6.0 * (seconds * 0.31).cos();
    let ripple = ((dx * dx + dy * dy).sqrt() * 0.18 - seconds * 1.1).sin();
    (across + down + diagonal + ripple) / 8.0 + 0.5
}

fn shade(value: f32) -> char {
    match value {
        v if v < 0.38 => ' ',
        v if v < 0.5 => '·',
        v if v < 0.62 => '░',
        v if v < 0.76 => '▒',
        _ => '▓',
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "positions are hashed, so wrapping them is fine"
)]
fn hash(x: usize, y: usize, salt: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1)
        ^ (y as u32).wrapping_mul(0x85EB_CA77)
        ^ salt.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    to_f32((h >> 8) as usize) / 16_777_216.0
}

fn ease_out(u: f32) -> f32 {
    1.0 - (1.0 - u.clamp(0.0, 1.0)).powi(3)
}

struct Scene {
    logo: Vec<Vec<bool>>,
    width: usize,
    left: usize,
    columns: usize,
    seconds: f32,
    colors: [Color; 4],
    background: Color,
    foreground: Color,
    accent: Color,
    ink: Color,
    cells: Vec<Vec<Option<Glyph>>>,
    pixels: Vec<Pixel>,
    tint: Color,
    fade: f32,
}

impl Scene {
    fn new(seconds: f32, columns: usize, faces: &Faces) -> Self {
        let logo = logo();
        let width = logo[0].len();
        let colors = palette(faces);
        Self {
            width,
            left: columns.saturating_sub(width) / 2,
            columns,
            seconds,
            colors,
            background: faces.default.background,
            foreground: faces.default.foreground,
            accent: faces.cursor,
            ink: faces.default.foreground.mix(colors[0], 0.35),
            cells: vec![vec![None; columns]; ROWS],
            pixels: Vec::new(),
            tint: faces.default.foreground,
            fade: 1.0,
            logo,
        }
    }

    fn put(&mut self, column: usize, row: usize, glyph: Glyph) {
        if let Some(cell) = self
            .cells
            .get_mut(row)
            .and_then(|cells| cells.get_mut(column))
        {
            *cell = Some(glyph);
        }
    }

    fn near_logo(&self, column: usize, row: usize) -> bool {
        (self.left.saturating_sub(1)..=self.left + self.width).contains(&column)
            && (TOP.saturating_sub(1)..=TOP + self.logo.len()).contains(&row)
    }

    fn distance_from_logo(&self, column: usize, row: usize) -> f32 {
        let outside = |at: usize, from: usize, to: usize| {
            to_f32(from.saturating_sub(at)).max(to_f32(at.saturating_sub(to)))
        };
        let across = outside(column, self.left, self.left + self.width);
        let down = outside(row, TOP, TOP + self.logo.len()) * 2.0;
        across.max(down)
    }

    fn letters(&self) -> Vec<(usize, usize)> {
        self.logo
            .iter()
            .enumerate()
            .flat_map(|(row, cells)| {
                cells
                    .iter()
                    .enumerate()
                    .filter(|(_, on)| **on)
                    .map(move |(column, _)| (column, row))
            })
            .collect()
    }

    fn plasma(&mut self) {
        for row in 0..ROWS {
            for column in 0..self.columns {
                let value = plasma(to_f32(column), to_f32(row), self.seconds);
                let ch = shade(value);
                if ch == ' ' {
                    continue;
                }
                let hue = value * 0.5
                    + self.seconds * 0.04
                    + to_f32(column) / to_f32(self.columns.max(1)) * 0.5;
                let dim = 0.3 + 0.7 * (self.distance_from_logo(column, row) / FADE).min(1.0);
                let color = self
                    .background
                    .mix(cyclic(&self.colors, hue), (0.25 + 0.5 * value) * dim);
                self.put(column, row, Glyph { ch, color });
            }
        }
        let phase = self.seconds.rem_euclid(SHIMMER_PERIOD);
        let band = (phase < SHIMMER_SWEEP)
            .then(|| phase / SHIMMER_SWEEP * (to_f32(self.width) + 16.0) - 8.0);
        for (column, row) in self.letters() {
            let glow = band.map_or(0.0, |band| {
                (1.0 - (to_f32(column) - to_f32(row) * 2.0 - band).abs() / 3.0).max(0.0)
            });
            let color = self.ink.mix(self.accent, glow);
            self.put(self.left + column, TOP + row, Glyph { ch: BLOCK, color });
        }
    }

    fn wave(&mut self) {
        for (column, row) in self.letters() {
            let position = to_f32(column) * 0.03 + to_f32(row) * 0.06 - self.seconds * 0.15;
            let pulse = 0.15 * (self.seconds * 1.7 + to_f32(column) * 0.2).sin().max(0.0);
            let color = cyclic(&self.colors, position).mix(self.foreground, pulse);
            self.put(self.left + column, TOP + row, Glyph { ch: BLOCK, color });
        }
    }

    fn sparkles(&mut self) {
        let fade = (self.seconds - SPARKLES_FROM).clamp(0.0, 1.0);
        if fade <= 0.0 {
            return;
        }
        for row in 0..ROWS {
            for column in 0..self.columns {
                if self.near_logo(column, row) || hash(column, row, 3) >= SPARKLE_DENSITY {
                    continue;
                }
                let phase = hash(column, row, 4);
                let brightness = (self.seconds * 2.4 + phase * TAU).sin().max(0.0).powi(3) * fade;
                let ch = match brightness {
                    level if level < 0.15 => continue,
                    level if level < 0.45 => '·',
                    level if level < 0.8 => '+',
                    _ => '*',
                };
                let color = self
                    .background
                    .mix(cyclic(&self.colors, phase), 0.3 + 0.7 * brightness);
                self.put(column, row, Glyph { ch, color });
            }
        }
    }

    fn dot(&mut self, at: Point2, color: Color) {
        self.pixels.push(Pixel {
            x: at.0.round(),
            y: at.1.round(),
            color,
        });
    }

    fn stroke(&mut self, from: Point2, to: Point2, color: Color) {
        let steps = (to.0 - from.0)
            .abs()
            .max((to.1 - from.1).abs())
            .ceil()
            .max(1.0);
        for step in 0..=floor_to_usize(steps) {
            self.dot(along(from, to, to_f32(step) / steps), color);
        }
    }

    fn dendrite(&mut self, from: Point2, angle: f32, length: f32, depth: usize, seed: usize) {
        let color = self.tint.mix(self.background, 0.22 * to_f32(depth));
        let mut at = from;
        let mut heading = angle;
        for segment in 0..BENDS {
            heading += (hash(seed, segment, 32) - 0.5) * 0.6;
            let next = (
                at.0 + heading.cos() * length / to_f32(BENDS),
                at.1 + heading.sin() * length / to_f32(BENDS),
            );
            self.stroke(at, next, color);
            at = next;
        }
        if depth >= 3 || length < 2.5 {
            return;
        }
        let forks = if hash(seed, depth, 33) < 0.65 { 2 } else { 1 };
        for fork in 0..forks {
            let turn = if forks == 1 {
                (hash(seed, fork, 34) - 0.5) * 0.6
            } else if fork == 0 {
                -(0.3 + hash(seed, fork, 34) * 0.45)
            } else {
                0.3 + hash(seed, fork, 34) * 0.45
            };
            let shorter = length * (0.5 + hash(seed, fork, 35) * 0.25);
            self.dendrite(at, heading + turn, shorter, depth + 1, seed * 4 + fork + 1);
        }
    }

    fn soma(&mut self, neuron: Neuron, color: Color) {
        let reach = neuron.size.ceil();
        let steps = floor_to_usize(reach * 2.0);
        for row in 0..=steps {
            for column in 0..=steps {
                let offset = (to_f32(column) - reach, to_f32(row) - reach);
                if offset.0.powi(2) + offset.1.powi(2) <= neuron.size.powi(2) {
                    self.dot((neuron.at.0 + offset.0, neuron.at.1 + offset.1), color);
                }
            }
        }
    }

    fn spots(&self) -> Vec<Point2> {
        let width = to_f32(self.columns) * SUB_X;
        let lanes = [to_f32(ROWS) * SUB_Y * 0.27, to_f32(ROWS) * SUB_Y * 0.75];
        let count = floor_to_usize(width / SPACING) + 1;
        let mut found = Vec::new();
        for (lane, row) in lanes.into_iter().enumerate() {
            let stagger = if lane == 0 { 0.0 } else { SPACING / 2.0 };
            for slot in 0..count {
                let column = to_f32(slot) * SPACING + stagger + (hash(slot, lane, 36) - 0.5) * 8.0;
                let row = row + (hash(slot, lane, 37) - 0.5) * 6.0;
                found.push((column, row));
            }
        }
        found
    }

    fn near_logo_column(&self, column: f32) -> bool {
        let left = to_f32(self.left) * SUB_X - CLEAR;
        let right = to_f32(self.left + self.width) * SUB_X + CLEAR;
        (left..=right).contains(&column)
    }

    fn neuron_layout(&self) -> Vec<Neuron> {
        let (width, height) = (to_f32(self.columns) * SUB_X, to_f32(ROWS) * SUB_Y);
        let mut found: Vec<Neuron> = Vec::new();
        for (seed, at) in self.spots().into_iter().enumerate() {
            let at = (at.0.round(), at.1.round());
            let visible =
                (-2.0..=width + 2.0).contains(&at.0) && (-1.0..=height + 1.0).contains(&at.1);
            let crowded = found.iter().any(|known| gap(known.at, at) < ROOM);
            if visible && !self.near_logo_column(at.0) && !crowded {
                found.push(Neuron {
                    at,
                    size: 1.8 + hash(seed, 0, 22) * 1.2,
                    color: cyclic(&self.colors, hash(seed, 0, 23)),
                });
            }
        }
        found
    }

    fn connect(&self, neurons: &[Neuron], from: usize, to: usize, lean: f32, seed: usize) -> Axon {
        let (neuron, target) = (neurons[from], neurons[to]);
        let length = gap(neuron.at, target.at).max(1.0);
        let direction = (
            (target.at.0 - neuron.at.0) / length,
            (target.at.1 - neuron.at.1) / length,
        );
        let start = (
            neuron.at.0 + direction.0 * (neuron.size + 0.5),
            neuron.at.1 + direction.1 * (neuron.size + 0.5),
        );
        let end = (
            target.at.0 - direction.0 * (target.size + 2.0),
            target.at.1 - direction.1 * (target.size + 2.0),
        );
        let middle = along(start, end, 0.5);
        let bend = (
            middle.0 - direction.1 * lean * length,
            middle.1 + direction.0 * lean * length,
        );
        let period = 1.6 + hash(seed, 0, 26) * 2.0;
        let progress = (self.seconds / period + hash(seed, 0, 27)).fract();
        Axon {
            from,
            to,
            start,
            bend,
            end,
            progress,
        }
    }

    fn axons(&self, neurons: &[Neuron]) -> Vec<Axon> {
        let mut found = Vec::new();
        for (from, neuron) in neurons.iter().enumerate() {
            let mut nearest: Vec<(f32, usize)> = neurons
                .iter()
                .enumerate()
                .filter(|(to, _)| *to != from)
                .map(|(to, other)| (gap(neuron.at, other.at), to))
                .collect();
            nearest.sort_by(|a, b| a.0.total_cmp(&b.0));
            let pick = usize::from(hash(from, 0, 24) < 0.5);
            if let Some(&(_, to)) = nearest.get(pick).or(nearest.first()) {
                let lean = (hash(from, 0, 25) - 0.5) * 0.5;
                found.push(self.connect(neurons, from, to, lean, from));
            }
        }
        let middle = to_f32(self.left) * SUB_X + to_f32(self.width) * SUB_X / 2.0;
        let inner = |left: bool| {
            neurons
                .iter()
                .enumerate()
                .filter(|(_, neuron)| (neuron.at.0 < middle) == left)
                .min_by(|a, b| {
                    (a.1.at.0 - middle)
                        .abs()
                        .total_cmp(&(b.1.at.0 - middle).abs())
                })
                .map(|(index, _)| index)
        };
        if let (Some(left), Some(right)) = (inner(true), inner(false)) {
            let seed = neurons.len();
            found.push(self.connect(neurons, left, right, 0.12, seed));
            found.push(self.connect(neurons, right, left, 0.12, seed + 1));
        }
        found
    }

    fn neurons(&mut self) {
        let neurons = self.neuron_layout();
        let axons = self.axons(&neurons);
        let mut charge = vec![0.0_f32; neurons.len()];
        for axon in &axons {
            charge[axon.to] = charge[axon.to].max(((axon.progress - 0.85) / 0.15).clamp(0.0, 1.0));
            charge[axon.from] =
                charge[axon.from].max(((0.1 - axon.progress) / 0.1).clamp(0.0, 1.0));
        }
        for (index, neuron) in neurons.iter().enumerate() {
            self.tint = self
                .background
                .mix(neuron.color, 0.35 + 0.45 * charge[index]);
            let branches = 4 + floor_to_usize(hash(index, 0, 28) * 3.0);
            for branch in 0..branches {
                let angle = to_f32(branch) / to_f32(branches) * TAU + hash(index, branch, 29);
                let length = 5.0 + hash(index, branch, 31) * 4.0;
                self.dendrite(neuron.at, angle, length, 0, index * 16 + branch);
            }
        }
        let mut signals: Vec<(Point2, Color)> = Vec::new();
        for axon in &axons {
            let color = self.background.mix(neurons[axon.from].color, 0.32);
            let steps = (gap(axon.start, axon.end) * 1.5).ceil().max(1.0);
            for step in 0..=floor_to_usize(steps) {
                let at = bezier(axon.start, axon.bend, axon.end, to_f32(step) / steps);
                self.dot(at, color);
            }
            let before = along(axon.bend, axon.end, 0.9);
            let heading = (axon.end.1 - before.1).atan2(axon.end.0 - before.0);
            for twig in [-0.7_f32, 0.0, 0.7] {
                let tip = (
                    axon.end.0 + (heading + twig).cos() * 1.5,
                    axon.end.1 + (heading + twig).sin() * 1.5,
                );
                self.stroke(axon.end, tip, color);
                self.dot(tip, self.background.mix(neurons[axon.from].color, 0.7));
            }
            let signal = self.foreground.mix(neurons[axon.from].color, 0.35);
            for back in (0..TAIL).rev() {
                let progress = axon.progress - to_f32(back) * 0.015;
                if progress >= 0.0 {
                    let fade = 1.0 - to_f32(back) / to_f32(TAIL);
                    let at = bezier(axon.start, axon.bend, axon.end, progress);
                    self.dot(at, self.background.mix(signal, fade));
                }
            }
            let head = bezier(axon.start, axon.bend, axon.end, axon.progress);
            signals.push(((head.0 / SUB_X, head.1 / SUB_Y), neurons[axon.from].color));
        }
        for (index, neuron) in neurons.iter().enumerate() {
            let body = self
                .background
                .mix(neuron.color, 0.65 + 0.35 * charge[index]);
            self.soma(*neuron, body);
            let nucleus = body.mix(self.foreground, 0.5 * charge[index]);
            self.dot(neuron.at, nucleus);
        }
        for (column, row) in self.letters() {
            let here = (to_f32(self.left + column), to_f32(TOP + row));
            let (glow, lit) = signals
                .iter()
                .map(|(at, color)| ((1.0 - cell_gap(here, *at) / REACH).max(0.0), *color))
                .fold((0.0, self.accent), |best, next| {
                    if next.0 > best.0 { next } else { best }
                });
            let color = self.ink.mix(lit, glow * 0.85);
            self.put(self.left + column, TOP + row, Glyph { ch: BLOCK, color });
        }
    }

    fn base_pair(&self, column: usize, depth: f32) -> (Color, Color) {
        let base = floor_to_usize(hash(column, 0, 40) * 4.0) % 4;
        let strength = (0.35 + 0.4 * (depth + 1.0) / 2.0) * self.fade;
        let dim = |index: usize| self.background.mix(self.colors[index], strength);
        (dim(base), dim(base ^ 1))
    }

    fn shaded(&self, color: Color, depth: f32) -> Color {
        self.background
            .mix(color, (0.35 + 0.6 * (depth + 1.0) / 2.0) * self.fade)
    }

    fn backbone(&mut self, column: f32, row: f32, depth: f32, color: Color) {
        if hash(floor_to_usize(column), floor_to_usize(row * 3.0), 50) < 0.06 {
            return;
        }
        let color = self.shaded(color, depth);
        self.dot((column, row), color);
        if depth > 0.5 {
            self.dot((column, row + 1.0), color);
        }
    }

    fn paired(&mut self, column: f32, one: (f32, f32, Color), two: (f32, f32, Color), step: usize) {
        let (back, front) = if one.1 < two.1 {
            (one, two)
        } else {
            (two, one)
        };
        self.backbone(column, back.0, back.1, back.2);
        let uneven = hash(step, 0, 52) < 0.25;
        if step.is_multiple_of(RUNG_EVERY) && !uneven && (front.0 - back.0).abs() > 1.5 {
            let colors = self.base_pair(step, f32::midpoint(back.1, front.1));
            let (from, to) = (one.0, two.0);
            let steps = floor_to_usize((to - from).abs().ceil());
            for at in 1..steps {
                let progress = to_f32(at) / to_f32(steps);
                let color = if progress < 0.5 { colors.0 } else { colors.1 };
                self.dot((column, from + (to - from) * progress), color);
            }
        }
        self.backbone(column, front.0, front.1, front.2);
    }

    fn blob(&mut self, at: Point2, radius: f32, color: Color, seed: usize) {
        let reach = (radius * 1.3).ceil();
        let steps = floor_to_usize(reach * 2.0);
        for row in 0..=steps {
            for column in 0..=steps {
                let offset = (to_f32(column) - reach, to_f32(row) - reach);
                let angle = offset.1.atan2(offset.0) + std::f32::consts::PI;
                let lobe = floor_to_usize(angle / TAU * 7.0) % 7;
                let edge = radius * (0.75 + 0.4 * hash(seed, lobe, 51));
                if offset.0.powi(2) + offset.1.powi(2) <= edge.powi(2) {
                    self.dot((at.0 + offset.0, at.1 + offset.1), color);
                }
            }
        }
    }

    fn ring_of(&mut self, at: Point2, radius: f32, beads: usize, turn: f32, color: Color) {
        for bead in 0..beads {
            let angle = turn + to_f32(bead) / to_f32(beads) * TAU;
            self.dot(
                (
                    at.0 + angle.cos() * radius,
                    at.1 + angle.sin() * radius * 0.8,
                ),
                color,
            );
        }
    }

    fn feed(&mut self, target: Point2, seed: usize) {
        for stream in 0..FEED {
            let angle = hash(seed, stream, 44) * TAU;
            let reach = 9.0 + hash(seed, stream, 45) * 7.0;
            let origin = (
                target.0 + angle.cos() * reach,
                target.1 + angle.sin() * reach * 0.5,
            );
            let progress = (self.seconds / 2.4 + hash(seed, stream, 46)).fract();
            let at = along(origin, target, progress * progress);
            let base = floor_to_usize(hash(seed, stream, 47) * 4.0) % 4;
            let color = self
                .background
                .mix(self.colors[base], 0.75 * (1.0 - progress * 0.7));
            self.dot(at, color);
        }
    }

    fn axis(&self, column: f32) -> f32 {
        let height = to_f32(ROWS) * SUB_Y;
        height / 2.0
            + 2.5 * (column * 0.021 + self.seconds * 0.15).sin()
            + 1.5 * (column * 0.057 + 1.3 - self.seconds * 0.1).sin()
    }

    fn wobble(&self, column: f32, strand: f32) -> f32 {
        0.45 * (column * 0.9 + self.seconds * 2.3 + strand * 1.7).sin()
    }

    fn daughters(&self, column: f32, fork: f32) -> (f32, f32) {
        let axis = self.axis(column);
        let spread = DIVERGE * smooth((fork - column) / SPREAD);
        (axis - spread, axis + spread)
    }

    fn dna(&mut self) {
        let width = to_f32(self.columns) * SUB_X;
        let fork = (self.seconds / FORK_CYCLE).fract() * (width + 80.0) - 40.0;
        let old = self.foreground;
        let new = self.accent.mix(self.colors[1], 0.4);
        let primer = self.colors[3].mix(self.foreground, 0.3);
        let coat = self.colors[0];
        for step in 0..floor_to_usize(width) {
            let column = to_f32(step);
            let axis = self.axis(column);
            let turn = column / HELIX_TURN * TAU - self.seconds * SPIN;
            let (one, two) = (turn.sin(), (turn + GROOVE).sin());
            let (one_depth, two_depth) = (turn.cos(), (turn + GROOVE).cos());
            let top = axis + one * HELIX_HEIGHT + self.wobble(column, 0.0);
            let bottom = axis + two * HELIX_HEIGHT + self.wobble(column, 1.0);
            if column >= fork {
                self.fade = 1.0 - smooth((column - fork - width * 0.45) / (width * 0.3));
                self.paired(
                    column,
                    (top, one_depth, old),
                    (bottom, two_depth, old),
                    step,
                );
                continue;
            }
            let behind = fork - column;
            self.fade = 1.0 - smooth((behind - width * 0.3) / (width * 0.25));
            if self.fade <= 0.02 {
                continue;
            }
            let open = smooth(behind / UNZIP);
            let (upper, lower) = self.daughters(column, fork);
            let partner = two * DAUGHTER_HEIGHT;
            let template_top = top + (upper + one * DAUGHTER_HEIGHT - top) * open;
            let template_bottom = bottom + (lower + one * DAUGHTER_HEIGHT - bottom) * open;
            if open >= 1.0 && behind >= LEAD_LAG {
                self.paired(
                    column,
                    (template_top, one_depth, old),
                    (upper + partner, two_depth, new),
                    step,
                );
            } else {
                self.backbone(column, template_top, one_depth, old);
            }
            match lagging(column, fork).filter(|_| open >= 1.0) {
                Some(false) => self.paired(
                    column,
                    (template_bottom, one_depth, old),
                    (lower + partner, two_depth, new),
                    step,
                ),
                Some(true) => {
                    self.backbone(column, template_bottom, one_depth, old);
                    self.backbone(column, lower + partner, two_depth, primer);
                }
                None => {
                    self.backbone(column, template_bottom, one_depth, old);
                    if open > 0.6 && step.is_multiple_of(4) {
                        self.dot((column, template_bottom + 1.5), self.shaded(coat, 0.0));
                    }
                }
            }
        }
        self.fade = 1.0;
        self.machinery(fork);
    }

    fn machinery(&mut self, fork: f32) {
        let seconds = self.seconds;
        let jiggle = move |seed: f32| (seconds * 3.1 + seed).sin() * 0.4;
        let topoisomerase = (fork + 22.0, self.axis(fork + 22.0));
        self.blob(
            topoisomerase,
            2.4,
            self.colors[3].mix(self.background, 0.3),
            3,
        );
        self.ring_of(
            (fork, self.axis(fork)),
            2.6,
            6,
            self.seconds * 2.2,
            self.accent,
        );
        let polymerase = self.colors[2].mix(self.foreground, 0.15);
        let lead = fork - LEAD_LAG;
        let leading_at = (
            lead + jiggle(0.0),
            self.daughters(lead, fork).0 + jiggle(1.7),
        );
        self.feed(leading_at, 1);
        self.blob(leading_at, 2.8, polymerase, 1);
        if let Some(front) = lagging_front(fork) {
            let lagging_at = (
                front + jiggle(3.1),
                self.daughters(front, fork).1 + jiggle(4.4),
            );
            self.feed(lagging_at, 2);
            self.blob(lagging_at, 2.6, polymerase, 2);
        }
        let helicase = (fork / SUB_X, self.axis(fork) / SUB_Y);
        for (column, row) in self.letters() {
            let here = (to_f32(self.left + column), to_f32(TOP + row));
            let glow = (1.0 - cell_gap(here, helicase) / 6.0).max(0.0);
            let color = self.ink.mix(self.accent, glow * 0.7);
            self.put(self.left + column, TOP + row, Glyph { ch: BLOCK, color });
        }
    }

    fn assemble(&mut self) {
        self.sparkles();
        for (column, row) in self.letters() {
            let delay = hash(column, row, 0) * SCATTER;
            let progress = ((self.seconds - delay) / TRAVEL).clamp(0.0, 1.0);
            let eased = ease_out(progress);
            let target = (to_f32(self.left + column), to_f32(TOP + row));
            let start = (
                hash(column, row, 1) * to_f32(self.columns),
                hash(column, row, 2) * to_f32(ROWS),
            );
            let at = (
                start.0 + (target.0 - start.0) * eased,
                start.1 + (target.1 - start.1) * eased,
            );
            let ch = match progress {
                done if done < 0.3 => '░',
                done if done < 0.6 => '▒',
                done if done < 1.0 => '▓',
                _ => BLOCK,
            };
            let color = cyclic(&self.colors, hash(column, row, 5)).mix(self.ink, eased);
            self.put(
                floor_to_usize(at.0 + 0.5),
                floor_to_usize(at.1 + 0.5),
                Glyph { ch, color },
            );
        }
    }
}

pub fn frame(animation: SplashAnimation, seconds: f32, columns: usize, faces: &Faces) -> Art {
    let mut scene = Scene::new(seconds, columns, faces);
    match animation {
        SplashAnimation::Plasma => scene.plasma(),
        SplashAnimation::Wave => scene.wave(),
        SplashAnimation::Assemble => scene.assemble(),
        SplashAnimation::Neurons => scene.neurons(),
        SplashAnimation::Dna => scene.dna(),
    }
    Art {
        cells: scene.cells,
        pixels: scene.pixels,
    }
}

pub fn runs(cells: &[Option<Glyph>]) -> Vec<(usize, String, Color)> {
    let mut found: Vec<(usize, String, Color)> = Vec::new();
    for (column, cell) in cells.iter().enumerate() {
        let Some(glyph) = cell else {
            continue;
        };
        match found.last_mut() {
            Some((start, text, color))
                if *color == glyph.color && *start + text.chars().count() == column =>
            {
                text.push(glyph.ch);
            }
            _ => found.push((column, glyph.ch.to_string(), glyph.color)),
        }
    }
    found
}

#[derive(Debug)]
pub struct SplashArt {
    animation: SplashAnimation,
    seconds: f32,
    faces: Faces,
    font: Font,
    size: Pixels,
}

#[derive(Debug, Default)]
struct State {
    measured_for: Option<(Font, Pixels)>,
    cell: Size,
}

impl SplashArt {
    pub fn new(
        animation: SplashAnimation,
        seconds: f32,
        faces: Faces,
        font: Font,
        size: impl Into<Pixels>,
    ) -> Self {
        Self {
            animation,
            seconds,
            faces,
            font,
            size: size.into(),
        }
    }

    fn text<C>(&self, content: C, bounds: Size) -> Text<C, Font> {
        Text {
            content,
            bounds,
            size: self.size,
            line_height: text::LineHeight::default(),
            font: self.font,
            align_x: text::Alignment::Left,
            align_y: Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::None,
        }
    }
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for SplashArt
where
    Renderer: text::Renderer<Font = Font>,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        if state.measured_for != Some((self.font, self.size)) {
            let bounds =
                Renderer::Paragraph::with_text(self.text(SAMPLE, Size::INFINITE)).min_bounds();
            state.cell = Size::new(bounds.width / to_f32(SAMPLE.len()), bounds.height);
            state.measured_for = Some((self.font, self.size));
        }
        let height = to_f32(ROWS) * state.cell.height;
        layout::Node::new(limits.resolve(Length::Fill, height, Size::new(0.0, height)))
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let cell = state.cell;
        if cell.width <= 0.0 {
            return;
        }
        let bounds = layout.bounds();
        let columns = floor_to_usize(bounds.width / cell.width);
        let art = frame(self.animation, self.seconds, columns, &self.faces);
        let dot = Size::new(cell.width / SUB_X, cell.height / SUB_Y);
        for pixel in &art.pixels {
            let at = Point::new(
                bounds.x + pixel.x * dot.width,
                bounds.y + pixel.y * dot.height,
            );
            if bounds.contains(at) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle::new(at, dot),
                        ..renderer::Quad::default()
                    },
                    style::color(pixel.color),
                );
            }
        }
        for (row, cells) in art.cells.iter().enumerate() {
            let y = bounds.y + to_f32(row) * cell.height;
            for (column, text, color) in runs(cells) {
                let x = bounds.x + to_f32(column) * cell.width;
                if text.chars().all(|ch| ch == BLOCK) {
                    let width = to_f32(text.chars().count()) * cell.width;
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle::new(Point::new(x, y), Size::new(width, cell.height)),
                            ..renderer::Quad::default()
                        },
                        style::color(color),
                    );
                } else {
                    renderer.fill_text(
                        self.text(text, Size::new(f32::INFINITY, cell.height)),
                        Point::new(x, y),
                        style::color(color),
                        bounds,
                    );
                }
            }
        }
    }
}

impl<'a, Message: 'a> From<SplashArt> for Element<'a, Message> {
    fn from(art: SplashArt) -> Self {
        Element::new(art)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxmacs_core::theme;

    fn faces() -> Faces {
        *theme::find("adwaita-dark").unwrap()
    }

    #[test]
    fn the_logo_spells_oxmacs_in_five_even_rows() {
        let logo = logo();

        assert_eq!(logo.len(), 5);
        assert!(logo.iter().all(|row| row.len() == 51));
        assert!(logo[0][1] && !logo[0][0]);
    }

    #[test]
    fn plasma_stays_between_zero_and_one() {
        for (x, y, t) in [(0.0, 0.0, 0.0), (40.0, 3.0, 2.5), (99.0, 8.0, 17.3)] {
            let value = plasma(x, y, t);
            assert!((0.0..=1.0).contains(&value), "{value}");
        }
    }

    #[test]
    fn every_animation_settles_with_the_logo_in_place() {
        let faces = faces();
        let logo = logo();
        for animation in [
            SplashAnimation::Plasma,
            SplashAnimation::Wave,
            SplashAnimation::Assemble,
            SplashAnimation::Neurons,
            SplashAnimation::Dna,
        ] {
            let grid = frame(animation, still(animation), 71, &faces).cells;
            assert_eq!(grid.len(), ROWS);
            for (ly, row) in logo.iter().enumerate() {
                for (lx, on) in row.iter().enumerate() {
                    if *on {
                        assert_eq!(
                            grid[TOP + ly][10 + lx].map(|glyph| glyph.ch),
                            Some(BLOCK),
                            "{animation:?} at {lx},{ly}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn neurons_grow_around_the_logo_and_send_signals() {
        let scene = Scene::new(0.0, 100, &faces());
        let neurons = scene.neuron_layout();
        let art = frame(
            SplashAnimation::Neurons,
            still(SplashAnimation::Neurons),
            100,
            &faces(),
        );

        assert!(neurons.len() >= 4);
        assert!(
            neurons
                .iter()
                .all(|neuron| !scene.near_logo_column(neuron.at.0))
        );
        assert!(art.pixels.len() > 200, "{}", art.pixels.len());
    }

    #[test]
    fn neurons_spread_all_around_the_logo() {
        let scene = Scene::new(0.0, 120, &faces());
        let neurons = scene.neuron_layout();
        let left = to_f32(scene.left) * SUB_X;
        let right = to_f32(scene.left + scene.width) * SUB_X;
        let middle = f32::midpoint(left, right);
        let side =
            |test: &dyn Fn(Point2) -> bool| neurons.iter().filter(|neuron| test(neuron.at)).count();

        assert!(side(&|at| at.0 < left) >= 1);
        assert!(side(&|at| at.0 > right) >= 1);
        assert!(side(&|at| at.0 < middle) >= 3);
        assert!(side(&|at| at.0 >= middle) >= 3);
        for (index, neuron) in neurons.iter().enumerate() {
            for other in &neurons[index + 1..] {
                assert!(gap(neuron.at, other.at) >= ROOM, "{neurons:?}");
            }
        }
    }

    #[test]
    fn every_neuron_sends_an_axon_to_another() {
        let scene = Scene::new(1.0, 120, &faces());
        let neurons = scene.neuron_layout();
        let axons = scene.axons(&neurons);

        assert_eq!(axons.len(), neurons.len() + 2);
        assert!(
            axons
                .iter()
                .all(|axon| axon.from != axon.to && (0.0..1.0).contains(&axon.progress))
        );
    }

    #[test]
    fn lagging_fragments_start_with_a_primer_and_grow_back_from_the_fork() {
        let fork = 95.0;

        assert_eq!(lagging(39.0, fork), Some(true));
        assert_eq!(lagging(30.0, fork), Some(false));
        assert_eq!(lagging(20.5, fork), None);
        assert_eq!(lagging(fork - 2.0, fork), None);
        assert_eq!(lagging(200.0, fork), None);
        let front = lagging_front(fork).unwrap();
        assert!(front < fork - OPENING && front > fork - OPENING - FRAGMENT * 2.0);
    }

    #[test]
    fn replicating_dna_draws_strands_enzymes_and_nucleotides() {
        let art = frame(
            SplashAnimation::Dna,
            still(SplashAnimation::Dna),
            100,
            &faces(),
        );

        assert!(art.pixels.len() > 600, "{}", art.pixels.len());
    }

    #[test]
    fn the_assembling_logo_starts_scattered() {
        let grid = frame(SplashAnimation::Assemble, 0.0, 71, &faces()).cells;

        let solid = grid
            .iter()
            .flatten()
            .filter(|cell| cell.is_some_and(|glyph| glyph.ch == BLOCK))
            .count();
        assert_eq!(solid, 0);
    }

    #[test]
    fn neighboring_glyphs_of_one_color_draw_as_one_run() {
        let red = Color::hex(0xFF_00_00);
        let blue = Color::hex(0x00_00_FF);
        let glyph = |ch, color| Some(Glyph { ch, color });

        assert_eq!(
            runs(&[
                glyph('a', red),
                glyph('b', red),
                None,
                glyph('c', red),
                glyph('d', blue)
            ]),
            [
                (0, "ab".to_owned(), red),
                (3, "c".to_owned(), red),
                (4, "d".to_owned(), blue)
            ]
        );
    }
}
