use std::f64::consts::TAU;

use image::{GrayImage, Rgba, RgbaImage, imageops};
use imageproc::edges::canny;
use imageproc::filter::median_filter;
use rayon::prelude::*;

use super::adjust::{byte, changed, factor, luma, narrow, outside, strength};
use super::{MODULE, Options, colors, number, size, wanted};
use ox_lang::Isolate;
use ox_lang::error::{Error, ErrorKind};
use ox_lang::heap::Heap;
use ox_lang::value::{Value, print};

type Pixel = [f32; 4];

const EMBOSS: [[f32; 3]; 3] = [[-2.0, -1.0, 0.0], [-1.0, 1.0, 1.0], [0.0, 1.0, 2.0]];
const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;
const BAND: usize = 32;
const STRIP: usize = 256;

fn to_index(value: f32) -> usize {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "kernel sizes are small and kept above 0"
    )]
    let index = value.max(0.0) as usize;
    index
}

fn to_float(count: usize) -> f32 {
    #[expect(clippy::cast_precision_loss, reason = "kernel sizes are small")]
    let float = count as f32;
    float
}

fn dimensions(pixels: &RgbaImage) -> (usize, usize) {
    let (width, height) = pixels.dimensions();
    (
        usize::try_from(width).unwrap_or(0),
        usize::try_from(height).unwrap_or(0),
    )
}

fn premultiplied(pixels: &RgbaImage) -> Vec<Pixel> {
    pixels
        .as_chunks::<4>()
        .0
        .par_iter()
        .map(|pixel| {
            let alpha = f32::from(pixel[3]);
            let by = alpha / 255.0;
            [
                f32::from(pixel[0]) * by,
                f32::from(pixel[1]) * by,
                f32::from(pixel[2]) * by,
                alpha,
            ]
        })
        .collect()
}

fn unpremultiplied(value: &Pixel, pixel: &mut [u8; 4]) {
    let alpha = value[3];
    if alpha <= 0.0 {
        pixel.fill(0);
        return;
    }
    let by = alpha.recip();
    for (channel, amount) in pixel.iter_mut().zip(value).take(3) {
        *channel = byte(amount * by);
    }
    pixel[3] = byte(alpha / 255.0);
}

fn straight(data: &[Pixel], width: u32, height: u32) -> RgbaImage {
    let mut out = RgbaImage::new(width, height);
    out.as_chunks_mut::<4>()
        .0
        .par_iter_mut()
        .zip(data.par_iter())
        .for_each(|(pixel, value)| unpremultiplied(value, pixel));
    out
}

fn across(data: &mut [Pixel], width: usize, line: impl Fn(&[Pixel], &mut [Pixel]) + Sync + Send) {
    data.par_chunks_mut(width).for_each_init(
        || Vec::with_capacity(width),
        |scratch: &mut Vec<Pixel>, row| {
            scratch.clear();
            scratch.extend_from_slice(row);
            line(scratch, row);
        },
    );
}

fn accumulate(sum: &mut Pixel, value: &Pixel, weight: f32) {
    for (total, part) in sum.iter_mut().zip(value) {
        *total += part * weight;
    }
}
fn add_row(sum: &mut [Pixel], row: &[Pixel], by: f32) {
    for (total, value) in sum.iter_mut().zip(row) {
        for (part, amount) in total.iter_mut().zip(value) {
            *part += amount * by;
        }
    }
}

fn down_weighted(data: &[Pixel], height: usize, kernel: &[f32], out: &mut RgbaImage) {
    let width = data.len() / height.max(1);
    let reach = kernel.len() / 2;
    out.par_chunks_mut(width * 4 * BAND)
        .enumerate()
        .for_each_init(
            || Vec::with_capacity(kernel.len()),
            |sources: &mut Vec<usize>, (band, rows)| {
                let lines = rows.len() / (width * 4);
                for left in (0..width).step_by(STRIP) {
                    let right = (left + STRIP).min(width);
                    for line in 0..lines {
                        let y = band * BAND + line;
                        sources.clear();
                        sources.extend((0..kernel.len()).map(|offset| {
                            (y + offset).saturating_sub(reach).min(height - 1) * width
                        }));
                        let start = line * width * 4;
                        let into = &mut rows[start + left * 4..start + right * 4];
                        for (x, pixel) in (left..right).zip(into.as_chunks_mut::<4>().0) {
                            let mut sum = [0.0; 4];
                            for (source, weight) in sources.iter().zip(kernel) {
                                accumulate(&mut sum, &data[source + x], *weight);
                            }
                            unpremultiplied(&sum, pixel);
                        }
                    }
                }
            },
        );
}

