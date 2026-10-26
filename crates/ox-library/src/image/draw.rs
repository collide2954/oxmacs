use std::sync::{Arc, Mutex};

use image::{Rgba, RgbaImage, imageops};
use rayon::prelude::*;
use tiny_skia::{
    BlendMode, ColorU8, FillRule, FilterQuality, IntSize, LineCap, LineJoin, Mask, MaskType, Paint,
    Path, PathBuilder, Pixmap, PixmapPaint, Rect, Stroke, StrokeDash, Transform,
};

use super::adjust::{byte, factor, luma, narrow, outside, strength, unit};
use super::{Image, MODULE, Options, colors, image_of, later, number, roomy, size, stored, wanted};
use ox_lang::Isolate;
use ox_lang::error::{Error, ErrorKind};
use ox_lang::heap::Heap;
use ox_lang::value::{Value, print};

const CANVAS: &str = "canvas";
const SHAPE: [&str; 8] = [
    "fill", "stroke", "width", "opacity", "blend", "cap", "join", "dash",
];
const RECT: [&str; 9] = [
    "fill", "stroke", "width", "opacity", "blend", "cap", "join", "dash", "radius",
];
const LINE: [&str; 7] = ["color", "width", "opacity", "blend", "cap", "join", "dash"];
const BLENDS: [(&str, BlendMode); 16] = [
    ("normal", BlendMode::SourceOver),
    ("multiply", BlendMode::Multiply),
    ("screen", BlendMode::Screen),
    ("overlay", BlendMode::Overlay),
    ("darken", BlendMode::Darken),
    ("lighten", BlendMode::Lighten),
    ("color-dodge", BlendMode::ColorDodge),
    ("color-burn", BlendMode::ColorBurn),
    ("hard-light", BlendMode::HardLight),
    ("soft-light", BlendMode::SoftLight),
    ("difference", BlendMode::Difference),
    ("exclusion", BlendMode::Exclusion),
    ("hue", BlendMode::Hue),
    ("saturation", BlendMode::Saturation),
    ("color", BlendMode::Color),
    ("luminosity", BlendMode::Luminosity),
];

#[derive(Debug)]
pub struct Canvas {
    painting: Mutex<Option<(Image, Pixmap)>>,
}

pub(super) fn to_pixmap(pixels: &RgbaImage) -> Pixmap {
    let mut data = pixels.as_raw().clone();
    data.as_chunks_mut::<4>()
        .0
        .par_iter_mut()
        .for_each(|pixel| {
            let alpha = u16::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel =
                    u8::try_from((u16::from(*channel) * alpha + 127) / 255).unwrap_or(u8::MAX);
            }
        });
    IntSize::from_wh(pixels.width(), pixels.height())
        .and_then(|size| Pixmap::from_vec(data, size))
        .expect("images have at least one pixel")
}

pub(super) fn from_pixmap(pixmap: Pixmap) -> RgbaImage {
    let (width, height) = (pixmap.width(), pixmap.height());
    let mut data = pixmap.take();
    data.as_chunks_mut::<4>()
        .0
        .par_iter_mut()
        .for_each(|pixel| {
            let alpha = u16::from(pixel[3]);
            if alpha == 0 {
                return;
            }
            for channel in &mut pixel[..3] {
                *channel = u8::try_from((u16::from(*channel) * 255 + alpha / 2) / alpha)
                    .unwrap_or(u8::MAX);
            }
        });
    RgbaImage::from_raw(width, height, data).unwrap_or_else(|| RgbaImage::new(width, height))
}

fn drawn(
    heap: &mut Heap,
    name: &'static str,
    target: Value,
    draw: impl FnOnce(&mut Pixmap) -> Result<(), String> + Send + 'static,
) -> Result<Value, Error> {
    if let Some(canvas) = heap.shared_of::<Canvas>(target) {
        let mut painting = canvas
            .painting
            .lock()
            .map_err(|_| Error::new(format!("image/{name} found its canvas broken")))?;
        let Some((_, pixmap)) = painting.as_mut() else {
            return Err(Error::new(format!(
                "image/{name} was given a canvas whose edit has ended"
            )));
        };
        draw(pixmap).map_err(Error::new)?;
        return Ok(target);
    }
    let Some(image) = image_of(heap, target) else {
        return Err(Error::new(format!(
            "image/{name} expects an image or a canvas, got {}",
            print::brief(heap, target)
        ))
        .of(ErrorKind::Type));
    };
    later(heap, name, move || {
        let mut pixmap = to_pixmap(&image.pixels);
        draw(&mut pixmap)?;
        Ok(image.with(from_pixmap(pixmap)))
    })
}

