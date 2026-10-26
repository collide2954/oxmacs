use std::sync::Arc;

use image::{GrayImage, Luma, RgbaImage};
use imageproc::contrast::otsu_level;
use rayon::prelude::*;

use super::{Image, MODULE, Options, colors, later, number, wanted};
use ox_lang::Isolate;
use ox_lang::error::{Error, ErrorKind};
use ox_lang::heap::Heap;
use ox_lang::value::{Value, print};

type Table = [u8; 256];
type Matrix = [[f32; 3]; 3];
type Check = fn(&Heap, &str, Value) -> Result<f32, Error>;
type Curve = fn(f32, f32) -> f32;
type Mix = fn(f32) -> Matrix;

const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
const CSS_LUMA: [f32; 3] = [0.213, 0.715, 0.072];
const IDENTITY: Matrix = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
const GRAY: Matrix = [LUMA, LUMA, LUMA];
const CSS_GRAY: Matrix = [CSS_LUMA, CSS_LUMA, CSS_LUMA];
const SEPIA: Matrix = [
    [0.393, 0.769, 0.189],
    [0.349, 0.686, 0.168],
    [0.272, 0.534, 0.131],
];
const TURN: Matrix = [
    [-0.213, -0.715, 0.928],
    [0.143, 0.140, -0.283],
    [-0.787, 0.715, 0.072],
];

pub(super) fn narrow(number: f64) -> f32 {
    #[expect(clippy::cast_possible_truncation, reason = "amounts fit in f32")]
    let narrowed = number as f32;
    narrowed
}

pub(super) fn unit(channel: u8) -> f32 {
    f32::from(channel) / 255.0
}

pub(super) fn byte(value: f32) -> u8 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to a byte"
    )]
    let fitted = (value * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    fitted
}

fn table(curve: impl Fn(f32) -> f32) -> Table {
    let mut out = [0; 256];
    for (slot, channel) in out.iter_mut().zip(0..=u8::MAX) {
        *slot = byte(curve(unit(channel)));
    }
    out
}

pub(super) fn each(pixels: &mut RgbaImage, change: impl Fn(&mut [u8]) + Sync + Send) {
    pixels.par_chunks_mut(4 * 4096).for_each(|chunk| {
        chunk
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|pixel| change(pixel));
    });
}

fn looked_up(pixels: &mut RgbaImage, tables: &[Table; 4]) {
    each(pixels, |pixel| {
        for (channel, table) in pixel.iter_mut().zip(tables) {
            *channel = table[usize::from(*channel)];
        }
    });
}

fn rgb_tables(red: Table, green: Table, blue: Table) -> [Table; 4] {
    [red, green, blue, table(|value| value)]
}

fn applied(pixel: &mut [u8], matrix: &Matrix) {
    let rgb = [unit(pixel[0]), unit(pixel[1]), unit(pixel[2])];
    for (channel, row) in pixel.iter_mut().zip(matrix) {
        *channel = byte(row[0] * rgb[0] + row[1] * rgb[1] + row[2] * rgb[2]);
    }
}

fn blend(from: &Matrix, to: &Matrix, amount: f32) -> Matrix {
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            from[row][column] + (to[row][column] - from[row][column]) * amount
        })
    })
}

fn hue(degrees: f32) -> Matrix {
    let (sin, cos) = degrees.to_radians().sin_cos();
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            let base = CSS_LUMA[column];
            base + cos * (IDENTITY[row][column] - base) + sin * TURN[row][column]
        })
    })
}

pub(super) fn luma(pixel: &[u8]) -> u8 {
    byte(LUMA[0] * unit(pixel[0]) + LUMA[1] * unit(pixel[1]) + LUMA[2] * unit(pixel[2]))
}

pub(super) fn changed(
    heap: &mut Heap,
    name: &'static str,
    image: Arc<Image>,
    change: impl FnOnce(&mut RgbaImage) + Send + 'static,
) -> Result<Value, Error> {
    later(heap, name, move || {
        let mut pixels = image.pixels.clone();
        change(&mut pixels);
        Ok(image.with(pixels))
    })
}