fn down_boxed(data: &[Pixel], width: usize, height: usize, reach: usize) -> Vec<Pixel> {
    let span = to_float(2 * reach + 1);
    let row = |offset: usize| {
        let y = offset.saturating_sub(reach).min(height - 1);
        &data[y * width..(y + 1) * width]
    };
    let mut sum = vec![[0.0_f32; 4]; width];
    for offset in 0..=2 * reach {
        add_row(&mut sum, row(offset), 1.0);
    }
    let mut out = Vec::with_capacity(data.len());
    for y in 0..height {
        out.extend(sum.iter().map(|total| total.map(|part| part / span)));
        add_row(&mut sum, row(y + 2 * reach + 1), 1.0);
        add_row(&mut sum, row(y), -1.0);
    }
    out
}
fn weighted(kernel: &[f32]) -> impl Fn(&[Pixel], &mut [Pixel]) + Sync + Send + '_ {
    let reach = kernel.len() / 2;
    move |from, into| {
        let last = from.len() - 1;
        for (x, slot) in into.iter_mut().enumerate() {
            let mut sum = [0.0; 4];
            if x >= reach && x + reach <= last {
                for (value, weight) in from[x - reach..=x + reach].iter().zip(kernel) {
                    accumulate(&mut sum, value, *weight);
                }
            } else {
                for (offset, weight) in kernel.iter().enumerate() {
                    accumulate(
                        &mut sum,
                        &from[(x + offset).saturating_sub(reach).min(last)],
                        *weight,
                    );
                }
            }
            *slot = sum;
        }
    }
}

fn boxed(reach: usize) -> impl Fn(&[Pixel], &mut [Pixel]) + Sync + Send {
    let span = to_float(2 * reach + 1);
    move |from, into| {
        let last = from.len() - 1;
        let at = |offset: usize| from[offset.saturating_sub(reach).min(last)];
        let mut sum = [0.0_f32; 4];
        for offset in 0..=2 * reach {
            for (total, value) in sum.iter_mut().zip(at(offset)) {
                *total += value;
            }
        }
        for (x, slot) in into.iter_mut().enumerate() {
            *slot = sum.map(|total| total / span);
            let (entering, leaving) = (at(x + 2 * reach + 1), at(x));
            for ((total, added), removed) in sum.iter_mut().zip(entering).zip(leaving) {
                *total += added - removed;
            }
        }
    }
}

fn gaussian_kernel(sigma: f32) -> Vec<f32> {
    let reach = to_index((sigma * 3.0).ceil());
    let middle = to_float(reach);
    let weights: Vec<f32> = (0..=2 * reach)
        .map(|index| {
            let away = to_float(index) - middle;
            (-away * away / (2.0 * sigma * sigma)).exp()
        })
        .collect();
    let total: f32 = weights.iter().sum();
    weights.iter().map(|weight| weight / total).collect()
}

fn box_widths(sigma: f32) -> [usize; 3] {
    let ideal = (4.0 * sigma * sigma + 1.0).sqrt();
    let mut lower = to_index(ideal.floor()).max(1);
    if lower.is_multiple_of(2) {
        lower -= 1;
    }
    let low = to_float(lower);
    let first = to_index(
        ((12.0 * sigma * sigma - 3.0 * low * low - 12.0 * low - 9.0) / (-4.0 * low - 4.0)).round(),
    );
    [0, 1, 2].map(|pass| if pass < first { lower } else { lower + 2 })
}

fn blurred(pixels: &RgbaImage, sigma: f32) -> RgbaImage {
    if sigma < 0.01 {
        return pixels.clone();
    }
    let (width, height) = dimensions(pixels);
    let mut data = premultiplied(pixels);
    if sigma <= 8.0 {
        let kernel = gaussian_kernel(sigma);
        across(&mut data, width, weighted(&kernel));
        let mut out = RgbaImage::new(pixels.width(), pixels.height());
        down_weighted(&data, height, &kernel, &mut out);
        return out;
    }
    for span in box_widths(sigma) {
        across(&mut data, width, boxed(span / 2));
        data = down_boxed(&data, width, height, span / 2);
    }
    straight(&data, pixels.width(), pixels.height())
}

