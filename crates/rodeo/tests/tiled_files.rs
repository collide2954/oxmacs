use std::fs;
use std::path::Path;

use rodeo::rect::Rect;
use rodeo::tiled::{Map, Property, Tile};

const TERRAIN: &str = r#"<tileset version="1.10" name="terrain" tilewidth="16" tileheight="16" tilecount="4" columns="2">
 <image source="terrain.png" width="32" height="32"/>
 <tile id="3">
  <properties>
   <property name="solid" type="bool" value="true"/>
  </properties>
 </tile>
</tileset>"#;

const COIN: &str = r#"<template>
 <tileset firstgid="1" source="../tiles/terrain.tsx"/>
 <object name="coin" type="pickup" gid="4" width="16" height="16">
  <properties>
   <property name="value" type="int" value="1"/>
   <property name="sound" type="file" value="coin.wav"/>
  </properties>
 </object>
</template>"#;

const LEVEL: &str = r#"<map orientation="orthogonal" width="2" height="1" tilewidth="16" tileheight="16">
 <tileset firstgid="1" name="other" tilewidth="16" tileheight="16" tilecount="10" columns="5">
  <image source="other.png" width="80" height="32"/>
 </tileset>
 <tileset firstgid="11" source="../tiles/terrain.tsx"/>
 <layer id="1" name="ground" width="2" height="1">
  <data encoding="csv">11,14</data>
 </layer>
 <objectgroup id="2" name="things">
  <object id="1" template="../templates/coin.tx" x="32" y="48"/>
  <object id="2" template="../templates/coin.tx" name="big coin" x="64" y="48">
   <properties>
    <property name="value" type="int" value="5"/>
   </properties>
  </object>
 </objectgroup>
</map>"#;

fn write(folder: &Path, name: &str, text: &str) {
    let path = folder.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn level(folder: &Path) -> Map {
    write(folder, "tiles/terrain.tsx", TERRAIN);
    write(folder, "templates/coin.tx", COIN);
    write(folder, "levels/one.tmx", LEVEL);
    Map::load(folder.join("levels/one.tmx")).unwrap()
}

#[test]
fn external_tilesets_load_beside_their_files() {
    let folder = tempfile::tempdir().unwrap();
    let map = level(folder.path());
    let terrain = &map.tilesets[1];
    assert_eq!((terrain.name.as_str(), terrain.first_gid), ("terrain", 11));
    assert_eq!(
        terrain.source.as_deref(),
        Some(folder.path().join("tiles").join("terrain.tsx").as_path())
    );
    assert_eq!(
        terrain.image.as_ref().map(|image| image.source.clone()),
        Some(folder.path().join("tiles").join("terrain.png"))
    );
    assert_eq!(map.tile_properties(14)["solid"], Property::Bool(true));
}

#[test]
fn templates_fill_in_objects_that_use_them() {
    let folder = tempfile::tempdir().unwrap();
    let map = level(folder.path());
    let coin = map.object("coin").unwrap();
    assert_eq!(coin.class, "pickup");
    assert_eq!(coin.tile, Some(Tile::new(14)));
    assert_eq!(coin.rect(), Rect::new(32.0, 32.0, 16.0, 16.0));
    assert_eq!(coin.properties["value"], Property::Int(1));
    assert_eq!(
        coin.properties["sound"],
        Property::File(folder.path().join("templates").join("coin.wav"))
    );
    let big = map.object("big coin").unwrap();
    assert_eq!(big.id, 2);
    assert_eq!(big.properties["value"], Property::Int(5));
    assert_eq!(big.tile, Some(Tile::new(14)));
}

#[test]
fn missing_files_and_other_orientations_are_errors() {
    let folder = tempfile::tempdir().unwrap();
    let missing = Map::load(folder.path().join("nowhere.tmx")).unwrap_err();
    assert!(missing.to_string().starts_with("can't read "));
    let iso =
        r#"<map orientation="isometric" width="1" height="1" tilewidth="16" tileheight="8"/>"#;
    assert_eq!(
        Map::parse(iso, Path::new("iso.tmx"))
            .unwrap_err()
            .to_string(),
        "iso.tmx: uses isometric tiles, but rodeo loads only orthogonal maps"
    );
    assert_eq!(
        Map::parse("<tileset/>", Path::new("odd.tmx"))
            .unwrap_err()
            .to_string(),
        "odd.tmx: isn't a Tiled map"
    );
}