pub(super) fn outside(heap: &Heap, name: &str, what: &str, value: Value) -> Error {
    Error::new(format!(
        "image/{name} expects {what}, got {}",
        print::brief(heap, value)
    ))
    .of(ErrorKind::Range)
}

pub(super) fn factor(heap: &Heap, name: &str, value: Value) -> Result<f32, Error> {
    let found = number(heap, name, "a factor", value)?;
    if found < 0.0 {
        return Err(outside(heap, name, "a factor of 0 or more", value));
    }
    Ok(narrow(found))
}

fn above_zero(heap: &Heap, name: &str, value: Value) -> Result<f32, Error> {
    let found = number(heap, name, "a gamma", value)?;
    if found <= 0.0 {
        return Err(outside(heap, name, "a gamma above 0", value));
    }
    Ok(narrow(found))
}

fn any(heap: &Heap, name: &str, value: Value) -> Result<f32, Error> {
    number(heap, name, "an amount", value).map(narrow)
}

pub(super) fn strength(heap: &Heap, name: &str, value: Option<&Value>) -> Result<f32, Error> {
    let Some(value) = value else {
        return Ok(1.0);
    };
    let found = number(heap, name, "a strength", *value)?;
    if !(0.0..=1.0).contains(&found) {
        return Err(outside(heap, name, "a strength from 0 to 1", *value));
    }
    Ok(narrow(found))
}

fn level(
    heap: &Heap,
    name: &str,
    key: &str,
    value: Option<Value>,
    default: f32,
) -> Result<f32, Error> {
    let Some(value) = value else {
        return Ok(default);
    };
    let found = number(heap, name, &format!(":{key}"), value)?;
    if !(0.0..=255.0).contains(&found) {
        return Err(outside(heap, name, &format!(":{key} from 0 to 255"), value));
    }
    Ok(narrow(found))
}