fn box_blurred(pixels: &RgbaImage, reach: usize) -> RgbaImage {
    let (width, height) = dimensions(pixels);
    let mut data = premultiplied(pixels);
    across(&mut data, width, boxed(reach));
    straight(
        &down_boxed(&data, width, height, reach),
        pixels.width(),
        pixels.height(),
    )
}
fn pixelated(pixels: &RgbaImage, block: usize) -> RgbaImage {
    let (width, _) = dimensions(pixels);
    let mut data = premultiplied(pixels);
    data.par_chunks_mut(width * block).for_each(|band| {
        let lines = band.len() / width;
        for left in (0..width).step_by(block) {
            let right = (left + block).min(width);
            let mut sum = [0.0_f32; 4];
            for line in 0..lines {
                for value in &band[line * width + left..line * width + right] {
                    for (total, part) in sum.iter_mut().zip(value) {
                        *total += part;
                    }
                }
            }
            let mean = sum.map(|total| total / to_float((right - left) * lines));
            for line in 0..lines {
                band[line * width + left..line * width + right].fill(mean);
            }
        }
    });
    straight(&data, pixels.width(), pixels.height())
}

fn unsharp(pixels: &RgbaImage, sigma: f32, amount: f32, threshold: f32) -> RgbaImage {
    let soft = blurred(pixels, sigma);
    let mut out = pixels.clone();
    out.as_chunks_mut::<4>()
        .0
        .par_iter_mut()
        .zip(soft.as_chunks::<4>().0.par_iter())
        .for_each(|(pixel, smooth)| {
            for (channel, near) in pixel.iter_mut().zip(smooth).take(3) {
                let sharp = f32::from(*channel);
                let difference = sharp - f32::from(*near);
                if difference.abs() >= threshold {
                    *channel = byte((sharp + amount * difference) / 255.0);
                }
            }
        });
    out
}

fn convolved(pixels: &RgbaImage, kernel: &[Vec<f32>], divisor: f32, offset: f32) -> RgbaImage {
    let (width, height) = dimensions(pixels);
    let (reach_x, reach_y) = (kernel[0].len() / 2, kernel.len() / 2);
    let source = pixels.as_raw();
    let mut out = pixels.clone();
    out.par_chunks_mut(width * 4)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, pixel) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let mut sum = [0.0_f32; 3];
                for (down, weights) in kernel.iter().enumerate() {
                    let from_y = (y + down).saturating_sub(reach_y).min(height - 1);
                    for (across, weight) in weights.iter().enumerate() {
                        let from_x = (x + across).saturating_sub(reach_x).min(width - 1);
                        let at = (from_y * width + from_x) * 4;
                        for (total, value) in sum.iter_mut().zip(&source[at..at + 3]) {
                            *total += f32::from(*value) * weight;
                        }
                    }
                }
                for (channel, total) in pixel.iter_mut().zip(sum) {
                    *channel = byte((total / divisor + offset) / 255.0);
                }
            }
        });
    out
}

fn mixed_toward(base: &RgbaImage, target: &RgbaImage, amount: f32) -> RgbaImage {
    let mut out = base.clone();
    out.as_chunks_mut::<4>()
        .0
        .par_iter_mut()
        .zip(target.as_chunks::<4>().0.par_iter())
        .for_each(|(pixel, goal)| {
            for (channel, wanted) in pixel.iter_mut().zip(goal).take(3) {
                let from = f32::from(*channel);
                *channel = byte((from + (f32::from(*wanted) - from) * amount) / 255.0);
            }
        });
    out
}

fn shades(pixels: &RgbaImage) -> Vec<u8> {
    pixels
        .as_chunks::<4>()
        .0
        .par_iter()
        .map(|pixel| luma(pixel))
        .collect()
}

fn opaque(map: &GrayImage) -> RgbaImage {
    let mut out = RgbaImage::new(map.width(), map.height());
    out.as_chunks_mut::<4>()
        .0
        .par_iter_mut()
        .zip(map.as_raw().par_iter())
        .for_each(|(pixel, shade)| *pixel = [*shade, *shade, *shade, 255]);
    out
}

