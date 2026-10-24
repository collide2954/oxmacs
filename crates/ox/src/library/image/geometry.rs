use image::imageops::{self, FilterType};
use image::{Rgba, RgbaImage};
use imageproc::geometric_transformations::{Border, Interpolation, Projection, warp_into};

use super::{MODULE, Options, colors, later, number, room, roomy, size, wanted};
use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::{Value, print};

const CLEAR: Rgba<u8> = Rgba([0, 0, 0, 0]);

fn filter(heap: &Heap, name: &str, options: &Options<'_>) -> Result<FilterType, Error> {
    let Some(value) = options.get("filter") else {
        return Ok(FilterType::Lanczos3);
    };
    match value {
        Value::Keyword(keyword) => match keyword.name() {
            "nearest" => Some(FilterType::Nearest),
            "linear" => Some(FilterType::Triangle),
            "cubic" => Some(FilterType::CatmullRom),
            "gaussian" => Some(FilterType::Gaussian),
            "lanczos" => Some(FilterType::Lanczos3),
            _ => None,
        },
        _ => None,
    }
    .ok_or_else(|| {
        Error::new(format!(
            "image/{name} takes :filter :nearest, :linear, :cubic, :gaussian or :lanczos, not {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })
}

fn fill_color(heap: &Heap, name: &str, options: &Options<'_>) -> Result<Rgba<u8>, Error> {
    options
        .get("color")
        .map_or(Ok(CLEAR), |value| colors::color(heap, name, value))
}

fn scaled(length: u32, by: f64) -> u32 {
    let scaled = (f64::from(length) * by).round().max(1.0);
    if scaled >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "checked to fit in a u32"
        )]
        let fitted = scaled as u32;
        fitted
    }
}

fn whole(heap: &Heap, name: &str, what: &str, value: Value) -> Result<i64, Error> {
    match value {
        Value::Int(number) => Ok(number),
        _ => Err(Error::new(format!(
            "image/{name} expects {what} as a whole number, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)),
    }
}

fn to_u32(number: i64) -> u32 {
    u32::try_from(number.max(0)).unwrap_or(u32::MAX)
}

fn crop(pixels: &RgbaImage, x: i64, y: i64, width: u32, height: u32) -> Result<RgbaImage, String> {
    let (left, top) = (to_u32(x), to_u32(y));
    if x < 0 || y < 0 || left >= pixels.width() || top >= pixels.height() {
        return Err(format!(
            "image/crop starts at {x},{y}, outside a {}×{} image",
            pixels.width(),
            pixels.height()
        ));
    }
    let width = width.min(pixels.width() - left);
    let height = height.min(pixels.height() - top);
    Ok(imageops::crop_imm(pixels, left, top, width, height).to_image())
}

fn anchor(heap: &Heap, options: &Options<'_>) -> Result<(f64, f64), Error> {
    let Some(value) = options.get("anchor") else {
        return Ok((0.5, 0.5));
    };
    let found = match value {
        Value::Keyword(keyword) => match keyword.name() {
            "center" => Some((0.5, 0.5)),
            "top-left" => Some((0.0, 0.0)),
            "top" => Some((0.5, 0.0)),
            "top-right" => Some((1.0, 0.0)),
            "left" => Some((0.0, 0.5)),
            "right" => Some((1.0, 0.5)),
            "bottom-left" => Some((0.0, 1.0)),
            "bottom" => Some((0.5, 1.0)),
            "bottom-right" => Some((1.0, 1.0)),
            _ => None,
        },
        _ => None,
    };
    found.ok_or_else(|| {
        Error::new(format!(
            "image/canvas takes an :anchor such as :center, :top-left or :bottom, not {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })
}

fn placed(
    pixels: &RgbaImage,
    width: u32,
    height: u32,
    at: (i64, i64),
    color: Rgba<u8>,
) -> RgbaImage {
    let mut out = RgbaImage::from_pixel(width, height, color);
    imageops::replace(&mut out, pixels, at.0, at.1);
    out
}

fn offset(room: u32, used: u32, fraction: f64) -> i64 {
    let left_over = f64::from(room) - f64::from(used);
    #[expect(clippy::cast_possible_truncation, reason = "image sizes are small")]
    let moved = (left_over * fraction).round() as i64;
    moved
}

fn rotate(
    pixels: &RgbaImage,
    degrees: f64,
    color: Rgba<u8>,
    expand: bool,
) -> Result<RgbaImage, String> {
    let turns = degrees.rem_euclid(360.0);
    if turns.abs() < f64::EPSILON {
        return Ok(pixels.clone());
    }
    for (exact, turned) in [
        (90.0, imageops::rotate90 as fn(&RgbaImage) -> RgbaImage),
        (180.0, imageops::rotate180),
        (270.0, imageops::rotate270),
    ] {
        if (turns - exact).abs() < 1e-9 {
            return Ok(turned(pixels));
        }
    }
    let radians = turns.to_radians();
    let (width, height) = (f64::from(pixels.width()), f64::from(pixels.height()));
    let (sin, cos) = radians.sin_cos();
    let (out_width, out_height) = if expand {
        (
            scaled(1, (width * cos).abs() + (height * sin).abs()),
            scaled(1, (width * sin).abs() + (height * cos).abs()),
        )
    } else {
        (pixels.width(), pixels.height())
    };
    room("rotate", out_width, out_height)?;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "image sizes fit in f32"
    )]
    let projection = Projection::translate(out_width as f32 / 2.0, out_height as f32 / 2.0)
        * Projection::rotate(radians as f32)
        * Projection::translate(-(width as f32) / 2.0, -(height as f32) / 2.0);
    let mut out = RgbaImage::from_pixel(out_width, out_height, color);
    warp_into(
        pixels,
        projection,
        Interpolation::Bilinear,
        Border::Constant(color),
        &mut out,
    );
    Ok(out)
}

fn trim(pixels: &RgbaImage, tolerance: f64) -> RgbaImage {
    let corner = *pixels.get_pixel(0, 0);
    let close = |pixel: &Rgba<u8>| {
        pixel
            .0
            .iter()
            .zip(corner.0)
            .all(|(a, b)| f64::from(a.abs_diff(b)) <= tolerance)
    };
    let (width, height) = pixels.dimensions();
    let row_clear = |y: u32| (0..width).all(|x| close(pixels.get_pixel(x, y)));
    let column_clear =
        |x: u32, top: u32, bottom: u32| (top..bottom).all(|y| close(pixels.get_pixel(x, y)));
    let top = (0..height).find(|y| !row_clear(*y)).unwrap_or(0);
    let bottom = (0..height)
        .rev()
        .find(|y| !row_clear(*y))
        .map_or(height, |y| y + 1);
    if bottom <= top {
        return pixels.clone();
    }
    let left = (0..width)
        .find(|x| !column_clear(*x, top, bottom))
        .unwrap_or(0);
    let right = (0..width)
        .rev()
        .find(|x| !column_clear(*x, top, bottom))
        .map_or(width, |x| x + 1);
    imageops::crop_imm(pixels, left, top, right - left, bottom - top).to_image()
}

fn points(heap: &Heap, name: &str, value: Value) -> Result<[(f32, f32); 4], Error> {
    let wrong = || {
        Error::new(format!(
            "image/{name} expects four [x y] points, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    };
    let items = heap.elements(value).ok_or_else(wrong)?;
    let pairs: Vec<(f32, f32)> = items
        .iter()
        .map(|item| {
            let both = heap.elements(*item)?;
            let [x, y] = both.as_slice() else {
                return None;
            };
            let x = number(heap, name, "a point", *x).ok()?;
            let y = number(heap, name, "a point", *y).ok()?;
            #[expect(clippy::cast_possible_truncation, reason = "coordinates fit in f32")]
            let point = (x as f32, y as f32);
            Some(point)
        })
        .collect::<Option<_>>()
        .ok_or_else(wrong)?;
    pairs.try_into().map_err(|_| wrong())
}

fn warped(pixels: &RgbaImage, projection: Projection, color: Rgba<u8>) -> RgbaImage {
    let mut out = RgbaImage::from_pixel(pixels.width(), pixels.height(), color);
    warp_into(
        pixels,
        projection,
        Interpolation::Bilinear,
        Border::Constant(color),
        &mut out,
    );
    out
}

pub(super) fn install(isolate: &mut Isolate) {
    install_scaling(isolate);
    install_layout(isolate);
    install_warps(isolate);
}

fn install_scaling(isolate: &mut Isolate) {
    isolate.define_native_awaiting_in(MODULE, "resize", 2, Some(6), |heap, arguments| {
        let image = wanted(heap, "resize", arguments[0])?;
        let width = size(heap, "resize", "a width", arguments[1])?;
        let options = Options::read(heap, "resize", &arguments[2..], &["height", "filter"])?;
        let filter = filter(heap, "resize", &options)?;
        let height = match options.get("height") {
            Some(value) => size(heap, "resize", ":height", value)?,
            None => scaled(
                image.pixels.height(),
                f64::from(width) / f64::from(image.pixels.width()),
            ),
        };
        roomy("resize", width, height)?;
        later(heap, "resize", move || {
            Ok(image.with(imageops::resize(&image.pixels, width, height, filter)))
        })
    });
    for (name, cover) in [("fit", false), ("fill", true)] {
        isolate.define_native_awaiting_in(MODULE, name, 3, Some(5), move |heap, arguments| {
            let image = wanted(heap, name, arguments[0])?;
            let width = size(heap, name, "a width", arguments[1])?;
            let height = size(heap, name, "a height", arguments[2])?;
            let options = Options::read(
                heap,
                if cover { "fill" } else { "fit" },
                &arguments[3..],
                &["filter"],
            )?;
            let filter = filter(heap, name, &options)?;
            later(heap, name, move || {
                let (from_width, from_height) = image.pixels.dimensions();
                let across = f64::from(width) / f64::from(from_width);
                let down = f64::from(height) / f64::from(from_height);
                let by = if cover {
                    across.max(down)
                } else {
                    across.min(down)
                };
                let (to_width, to_height) = (scaled(from_width, by), scaled(from_height, by));
                room(name, to_width, to_height)?;
                let resized = imageops::resize(&image.pixels, to_width, to_height, filter);
                if !cover {
                    return Ok(image.with(resized));
                }
                let left = to_u32(offset(resized.width(), width, 0.5));
                let top = to_u32(offset(resized.height(), height, 0.5));
                Ok(image.with(imageops::crop_imm(&resized, left, top, width, height).to_image()))
            })
        });
    }
    isolate.define_native_awaiting_in(MODULE, "thumbnail", 2, Some(2), |heap, arguments| {
        let image = wanted(heap, "thumbnail", arguments[0])?;
        let most = size(heap, "thumbnail", "a size", arguments[1])?;
        later(heap, "thumbnail", move || {
            let (width, height) = image.pixels.dimensions();
            let by = (f64::from(most) / f64::from(width.max(height))).min(1.0);
            Ok(image.with(imageops::thumbnail(
                &image.pixels,
                scaled(width, by),
                scaled(height, by),
            )))
        })
    });
}

fn install_layout(isolate: &mut Isolate) {
    isolate.define_native_awaiting_in(MODULE, "crop", 5, Some(5), |heap, arguments| {
        let image = wanted(heap, "crop", arguments[0])?;
        let x = whole(heap, "crop", "x", arguments[1])?;
        let y = whole(heap, "crop", "y", arguments[2])?;
        let width = size(heap, "crop", "a width", arguments[3])?;
        let height = size(heap, "crop", "a height", arguments[4])?;
        later(heap, "crop", move || {
            Ok(image.with(crop(&image.pixels, x, y, width, height)?))
        })
    });
    isolate.define_native_awaiting_in(MODULE, "pad", 2, Some(12), |heap, arguments| {
        let image = wanted(heap, "pad", arguments[0])?;
        let amount = whole(heap, "pad", "an amount", arguments[1])?;
        let options = Options::read(
            heap,
            "pad",
            &arguments[2..],
            &["top", "right", "bottom", "left", "color"],
        )?;
        let side = |key: &str| -> Result<i64, Error> {
            options.get(key).map_or(Ok(amount), |value| {
                whole(heap, "pad", &format!(":{key}"), value)
            })
        };
        let (top, right, bottom, left) =
            (side("top")?, side("right")?, side("bottom")?, side("left")?);
        let color = fill_color(heap, "pad", &options)?;
        later(heap, "pad", move || {
            let width = i64::from(image.pixels.width()) + left + right;
            let height = i64::from(image.pixels.height()) + top + bottom;
            if width < 1 || height < 1 {
                return Err("image/pad would leave no pixels".to_owned());
            }
            room("pad", to_u32(width), to_u32(height))?;
            Ok(image.with(placed(
                &image.pixels,
                to_u32(width),
                to_u32(height),
                (left, top),
                color,
            )))
        })
    });
    isolate.define_native_awaiting_in(MODULE, "canvas", 3, Some(7), |heap, arguments| {
        let image = wanted(heap, "canvas", arguments[0])?;
        let width = size(heap, "canvas", "a width", arguments[1])?;
        let height = size(heap, "canvas", "a height", arguments[2])?;
        let options = Options::read(heap, "canvas", &arguments[3..], &["anchor", "color"])?;
        let (across, down) = anchor(heap, &options)?;
        let color = fill_color(heap, "canvas", &options)?;
        roomy("canvas", width, height)?;
        later(heap, "canvas", move || {
            let at = (
                offset(width, image.pixels.width(), across),
                offset(height, image.pixels.height(), down),
            );
            Ok(image.with(placed(&image.pixels, width, height, at, color)))
        })
    });
    isolate.define_native_awaiting_in(MODULE, "rotate", 2, Some(6), |heap, arguments| {
        let image = wanted(heap, "rotate", arguments[0])?;
        let degrees = number(heap, "rotate", "an angle in degrees", arguments[1])?;
        let options = Options::read(heap, "rotate", &arguments[2..], &["color", "expand"])?;
        let color = fill_color(heap, "rotate", &options)?;
        let expand = options.get("expand").is_none_or(Value::truthy);
        later(heap, "rotate", move || {
            Ok(image.with(rotate(&image.pixels, degrees, color, expand)?))
        })
    });
    isolate.define_native_awaiting_in(MODULE, "flip", 2, Some(2), |heap, arguments| {
        let image = wanted(heap, "flip", arguments[0])?;
        let horizontal = match arguments[1] {
            Value::Keyword(keyword) if keyword.name() == "horizontal" => true,
            Value::Keyword(keyword) if keyword.name() == "vertical" => false,
            other => {
                return Err(Error::new(format!(
                    "image/flip takes :horizontal or :vertical, not {}",
                    print::brief(heap, other)
                ))
                .of(ErrorKind::Type));
            }
        };
        later(heap, "flip", move || {
            Ok(image.with(if horizontal {
                imageops::flip_horizontal(&image.pixels)
            } else {
                imageops::flip_vertical(&image.pixels)
            }))
        })
    });
    isolate.define_native_awaiting_in(MODULE, "trim", 1, Some(3), |heap, arguments| {
        let image = wanted(heap, "trim", arguments[0])?;
        let options = Options::read(heap, "trim", &arguments[1..], &["tolerance"])?;
        let tolerance = options.number(heap, "tolerance", 0.0)?;
        later(heap, "trim", move || {
            Ok(image.with(trim(&image.pixels, tolerance)))
        })
    });
}

fn install_warps(isolate: &mut Isolate) {
    isolate.define_native_awaiting_in(MODULE, "affine", 2, Some(4), |heap, arguments| {
        let image = wanted(heap, "affine", arguments[0])?;
        let wrong = || {
            Error::new(format!(
                "image/affine expects a matrix [a b c d e f], got {}",
                print::brief(heap, arguments[1])
            ))
            .of(ErrorKind::Type)
        };
        let items = heap.elements(arguments[1]).ok_or_else(wrong)?;
        let numbers: Vec<f32> = items
            .iter()
            .map(|item| number(heap, "affine", "a matrix entry", *item))
            .collect::<Result<Vec<f64>, Error>>()?
            .into_iter()
            .map(|entry| {
                #[expect(clippy::cast_possible_truncation, reason = "matrix entries fit in f32")]
                let narrowed = entry as f32;
                narrowed
            })
            .collect();
        let matrix: [f32; 6] = numbers.as_slice().try_into().map_err(|_| wrong())?;
        let [xx, xy, x0, yx, yy, y0] = matrix;
        let projection = Projection::from_matrix([xx, xy, x0, yx, yy, y0, 0.0, 0.0, 1.0])
            .ok_or_else(|| Error::new("image/affine's matrix can't be inverted"))?;
        let options = Options::read(heap, "affine", &arguments[2..], &["color"])?;
        let color = fill_color(heap, "affine", &options)?;
        later(heap, "affine", move || {
            Ok(image.with(warped(&image.pixels, projection, color)))
        })
    });
    isolate.define_native_awaiting_in(MODULE, "perspective", 3, Some(5), |heap, arguments| {
        let image = wanted(heap, "perspective", arguments[0])?;
        let from = points(heap, "perspective", arguments[1])?;
        let to = points(heap, "perspective", arguments[2])?;
        let projection = Projection::from_control_points(from, to)
            .ok_or_else(|| Error::new("image/perspective can't map those points"))?;
        let options = Options::read(heap, "perspective", &arguments[3..], &["color"])?;
        let color = fill_color(heap, "perspective", &options)?;
        later(heap, "perspective", move || {
            Ok(image.with(warped(&image.pixels, projection, color)))
        })
    });
}
