use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

use super::{Image, MODULE, Options, later, text, wanted};
use crate::error::Error;
use crate::isolate::Isolate;
use crate::portable::Portable;
use crate::symbol::Sym;
use crate::value::Value;

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

fn svg(path: &Path) -> Result<Image, String> {
    let data =
        fs::read(path).map_err(|error| format!("Couldn't read {}: {error}", path.display()))?;
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_data(&data, &options)
        .map_err(|error| format!("Couldn't read {} as SVG: {error}", path.display()))?;
    let size = tree.size().to_int_size();
    super::room("open", size.width(), size.height())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| format!("{} has no size to draw", path.display()))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    let pixels: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect();
    let pixels = image::RgbaImage::from_raw(size.width(), size.height(), pixels)
        .ok_or_else(|| format!("{} drew to the wrong size", path.display()))?;
    let mut image = Image::new(pixels);
    image.format = Some("svg".to_owned());
    Ok(image)
}

fn format_name(format: ImageFormat) -> String {
    format
        .extensions_str()
        .first()
        .copied()
        .unwrap_or("image")
        .to_owned()
}

pub(super) fn open(path: &Path) -> Result<Image, String> {
    let extension = extension(path);
    if extension == "svg" || extension == "svgz" {
        return svg(path);
    }
    let failed =
        |error: &dyn std::fmt::Display| format!("Couldn't open {}: {error}", path.display());
    let reader = ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|error| failed(&error))?;
    let format = reader.format().map(format_name);
    let mut decoder = reader.into_decoder().map_err(|error| failed(&error))?;
    let exif = decoder.exif_metadata().ok().flatten();
    let icc = decoder.icc_profile().ok().flatten();
    let xmp = decoder.xmp_metadata().ok().flatten();
    let orientation = decoder.orientation().ok();
    let mut dynamic = DynamicImage::from_decoder(decoder).map_err(|error| failed(&error))?;
    if let Some(orientation) = orientation {
        dynamic.apply_orientation(orientation);
    }
    let alpha = dynamic.color().has_alpha();
    Ok(Image {
        pixels: dynamic.into_rgba8(),
        format,
        alpha,
        exif,
        icc,
        xmp,
    })
}

fn chosen_format(path: &Path, given: Option<&str>) -> Result<ImageFormat, String> {
    let name = given.map_or_else(|| extension(path), str::to_ascii_lowercase);
    ImageFormat::from_extension(&name)
        .filter(|format| {
            matches!(
                format,
                ImageFormat::Png
                    | ImageFormat::Jpeg
                    | ImageFormat::Gif
                    | ImageFormat::WebP
                    | ImageFormat::Bmp
                    | ImageFormat::Tiff
                    | ImageFormat::Ico
                    | ImageFormat::Qoi
            )
        })
        .ok_or_else(|| {
            format!(
                "image/save can't write {name:?}; use png, jpg, webp, gif, bmp, tiff, ico or qoi"
            )
        })
}

pub(super) fn save(
    image: &Image,
    path: &Path,
    format: ImageFormat,
    quality: u8,
) -> Result<(), String> {
    let failed =
        |error: &dyn std::fmt::Display| format!("Couldn't save {}: {error}", path.display());
    let file = File::create(path).map_err(|error| failed(&error))?;
    let mut writer = BufWriter::new(file);
    let dynamic = DynamicImage::ImageRgba8(image.pixels.clone());
    match format {
        ImageFormat::Jpeg => {
            let encoder = JpegEncoder::new_with_quality(&mut writer, quality);
            dynamic.to_rgb8().write_with_encoder(encoder)
        }
        ImageFormat::Bmp | ImageFormat::Ico if !image.alpha => {
            dynamic.to_rgb8().write_to(&mut writer, format)
        }
        _ => dynamic.write_to(&mut writer, format),
    }
    .map_err(|error| failed(&error))
}

fn probe(path: &Path) -> Result<Portable, String> {
    let failed =
        |error: &dyn std::fmt::Display| format!("Couldn't read {}: {error}", path.display());
    let (width, height, format) = if matches!(extension(path).as_str(), "svg" | "svgz") {
        let image = svg(path)?;
        (
            image.pixels.width(),
            image.pixels.height(),
            Some("svg".to_owned()),
        )
    } else {
        let reader = ImageReader::open(path)
            .and_then(ImageReader::with_guessed_format)
            .map_err(|error| failed(&error))?;
        let format = reader.format().map(format_name);
        let (width, height) = reader.into_dimensions().map_err(|error| failed(&error))?;
        (width, height, format)
    };
    let key = |name: &str| Portable::Keyword(Sym::intern(name));
    Ok(Portable::Map(vec![
        (key("width"), Portable::Int(i64::from(width))),
        (key("height"), Portable::Int(i64::from(height))),
        (
            key("format"),
            format.map_or(Portable::Nil, |format| Portable::String(format.into())),
        ),
    ]))
}
pub(super) fn install(isolate: &mut Isolate) {
    isolate.define_native_in(MODULE, "open", 1, Some(1), |heap, arguments| {
        let path = PathBuf::from(text(heap, "open", "a path", arguments[0])?);
        later(heap, "open", move || open(&path))
    });
    isolate.define_native_in(MODULE, "probe", 1, Some(1), |heap, arguments| {
        let path = PathBuf::from(text(heap, "probe", "a path", arguments[0])?);
        heap.background("ox image/probe", move || {
            probe(&path).map_err(super::failure)
        })
    });
    isolate.define_native_awaiting_in(MODULE, "save", 2, Some(6), |heap, arguments| {
        let image = wanted(heap, "save", arguments[0])?;
        let path = PathBuf::from(text(heap, "save", "a path", arguments[1])?);
        let options = Options::read(heap, "save", &arguments[2..], &["quality", "format"])?;
        let quality = options.number(heap, "quality", 90.0)?;
        let given = match options.get("format") {
            Some(Value::Keyword(name)) => Some(name.name().to_owned()),
            Some(value) => Some(text(heap, "save", "a format", value)?.to_owned()),
            None => None,
        };
        let format = chosen_format(&path, given.as_deref()).map_err(Error::new)?;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to 1..=100"
        )]
        let quality = quality.clamp(1.0, 100.0).round() as u8;
        let shown = path.display().to_string();
        heap.background_json("ox image/save", move || {
            save(&image, &path, format, quality).map(|()| serde_json::Value::String(shown))
        })
    });
}
