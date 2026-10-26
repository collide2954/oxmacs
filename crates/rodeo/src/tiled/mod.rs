mod data;
mod parse;

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::animation::Animation;
use crate::color::Color;
use crate::rect::Rect;
use crate::vec::Vec2;

const FLIP_HORIZONTAL: u32 = 0x8000_0000;
const FLIP_VERTICAL: u32 = 0x4000_0000;
const FLIP_DIAGONAL: u32 = 0x2000_0000;
const ROTATE_HEXAGONAL: u32 = 0x1000_0000;
const FLAGS: u32 = FLIP_HORIZONTAL | FLIP_VERTICAL | FLIP_DIAGONAL | ROTATE_HEXAGONAL;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("can't read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("{} isn't valid XML: {source}", path.display())]
    Xml {
        path: PathBuf,
        source: roxmltree::Error,
    },
    #[error("{}: {message}", path.display())]
    Invalid { path: PathBuf, message: String },
}

pub type Properties = BTreeMap<String, Property>;

#[derive(Clone, Debug, PartialEq)]
pub enum Property {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Color(Color),
    File(PathBuf),
    Object(u32),
    Class(String, Properties),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Tile {
    pub gid: u32,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    pub flip_diagonal: bool,
}

impl Tile {
    pub const fn new(gid: u32) -> Self {
        Self {
            gid,
            flip_horizontal: false,
            flip_vertical: false,
            flip_diagonal: false,
        }
    }

    pub fn from_raw(raw: u32) -> Option<Self> {
        let gid = raw & !FLAGS;
        (gid != 0).then_some(Self {
            gid,
            flip_horizontal: raw & FLIP_HORIZONTAL != 0,
            flip_vertical: raw & FLIP_VERTICAL != 0,
            flip_diagonal: raw & FLIP_DIAGONAL != 0,
        })
    }

