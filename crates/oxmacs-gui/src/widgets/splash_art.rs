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

pub fn frame(
    animation: SplashAnimation,
    seconds: f32,
    columns: usize,
    faces: &Faces,
) -> Vec<Vec<Option<Glyph>>> {
    let mut scene = Scene::new(seconds, columns, faces);
    match animation {
        SplashAnimation::Plasma => scene.plasma(),
        SplashAnimation::Wave => scene.wave(),
        SplashAnimation::Assemble => scene.assemble(),
    }
    scene.cells
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
        let grid = frame(self.animation, self.seconds, columns, &self.faces);
        for (row, cells) in grid.iter().enumerate() {
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
        ] {
            let grid = frame(animation, still(animation), 71, &faces);
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
    fn the_assembling_logo_starts_scattered() {
        let grid = frame(SplashAnimation::Assemble, 0.0, 71, &faces());

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
