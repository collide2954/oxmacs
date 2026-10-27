use super::Sky;
use super::light::{
    Rgb, V3, added, blackbody, fbm, floor_of, hash, mixed, real, scaled, smoothstep, unit,
};

struct Layer {
    cells: f64,
    density: f64,
    gain: f64,
    salt: u64,
}

const LAYERS: [Layer; 3] = [
    Layer {
        cells: 24.0,
        density: 0.3,
        gain: 6.0,
        salt: 11,
    },
    Layer {
        cells: 70.0,
        density: 0.22,
        gain: 1.2,
        salt: 23,
    },
    Layer {
        cells: 180.0,
        density: 0.15,
        gain: 0.35,
        salt: 37,
    },
];

fn cube(direction: V3) -> (i64, f64, f64) {
    let (x, y, z) = (direction.x.abs(), direction.y.abs(), direction.z.abs());
    if x >= y && x >= z {
        (
            i64::from(direction.x < 0.0),
            direction.y / x,
            direction.z / x,
        )
    } else if y >= z {
        (
            2 + i64::from(direction.y < 0.0),
            direction.x / y,
            direction.z / y,
        )
    } else {
        (
            4 + i64::from(direction.z < 0.0),
            direction.x / z,
            direction.y / z,
        )
    }
}

fn from_cube(face: i64, u: f64, v: f64) -> V3 {
    match face {
        0 => V3::new(1.0, u, v),
        1 => V3::new(-1.0, u, v),
        2 => V3::new(u, 1.0, v),
        3 => V3::new(u, -1.0, v),
        4 => V3::new(u, v, 1.0),
        _ => V3::new(u, v, -1.0),
    }
}

fn stars(direction: V3, layer: &Layer, sky: &Sky, spread: f64) -> Rgb {
    let seed = sky.seed.wrapping_mul(1_000_003).wrapping_add(layer.salt);
    let (face, u, v) = cube(direction);
    let place = |coordinate: f64| f64::midpoint(coordinate, 1.0) * layer.cells;
    let (column, row) = (floor_of(place(u)), floor_of(place(v)));
    let count = floor_of(layer.cells);
    let width = spread.max(1e-5);
    let mut light = [0.0; 3];
    for dc in -1..=1 {
        for dr in -1..=1 {
            let (c, r) = (column + dc, row + dr);
            if c < 0 || r < 0 || c >= count || r >= count {
                continue;
            }
            let cell = hash(face, c, r, seed);
            if unit(cell) > layer.density * sky.stars.min(1.0) {
                continue;
            }
            let jitter = |salt: u64| unit(hash(face, c, r, seed ^ salt));
            let at = |index: i64, offset: f64| (real(index) + offset) / layer.cells * 2.0 - 1.0;
            let star = from_cube(face, at(c, jitter(1)), at(r, jitter(2))).normalized();
            let apart = (direction - star).length();
            let falloff = (-(apart * apart) / (2.0 * width * width)).exp();
            if falloff < 1e-3 {
                continue;
            }
            let brightness = layer.gain * (0.05 + jitter(3).powi(6)) * sky.stars.max(0.0);
            let kelvin = 2800.0 + 12_000.0 * jitter(4).powi(2);
            light = added(light, scaled(blackbody(kelvin), brightness * falloff));
        }
    }
    light
}

fn pole(seed: u64) -> (V3, V3) {
    let angle = |salt: u64| unit(hash(17, 3, 5, seed ^ salt)) * std::f64::consts::TAU;
    let (lean, around) = (0.35 + angle(1) * 0.1, angle(2));
    let normal = V3::new(
        lean.sin() * around.cos(),
        lean.sin() * around.sin(),
        lean.cos(),
    );
    let center = normal
        .cross(V3::new(around.sin(), -around.cos(), 0.0))
        .normalized();
    (normal, center)
}

fn band(direction: V3, sky: &Sky) -> Rgb {
    let (normal, center) = pole(sky.seed);
    let height = direction.dot(normal);
    let core = (-(height * height) / (2.0 * 0.2 * 0.2)).exp();
    if core < 1e-3 {
        return [0.0; 3];
    }
    let texture = fbm(direction * 3.0, 5, sky.seed ^ 0x65);
    let dust = fbm(direction * 7.5, 4, sky.seed ^ 0xCA);
    let lane = (-(height * height) / (2.0 * 0.06 * 0.06)).exp() * smoothstep(0.42, 0.68, dust);
    let bulge = f64::midpoint(direction.dot(center), 1.0).powi(4);
    let intensity = sky.band.max(0.0)
        * core
        * (0.25 + 0.75 * smoothstep(0.3, 0.75, texture))
        * (1.0 - 0.9 * lane)
        * (0.3 + 1.7 * bulge)
        * 0.4;
    scaled(
        mixed([0.72, 0.8, 1.0], [1.0, 0.8, 0.58], bulge.sqrt()),
        intensity,
    )
}

fn nebulae(direction: V3, sky: &Sky) -> Rgb {
    let cloud = fbm(direction * 1.7, 4, sky.seed ^ 0x12F);
    let mask = smoothstep(0.56, 0.8, cloud);
    if mask <= 0.0 {
        return [0.0; 3];
    }
    let hue = fbm(direction * 2.6, 3, sky.seed ^ 0x194);
    let detail = 0.6 + 0.4 * fbm(direction * 9.0, 3, sky.seed ^ 0x1F9);
    let color = mixed(
        [0.85, 0.25, 0.55],
        [0.2, 0.6, 0.75],
        smoothstep(0.35, 0.65, hue),
    );
    scaled(color, mask * detail * 0.09 * sky.nebulae.max(0.0))
}

pub(crate) fn light(direction: V3, sky: &Sky, spread: f64) -> Rgb {
    let direction = direction.normalized();
    let mut light = [sky.glow.max(0.0); 3];
    if sky.band > 0.0 {
        light = added(light, band(direction, sky));
    }
    if sky.nebulae > 0.0 {
        light = added(light, nebulae(direction, sky));
    }
    if sky.stars > 0.0 {
        for layer in &LAYERS {
            light = added(light, stars(direction, layer, sky, spread));
        }
    }
    light
}