fn blend(heap: &Heap, name: &str, options: &Options<'_>) -> Result<BlendMode, Error> {
    let Some(value) = options.get("blend") else {
        return Ok(BlendMode::SourceOver);
    };
    let found = match value {
        Value::Keyword(keyword) => BLENDS
            .iter()
            .find(|(known, _)| *known == keyword.name())
            .map(|(_, mode)| *mode),
        _ => None,
    };
    found.ok_or_else(|| {
        Error::new(format!(
            "image/{name} takes a :blend mode such as :multiply, :screen or :overlay, not {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })
}

fn opacity(heap: &Heap, name: &str, options: &Options<'_>) -> Result<f32, Error> {
    strength(heap, name, options.get("opacity").as_ref())
}

fn keyword_choice<T: Copy>(
    heap: &Heap,
    name: &str,
    key: &str,
    options: &Options<'_>,
    choices: &[(&str, T)],
) -> Result<T, Error> {
    let Some(value) = options.get(key) else {
        return Ok(choices[0].1);
    };
    let found = match value {
        Value::Keyword(keyword) => choices
            .iter()
            .find(|(known, _)| *known == keyword.name())
            .map(|(_, choice)| *choice),
        _ => None,
    };
    found.ok_or_else(|| {
        let listed: Vec<String> = choices
            .iter()
            .map(|(known, _)| format!(":{known}"))
            .collect();
        Error::new(format!(
            "image/{name} takes :{key} {}, not {}",
            listed.join(", "),
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })
}

struct Style {
    fill: Option<Rgba<u8>>,
    stroke: Option<Rgba<u8>>,
    width: f32,
    opacity: f32,
    blend: BlendMode,
    cap: LineCap,
    join: LineJoin,
    dash: Option<Vec<f32>>,
}

impl Style {
    fn read(heap: &Heap, name: &str, options: &Options<'_>, line: bool) -> Result<Self, Error> {
        let color = |key: &str| -> Result<Option<Rgba<u8>>, Error> {
            options
                .get(key)
                .map(|value| colors::color(heap, name, value))
                .transpose()
        };
        let (fill, stroke) = if line {
            (None, Some(color("color")?.unwrap_or(Rgba([0, 0, 0, 255]))))
        } else {
            match (color("fill")?, color("stroke")?) {
                (None, None) => (Some(Rgba([0, 0, 0, 255])), None),
                both => both,
            }
        };
        let width = match options.get("width") {
            Some(value) => factor(heap, name, value)?,
            None => 1.0,
        };
        let dash = match options.get("dash") {
            None | Some(Value::Nil) => None,
            Some(value) => Some(
                heap.elements(value)
                    .and_then(|items| {
                        items
                            .iter()
                            .map(|item| number(heap, name, "a dash length", *item).ok().map(narrow))
                            .collect::<Option<Vec<f32>>>()
                    })
                    .filter(|lengths| lengths.len() >= 2 && lengths.len().is_multiple_of(2))
                    .ok_or_else(|| outside(heap, name, "a :dash of on and off lengths", value))?,
            ),
        };
        Ok(Self {
            fill,
            stroke,
            width,
            opacity: opacity(heap, name, options)?,
            blend: blend(heap, name, options)?,
            cap: keyword_choice(
                heap,
                name,
                "cap",
                options,
                &[
                    ("butt", LineCap::Butt),
                    ("round", LineCap::Round),
                    ("square", LineCap::Square),
                ],
            )?,
            join: keyword_choice(
                heap,
                name,
                "join",
                options,
                &[
                    ("miter", LineJoin::Miter),
                    ("round", LineJoin::Round),
                    ("bevel", LineJoin::Bevel),
                ],
            )?,
            dash,
        })
    }

    fn paint(&self, color: Rgba<u8>) -> Paint<'static> {
        let mut paint = Paint::default();
        paint.set_color_rgba8(
            color[0],
            color[1],
            color[2],
            byte(unit(color[3]) * self.opacity),
        );
        paint.anti_alias = true;
        paint.blend_mode = self.blend;
        paint
    }

    fn painted(&self, pixmap: &mut Pixmap, path: &Path) {
        if let Some(fill) = self.fill {
            pixmap.fill_path(
                path,
                &self.paint(fill),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        if let Some(color) = self.stroke {
            let stroke = Stroke {
                width: self.width,
                line_cap: self.cap,
                line_join: self.join,
                dash: self
                    .dash
                    .clone()
                    .and_then(|lengths| StrokeDash::new(lengths, 0.0)),
                ..Stroke::default()
            };
            pixmap.stroke_path(
                path,
                &self.paint(color),
                &stroke,
                Transform::identity(),
                None,
            );
        }
    }
}

fn coordinate(heap: &Heap, name: &str, value: Value) -> Result<f32, Error> {
    number(heap, name, "a coordinate", value).map(narrow)
}

fn coordinates<const N: usize>(
    heap: &Heap,
    name: &str,
    values: &[Value],
) -> Result<[f32; N], Error> {
    let mut out = [0.0; N];
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = coordinate(heap, name, *value)?;
    }
    Ok(out)
}

fn point_list(heap: &Heap, name: &str, value: Value) -> Result<Vec<(f32, f32)>, Error> {
    let wrong = || outside(heap, name, "a vector of two or more [x y] points", value);
    let items = heap.elements(value).ok_or_else(wrong)?;
    let points = items
        .iter()
        .map(|item| {
            let pair = heap.elements(*item)?;
            let [x, y] = pair.as_slice() else {
                return None;
            };
            Some((
                coordinate(heap, name, *x).ok()?,
                coordinate(heap, name, *y).ok()?,
            ))
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(wrong)?;
    if points.len() < 2 {
        return Err(wrong());
    }
    Ok(points)
}

fn joined(points: &[(f32, f32)], close: bool) -> Option<Path> {
    let mut builder = PathBuilder::new();
    let (first, rest) = points.split_first()?;
    builder.move_to(first.0, first.1);
    for (x, y) in rest {
        builder.line_to(*x, *y);
    }
    if close {
        builder.close();
    }
    builder.finish()
}

fn rounded(left: f32, top: f32, width: f32, height: f32, radius: f32) -> Option<Path> {
    let round = radius.min(width / 2.0).min(height / 2.0);
    if round <= 0.0 {
        return Some(PathBuilder::from_rect(Rect::from_xywh(
            left, top, width, height,
        )?));
    }
    let pull = round * 0.552_284_8;
    let (right, bottom) = (left + width, top + height);
    let mut builder = PathBuilder::new();
    builder.move_to(left + round, top);
    builder.line_to(right - round, top);
    builder.cubic_to(
        right - round + pull,
        top,
        right,
        top + round - pull,
        right,
        top + round,
    );
    builder.line_to(right, bottom - round);
    builder.cubic_to(
        right,
        bottom - round + pull,
        right - round + pull,
        bottom,
        right - round,
        bottom,
    );
    builder.line_to(left + round, bottom);
    builder.cubic_to(
        left + round - pull,
        bottom,
        left,
        bottom - round + pull,
        left,
        bottom - round,
    );
    builder.line_to(left, top + round);
    builder.cubic_to(
        left,
        top + round - pull,
        left + round - pull,
        top,
        left + round,
        top,
    );
    builder.close();
    builder.finish()
}

fn shape(
    heap: &mut Heap,
    name: &'static str,
    target: Value,
    style: Style,
    path: Option<Path>,
) -> Result<Value, Error> {
    drawn(heap, name, target, move |pixmap| {
        if let Some(path) = path {
            style.painted(pixmap, &path);
        }
        Ok(())
    })
}

fn masked(pixels: &mut RgbaImage, mask: &RgbaImage) {
    pixels
        .as_chunks_mut::<4>()
        .0
        .par_iter_mut()
        .zip(mask.as_chunks::<4>().0.par_iter())
        .for_each(|(pixel, cover)| {
            let keep = unit(luma(cover)) * unit(cover[3]);
            pixel[3] = byte(unit(pixel[3]) * keep);
        });
}

fn pasted(
    pixmap: &mut Pixmap,
    top: &RgbaImage,
    at: (f32, f32),
    scale: f32,
    paint: PixmapPaint,
    mask: Option<&RgbaImage>,
) {
    let layer = to_pixmap(top);
    let whole =
        (at.0.fract() == 0.0) && (at.1.fract() == 0.0) && (scale - 1.0).abs() < f32::EPSILON;
    let transform = Transform::from_row(scale, 0.0, 0.0, scale, at.0, at.1);
    let cover = mask.and_then(|mask| {
        let mut shape = Pixmap::new(pixmap.width(), pixmap.height())?;
        let fitted = sized_mask(mask, top.width(), top.height());
        shape.draw_pixmap(
            0,
            0,
            to_pixmap(&fitted).as_ref(),
            &PixmapPaint::default(),
            transform,
            None,
        );
        Some(Mask::from_pixmap(shape.as_ref(), MaskType::Luminance))
    });
    if whole {
        #[expect(clippy::cast_possible_truncation, reason = "whole coordinates")]
        let (x, y) = (at.0 as i32, at.1 as i32);
        pixmap.draw_pixmap(
            x,
            y,
            layer.as_ref(),
            &paint,
            Transform::identity(),
            cover.as_ref(),
        );
    } else {
        pixmap.draw_pixmap(0, 0, layer.as_ref(), &paint, transform, cover.as_ref());
    }
}

fn sized_mask(mask: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    if mask.dimensions() == (width, height) {
        mask.clone()
    } else {
        imageops::resize(mask, width, height, imageops::FilterType::Triangle)
    }
}

fn tiled(pixels: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    let (from_width, from_height) = pixels.dimensions();
    let mut out = RgbaImage::new(width, height);
    out.par_chunks_mut(usize::try_from(width).unwrap_or(0) * 4)
        .enumerate()
        .for_each(|(y, row)| {
            let source = u32::try_from(y).unwrap_or(0) % from_height;
            for (x, pixel) in (0..width).zip(row.as_chunks_mut::<4>().0) {
                *pixel = pixels.get_pixel(x % from_width, source).0;
            }
        });
    out
}

fn canvas_of(heap: &Heap, name: &str, value: Value) -> Result<Arc<Canvas>, Error> {
    heap.shared_of::<Canvas>(value).ok_or_else(|| {
        Error::new(format!(
            "image/{name} expects a canvas, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })
}

pub(super) fn install(isolate: &mut Isolate) {
    install_edits(isolate);
    install_shapes(isolate);
    install_layers(isolate);
}

fn install_edits(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "begin-edit")
        .arity(1, Some(1))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "begin-edit", arguments[0])?;
            let pixmap = to_pixmap(&image.pixels);
            let canvas = Canvas {
                painting: Mutex::new(Some((image.with(RgbaImage::new(0, 0)), pixmap))),
            };
            heap.shared(CANVAS, Arc::new(canvas)).map_err(Error::from)
        });
    isolate
        .native(MODULE, "end-edit")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let canvas = canvas_of(heap, "end-edit", arguments[0])?;
            let taken = canvas
                .painting
                .lock()
                .map_err(|_| Error::new("image/end-edit found its canvas broken"))?
                .take();
            let Some((image, pixmap)) = taken else {
                return Err(Error::new(
                    "image/end-edit was given a canvas whose edit has ended",
                ));
            };
            stored(heap, image.with(from_pixmap(pixmap)))
        });
    isolate
        .native(MODULE, "set-pixel!")
        .arity(4, Some(4))
        .suspends()
        .function(|heap, arguments| {
            let [x, y] = [arguments[1], arguments[2]].map(|value| match value {
                Value::Int(number) => u32::try_from(number).ok(),
                _ => None,
            });
            let (Some(x), Some(y)) = (x, y) else {
                return Err(outside(
                    heap,
                    "set-pixel!",
                    "a point inside the image",
                    arguments[1],
                ));
            };
            let color = colors::color(heap, "set-pixel!", arguments[3])?;
            drawn(heap, "set-pixel!", arguments[0], move |pixmap| {
                let width = pixmap.width();
                if x >= width || y >= pixmap.height() {
                    return Err(format!(
                        "image/set-pixel! expects a point inside the {width}×{} image, got {x} {y}",
                        pixmap.height()
                    ));
                }
                let index = usize::try_from(y * width + x).unwrap_or(0);
                pixmap.pixels_mut()[index] =
                    ColorU8::from_rgba(color[0], color[1], color[2], color[3]).premultiply();
                Ok(())
            })
        });
}

fn install_shapes(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "rect!")
        .arity(5, Some(5))
        .keywords(&RECT)
        .suspends()
        .function(|heap, arguments| {
            let [x, y] = coordinates(heap, "rect!", &arguments[1..3])?;
            let width = factor(heap, "rect!", arguments[3])?;
            let height = factor(heap, "rect!", arguments[4])?;
            let options = Options::slots("rect!", &RECT, &arguments[5..]);
            let radius = narrow(options.number(heap, "radius", 0.0)?);
            let style = Style::read(heap, "rect!", &options, false)?;
            shape(
                heap,
                "rect!",
                arguments[0],
                style,
                rounded(x, y, width, height, radius),
            )
        });
    isolate
        .native(MODULE, "ellipse!")
        .arity(5, Some(5))
        .keywords(&SHAPE)
        .suspends()
        .function(|heap, arguments| {
            let [x, y, across, down] = coordinates(heap, "ellipse!", &arguments[1..5])?;
            let options = Options::slots("ellipse!", &SHAPE, &arguments[5..]);
            let style = Style::read(heap, "ellipse!", &options, false)?;
            let path = Rect::from_xywh(x - across, y - down, across * 2.0, down * 2.0)
                .and_then(PathBuilder::from_oval);
            shape(heap, "ellipse!", arguments[0], style, path)
        });
    isolate
        .native(MODULE, "circle!")
        .arity(4, Some(4))
        .keywords(&SHAPE)
        .suspends()
        .function(|heap, arguments| {
            let [x, y, radius] = coordinates(heap, "circle!", &arguments[1..4])?;
            let options = Options::slots("circle!", &SHAPE, &arguments[4..]);
            let style = Style::read(heap, "circle!", &options, false)?;
            shape(
                heap,
                "circle!",
                arguments[0],
                style,
                PathBuilder::from_circle(x, y, radius),
            )
        });
    isolate
        .native(MODULE, "polygon!")
        .arity(2, Some(2))
        .keywords(&SHAPE)
        .suspends()
        .function(|heap, arguments| {
            let points = point_list(heap, "polygon!", arguments[1])?;
            let options = Options::slots("polygon!", &SHAPE, &arguments[2..]);
            let style = Style::read(heap, "polygon!", &options, false)?;
            shape(heap, "polygon!", arguments[0], style, joined(&points, true))
        });
    isolate
        .native(MODULE, "line!")
        .arity(5, Some(5))
        .keywords(&LINE)
        .suspends()
        .function(|heap, arguments| {
            let [x1, y1, x2, y2] = coordinates(heap, "line!", &arguments[1..5])?;
            let options = Options::slots("line!", &LINE, &arguments[5..]);
            let style = Style::read(heap, "line!", &options, true)?;
            shape(
                heap,
                "line!",
                arguments[0],
                style,
                joined(&[(x1, y1), (x2, y2)], false),
            )
        });
    isolate
        .native(MODULE, "polyline!")
        .arity(2, Some(2))
        .keywords(&LINE)
        .suspends()
        .function(|heap, arguments| {
            let points = point_list(heap, "polyline!", arguments[1])?;
            let options = Options::slots("polyline!", &LINE, &arguments[2..]);
            let style = Style::read(heap, "polyline!", &options, true)?;
            shape(
                heap,
                "polyline!",
                arguments[0],
                style,
                joined(&points, false),
            )
        });
}

fn install_layers(isolate: &mut Isolate) {
    const PASTE: [&str; 4] = ["blend", "opacity", "mask", "scale"];
    isolate
        .native(MODULE, "paste!")
        .arity(4, Some(4))
        .keywords(&PASTE)
        .suspends()
        .function(|heap, arguments| {
            let top = wanted(heap, "paste!", arguments[1])?;
            let at = (
                coordinate(heap, "paste!", arguments[2])?,
                coordinate(heap, "paste!", arguments[3])?,
            );
            let options = Options::slots("paste!", &PASTE, &arguments[4..]);
            let paint = PixmapPaint {
                opacity: opacity(heap, "paste!", &options)?,
                blend_mode: blend(heap, "paste!", &options)?,
                quality: FilterQuality::Bicubic,
            };
            let scale = match options.get("scale") {
                Some(value) => factor(heap, "paste!", value)?,
                None => 1.0,
            };
            let mask = options
                .get("mask")
                .map(|value| wanted(heap, "paste!", value))
                .transpose()?;
            drawn(heap, "paste!", arguments[0], move |pixmap| {
                let mask = mask.map(|mask| mask.pixels.clone());
                pasted(pixmap, &top.pixels, at, scale, paint, mask.as_ref());
                Ok(())
            })
        });
    isolate
        .native(MODULE, "mask")
        .arity(2, Some(2))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "mask", arguments[0])?;
            let mask = wanted(heap, "mask", arguments[1])?;
            later(heap, "mask", move || {
                let (width, height) = image.pixels.dimensions();
                let mut pixels = image.pixels.clone();
                masked(&mut pixels, &sized_mask(&mask.pixels, width, height));
                Ok(image.with(pixels))
            })
        });
    isolate
        .native(MODULE, "tile")
        .arity(3, Some(3))
        .suspends()
        .function(|heap, arguments| {
            let image = wanted(heap, "tile", arguments[0])?;
            let width = size(heap, "tile", "a width", arguments[1])?;
            let height = size(heap, "tile", "a height", arguments[2])?;
            roomy("tile", width, height)?;
            later(heap, "tile", move || {
                Ok(image.with(tiled(&image.pixels, width, height)))
            })
        });
}