fn curve_points(heap: &Heap, value: Value) -> Result<Vec<(f32, f32)>, Error> {
    let wrong = || {
        outside(
            heap,
            "curves",
            "two or more [in out] points from 0 to 255",
            value,
        )
    };
    let within = |item: Value| {
        let found = number(heap, "curves", "a point", item).ok()?;
        (0.0..=255.0).contains(&found).then(|| narrow(found))
    };
    let items = heap.elements(value).ok_or_else(wrong)?;
    let mut points = items
        .iter()
        .map(|item| {
            let pair = heap.elements(*item)?;
            let [input, output] = pair.as_slice() else {
                return None;
            };
            Some((within(*input)?, within(*output)?))
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(wrong)?;
    points.sort_by(|left, right| left.0.total_cmp(&right.0));
    if points.len() < 2 || points.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
        return Err(wrong());
    }
    Ok(points)
}

fn tangents(points: &[(f32, f32)]) -> Vec<f32> {
    let slopes: Vec<f32> = points
        .windows(2)
        .map(|pair| (pair[1].1 - pair[0].1) / (pair[1].0 - pair[0].0))
        .collect();
    let mut tangents: Vec<f32> = (0..points.len())
        .map(|index| {
            let before = index.checked_sub(1).map(|earlier| slopes[earlier]);
            match (before, slopes.get(index).copied()) {
                (Some(left), Some(right)) if left * right > 0.0 => f32::midpoint(left, right),
                (Some(only), None) | (None, Some(only)) => only,
                _ => 0.0,
            }
        })
        .collect();
    for (index, slope) in slopes.iter().enumerate() {
        if slope.abs() < f32::EPSILON {
            tangents[index] = 0.0;
            tangents[index + 1] = 0.0;
            continue;
        }
        let (alpha, beta) = (tangents[index] / slope, tangents[index + 1] / slope);
        let length = alpha.hypot(beta);
        if length > 3.0 {
            tangents[index] = 3.0 / length * alpha * slope;
            tangents[index + 1] = 3.0 / length * beta * slope;
        }
    }
    tangents
}

fn curve_table(points: &[(f32, f32)]) -> Table {
    let tangents = tangents(points);
    let (first, last) = (points[0], points[points.len() - 1]);
    let mut out = [0; 256];
    for (slot, channel) in out.iter_mut().zip(0..=u8::MAX) {
        let input = f32::from(channel);
        let value = if input <= first.0 {
            first.1
        } else if input >= last.0 {
            last.1
        } else {
            let index = points
                .windows(2)
                .position(|pair| input <= pair[1].0)
                .unwrap_or(0);
            let ((x0, y0), (x1, y1)) = (points[index], points[index + 1]);
            let width = x1 - x0;
            let along = (input - x0) / width;
            let (squared, cubed) = (along * along, along * along * along);
            (2.0 * cubed - 3.0 * squared + 1.0) * y0
                + (cubed - 2.0 * squared + along) * width * tangents[index]
                + (-2.0 * cubed + 3.0 * squared) * y1
                + (cubed - squared) * width * tangents[index + 1]
        };
        *slot = byte(value / 255.0);
    }
    out
}

fn spread(colors: &[[u8; 3]]) -> (usize, u8) {
    (0..3)
        .map(|channel| {
            let (low, high) = colors.iter().fold((u8::MAX, 0), |(low, high), color| {
                (low.min(color[channel]), high.max(color[channel]))
            });
            (channel, high.saturating_sub(low))
        })
        .max_by_key(|(_, range)| *range)
        .unwrap_or((0, 0))
}

fn average(colors: &[[u8; 3]]) -> [u8; 3] {
    let mut sums = [0_u64; 3];
    for color in colors {
        for (sum, channel) in sums.iter_mut().zip(color) {
            *sum += u64::from(*channel);
        }
    }
    let count = u64::try_from(colors.len()).unwrap_or(1).max(1);
    sums.map(|sum| u8::try_from((sum + count / 2) / count).unwrap_or(u8::MAX))
}

fn median_cut(pixels: &RgbaImage, count: usize) -> Vec<[u8; 3]> {
    let step = (pixels.as_raw().len() / 4 / 65_536).max(1);
    let samples: Vec<[u8; 3]> = pixels
        .pixels()
        .step_by(step)
        .filter(|pixel| pixel[3] > 0)
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    if samples.is_empty() {
        return vec![[0, 0, 0]];
    }
    let mut boxes = vec![samples];
    while boxes.len() < count {
        let widest = boxes
            .iter()
            .enumerate()
            .map(|(index, colors)| (index, spread(colors)))
            .max_by_key(|(_, (_, range))| *range);
        let Some((index, (channel, range))) = widest else {
            break;
        };
        if range == 0 {
            break;
        }
        let mut colors = boxes.swap_remove(index);
        colors.sort_unstable_by_key(|color| color[channel]);
        let middle = colors.len() / 2;
        let split = (1..colors.len())
            .filter(|at| colors[at - 1][channel] != colors[*at][channel])
            .min_by_key(|at| at.abs_diff(middle))
            .unwrap_or(middle);
        let upper = colors.split_off(split);
        boxes.push(colors);
        boxes.push(upper);
    }
    boxes.iter().map(|colors| average(colors)).collect()
}

fn distance(candidate: [u8; 3], color: [f32; 3]) -> f32 {
    candidate
        .iter()
        .zip(color)
        .map(|(channel, wanted)| (f32::from(*channel) - wanted).powi(2))
        .sum()
}

fn nearest(palette: &[[u8; 3]], color: [f32; 3]) -> [u8; 3] {
    palette
        .iter()
        .copied()
        .min_by(|left, right| distance(*left, color).total_cmp(&distance(*right, color)))
        .unwrap_or([0, 0, 0])
}

fn dithered(pixels: &mut RgbaImage, palette: &[[u8; 3]]) {
    let width = usize::try_from(pixels.width()).unwrap_or(0);
    let mut here = vec![[0.0_f32; 3]; width + 2];
    let mut below = vec![[0.0_f32; 3]; width + 2];
    for row in pixels.rows_mut() {
        for (column, pixel) in row.enumerate() {
            let goal: [f32; 3] = std::array::from_fn(|channel| {
                f32::from(pixel[channel]) + here[column + 1][channel]
            });
            let chosen = nearest(palette, goal);
            for (channel, (wanted, made)) in goal.iter().zip(chosen).enumerate() {
                let error = wanted - f32::from(made);
                here[column + 2][channel] += error * 7.0 / 16.0;
                below[column][channel] += error * 3.0 / 16.0;
                below[column + 1][channel] += error * 5.0 / 16.0;
                below[column + 2][channel] += error / 16.0;
                pixel[channel] = made;
            }
        }
        std::mem::swap(&mut here, &mut below);
        below.fill([0.0; 3]);
    }
}

enum Palette {
    Count(usize),
    Colors(Vec<[u8; 3]>),
}

fn palette(heap: &Heap, value: Value) -> Result<Palette, Error> {
    let wrong = || {
        outside(
            heap,
            "quantize",
            "2 to 256 colors or a vector of colors",
            value,
        )
    };
    if let Value::Int(count) = value {
        return usize::try_from(count)
            .ok()
            .filter(|count| (2..=256).contains(count))
            .map(Palette::Count)
            .ok_or_else(wrong);
    }
    let items = heap
        .elements(value)
        .filter(|items| !items.is_empty())
        .ok_or_else(wrong)?;
    items
        .iter()
        .map(|item| {
            colors::color(heap, "quantize", *item).map(|color| [color[0], color[1], color[2]])
        })
        .collect::<Result<_, _>>()
        .map(Palette::Colors)
}

pub(super) fn install(isolate: &mut Isolate) {
    install_tones(isolate);
    install_mixes(isolate);
    install_levels(isolate);
    install_reductions(isolate);
}

fn install_tones(isolate: &mut Isolate) {
    let tones: [(&'static str, Check, Curve); 4] = [
        ("brightness", factor, |value, by| value * by),
        ("contrast", factor, |value, by| (value - 0.5) * by + 0.5),
        ("gamma", above_zero, |value, gamma| value.powf(1.0 / gamma)),
        ("exposure", any, |value, stops| value * stops.exp2()),
    ];
    for (name, check, curve) in tones {
        isolate
            .native(MODULE, name)
            .arity(2, Some(2))
            .suspends()
            .function(move |heap, arguments| {
                let image = wanted(heap, name, arguments[0])?;
                let amount = check(heap, name, arguments[1])?;
                let shared = table(|value| curve(value, amount));
                changed(heap, name, image, move |pixels| {
                    looked_up(pixels, &rgb_tables(shared, shared, shared));
                })
            });
    }
    isolate
        .native(MODULE, "invert")
        .arity(1, Some(2))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "invert", arguments[0])?;
            let amount = strength(heap, "invert", arguments.get(1))?;
            let shared = table(|value| value * (1.0 - amount) + (1.0 - value) * amount);
            changed(heap, "invert", image, move |pixels| {
                looked_up(pixels, &rgb_tables(shared, shared, shared));
            })
        });
    isolate
        .native(MODULE, "posterize")
        .arity(2, Some(2))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "posterize", arguments[0])?;
            let levels = match arguments[1] {
                Value::Int(levels) if (2..=256).contains(&levels) => {
                    f32::from(u16::try_from(levels - 1).unwrap_or(1))
                }
                other => return Err(outside(heap, "posterize", "2 to 256 levels", other)),
            };
            let shared = table(|value| (value * levels).round() / levels);
            changed(heap, "posterize", image, move |pixels| {
                looked_up(pixels, &rgb_tables(shared, shared, shared));
            })
        });
}

