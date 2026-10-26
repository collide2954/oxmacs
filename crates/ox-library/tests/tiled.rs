use std::path::{Path, PathBuf};

use ox_library::animation::Frame;
use ox_library::color::Color;
use ox_library::geometry::Rect;
use ox_library::geometry::Vec2;
use ox_library::tiled::{Layer, LayerKind, Map, Property, Tile, TileLayer};

use crate::support::evaluated;

const MEADOW: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<map version="1.10" tiledversion="1.11.0" orientation="orthogonal" renderorder="right-down" width="3" height="2" tilewidth="16" tileheight="16" infinite="0" backgroundcolor="#80ff0000" nextlayerid="2" nextobjectid="1">
 <properties>
  <property name="title" value="Meadow"/>
  <property name="lives" type="int" value="3"/>
  <property name="gravity" type="float" value="9.5"/>
  <property name="dark" type="bool" value="true"/>
  <property name="tint" type="color" value="#ff00ff00"/>
  <property name="music" type="file" value="music/meadow.ogg"/>
  <property name="boss" type="object" value="12"/>
  <property name="note">two
lines</property>
 </properties>
 <tileset firstgid="1" name="terrain" tilewidth="16" tileheight="16" spacing="1" margin="2" tilecount="6" columns="3">
  <image source="terrain.png" width="55" height="38"/>
  <tile id="4" type="spike">
   <properties>
    <property name="solid" type="bool" value="true"/>
   </properties>
   <animation>
    <frame tileid="4" duration="100"/>
    <frame tileid="5" duration="150"/>
   </animation>
  </tile>
 </tileset>
 <layer id="1" name="ground" width="3" height="2" opacity="0.5" visible="0" offsetx="4" offsety="-2">
  <data encoding="csv">
0,1,2,
2147483653,0,6
</data>
 </layer>
</map>
"##;

fn meadow() -> Map {
    Map::parse(MEADOW, Path::new("levels/meadow.tmx")).unwrap()
}

fn tiles(layer: &Layer) -> &TileLayer {
    match &layer.kind {
        LayerKind::Tiles(tiles) => tiles,
        other => panic!("expected tiles, got {other:?}"),
    }
}

#[test]
fn maps_read_their_size_background_and_properties() {
    let map = meadow();
    assert_eq!([map.width, map.height], [3, 2]);
    assert_eq!(map.tile_size(), Vec2::new(16.0, 16.0));
    assert_eq!(map.background, Some(Color::rgba8(255, 0, 0, 128)));
    let properties = &map.properties;
    assert_eq!(properties["title"], Property::String("Meadow".into()));
    assert_eq!(properties["lives"], Property::Int(3));
    assert_eq!(properties["gravity"], Property::Float(9.5));
    assert_eq!(properties["dark"], Property::Bool(true));
    assert_eq!(
        properties["tint"],
        Property::Color(Color::rgba8(0, 255, 0, 255))
    );
    assert_eq!(
        properties["music"],
        Property::File(PathBuf::from("levels/music/meadow.ogg"))
    );
    assert_eq!(properties["boss"], Property::Object(12));
    assert_eq!(properties["note"], Property::String("two\nlines".into()));
}

#[test]
fn tile_layers_hold_tiles_with_their_flips() {
    let map = meadow();
    let layer = map.layer("ground").unwrap();
    assert_eq!(layer.opacity, 0.5);
    assert!(!layer.visible);
    assert_eq!(layer.offset, Vec2::new(4.0, -2.0));
    let ground = tiles(layer);
    assert_eq!(ground.get(0, 0), None);
    assert_eq!(ground.get(1, 0), Some(Tile::new(1)));
    assert_eq!(
        ground.get(0, 1),
        Some(Tile {
            gid: 5,
            flip_horizontal: true,
            flip_vertical: false,
            flip_diagonal: false,
        })
    );
    assert_eq!(ground.get(2, 1), Some(Tile::new(6)));
    assert_eq!(ground.get(3, 0), None);
    assert_eq!(ground.get(-1, 0), None);
}

#[test]
fn tilesets_find_regions_properties_and_animations() {
    let map = meadow();
    let (tileset, local) = map.tileset_for(5).unwrap();
    assert_eq!((tileset.name.as_str(), local), ("terrain", 4));
    assert_eq!(
        tileset.image.as_ref().map(|image| image.source.clone()),
        Some(PathBuf::from("levels/terrain.png"))
    );
    assert_eq!(tileset.region(4), Some(Rect::new(19.0, 19.0, 16.0, 16.0)));
    assert_eq!(tileset.region(6), None);
    let spike = &tileset.tiles[&4];
    assert_eq!(spike.class, "spike");
    assert_eq!(spike.properties["solid"], Property::Bool(true));
    let animation = spike.animation.as_ref().unwrap();
    assert_eq!(
        animation.frames,
        [
            Frame {
                region: 4,
                seconds: 0.1
            },
            Frame {
                region: 5,
                seconds: 0.15
            }
        ]
    );
    assert_eq!(map.tile_properties(5)["solid"], Property::Bool(true));
    assert!(map.tile_properties(2).is_empty());
    assert!(map.tileset_for(0).is_none());
}

