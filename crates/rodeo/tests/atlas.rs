use image::RgbaImage;
use ox_library::animation::{Animation, Direction, Frame};
use ox_library::geometry::Rect;
use ox_library::geometry::Vec2;
use rodeo::atlas::{Atlas, Region};
use rodeo::texture::Textures;

const HASH: &str = r##"{
 "frames": {
  "idle": { "frame": { "x": 0, "y": 0, "w": 16, "h": 16 }, "rotated": false, "trimmed": false, "spriteSourceSize": { "x": 0, "y": 0, "w": 16, "h": 16 }, "sourceSize": { "w": 16, "h": 16 }, "duration": 100 },
  "run a": { "frame": { "x": 16, "y": 0, "w": 12, "h": 14 }, "rotated": false, "trimmed": true, "spriteSourceSize": { "x": 2, "y": 1, "w": 12, "h": 14 }, "sourceSize": { "w": 16, "h": 16 }, "duration": 150 },
  "a last": { "frame": { "x": 32, "y": 0, "w": 16, "h": 16 }, "rotated": false, "trimmed": false, "spriteSourceSize": { "x": 0, "y": 0, "w": 16, "h": 16 }, "sourceSize": { "w": 16, "h": 16 }, "duration": 100 }
 },
 "meta": {
  "app": "https://www.aseprite.org/",
  "image": "hero.png",
  "format": "RGBA8888",
  "size": { "w": 48, "h": 16 },
  "scale": "1",
  "frameTags": [
   { "name": "run", "from": 1, "to": 2, "direction": "pingpong", "color": "#000000ff" },
   { "name": "idle", "from": 0, "to": 0, "direction": "forward", "repeat": "2", "color": "#000000ff" }
  ]
 }
}"##;

const ARRAY: &str = r#"{
 "frames": [
  { "filename": "idle", "frame": { "x": 0, "y": 0, "w": 16, "h": 16 }, "duration": 100 },
  { "filename": "run a", "frame": { "x": 16, "y": 0, "w": 12, "h": 14 }, "spriteSourceSize": { "x": 2, "y": 1, "w": 12, "h": 14 }, "sourceSize": { "w": 16, "h": 16 }, "duration": 150 },
  { "filename": "a last", "frame": { "x": 32, "y": 0, "w": 16, "h": 16 }, "duration": 100 }
 ],
 "meta": { "image": "hero.png", "frameTags": [ { "name": "run", "from": 1, "to": 2, "direction": "pingpong" } ] }
}"#;

#[test]
fn grids_cut_textures_into_cells() {
    let mut textures = Textures::new();
    let sheet = textures.insert(64, 32, vec![0; 64 * 32 * 4]).unwrap();
    let atlas = Atlas::grid(sheet, 64, 32, 16, 16, 0, 0);
    assert_eq!(atlas.regions.len(), 8);
    assert_eq!(atlas.regions[5].rect, Rect::new(16.0, 16.0, 16.0, 16.0));
    assert_eq!(atlas.regions[5].size, Vec2::new(16.0, 16.0));
    let spaced = Atlas::grid(sheet, 40, 22, 16, 16, 1, 2);
    let rects: Vec<Rect> = spaced.regions.iter().map(|region| region.rect).collect();
    assert_eq!(
        rects,
        [
            Rect::new(1.0, 1.0, 16.0, 16.0),
            Rect::new(19.0, 1.0, 16.0, 16.0)
        ]
    );
}

#[test]
fn aseprite_sheets_keep_frame_order_trims_and_durations() {
    let mut textures = Textures::new();
    let sheet = textures.insert(48, 16, vec![0; 48 * 16 * 4]).unwrap();
    for json in [HASH, ARRAY] {
        let atlas = Atlas::aseprite(sheet, json).unwrap();
        assert_eq!(atlas.regions.len(), 3);
        assert_eq!(
            atlas.regions[1],
            Region {
                rect: Rect::new(16.0, 0.0, 12.0, 14.0),
                offset: Vec2::new(2.0, 1.0),
                size: Vec2::new(16.0, 16.0),
                seconds: 0.15,
            }
        );
        assert_eq!(atlas.named("run a"), Some(1));
        assert_eq!(atlas.named("a last"), Some(2));
        assert_eq!(
            atlas.animation("run"),
            Some(Animation {
                frames: vec![
                    Frame {
                        region: 1,
                        seconds: 0.15
                    },
                    Frame {
                        region: 2,
                        seconds: 0.1
                    }
                ],
                direction: Direction::PingPong,
                repeat: None,
            })
        );
    }
    let atlas = Atlas::aseprite(sheet, HASH).unwrap();
    assert_eq!(atlas.tag("idle").map(|tag| tag.repeat), Some(Some(2)));
    assert_eq!(atlas.animation("missing"), None);
}

#[test]
fn bad_sheets_explain_themselves() {
    let mut textures = Textures::new();
    let sheet = textures.insert(1, 1, vec![0; 4]).unwrap();
    assert!(
        Atlas::aseprite(sheet, "{")
            .unwrap_err()
            .to_string()
            .starts_with("the sprite sheet isn't valid JSON")
    );
    let wrong = r#"{"frames": [], "meta": {"frameTags": [{"name": "run", "from": 0, "to": 3}]}}"#;
    assert_eq!(
        Atlas::aseprite(sheet, wrong).unwrap_err().to_string(),
        "the tag run covers frames 0 to 3, but the sheet has 0 frames"
    );
}

#[test]
fn aseprite_files_load_their_image() {
    let folder = tempfile::tempdir().unwrap();
    RgbaImage::new(48, 16)
        .save(folder.path().join("hero.png"))
        .unwrap();
    std::fs::write(folder.path().join("hero.json"), HASH).unwrap();
    let mut textures = Textures::new();
    let atlas = Atlas::load_aseprite(&mut textures, folder.path().join("hero.json")).unwrap();
    assert_eq!(
        Some(atlas.texture),
        textures.id(folder.path().join("hero.png"))
    );
    assert_eq!(atlas.regions.len(), 3);
}
