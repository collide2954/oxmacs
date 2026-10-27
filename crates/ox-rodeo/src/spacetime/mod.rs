mod light;
mod orbits;
mod sky;

pub use orbits::deflection;

use std::f64::consts::PI;
use std::sync::Mutex;

use light::{Rgb, V3, added, blackbody, displayed, fbm, scaled, smoothstep};
use orbits::{End, Path};
use ox_library::color::Color;
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Disk {
    pub inner: f64,
    pub outer: f64,
    pub brightness: f64,
    pub temperature: f64,
    pub turbulence: f64,
}

impl Default for Disk {
    fn default() -> Self {
        Self {
            inner: 3.0,
            outer: 12.0,
            brightness: 1.0,
            temperature: 4800.0,
            turbulence: 0.6,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Star {
    pub radius: f64,
    pub temperature: f64,
    pub brightness: f64,
}

impl Default for Star {
    fn default() -> Self {
        Self {
            radius: 5.0,
            temperature: 9500.0,
            brightness: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky {
    pub seed: u64,
    pub stars: f64,
    pub band: f64,
    pub nebulae: f64,
    pub glow: f64,
}

impl Default for Sky {
    fn default() -> Self {
        Self {
            seed: 7,
            stars: 1.0,
            band: 1.0,
            nebulae: 1.0,
            glow: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scene {
    pub lensing: bool,
    pub tilt: f64,
    pub turn: f64,
    pub distance: f64,
    pub fov: f64,
    pub time: f64,
    pub disk: Option<Disk>,
    pub star: Option<Star>,
    pub sky: Option<Sky>,
    pub doppler: bool,
    pub redshift: bool,
    pub exposure: f64,
    pub samples: u32,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            lensing: true,
            tilt: 0.1,
            turn: 0.0,
            distance: 30.0,
            fov: 1.0,
            time: 0.0,
            disk: None,
            star: None,
            sky: Some(Sky::default()),
            doppler: true,
            redshift: true,
            exposure: 1.0,
            samples: 1,
        }
    }
}

const POLE: V3 = V3::new(0.0, 0.0, 1.0);

struct View<'a> {
    scene: &'a Scene,
    toward: V3,
    right: V3,
    up: V3,
    focal: f64,
    spread: f64,
    distance: f64,
}

fn real(value: u32) -> f64 {
    f64::from(value)
}

impl<'a> View<'a> {
    fn new(scene: &'a Scene, width: u32) -> Self {
        let (tilt, turn) = (scene.tilt, scene.turn);
        let toward = V3::new(tilt.cos() * turn.cos(), tilt.cos() * turn.sin(), tilt.sin());
        let lifted = POLE - toward * POLE.dot(toward);
        let up = if lifted.length() < 1e-9 {
            V3::new(-turn.cos(), -turn.sin(), 0.0)
        } else {
            lifted.normalized()
        };
        let right = (toward * -1.0).cross(up);
        let fov = scene.fov.clamp(1e-4, 3.0);
        let focal = real(width.max(1)) / 2.0 / (fov / 2.0).tan();
        Self {
            scene,
            toward,
            right,
            up,
            focal,
            spread: fov / real(width.max(1)),
            distance: scene.distance.max(1.5),
        }
    }

    fn pixel(&self, x: f64, y: f64, sky: impl FnOnce(V3) -> Rgb) -> Rgb {
        let along = (x * x + y * y).sqrt();
        let angle = (along / self.focal).atan();
        let around = y.atan2(x);
        let side = self.right * around.cos() + self.up * around.sin();
        let mut impact = self.distance * angle.sin();
        if self.scene.lensing {
            impact /= (1.0 - 1.0 / self.distance).sqrt();
        }
        let path = Path::of(impact, self.scene.lensing);
        let start = path.reach(1.0 / self.distance).unwrap_or(0.0);
        let star = self
            .scene
            .star
            .filter(|star| star.radius > if self.scene.lensing { 1.0 } else { 0.0 });
        let surface = star.and_then(|star| Some((star, path.reach(1.0 / star.radius)?)));
        let limit = surface.map_or(
            match path.end() {
                End::Escapes(end) | End::Falls(end) => end,
            },
            |(_, at)| at,
        );
        let mut light = [0.0; 3];
        let mut through = 1.0;
        if let Some(disk) = self.scene.disk {
            let facing = POLE.dot(self.toward);
            let across = POLE.dot(side);
            let first = (-facing).atan2(across).rem_euclid(PI);
            let first = if first < 1e-9 { PI } else { first };
            for crossing in 0..6 {
                let actual = first + f64::from(crossing) * PI;
                if actual + start >= limit {
                    break;
                }
                let Some(u) = path.at(actual + start) else {
                    break;
                };
                if u <= 0.0 {
                    continue;
                }
                let radius = 1.0 / u;
                if radius < disk.inner || radius > disk.outer {
                    continue;
                }
                let point = (self.toward * actual.cos() + side * actual.sin()) * radius;
                let (shade, cover) = self.disk_light(&disk, point, radius, impact, side);
                light = added(light, scaled(shade, through * cover));
                through *= 1.0 - cover;
                if through < 0.01 {
                    return light;
                }
            }
        }
        let behind = match (surface, path.end()) {
            (Some((star, at)), _) => self.star_light(&star, at - start, impact, side),
            (None, End::Escapes(end)) => {
                let out = end - start;
                sky(self.toward * out.cos() + side * out.sin())
            }
            (None, End::Falls(_)) => [0.0; 3],
        };
        added(light, scaled(behind, through))
    }

    fn shift(&self, radius: f64, impact: f64, side: V3) -> f64 {
        let spin = (0.5 / radius.powi(3)).sqrt();
        let momentum = impact * side.cross(self.toward).dot(POLE);
        let lensing = self.scene.lensing;
        let orbit = |gravity: bool| {
            let moving = if lensing {
                (1.0 - 1.5 / radius).max(1e-6)
                    / if gravity {
                        1.0
                    } else {
                        (1.0 - 1.0 / radius).max(1e-6)
                    }
            } else {
                (1.0 - 0.5 / radius).max(1e-6)
            };
            moving.sqrt() / (1.0 - spin * momentum).max(1e-6)
        };
        match (self.scene.doppler, self.scene.redshift && lensing) {
            (true, gravity) => orbit(gravity),
            (false, true) => (1.0 - 1.0 / radius).max(0.0).sqrt(),
            (false, false) => 1.0,
        }
    }

    fn disk_light(&self, disk: &Disk, point: V3, radius: f64, impact: f64, side: V3) -> (Rgb, f64) {
        let inner = disk.inner.max(1.0);
        let peak = 49.0 / 36.0 * inner;
        let flux = |r: f64| (1.0 - (inner / r).sqrt()).max(0.0) / r.powi(3);
        let warmth = (flux(radius) / flux(peak)).clamp(0.0, 1.0);
        let spin = (0.5 / radius.powi(3)).sqrt();
        let angle = point.y.atan2(point.x) - spin * self.scene.time;
        let ring = radius.ln();
        let swirl = V3::new(ring * 8.0, angle.cos() * 3.0, angle.sin() * 3.0);
        let streaks = V3::new(ring * 45.0, angle.cos() * 1.6, angle.sin() * 1.6);
        let grain = 0.5 * fbm(swirl, 4, 41) + 0.5 * fbm(streaks, 3, 43);
        let texture = (1.0 + disk.turbulence * (grain - 0.5) * 3.2).max(0.0);
        let g = self.shift(radius, impact, side);
        let kelvin = disk.temperature * warmth.powf(0.25) * g;
        let intensity = disk.brightness.max(1.0) * warmth * texture * g.powi(4) * 0.85;
        let cover = smoothstep(disk.outer, disk.outer * 0.72, radius)
            * smoothstep(disk.inner * 0.98, disk.inner * 1.05, radius)
            * disk.brightness.clamp(0.0, 1.0);
        (scaled(blackbody(kelvin), intensity), cover)
    }

    fn star_light(&self, star: &Star, angle: f64, impact: f64, side: V3) -> Rgb {
        let lensing = self.scene.lensing;
        let g = if lensing && self.scene.redshift {
            (1.0 - 1.0 / star.radius).max(0.0).sqrt()
        } else {
            1.0
        };
        let squeeze = if lensing {
            (1.0 - 1.0 / star.radius).max(0.0).sqrt()
        } else {
            1.0
        };
        let slant = (impact / star.radius * squeeze).clamp(0.0, 1.0);
        let facing = (1.0 - slant * slant).sqrt();
        let normal = self.toward * angle.cos() + side * angle.sin();
        let cells = 0.7
            + 0.6
                * fbm(
                    normal * 6.0 + V3::new(0.0, 0.0, self.scene.time * 0.05),
                    4,
                    61,
                );
        let intensity = star.brightness * g.powi(4) * (0.2 + 0.8 * facing) * cells * 1.5;
        scaled(blackbody(star.temperature * g), intensity)
    }
}

fn offsets(samples: u32) -> Vec<f64> {
    let count = samples.clamp(1, 4);
    (0..count)
        .map(|index| (real(index) + 0.5) / real(count) - 0.5)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Key {
    width: u32,
    height: u32,
    samples: u32,
    lensing: bool,
    tilt: f64,
    turn: f64,
    distance: f64,
    fov: f64,
    sky: Option<Sky>,
}

#[derive(Debug)]
struct Skies {
    key: Key,
    light: Vec<Option<[f32; 3]>>,
}

static SKIES: Mutex<Option<Skies>> = Mutex::new(None);

fn kept(light: Rgb) -> [f32; 3] {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "sky radiance is a small positive number"
    )]
    let kept = light.map(|channel| channel as f32);
    kept
}

pub fn render(scene: &Scene, width: u32, height: u32) -> Vec<Color> {
    if scene.lensing {
        orbits::warm();
    }
    let view = View::new(scene, width);
    let jitter = offsets(scene.samples);
    let per_pixel = jitter.len() * jitter.len();
    let weight = 1.0 / real(u32::try_from(per_pixel).unwrap_or(1));
    let columns = usize::try_from(width).unwrap_or(0);
    let rows = usize::try_from(height).unwrap_or(0);
    let mut pixels = vec![Color::BLACK; columns * rows];
    if columns == 0 || rows == 0 {
        return pixels;
    }
    let key = Key {
        width,
        height,
        samples: scene.samples,
        lensing: scene.lensing,
        tilt: scene.tilt,
        turn: scene.turn,
        distance: scene.distance,
        fov: scene.fov,
        sky: scene.sky,
    };
    let mut skies = SKIES
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .filter(|skies| skies.key == key && skies.light.len() == columns * rows * per_pixel)
        .unwrap_or_else(|| Skies {
            key,
            light: vec![None; columns * rows * per_pixel],
        });
    let (half_width, half_height) = (real(width) / 2.0, real(height) / 2.0);
    pixels
        .par_chunks_mut(columns)
        .zip(skies.light.par_chunks_mut(columns * per_pixel))
        .enumerate()
        .for_each(|(row, (line, known))| {
            let row = real(u32::try_from(row).unwrap_or(0));
            for (column, pixel) in line.iter_mut().enumerate() {
                let slots = &mut known[column * per_pixel..(column + 1) * per_pixel];
                let column = real(u32::try_from(column).unwrap_or(0));
                let mut light = [0.0; 3];
                for (index, (dy, dx)) in jitter
                    .iter()
                    .flat_map(|dy| jitter.iter().map(move |dx| (dy, dx)))
                    .enumerate()
                {
                    let x = column + 0.5 + dx - half_width;
                    let y = half_height - (row + 0.5 + dy);
                    let slot = &mut slots[index];
                    let seen = view.pixel(x, y, |direction| {
                        if let Some(stored) = slot {
                            return stored.map(f64::from);
                        }
                        let found = kept(
                            scene
                                .sky
                                .map_or([0.0; 3], |sky| sky::light(direction, &sky, view.spread)),
                        );
                        *slot = Some(found);
                        found.map(f64::from)
                    });
                    light = added(light, seen);
                }
                *pixel = displayed(scaled(light, weight), scene.exposure);
            }
        });
    if let Ok(mut slot) = SKIES.lock() {
        *slot = Some(skies);
    }
    pixels
}