const LEVEL: &str = r##"<map orientation="orthogonal" width="2" height="1" tilewidth="16" tileheight="16" backgroundcolor="#80ff0000">
 <properties>
  <property name="title" value="Meadow"/>
  <property name="lives" type="int" value="3"/>
  <property name="tint" type="color" value="#ff00ff00"/>
 </properties>
 <tileset firstgid="1" name="terrain" tilewidth="16" tileheight="16" tilecount="4" columns="2">
  <image source="terrain.png" width="32" height="32"/>
  <tile id="1" type="spike">
   <properties>
    <property name="solid" type="bool" value="true"/>
   </properties>
   <animation>
    <frame tileid="1" duration="100"/>
    <frame tileid="2" duration="150"/>
   </animation>
  </tile>
 </tileset>
 <layer id="1" name="ground" width="2" height="1">
  <data encoding="csv">2,2147483651</data>
 </layer>
 <group id="3" name="world">
  <objectgroup id="2" name="things">
   <object id="1" name="start" type="spawn" x="10" y="20" width="16" height="32" rotation="90">
    <properties>
     <property name="facing" value="left"/>
    </properties>
   </object>
   <object id="2" name="path" x="0" y="0"><polyline points="0,0 4,4"/></object>
  </objectgroup>
 </group>
</map>"##;

fn tiled(source: &str) -> String {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("level.tmx");
    std::fs::write(&path, LEVEL).unwrap();
    let shown = path.display().to_string().replace('\\', "/");
    evaluated(
        "(ox tiled)",
        &format!("(def level (tiled/load \"{shown}\")) {source}"),
    )
}

#[test]
fn maps_load_as_plain_data() {
    assert_eq!(
        tiled(
            r#"[level.width level.height level.tile-width (get level.properties "title") (get level.properties "lives") (get level.properties "tint") level.background]"#
        ),
        r#"[2 1 16 "Meadow" 3 [0 255 0 255] [255 0 0 128]]"#
    );
}

#[test]
fn layers_hold_tiles_with_their_flips() {
    assert_eq!(
        tiled(
            "(let [ground (tiled/layer level \"ground\") t (tiled/tile ground 1 0)] [ground.kind ground.data ground.tile-size t.gid t.flip-horizontal (tiled/tile ground 5 0)])"
        ),
        "[:tiles [2 2147483651] [16 16] 3 true nil]"
    );
    assert_eq!(tiled("(get (tiled/layer level \"world\") :kind)"), ":group");
}

#[test]
fn objects_keep_their_shapes_and_properties() {
    assert_eq!(
        tiled(
            r#"(let [start (tiled/object level "start")] [start.class start.x start.rotation (get start.properties "facing") start.shape])"#
        ),
        r#"["spawn" 10.0 1.5707963267948966 "left" :rect]"#
    );
    assert_eq!(
        tiled(r#"(get (tiled/object level "path") :points)"#),
        "[[0.0 0.0] [4.0 4.0]]"
    );
    assert_eq!(tiled("(count (tiled/objects level))"), "2");
}

#[test]
fn tilesets_give_regions_properties_and_animations() {
    assert_eq!(
        tiled(
            r#"(let [ts (first level.tilesets) spike (get ts.tiles 1) frame (first spike.animation)] [ts.name spike.class (get spike.properties "solid") (count spike.animation) frame.tile frame.seconds])"#
        ),
        r#"["terrain" "spike" true 2 1 0.1]"#
    );
    assert_eq!(
        tiled("(get (tiled/region level 2) :rect)"),
        "[16.0 0.0 16.0 16.0]"
    );
    assert_eq!(tiled("(tiled/region level 0)"), "nil");
}

#[test]
fn loading_reaches_outside_and_explains_failures() {
    let mut isolate = ox_library::isolate();
    let load = isolate
        .eval("(module (import (ox tiled))) tiled/load")
        .unwrap();
    assert!(isolate.heap().native_of(load).unwrap().outside);
    let missing = evaluated("(ox tiled)", "(tiled/load \"no/such/level.tmx\")");
    assert!(missing.starts_with("error: can't read "), "{missing}");
}
