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
fn images_decode_from_base64_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let png = slashed(&dir.path().join("dot.png"));
    let mut isolate = ox_library::isolate();
    shown(
        &mut isolate,
        &format!("(await (image/save (image/new 3 2 :color :red) \"{png}\"))"),
    );

    assert_eq!(
        shown(
            &mut isolate,
            &format!(
                "(module (import (ox image) (ox file))) (image/info (await (image/decode (await (file/read-base64 \"{png}\")))))"
            )
        ),
        "{width: 3 height: 2 format: \"png\" alpha: true}"
    );
    assert_eq!(
        shown(
            &mut isolate,
            "(module (import (ox image) (ox crypto))) (image/width (await (image/decode (crypto/to-base64 \"<svg xmlns='http://www.w3.org/2000/svg' width='5' height='4'/>\"))))"
        ),
        "5"
    );
    assert!(
        failure(&mut isolate, "(await (image/decode \"bm90IGFuIGltYWdl\"))")
            .starts_with("Couldn't decode the image")
    );
}

#[test]
fn images_are_made_saved_and_opened_again() {
    let dir = tempfile::tempdir().unwrap();
    let png = slashed(&dir.path().join("dot.png"));
    let mut isolate = ox_library::isolate();

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
    let mut isolate = ox_library::isolate();

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
    let mut isolate = ox_library::isolate();

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
    let mut isolate = ox_library::isolate();

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
        failure(&mut isolate, "(image/new 40000 40000)"),
        "image/new would make a 40000×40000 image; images can be at most 32768 pixels a side and 134 megapixels"
    );
    assert_eq!(
        failure(
            &mut isolate,
            "(await (image/resize (image/new 10 10) 30000))"
        ),
        "image/resize would make a 30000×30000 image; images can be at most 32768 pixels a side and 134 megapixels"
    );
    assert_eq!(
        failure(
            &mut isolate,
            "(await (image/tile (image/new 1 1) 20000 20000))"
        ),
        "image/tile would make a 20000×20000 image; images can be at most 32768 pixels a side and 134 megapixels"
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

const HALVES: &str = "(def halves (image/canvas (image/new 2 2 :color :red) 4 2 :anchor :left))";

#[test]
fn geometry_resizes_crops_pads_and_turns_images() {
    let mut isolate = ox_library::isolate();
    shown(&mut isolate, HALVES);
    let size = |isolate: &mut Isolate, source: &str| {
        shown(
            isolate,
            &format!("(let [found {source}] [(image/width found) (image/height found)])"),
        )
    };

    assert_eq!(
        shown(
            &mut isolate,
            "[(image/pixel halves 0 0) (image/pixel halves 3 1)]"
        ),
        "[[255 0 0 255] [0 0 0 0]]"
    );
    assert_eq!(size(&mut isolate, "(image/resize halves 8)"), "[8 4]");
    assert_eq!(
        size(
            &mut isolate,
            "(image/resize halves 8 :height 3 :filter :nearest)"
        ),
        "[8 3]"
    );
    assert_eq!(
        size(&mut isolate, "(image/fit (image/new 100 50) 10 10)"),
        "[10 5]"
    );
    assert_eq!(
        size(&mut isolate, "(image/fill (image/new 100 50) 10 10)"),
        "[10 10]"
    );
    assert_eq!(
        size(&mut isolate, "(image/thumbnail (image/new 100 50) 20)"),
        "[20 10]"
    );
    assert_eq!(
        shown(
            &mut isolate,
            "(image/pixel (image/crop halves 1 0 2 2) 0 0)"
        ),
        "[255 0 0 255]"
    );
    assert_eq!(
        shown(
            &mut isolate,
            "(image/pixel (image/crop halves 2 0 2 2) 0 0)"
        ),
        "[0 0 0 0]"
    );
    assert_eq!(size(&mut isolate, "(image/pad halves 1)"), "[6 4]");
    assert_eq!(
        shown(
            &mut isolate,
            "(let [padded (image/pad halves 1 :color :white)] [(image/pixel padded 0 0) (image/pixel padded 1 1)])"
        ),
        "[[255 255 255 255] [255 0 0 255]]"
    );
    assert_eq!(size(&mut isolate, "(image/pad halves 0 :left 2)"), "[6 2]");
    assert_eq!(
        shown(
            &mut isolate,
            "(image/pixel (image/flip halves :horizontal) 3 0)"
        ),
        "[255 0 0 255]"
    );
    assert_eq!(size(&mut isolate, "(image/rotate halves 90)"), "[2 4]");
    assert_eq!(
        shown(&mut isolate, "(image/pixel (image/rotate halves 90) 0 0)"),
        "[255 0 0 255]"
    );
    assert_eq!(
        size(&mut isolate, "(image/rotate halves 30 :expand false)"),
        "[4 2]"
    );
    assert_eq!(
        size(&mut isolate, "(image/rotate (image/new 10 10) 45)"),
        "[14 14]"
    );
    assert_eq!(
        size(
            &mut isolate,
            "(image/trim (image/canvas (image/new 2 2 :color :red) 6 6 :color :white))"
        ),
        "[2 2]"
    );
    assert_eq!(
        shown(
            &mut isolate,
            "(image/pixel (image/affine halves [1 0 2 0 1 0]) 2 0)"
        ),
        "[255 0 0 255]"
    );
    assert_eq!(
        shown(
            &mut isolate,
            "(image/pixel (image/perspective halves [[0 0] [4 0] [4 2] [0 2]] [[0 0] [4 0] [4 2] [0 2]]) 0 1)"
        ),
        "[255 0 0 255]"
    );
}

fn adjusted(isolate: &mut Isolate, color: &str, change: &str) -> String {
    shown(
        isolate,
        &format!("(-> (image/new 1 1 :color {color}) {change} (image/pixel 0 0))"),
    )
}

#[test]
fn color_adjustments_follow_css_filters() {
    let isolate = &mut ox_library::isolate();

    assert_eq!(
        adjusted(isolate, "[100 50 200]", "(image/brightness 1.5)"),
        "[150 75 255 255]"
    );
    assert_eq!(
        adjusted(isolate, "[100 200 0]", "(image/contrast 0.5)"),
        "[114 164 64 255]"
    );
    assert_eq!(
        adjusted(isolate, "[64 0 255]", "(image/gamma 2)"),
        "[128 0 255 255]"
    );
    assert_eq!(
        adjusted(isolate, "[50 60 70]", "(image/exposure 1)"),
        "[100 120 140 255]"
    );
    assert_eq!(
        adjusted(isolate, ":red", "(image/hue 180)"),
        "[0 109 109 255]"
    );
    assert_eq!(
        adjusted(isolate, ":red", "(image/saturation 0)"),
        "[54 54 54 255]"
    );
    assert_eq!(
        adjusted(isolate, ":red", "(image/grayscale)"),
        "[54 54 54 255]"
    );
    assert_eq!(
        adjusted(isolate, ":white", "(image/sepia)"),
        "[255 255 239 255]"
    );
    assert_eq!(
        adjusted(isolate, "[10 20 30 128]", "(image/invert)"),
        "[245 235 225 128]"
    );
    assert_eq!(
        adjusted(isolate, ":red", "(image/vibrance 2)"),
        "[255 0 0 255]"
    );
    assert_eq!(
        shown(
            isolate,
            "(> (first (-> (image/new 1 1 :color [150 100 100]) (image/vibrance 2) (image/pixel 0 0))) 160)"
        ),
        "true"
    );
    assert_eq!(
        adjusted(isolate, ":white", "(image/tint :red)"),
        "[255 0 0 255]"
    );
    assert_eq!(
        adjusted(isolate, "[50 60 70]", "(image/balance :red 2)"),
        "[100 60 70 255]"
    );
    assert_eq!(
        adjusted(
            isolate,
            "[200 100 50]",
            "(image/balance :neutral [200 100 50])"
        ),
        "[117 117 117 255]"
    );
}

#[test]
fn levels_curves_and_fewer_colors() {
    let isolate = &mut ox_library::isolate();
    shown(isolate, HALVES);

    assert_eq!(
        adjusted(
            isolate,
            "[75 20 200]",
            "(image/levels :black 50 :white 150)"
        ),
        "[64 0 255 255]"
    );
    assert_eq!(
        adjusted(isolate, "[10 20 30]", "(image/curves [[0 255] [255 0]])"),
        "[245 235 225 255]"
    );
    assert_eq!(
        adjusted(
            isolate,
            "[10 20 30]",
            "(image/curves [[0 255] [255 0]] :channel :red)"
        ),
        "[245 20 30 255]"
    );
    assert_eq!(
        adjusted(isolate, "[100 200 30]", "(image/posterize 2)"),
        "[0 255 0 255]"
    );
    assert_eq!(
        adjusted(isolate, "[100 200 30]", "(image/threshold 128)"),
        "[255 255 255 255]"
    );
    assert_eq!(
        adjusted(isolate, "[100 100 100]", "(image/threshold)"),
        "[0 0 0 255]"
    );
    assert_eq!(
        shown(
            isolate,
            "(let [found (image/threshold halves :otsu)] [(image/pixel found 0 0) (image/pixel found 3 1)])"
        ),
        "[[255 255 255 255] [0 0 0 0]]"
    );
    assert_eq!(
        shown(
            isolate,
            "(image/pixel (image/replace halves :red :blue) 0 0)"
        ),
        "[0 0 255 255]"
    );
    assert_eq!(
        adjusted(
            isolate,
            "[250 5 0]",
            "(image/replace :red :blue :tolerance 10)"
        ),
        "[0 0 255 255]"
    );
    assert_eq!(
        adjusted(
            isolate,
            "[200 5 0]",
            "(image/replace :red :blue :tolerance 10)"
        ),
        "[200 5 0 255]"
    );
    assert_eq!(
        shown(
            isolate,
            "(let [found (image/quantize (image/new 4 4 :color [128 128 128]) [:black :white])] [(image/pixel found 0 0) (image/pixel found 1 0)])"
        ),
        "[[255 255 255 255] [0 0 0 255]]"
    );
    assert_eq!(
        shown(
            isolate,
            "(let [found (image/quantize (image/new 4 4 :color [128 128 128]) [:black :white] :dither false)] [(image/pixel found 0 0) (image/pixel found 1 0)])"
        ),
        "[[255 255 255 255] [255 255 255 255]]"
    );
    assert_eq!(
        shown(
            isolate,
            "(let [found (image/quantize (image/canvas (image/new 2 2 :color :red) 4 2 :anchor :left :color :blue) 2)] [(image/pixel found 0 0) (image/pixel found 3 1)])"
        ),
        "[[255 0 0 255] [0 0 255 255]]"
    );
}

#[test]
fn color_mistakes_are_reported() {
    let isolate = &mut ox_library::isolate();
    shown(isolate, HALVES);

    assert_eq!(
        failure(isolate, "(image/brightness halves -1)"),
        "image/brightness expects a factor of 0 or more, got -1"
    );
    assert_eq!(
        failure(isolate, "(image/sepia halves 2)"),
        "image/sepia expects a strength from 0 to 1, got 2"
    );
    assert_eq!(
        failure(isolate, "(image/posterize halves 1)"),
        "image/posterize expects 2 to 256 levels, got 1"
    );
    assert_eq!(
        failure(isolate, "(image/curves halves [[0 0]])"),
        "image/curves expects two or more [in out] points from 0 to 255, got [[0 0]]"
    );
    assert_eq!(
        failure(isolate, "(image/threshold halves :median)"),
        "image/threshold expects a level from 0 to 255 or :otsu, got :median"
    );
    assert_eq!(
        failure(isolate, "(image/quantize halves 1)"),
        "image/quantize expects 2 to 256 colors or a vector of colors, got 1"
    );
}

const STRIPES: &str = "(def stripes (image/canvas (image/new 1 1 :color :white) 3 1 :color :black)) (def step (image/canvas (image/new 10 20 :color :white) 20 20 :anchor :left :color :black))";

#[test]
fn blurs_keep_edges_clean_and_flat_areas_flat() {
    let isolate = &mut ox_library::isolate();
    shown(isolate, HALVES);
    shown(isolate, STRIPES);

    assert_eq!(
        shown(
            isolate,
            "(let [p (image/pixel (image/blur halves 1) 2 0)] [(nth p 0) (nth p 1) (and (> (nth p 3) 0) (< (nth p 3) 255))])"
        ),
        "[255 0 true]"
    );
    assert_eq!(
        adjusted(isolate, "[10 20 30]", "(image/blur 2)"),
        "[10 20 30 255]"
    );
    assert_eq!(
        shown(
            isolate,
            "(image/pixel (image/blur (image/new 5 5 :color [10 20 30]) 30) 2 2)"
        ),
        "[10 20 30 255]"
    );
    assert_eq!(
        shown(isolate, "(image/pixel (image/box-blur stripes 1) 1 0)"),
        "[85 85 85 255]"
    );
    assert_eq!(
        shown(
            isolate,
            "(image/pixel (image/median (image/canvas (image/new 1 1 :color :white) 3 3 :color :black) 1) 1 1)"
        ),
        "[0 0 0 255]"
    );
    assert_eq!(
        shown(
            isolate,
            "(image/pixel (image/pixelate (image/canvas (image/new 1 1 :color :white) 4 1 :anchor :left :color :black) 4) 2 0)"
        ),
        "[64 64 64 255]"
    );
}

#[test]
fn sharpening_edges_and_kernels() {
    let isolate = &mut ox_library::isolate();
    shown(isolate, STRIPES);
    shown(
        isolate,
        "(def ridge (image/canvas (image/new 1 1 :color [150 150 150]) 3 1 :color [100 100 100]))",
    );

    assert_eq!(
        adjusted(isolate, "[100 100 100]", "(image/sharpen)"),
        "[100 100 100 255]"
    );
    assert_eq!(
        shown(
            isolate,
            "(> (first (image/pixel (image/sharpen ridge) 1 0)) 150)"
        ),
        "true"
    );
    assert_eq!(
        shown(
            isolate,
            "(image/pixel (image/unsharp ridge 1 1 :threshold 100) 1 0)"
        ),
        "[150 150 150 255]"
    );
    assert_eq!(
        shown(isolate, "(image/pixel (image/edges step) 9 10)"),
        "[255 255 255 255]"
    );
    assert_eq!(
        shown(isolate, "(image/pixel (image/edges step) 2 10)"),
        "[0 0 0 255]"
    );
    assert_eq!(
        shown(
            isolate,
            "(let [found (image/edges step :method :canny)] [(image/pixel found 2 10) (or (= (image/pixel found 9 10) [255 255 255 255]) (= (image/pixel found 10 10) [255 255 255 255]))])"
        ),
        "[[0 0 0 255] true]"
    );
    assert_eq!(
        adjusted(isolate, "[100 100 100]", "(image/emboss)"),
        "[100 100 100 255]"
    );
    assert_eq!(
        shown(
            isolate,
            "(image/pixel (image/convolve stripes [[0 0 0] [0 1 0] [0 0 0]]) 1 0)"
        ),
        "[255 255 255 255]"
    );
    assert_eq!(
        shown(
            isolate,
            "(image/pixel (image/convolve stripes [[1 1 1]]) 1 0)"
        ),
        "[85 85 85 255]"
    );
}

#[test]
fn noise_repeats_and_vignettes_darken_corners() {
    let isolate = &mut ox_library::isolate();
    shown(isolate, "(def gray (image/new 4 4 :color [128 128 128]))");

    assert_eq!(
        shown(
            isolate,
            "(= (image/pixel (image/noise gray 30) 1 1) (image/pixel (image/noise gray 30) 1 1))"
        ),
        "true"
    );
    assert_eq!(
        shown(
            isolate,
            "(= (image/pixel (image/noise gray 30) 1 1) (image/pixel (image/noise gray 30 :seed 7) 1 1))"
        ),
        "false"
    );
    assert_eq!(
        shown(
            isolate,
            "(let [[r g b] (image/pixel (image/noise gray 30 :mono true) 2 2)] (and (= r g) (= g b)))"
        ),
        "true"
    );
    assert_eq!(
        shown(isolate, "(image/pixel (image/noise gray 0) 1 1)"),
        "[128 128 128 255]"
    );
    assert_eq!(
        shown(
            isolate,
            "(let [found (image/vignette (image/new 9 9 :color :white))] [(image/pixel found 4 4) (< (first (image/pixel found 0 0)) 255)])"
        ),
        "[[255 255 255 255] true]"
    );
}

#[test]
fn filter_mistakes_are_reported() {
    let isolate = &mut ox_library::isolate();
    shown(isolate, HALVES);

    assert_eq!(
        failure(isolate, "(image/blur halves -1)"),
        "image/blur expects a radius of 0 or more, got -1"
    );
    assert_eq!(
        failure(isolate, "(image/edges halves :method :roberts)"),
        "image/edges takes :method :sobel or :canny, not :roberts"
    );
    assert_eq!(
        failure(isolate, "(image/pixelate halves 0)"),
        "image/pixelate expects a block size as a whole number above 0, got 0"
    );
    assert_eq!(
        failure(isolate, "(image/convolve halves [[1 1]])"),
        "image/convolve expects rows of numbers with an odd width and height, got [[1 1]]"
    );
}

const BLANK: &str = "(def blank (image/new 10 10 :color :white))";

#[test]
fn edits_draw_shapes_on_a_private_copy() {
    let isolate = &mut ox_library::isolate();
    shown(isolate, BLANK);

    assert_eq!(
        shown(
            isolate,
            "(let [drawn (image/edit blank (fn [c] (image/rect! c 0 0 5 5 :fill :red)))] [(image/pixel drawn 2 2) (image/pixel drawn 7 7) (image/pixel blank 2 2)])"
        ),
        "[[255 0 0 255] [255 255 255 255] [255 255 255 255]]"
    );
    assert_eq!(
        shown(
            isolate,
            "(image/pixel (image/rect! blank 0 0 5 5 :fill :red) 2 2)"
        ),
        "[255 0 0 255]"
    );
    assert_eq!(
        shown(
            isolate,
            "(let [drawn (image/circle! blank 5 5 3 :fill :blue)] [(image/pixel drawn 5 5) (image/pixel drawn 0 0)])"
        ),
        "[[0 0 255 255] [255 255 255 255]]"
    );
    assert_eq!(
        shown(
            isolate,
            "(image/pixel (image/line! blank 0 5 10 5 :width 2) 3 4)"
        ),
        "[0 0 0 255]"
    );
    assert_eq!(
        shown(
            isolate,
            "(let [drawn (image/rect! blank 1 1 8 8 :stroke :black :width 2)] [(image/pixel drawn 1 5) (image/pixel drawn 5 5)])"
        ),
        "[[0 0 0 255] [255 255 255 255]]"
    );
    assert_eq!(
        shown(
            isolate,
            "(let [drawn (image/polygon! blank [[0 0] [10 0] [0 10]] :fill :red)] [(image/pixel drawn 1 1) (image/pixel drawn 8 8)])"
        ),
        "[[255 0 0 255] [255 255 255 255]]"
    );
    assert_eq!(
        shown(
            isolate,
            "(let [[r g b] (image/pixel (image/rect! blank 0 0 10 10 :fill :red :opacity 0.5) 5 5)] [r (< 120 g 136) (< 120 b 136)])"
        ),
        "[255 true true]"
    );
    assert_eq!(
        shown(
            isolate,
            "(image/pixel (image/edit blank (fn [c] (image/set-pixel! c 3 3 :red))) 3 3)"
        ),
        "[255 0 0 255]"
    );
    assert_eq!(
        failure(
            isolate,
            "(let [c (image/begin-edit blank)] (image/end-edit c) (image/rect! c 0 0 1 1))"
        ),
        "image/rect! was given a canvas whose edit has ended"
    );
    assert_eq!(
        failure(isolate, "(image/rect! 5 0 0 1 1)"),
        "image/rect! expects an image or a canvas, got 5"
    );
}

#[test]
fn edit_is_built_in_and_waits_for_what_it_is_given() {
    let isolate = &mut ox_library::isolate();
    shown(isolate, BLANK);

    assert_eq!(shown(isolate, "image/edit"), "#<native image/edit>");
    assert_eq!(
        shown(
            isolate,
            "(image/pixel (image/edit (image/resize blank 4) (fn [c] (sleep 1) (image/set-pixel! c 1 1 :red))) 1 1)"
        ),
        "[255 0 0 255]"
    );
    assert_eq!(
        shown(isolate, "(image/width (image/edit blank (fn [c] 5)))"),
        "10"
    );
    assert_eq!(
        failure(isolate, "(image/edit 5 identity)"),
        "image/begin-edit expects an image, got 5"
    );
    assert_eq!(
        failure(
            isolate,
            "(image/edit blank (fn [c] (raise (error \"inside\"))))"
        ),
        "inside"
    );
    assert_eq!(
        failure(isolate, "(image/edit blank 5)"),
        "5 is not a function"
    );
}

#[test]
fn layers_blend_mask_and_tile() {
    let isolate = &mut ox_library::isolate();
    shown(isolate, BLANK);
    shown(isolate, HALVES);
    shown(
        isolate,
        "(def gray (image/new 2 2 :color [128 128 128])) (def red (image/new 2 2 :color :red)) (def left (image/canvas (image/new 1 2 :color :white) 2 2 :anchor :left :color :black))",
    );
    let blended = |isolate: &mut Isolate, mode: &str| {
        shown(
            isolate,
            &format!("(image/pixel (image/paste! gray red 0 0 :blend :{mode}) 0 0)"),
        )
    };

    assert_eq!(
        shown(
            isolate,
            "(let [pasted (image/paste! blank red 4 4)] [(image/pixel pasted 5 5) (image/pixel pasted 3 3)])"
        ),
        "[[255 0 0 255] [255 255 255 255]]"
    );
    assert_eq!(blended(isolate, "multiply"), "[128 0 0 255]");
    assert_eq!(blended(isolate, "screen"), "[255 128 128 255]");
    assert_eq!(blended(isolate, "difference"), "[127 128 128 255]");
    assert_eq!(
        shown(
            isolate,
            "(image/width (image/paste! gray red 0 0 :blend :luminosity))"
        ),
        "2"
    );
    assert_eq!(
        shown(
            isolate,
            "(image/pixel (image/paste! gray red 0 0 :opacity 0) 0 0)"
        ),
        "[128 128 128 255]"
    );
    assert_eq!(
        shown(
            isolate,
            "(let [pasted (image/paste! blank red 0 0 :mask left)] [(image/pixel pasted 0 0) (image/pixel pasted 1 0)])"
        ),
        "[[255 0 0 255] [255 255 255 255]]"
    );
    assert_eq!(
        shown(
            isolate,
            "(let [masked (image/mask red left)] [(image/pixel masked 0 0) (image/pixel masked 1 0)])"
        ),
        "[[255 0 0 255] [255 0 0 0]]"
    );
    assert_eq!(
        shown(
            isolate,
            "(let [tiled (image/tile halves 8 4)] [(image/width tiled) (image/height tiled) (image/pixel tiled 4 0) (image/pixel tiled 6 0)])"
        ),
        "[8 4 [255 0 0 255] [0 0 0 0]]"
    );
    assert_eq!(
        failure(isolate, "(image/paste! gray red 0 0 :blend :glow)"),
        "image/paste! takes a :blend mode such as :multiply, :screen or :overlay, not :glow"
    );
}

#[test]
fn geometry_mistakes_are_reported() {
    let mut isolate = ox_library::isolate();
    shown(&mut isolate, HALVES);

    assert_eq!(
        failure(&mut isolate, "(image/flip halves :sideways)"),
        "image/flip takes :horizontal or :vertical, not :sideways"
    );
    assert_eq!(
        failure(&mut isolate, "(image/width (image/crop halves 9 0 1 1))"),
        "image/crop starts at 9,0, outside a 4×2 image"
    );
    assert_eq!(
        failure(&mut isolate, "(image/resize halves 2 :filter :blurry)"),
        "image/resize takes :filter :nearest, :linear, :cubic, :gaussian or :lanczos, not :blurry"
    );
    assert_eq!(
        failure(&mut isolate, "(image/pixel halves 4 0)"),
        "image/pixel expects a point inside the 4×2 image, got 4 0"
    );
}