    pub fn raw(self) -> u32 {
        let flag = |on: bool, bit: u32| if on { bit } else { 0 };
        self.gid
            | flag(self.flip_horizontal, FLIP_HORIZONTAL)
            | flag(self.flip_vertical, FLIP_VERTICAL)
            | flag(self.flip_diagonal, FLIP_DIAGONAL)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub source: PathBuf,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TileInfo {
    pub class: String,
    pub properties: Properties,
    pub image: Option<Image>,
    pub animation: Option<Animation>,
    pub collision: Vec<Object>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tileset {
    pub first_gid: u32,
    pub name: String,
    pub class: String,
    pub source: Option<PathBuf>,
    pub tile_width: u32,
    pub tile_height: u32,
    pub spacing: u32,
    pub margin: u32,
    pub tile_count: u32,
    pub columns: u32,
    pub offset: Vec2,
    pub image: Option<Image>,
    pub properties: Properties,
    pub tiles: BTreeMap<u32, TileInfo>,
}

impl Tileset {
    pub fn tile_size(&self) -> Vec2 {
        Vec2::new(f64::from(self.tile_width), f64::from(self.tile_height))
    }

    pub fn region(&self, local: u32) -> Option<Rect> {
        if local >= self.tile_count {
            return None;
        }
        if let Some(image) = self.tiles.get(&local).and_then(|tile| tile.image.as_ref()) {
            return Some(Rect::new(
                0.0,
                0.0,
                f64::from(image.width),
                f64::from(image.height),
            ));
        }
        let columns = self.columns.max(1);
        let place = |index: u32, size: u32| {
            f64::from(self.margin) + f64::from(index) * f64::from(size + self.spacing)
        };
        Some(Rect::new(
            place(local % columns, self.tile_width),
            place(local / columns, self.tile_height),
            f64::from(self.tile_width),
            f64::from(self.tile_height),
        ))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TileLayer {
    pub x: i64,
    pub y: i64,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u32>,
}

impl TileLayer {
    pub fn get(&self, column: i64, row: i64) -> Option<Tile> {
        let column = u32::try_from(column - self.x).ok()?;
        let row = u32::try_from(row - self.y).ok()?;
        if column >= self.width || row >= self.height {
            return None;
        }
        let index =
            usize::try_from(u64::from(row) * u64::from(self.width) + u64::from(column)).ok()?;
        Tile::from_raw(*self.data.get(index)?)
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, strum::EnumString, strum::IntoStaticStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum HorizontalAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, strum::EnumString, strum::IntoStaticStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum VerticalAlign {
    #[default]
    Top,
    Center,
    Bottom,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub content: String,
    pub font_family: String,
    pub pixel_size: u32,
    pub wrap: bool,
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
    pub horizontal: HorizontalAlign,
    pub vertical: VerticalAlign,
}

impl Default for Text {
    fn default() -> Self {
        Self {
            content: String::new(),
            font_family: "sans-serif".to_owned(),
            pixel_size: 16,
            wrap: false,
            color: Color::BLACK,
            bold: false,
            italic: false,
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Top,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Shape {
    #[default]
    Rect,
    Ellipse,
    Point,
    Polygon(Vec<Vec2>),
    Polyline(Vec<Vec2>),
    Text(Text),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    pub id: u32,
    pub name: String,
    pub class: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    pub visible: bool,
    pub tile: Option<Tile>,
    pub shape: Shape,
    pub properties: Properties,
}

impl Default for Object {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            class: String::new(),
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            rotation: 0.0,
            visible: true,
            tile: None,
            shape: Shape::Rect,
            properties: Properties::new(),
        }
    }
}

impl Object {
    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayerKind {
    Tiles(TileLayer),
    Objects(Vec<Object>),
    Image {
        image: Option<Image>,
        repeat_x: bool,
        repeat_y: bool,
    },
    Group(Vec<Layer>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub id: u32,
    pub name: String,
    pub class: String,
    pub visible: bool,
    pub opacity: f64,
    pub offset: Vec2,
    pub parallax: Vec2,
    pub tint: Option<Color>,
    pub properties: Properties,
    pub kind: LayerKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Map {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    pub infinite: bool,
    pub background: Option<Color>,
    pub class: String,
    pub properties: Properties,
    pub tilesets: Vec<Tileset>,
    pub layers: Vec<Layer>,
}

fn find<'a>(layers: &'a [Layer], name: &str) -> Option<&'a Layer> {
    layers.iter().find_map(|layer| {
        if layer.name == name {
            return Some(layer);
        }
        match &layer.kind {
            LayerKind::Group(children) => find(children, name),
            _ => None,
        }
    })
}

impl Map {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let text = parse::read(path)?;
        Self::parse(&text, path)
    }

    pub fn parse(text: &str, path: &Path) -> Result<Self, Error> {
        parse::map(text, path)
    }

    pub fn tile_size(&self) -> Vec2 {
        Vec2::new(f64::from(self.tile_width), f64::from(self.tile_height))
    }

    pub fn layer(&self, name: &str) -> Option<&Layer> {
        find(&self.layers, name)
    }

    pub fn tileset_for(&self, gid: u32) -> Option<(&Tileset, u32)> {
        let gid = gid & !FLAGS;
        if gid == 0 {
            return None;
        }
        let tileset = self
            .tilesets
            .iter()
            .filter(|tileset| tileset.first_gid <= gid)
            .max_by_key(|tileset| tileset.first_gid)?;
        Some((tileset, gid - tileset.first_gid))
    }

    pub fn tile_info(&self, gid: u32) -> Option<&TileInfo> {
        let (tileset, local) = self.tileset_for(gid)?;
        tileset.tiles.get(&local)
    }

    pub fn tile_properties(&self, gid: u32) -> &Properties {
        static EMPTY: Properties = Properties::new();
        self.tile_info(gid).map_or(&EMPTY, |info| &info.properties)
    }
}
