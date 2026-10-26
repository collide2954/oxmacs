use crate::support::evaluated;

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
        "(rodeo tiled)",
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
    let mut isolate = ox_lang::Isolate::new();
    rodeo::script::install(&mut isolate);
    let load = isolate
        .eval("(module (import (rodeo tiled))) tiled/load")
        .unwrap();
    assert!(isolate.heap().native_of(load).unwrap().outside);
    let missing = evaluated("(rodeo tiled)", "(tiled/load \"no/such/level.tmx\")");
    assert!(missing.starts_with("error: can't read "), "{missing}");
}
