use std::fs;
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::{ImageEncoder, RgbImage};
use ox::Isolate;

fn slashed(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn shown(isolate: &mut Isolate, source: &str) -> String {
    let source = format!("(module (import (ox image))) {source}");
    let value = isolate
        .eval(&source)
        .unwrap_or_else(|error| panic!("{source}: {}", error.message));
    isolate.write(value)
}

fn failure(isolate: &mut Isolate, source: &str) -> String {
    let source = format!("(module (import (ox image))) {source}");
    isolate.eval(&source).unwrap_err().message
}

fn rotated_jpeg(path: &Path) {
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 90)
        .write_image(
            RgbImage::from_pixel(4, 2, image::Rgb([200, 30, 30])).as_raw(),
            4,
            2,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    let tiff: [u8; 26] = [
        b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
    ];
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend_from_slice(&tiff);
    let length = u16::try_from(payload.len() + 2).unwrap().to_be_bytes();
    let mut file = vec![0xFF, 0xD8, 0xFF, 0xE1, length[0], length[1]];
    file.extend_from_slice(&payload);
    file.extend_from_slice(&jpeg[2..]);
    fs::write(path, file).unwrap();
}

#[test]
fn images_are_made_saved_and_opened_again() {
    let dir = tempfile::tempdir().unwrap();
    let png = slashed(&dir.path().join("dot.png"));
    let mut isolate = Isolate::new();

    assert_eq!(
        shown(
            &mut isolate,
            "(image/info (image/new 4 2 :color \"#ff000080\"))"
        ),
        "{width: 4 height: 2 format: nil alpha: true}"
    );
    assert_eq!(
        shown(
            &mut isolate,
            &format!("(await (image/save (image/new 4 2 :color :teal) \"{png}\"))")
        ),
        format!("\"{png}\"")
    );
    assert_eq!(
        shown(
            &mut isolate,
            &format!("(image/info (image/open \"{png}\"))")
        ),
        "{width: 4 height: 2 format: \"png\" alpha: true}"
    );
    assert_eq!(
        shown(&mut isolate, &format!("(await (image/probe \"{png}\"))")),
        "{width: 4 height: 2 format: \"png\"}"
    );
    assert_eq!(
        shown(
            &mut isolate,
            &format!("(image/width (image/open \"{png}\"))")
        ),
        "4"
    );
    assert_eq!(
        shown(
            &mut isolate,
            "[(image/image? (image/new 1 1)) (image/image? 5)]"
        ),
        "[true false]"
    );
}

#[test]
fn jpeg_quality_changes_the_file_and_exif_orientation_is_applied() {
    let dir = tempfile::tempdir().unwrap();
    let low = slashed(&dir.path().join("low.jpg"));
    let high = slashed(&dir.path().join("high.jpg"));
    let turned = dir.path().join("turned.jpg");
    rotated_jpeg(&turned);
    let turned = slashed(&turned);
    let mut isolate = Isolate::new();

    shown(
        &mut isolate,
        &format!(
            "(def photo (image/new 64 64 :color :orange)) (await (image/save photo \"{low}\" :quality 10)) (await (image/save photo \"{high}\" :quality 95))"
        ),
    );

    let size = |path: &str| fs::metadata(path).unwrap().len();
    assert!(size(&low) < size(&high), "{} {}", size(&low), size(&high));
    assert_eq!(
        shown(
            &mut isolate,
            &format!("(image/info (image/open \"{turned}\"))")
        ),
        "{width: 2 height: 4 format: \"jpg\" alpha: false}"
    );
}

#[test]
fn svg_files_are_drawn_and_colors_can_be_written_many_ways() {
    let dir = tempfile::tempdir().unwrap();
    let svg = dir.path().join("logo.svg");
    fs::write(
        &svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="6"><rect width="10" height="6" fill="red"/></svg>"#,
    )
    .unwrap();
    let svg = slashed(&svg);
    let mut isolate = Isolate::new();

    assert_eq!(
        shown(
            &mut isolate,
            &format!("(image/info (image/open \"{svg}\"))")
        ),
        "{width: 10 height: 6 format: \"svg\" alpha: true}"
    );
    assert_eq!(
        shown(
            &mut isolate,
            "[(image/color \"#f80\") (image/color \"#ff880080\") (image/color [1 2 3]) (image/color :teal) (image/color :transparent) (image/color \"RebeccaPurple\")]"
        ),
        "[[255 136 0 255] [255 136 0 128] [1 2 3 255] [0 128 128 255] [0 0 0 0] [102 51 153 255]]"
    );
    assert_eq!(
        failure(&mut isolate, "(image/color :no-such-color)"),
        "image/color expects a color such as \"#ff8800\", [255 136 0], :orange or a theme face, got :no-such-color"
    );
}

#[test]
fn mistakes_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let missing = slashed(&dir.path().join("missing.png"));
    let mut isolate = Isolate::new();

    assert!(
        failure(&mut isolate, &format!("(await (image/open \"{missing}\"))"))
            .starts_with(&format!("Couldn't open {missing}")),
    );
    assert_eq!(
        failure(&mut isolate, "(image/width 5)"),
        "image/width expects an image, got 5"
    );
    assert_eq!(
        failure(&mut isolate, "(image/new 0 3)"),
        "image/new expects a width as a whole number above 0, got 0"
    );
    assert_eq!(
        failure(
            &mut isolate,
            &format!(
                "(await (image/save (image/new 1 1) \"{}\"))",
                slashed(&dir.path().join("x.psd"))
            )
        ),
        "image/save can't write \"psd\"; use png, jpg, webp, gif, bmp, tiff, ico or qoi"
    );
}
