use std::path::{Path, PathBuf};

use rodeo::animation::Frame;
use rodeo::color::Color;
use rodeo::rect::Rect;
use rodeo::tiled::{Layer, LayerKind, Map, Property, Tile, TileLayer};
use rodeo::vec::Vec2;

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