fn sobel(pixels: &RgbaImage) -> RgbaImage {
    let (width, height) = dimensions(pixels);
    let shades: Vec<f32> = shades(pixels).into_iter().map(f32::from).collect();
    let line = |y: usize| &shades[y * width..(y + 1) * width];
    let mut out = RgbaImage::new(pixels.width(), pixels.height());
    out.par_chunks_mut(width * 4)
        .enumerate()
        .for_each(|(y, row)| {
            let (up, middle, down) = (
                line(y.saturating_sub(1)),
                line(y),
                line((y + 1).min(height - 1)),
            );
            for (x, pixel) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let (left, right) = (x.saturating_sub(1), (x + 1).min(width - 1));
                let across = (up[right] + 2.0 * middle[right] + down[right])
                    - (up[left] + 2.0 * middle[left] + down[left]);
                let downward = (down[left] + 2.0 * down[x] + down[right])
                    - (up[left] + 2.0 * up[x] + up[right]);
                let shade = byte(across.hypot(downward) / 4.0 / 255.0);
                *pixel = [shade, shade, shade, 255];
            }
        });
    out
}

fn canny_lines(pixels: &RgbaImage, low: f32, high: f32) -> RgbaImage {
    let gray = GrayImage::from_raw(pixels.width(), pixels.height(), shades(pixels))
        .unwrap_or_else(|| GrayImage::new(pixels.width(), pixels.height()));
    opaque(&canny(&gray, low, high))
}

fn median(pixels: &RgbaImage, reach: u32) -> RgbaImage {
    let (width, height) = pixels.dimensions();
    let threads = u32::try_from(rayon::current_num_threads())
        .unwrap_or(1)
        .max(1);
    let band = height.div_ceil(threads * 2).max(1);
    let pieces: Vec<(u32, RgbaImage)> = (0..height.div_ceil(band))
        .into_par_iter()
        .map(|index| {
            let top = index * band;
            let bottom = (top + band).min(height);
            let from = top.saturating_sub(reach);
            let to = (bottom + reach).min(height);
            let piece = imageops::crop_imm(pixels, 0, from, width, to - from).to_image();
            let filtered = median_filter(&piece, reach, reach);
            (
                top,
                imageops::crop_imm(&filtered, 0, top - from, width, bottom - top).to_image(),
            )
        })
        .collect();
    let mut out = RgbaImage::new(width, height);
    for (top, piece) in pieces {
        imageops::replace(&mut out, &piece, 0, i64::from(top));
    }
    out
}