fn install_mixes(isolate: &mut Isolate) {
    let mixes: [(&'static str, Check, Mix); 2] = [
        ("saturation", factor, |by| blend(&CSS_GRAY, &IDENTITY, by)),
        ("hue", any, hue),
    ];
    for (name, check, matrix) in mixes {
        isolate
            .native(MODULE, name)
            .arity(2, Some(2))
            .suspends()
            .function(move |heap, arguments| {
                let image = wanted(heap, name, arguments[0])?;
                let matrix = matrix(check(heap, name, arguments[1])?);
                changed(heap, name, image, move |pixels| {
                    each(pixels, |pixel| applied(pixel, &matrix));
                })
            });
    }
    for (name, target) in [("grayscale", GRAY), ("sepia", SEPIA)] {
        isolate
            .native(MODULE, name)
            .arity(1, Some(2))
            .suspends()
            .function(move |heap, arguments| {
                let image = wanted(heap, name, arguments[0])?;
                let matrix = blend(&IDENTITY, &target, strength(heap, name, arguments.get(1))?);
                changed(heap, name, image, move |pixels| {
                    each(pixels, |pixel| applied(pixel, &matrix));
                })
            });
    }
    isolate
        .native(MODULE, "vibrance")
        .arity(2, Some(2))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "vibrance", arguments[0])?;
            let by = factor(heap, "vibrance", arguments[1])?;
            changed(heap, "vibrance", image, move |pixels| {
                each(pixels, |pixel| {
                    let rgb = &pixel[..3];
                    let low = rgb.iter().copied().min().unwrap_or(0);
                    let high = rgb.iter().copied().max().unwrap_or(0);
                    let muted = 1.0 - unit(high - low);
                    applied(
                        pixel,
                        &blend(&CSS_GRAY, &IDENTITY, 1.0 + (by - 1.0) * muted),
                    );
                });
            })
        });
    isolate
        .native(MODULE, "tint")
        .arity(2, Some(3))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "tint", arguments[0])?;
            let color = colors::color(heap, "tint", arguments[1])?;
            let amount = strength(heap, "tint", arguments.get(2))?;
            let [red, green, blue] = [0, 1, 2].map(|channel| {
                let by = 1.0 + (unit(color[channel]) - 1.0) * amount;
                table(|value| value * by)
            });
            changed(heap, "tint", image, move |pixels| {
                looked_up(pixels, &rgb_tables(red, green, blue));
            })
        });
    isolate
        .native(MODULE, "balance")
        .arity(1, Some(9))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "balance", arguments[0])?;
            let options = Options::read(
                heap,
                "balance",
                &arguments[1..],
                &["red", "green", "blue", "neutral"],
            )?;
            let mut factors = [1.0_f32; 3];
            for (by, key) in factors.iter_mut().zip(["red", "green", "blue"]) {
                if let Some(value) = options.get(key) {
                    *by = factor(heap, "balance", value)?;
                }
            }
            if let Some(value) = options.get("neutral") {
                let neutral = colors::color(heap, "balance", value)?;
                let rgb = [unit(neutral[0]), unit(neutral[1]), unit(neutral[2])];
                let mean = rgb.iter().sum::<f32>() / 3.0;
                for (by, channel) in factors.iter_mut().zip(rgb) {
                    if channel > 0.0 {
                        *by *= mean / channel;
                    }
                }
            }
            let [red, green, blue] = factors.map(|by| table(|value| value * by));
            changed(heap, "balance", image, move |pixels| {
                looked_up(pixels, &rgb_tables(red, green, blue));
            })
        });
}

