use std::f64::consts::FRAC_PI_2;
use std::path::{Path, PathBuf};

use ox_library::color::Color;
use ox_library::geometry::Rect;
use ox_library::geometry::Vec2;

use ox_library::tiled::{
    HorizontalAlign, Image, LayerKind, Map, Object, Property, Shape, Text, Tile, VerticalAlign,
};

const WORLD: &str = r##"<map orientation="orthogonal" width="4" height="4" tilewidth="16" tileheight="16">
 <tileset firstgid="1" name="items" tilewidth="16" tileheight="16" tilecount="4" columns="2">
  <image source="items.png" width="32" height="32"/>
  <tile id="1">
   <objectgroup draworder="index">
    <object id="1" x="2" y="4" width="12" height="12"/>
   </objectgroup>
  </tile>
 </tileset>
 <group id="5" name="world" opacity="0.8">
  <objectgroup id="2" name="things" color="#ff0000">
   <object id="1" name="start" type="spawn" x="10" y="20" width="16" height="32" rotation="90">
    <properties>
     <property name="facing" value="left"/>
    </properties>
   </object>
   <object id="2" name="pond" x="40" y="40" width="20" height="10"><ellipse/></object>
   <object id="3" x="5" y="6"><point/></object>
   <object id="4" x="0" y="0"><polygon points="0,0 10,0 5,8"/></object>
   <object id="5" x="1" y="2"><polyline points="0,0 4,4"/></object>
   <object id="6" name="coin" gid="2" x="32" y="48" width="16" height="16"/>
   <object id="7" x="0" y="64" width="80" height="20">
    <text fontfamily="Pixel" pixelsize="12" wrap="1" color="#ff112233" bold="1" halign="center" valign="bottom">Hello there</text>
   </object>
   <object id="8" x="0" y="0" width="4" height="4" visible="0"/>
  </objectgroup>
  <imagelayer id="3" name="sky" repeatx="1">
   <image source="sky.png" width="320" height="180"/>
  </imagelayer>
 </group>
</map>"##;

fn world() -> Map {
    Map::parse(WORLD, Path::new("world.tmx")).unwrap()
}

fn things(map: &Map) -> &[Object] {
    match &map.layer("things").unwrap().kind {
        LayerKind::Objects(objects) => objects,
        other => panic!("expected objects, got {other:?}"),
    }
}

#[test]
fn groups_nest_layers() {
    let map = world();
    let group = &map.layers[0];
    assert_eq!(group.opacity, 0.8);
    let LayerKind::Group(children) = &group.kind else {
        panic!("expected a group");
    };
    let names: Vec<&str> = children.iter().map(|layer| layer.name.as_str()).collect();
    assert_eq!(names, ["things", "sky"]);
    assert_eq!(
        map.layer("sky").unwrap().kind,
        LayerKind::Image {
            image: Some(Image {
                source: PathBuf::from("sky.png"),
                width: 320,
                height: 180,
            }),
            repeat_x: true,
            repeat_y: false,
        }
    );
}

#[test]
fn objects_keep_their_place_class_rotation_and_properties() {
    let map = world();
    let start = &things(&map)[0];
    assert_eq!((start.id, start.name.as_str()), (1, "start"));
    assert_eq!(start.class, "spawn");
    assert_eq!(start.rect(), Rect::new(10.0, 20.0, 16.0, 32.0));
    assert_eq!(start.rotation, FRAC_PI_2);
    assert_eq!(start.properties["facing"], Property::String("left".into()));
    assert_eq!(start.shape, Shape::Rect);
    assert!(start.visible);
    assert!(!things(&map)[7].visible);
    assert_eq!(map.object("pond").map(|pond| pond.id), Some(2));
    assert_eq!(map.objects().count(), 8);
}

#[test]
fn objects_come_in_every_shape() {
    let map = world();
    let objects = things(&map);
    assert_eq!(objects[1].shape, Shape::Ellipse);
    assert_eq!(objects[2].shape, Shape::Point);
    assert_eq!(
        objects[3].shape,
        Shape::Polygon(vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(5.0, 8.0)
        ])
    );
    assert_eq!(
        objects[4].shape,
        Shape::Polyline(vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 4.0)])
    );
    assert_eq!(
        objects[6].shape,
        Shape::Text(Text {
            content: "Hello there".into(),
            font_family: "Pixel".into(),
            pixel_size: 12,
            wrap: true,
            color: Color::rgba8(0x11, 0x22, 0x33, 0xff),
            bold: true,
            italic: false,
            horizontal: HorizontalAlign::Center,
            vertical: VerticalAlign::Bottom,
        })
    );
}

#[test]
fn tile_objects_move_to_their_top_left_corner() {
    let map = world();
    let coin = map.object("coin").unwrap();
    assert_eq!(coin.tile, Some(Tile::new(2)));
    assert_eq!(coin.rect(), Rect::new(32.0, 32.0, 16.0, 16.0));
}

#[test]
fn tiles_carry_collision_shapes() {
    let map = world();
    let collision = &map.tile_info(2).unwrap().collision;
    assert_eq!(collision[0].rect(), Rect::new(2.0, 4.0, 12.0, 12.0));
}