fn mix(mut state: u64) -> u64 {
    state = (state ^ (state >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    state = (state ^ (state >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    state ^ (state >> 31)
}

struct Random {
    state: u64,
    spare: Option<f32>,
}

impl Random {
    fn uniform(&mut self) -> f64 {
        self.state = self.state.wrapping_add(GOLDEN);
        let bits = u32::try_from(mix(self.state) >> 40).unwrap_or(0);
        (f64::from(bits) + 1.0) / 16_777_216.0
    }

    fn normal(&mut self) -> f32 {
        if let Some(spare) = self.spare.take() {
            return spare;
        }
        let (first, second) = (self.uniform(), self.uniform());
        let (length, angle) = ((-2.0 * first.ln()).sqrt(), TAU * second);
        self.spare = Some(narrow(length * angle.sin()));
        narrow(length * angle.cos())
    }
}

fn noisy(pixels: &mut RgbaImage, amount: f32, seed: u64, mono: bool) {
    let (width, _) = dimensions(pixels);
    pixels
        .par_chunks_mut(width * 4)
        .enumerate()
        .for_each(|(y, row)| {
            let line = u64::try_from(y).unwrap_or(0);
            let mut random = Random {
                state: mix(seed ^ line.wrapping_mul(GOLDEN)),
                spare: None,
            };
            for pixel in row.as_chunks_mut::<4>().0 {
                let shared = if mono { random.normal() * amount } else { 0.0 };
                for channel in pixel.iter_mut().take(3) {
                    let grain = if mono {
                        shared
                    } else {
                        random.normal() * amount
                    };
                    *channel = byte((f32::from(*channel) + grain) / 255.0);
                }
            }
        });
}

fn vignetted(pixels: &mut RgbaImage, amount: f32, radius: f32, color: Rgba<u8>) {
    let (columns, _) = dimensions(pixels);
    let (width, height) = (
        narrow(f64::from(pixels.width())),
        narrow(f64::from(pixels.height())),
    );
    pixels
        .par_chunks_mut(columns * 4)
        .enumerate()
        .for_each(|(y, row)| {
            let down = (to_float(y) + 0.5) / height - 0.5;
            for (x, pixel) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let across = (to_float(x) + 0.5) / width - 0.5;
                let away = across.hypot(down) / 0.5_f32.sqrt();
                let along = ((away - radius) / (1.0 - radius).max(f32::EPSILON)).clamp(0.0, 1.0);
                let weight = along * along * (3.0 - 2.0 * along) * amount;
                for (channel, toward) in pixel.iter_mut().zip(color.0).take(3) {
                    let from = f32::from(*channel);
                    *channel = byte((from + (f32::from(toward) - from) * weight) / 255.0);
                }
            }
        });
}
fn radius(heap: &Heap, name: &str, value: Value) -> Result<f32, Error> {
    let found = number(heap, name, "a radius", value)?;
    if found < 0.0 {
        return Err(outside(heap, name, "a radius of 0 or more", value));
    }
    Ok(narrow(found))
}

fn kernel(heap: &Heap, value: Value) -> Result<Vec<Vec<f32>>, Error> {
    let wrong = || {
        outside(
            heap,
            "convolve",
            "rows of numbers with an odd width and height",
            value,
        )
    };
    let rows = heap.elements(value).ok_or_else(wrong)?;
    let kernel = rows
        .iter()
        .map(|row| {
            heap.elements(*row)?
                .iter()
                .map(|item| number(heap, "convolve", "a weight", *item).ok().map(narrow))
                .collect::<Option<Vec<f32>>>()
        })
        .collect::<Option<Vec<Vec<f32>>>>()
        .ok_or_else(wrong)?;
    let width = kernel.first().map_or(0, Vec::len);
    if kernel.len().is_multiple_of(2)
        || width.is_multiple_of(2)
        || kernel.iter().any(|row| row.len() != width)
    {
        return Err(wrong());
    }
    Ok(kernel)
}

pub(super) fn install(isolate: &mut Isolate) {
    install_blurs(isolate);
    install_sharpening(isolate);
    install_effects(isolate);
}

fn install_blurs(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "blur")
        .arity(2, Some(2))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "blur", arguments[0])?;
            let sigma = radius(heap, "blur", arguments[1])?;
            changed(heap, "blur", image, move |pixels| {
                *pixels = blurred(pixels, sigma);
            })
        });
    isolate
        .native(MODULE, "box-blur")
        .arity(2, Some(2))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "box-blur", arguments[0])?;
            let reach = to_index(radius(heap, "box-blur", arguments[1])?.round());
            changed(heap, "box-blur", image, move |pixels| {
                *pixels = box_blurred(pixels, reach);
            })
        });
    isolate
        .native(MODULE, "median")
        .arity(2, Some(2))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "median", arguments[0])?;
            let reach = to_index(radius(heap, "median", arguments[1])?.round());
            let reach = u32::try_from(reach).unwrap_or(u32::MAX);
            changed(heap, "median", image, move |pixels| {
                *pixels = median(pixels, reach);
            })
        });
    isolate
        .native(MODULE, "pixelate")
        .arity(2, Some(2))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "pixelate", arguments[0])?;
            let block = size(heap, "pixelate", "a block size", arguments[1])?;
            let block = usize::try_from(block).unwrap_or(usize::MAX);
            changed(heap, "pixelate", image, move |pixels| {
                *pixels = pixelated(pixels, block);
            })
        });
}

