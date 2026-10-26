use ox_library::color::Color;

#[test]
fn colors_come_from_bytes_and_go_back() {
    let orange = Color::rgba8(255, 128, 0, 255);
    assert_eq!(orange.r, 1.0);
    assert!((orange.g - 128.0 / 255.0).abs() < 1e-6);
    assert_eq!(orange.to_rgba8(), [255, 128, 0, 255]);
    assert_eq!(Color::WHITE.to_rgba8(), [255; 4]);
    assert_eq!(Color::TRANSPARENT.a, 0.0);
}

#[test]
fn colors_parse_from_hex() {
    assert_eq!(Color::hex("#ff8000"), Some(Color::rgba8(255, 128, 0, 255)));
    assert_eq!(
        Color::hex("#ff800080"),
        Some(Color::rgba8(255, 128, 0, 128))
    );
    assert_eq!(Color::hex("#f80"), Some(Color::rgba8(255, 136, 0, 255)));
    assert_eq!(Color::hex("#f808"), Some(Color::rgba8(255, 136, 0, 136)));
    assert_eq!(Color::hex("ff8000"), None);
    assert_eq!(Color::hex("#ff80"), Some(Color::rgba8(255, 255, 136, 0)));
    assert_eq!(Color::hex("#gg0000"), None);
}

#[test]
fn colors_mix_and_multiply() {
    let mixed = Color::BLACK.lerp(Color::WHITE, 0.5);
    assert_eq!(mixed.to_rgba8(), [128, 128, 128, 255]);
    let tinted = Color::rgba8(255, 255, 255, 255) * Color::rgba8(255, 0, 0, 128);
    assert_eq!(tinted.to_rgba8(), [255, 0, 0, 128]);
}