fn install_levels(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "levels")
        .arity(1, Some(11))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "levels", arguments[0])?;
            let options = Options::read(
                heap,
                "levels",
                &arguments[1..],
                &["black", "white", "gamma", "output-black", "output-white"],
            )?;
            let black = level(heap, "levels", "black", options.get("black"), 0.0)?;
            let white = level(heap, "levels", "white", options.get("white"), 255.0)?;
            let low = level(
                heap,
                "levels",
                "output-black",
                options.get("output-black"),
                0.0,
            )?;
            let high = level(
                heap,
                "levels",
                "output-white",
                options.get("output-white"),
                255.0,
            )?;
            let gamma = match options.get("gamma") {
                Some(value) => above_zero(heap, "levels", value)?,
                None => 1.0,
            };
            if white <= black {
                return Err(
                    Error::new("image/levels expects :white above :black").of(ErrorKind::Range)
                );
            }
            let shared = table(|value| {
                let along = ((value * 255.0 - black) / (white - black)).clamp(0.0, 1.0);
                (low + (high - low) * along.powf(1.0 / gamma)) / 255.0
            });
            changed(heap, "levels", image, move |pixels| {
                looked_up(pixels, &rgb_tables(shared, shared, shared));
            })
        });
    isolate
        .native(MODULE, "curves")
        .arity(2, Some(4))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "curves", arguments[0])?;
            let points = curve_points(heap, arguments[1])?;
            let options = Options::read(heap, "curves", &arguments[2..], &["channel"])?;
            let chosen = match options.get("channel") {
                None => Some(0..3),
                Some(Value::Keyword(keyword)) => match keyword.name() {
                    "all" => Some(0..3),
                    "red" => Some(0..1),
                    "green" => Some(1..2),
                    "blue" => Some(2..3),
                    "alpha" => Some(3..4),
                    _ => None,
                },
                Some(_) => None,
            };
            let Some(chosen) = chosen else {
                return Err(Error::new(format!(
                    "image/curves takes :channel :all, :red, :green, :blue or :alpha, not {}",
                    print::brief(heap, options.get("channel").unwrap_or(Value::Nil))
                ))
                .of(ErrorKind::Type));
            };
            let curve = curve_table(&points);
            let mut tables = [table(|value| value); 4];
            for slot in chosen {
                tables[slot] = curve;
            }
            changed(heap, "curves", image, move |pixels| {
                looked_up(pixels, &tables);
            })
        });
}