fn install_sharpening(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "sharpen")
        .arity(1, Some(2))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "sharpen", arguments[0])?;
            let amount = match arguments.get(1) {
                Some(value) => factor(heap, "sharpen", *value)?,
                None => 1.0,
            };
            changed(heap, "sharpen", image, move |pixels| {
                *pixels = unsharp(pixels, 1.0, amount, 0.0);
            })
        });
    isolate
        .native(MODULE, "unsharp")
        .arity(3, Some(3))
        .keywords(&["threshold"])
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "unsharp", arguments[0])?;
            let sigma = radius(heap, "unsharp", arguments[1])?;
            let amount = factor(heap, "unsharp", arguments[2])?;
            let options = Options::slots("unsharp", &["threshold"], &arguments[3..]);
            let threshold = narrow(options.number(heap, "threshold", 0.0)?);
            changed(heap, "unsharp", image, move |pixels| {
                *pixels = unsharp(pixels, sigma, amount, threshold);
            })
        });
    isolate
        .native(MODULE, "edges")
        .arity(1, Some(1))
        .keywords(&["method", "low", "high"])
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "edges", arguments[0])?;
            let options = Options::slots("edges", &["method", "low", "high"], &arguments[1..]);
            let thin = match options.get("method") {
                None => false,
                Some(Value::Keyword(keyword)) if keyword.name() == "sobel" => false,
                Some(Value::Keyword(keyword)) if keyword.name() == "canny" => true,
                Some(other) => {
                    return Err(Error::new(format!(
                        "image/edges takes :method :sobel or :canny, not {}",
                        print::brief(heap, other)
                    ))
                    .of(ErrorKind::Type));
                }
            };
            let low = narrow(options.number(heap, "low", 20.0)?) * 4.0;
            let high = narrow(options.number(heap, "high", 50.0)?) * 4.0;
            changed(heap, "edges", image, move |pixels| {
                *pixels = if thin {
                    canny_lines(pixels, low, high)
                } else {
                    sobel(pixels)
                };
            })
        });
    isolate
        .native(MODULE, "emboss")
        .arity(1, Some(2))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "emboss", arguments[0])?;
            let amount = strength(heap, "emboss", arguments.get(1))?;
            let kernel: Vec<Vec<f32>> = EMBOSS.iter().map(|row| row.to_vec()).collect();
            changed(heap, "emboss", image, move |pixels| {
                let embossed = convolved(pixels, &kernel, 1.0, 0.0);
                *pixels = mixed_toward(pixels, &embossed, amount);
            })
        });
    isolate
        .native(MODULE, "convolve")
        .arity(2, Some(2))
        .keywords(&["divisor", "offset"])
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "convolve", arguments[0])?;
            let kernel = kernel(heap, arguments[1])?;
            let options = Options::slots("convolve", &["divisor", "offset"], &arguments[2..]);
            let sum: f32 = kernel.iter().flatten().sum();
            let fallback = if sum.abs() < f32::EPSILON { 1.0 } else { sum };
            let divisor = match options.get("divisor") {
                Some(value) => narrow(number(heap, "convolve", ":divisor", value)?),
                None => fallback,
            };
            if divisor.abs() < f32::EPSILON {
                return Err(Error::new("image/convolve can't divide by a :divisor of 0")
                    .of(ErrorKind::Range));
            }
            let offset = narrow(options.number(heap, "offset", 0.0)?);
            changed(heap, "convolve", image, move |pixels| {
                *pixels = convolved(pixels, &kernel, divisor, offset);
            })
        });
}

fn install_effects(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "noise")
        .arity(2, Some(2))
        .keywords(&["seed", "mono"])
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "noise", arguments[0])?;
            let amount = radius(heap, "noise", arguments[1])?;
            let options = Options::slots("noise", &["seed", "mono"], &arguments[2..]);
            let seed = match options.get("seed") {
                None => 0,
                Some(Value::Int(seed)) => u64::from_ne_bytes(seed.to_ne_bytes()),
                Some(other) => {
                    return Err(Error::new(format!(
                        "image/noise expects a whole number :seed, got {}",
                        print::brief(heap, other)
                    ))
                    .of(ErrorKind::Type));
                }
            };
            let mono = options.get("mono").is_some_and(Value::truthy);
            changed(heap, "noise", image, move |pixels| {
                noisy(pixels, amount, seed, mono);
            })
        });
    isolate
        .native(MODULE, "vignette")
        .arity(1, Some(2))
        .keywords(&["radius", "color"])
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "vignette", arguments[0])?;
            let given = Some(&arguments[1]).filter(|value| **value != Value::Nil);
            let amount = strength(heap, "vignette", given)?;
            let options = Options::slots("vignette", &["radius", "color"], &arguments[2..]);
            let radius = narrow(options.number(heap, "radius", 0.5)?.clamp(0.0, 1.0));
            let color = match options.get("color") {
                Some(value) => colors::color(heap, "vignette", value)?,
                None => Rgba([0, 0, 0, 255]),
            };
            changed(heap, "vignette", image, move |pixels| {
                vignetted(pixels, amount, radius, color);
            })
        });
}