fn install_reductions(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "threshold")
        .arity(1, Some(2))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "threshold", arguments[0])?;
            let wrong = |value| outside(heap, "threshold", "a level from 0 to 255 or :otsu", value);
            let cut = match arguments.get(1).copied() {
                None => Some(128),
                Some(Value::Keyword(keyword)) if keyword.name() == "otsu" => None,
                Some(value @ (Value::Int(_) | Value::Float(_))) => {
                    let found = number(heap, "threshold", "a level", value)?;
                    if !(0.0..=255.0).contains(&found) {
                        return Err(wrong(value));
                    }
                    Some(byte(narrow(found) / 255.0))
                }
                Some(other) => return Err(wrong(other)),
            };
            changed(heap, "threshold", image, move |pixels| {
                let cut = cut.unwrap_or_else(|| {
                    otsu_level(&GrayImage::from_fn(
                        pixels.width(),
                        pixels.height(),
                        |x, y| Luma([luma(&pixels.get_pixel(x, y).0)]),
                    ))
                });
                each(pixels, |pixel| {
                    let shade = if luma(pixel) > cut { 255 } else { 0 };
                    pixel[..3].fill(shade);
                });
            })
        });
    isolate
        .native(MODULE, "replace")
        .arity(3, Some(5))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "replace", arguments[0])?;
            let from = colors::color(heap, "replace", arguments[1])?;
            let to = colors::color(heap, "replace", arguments[2])?;
            let options = Options::read(heap, "replace", &arguments[3..], &["tolerance"])?;
            let tolerance = options.number(heap, "tolerance", 0.0)?;
            changed(heap, "replace", image, move |pixels| {
                each(pixels, |pixel| {
                    let close = pixel
                        .iter()
                        .zip(from.0)
                        .all(|(have, want)| f64::from(have.abs_diff(want)) <= tolerance);
                    if close {
                        pixel.copy_from_slice(&to.0);
                    }
                });
            })
        });
    isolate
        .native(MODULE, "quantize")
        .arity(2, Some(4))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "quantize", arguments[0])?;
            let palette = palette(heap, arguments[1])?;
            let options = Options::read(heap, "quantize", &arguments[2..], &["dither"])?;
            let dither = options.get("dither").is_none_or(Value::truthy);
            changed(heap, "quantize", image, move |pixels| {
                let palette = match palette {
                    Palette::Count(count) => median_cut(pixels, count),
                    Palette::Colors(colors) => colors,
                };
                if dither {
                    dithered(pixels, &palette);
                } else {
                    each(pixels, |pixel| {
                        let goal = [0, 1, 2].map(|channel| f32::from(pixel[channel]));
                        pixel[..3].copy_from_slice(&nearest(&palette, goal));
                    });
                }
            })
        });
}
