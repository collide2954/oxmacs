use std::path::Path;

use ox_lang::Isolate;
use ox_lang::build::{make_map, make_vector};
use ox_lang::error::Error;
use ox_lang::heap::Heap;
use ox_lang::native::{expected, field};
use ox_lang::value::Value;

use super::{
    Image, Layer, LayerKind, Map, Object, Properties, Property, Shape, Text, Tile, TileInfo,
    Tileset,
};
use crate::color::Color;
use crate::geometry::Rect;
use crate::geometry::Vec2;
use crate::geometry::{point_value, rect_value};
use crate::map_value;

const MODULE: &str = "ox/tiled";

fn text(heap: &mut Heap, value: &str) -> Result<Value, Error> {
    heap.string(value).map_err(Error::from)
}

fn path(heap: &mut Heap, value: &Path) -> Result<Value, Error> {
    text(heap, &value.display().to_string().replace('\\', "/"))
}

fn color(heap: &mut Heap, color: Color) -> Result<Value, Error> {
    let bytes = color.to_rgba8().map(|byte| Value::Int(i64::from(byte)));
    make_vector(heap, &bytes)
}

fn optional_color(heap: &mut Heap, found: Option<Color>) -> Result<Value, Error> {
    found.map_or(Ok(Value::Nil), |found| color(heap, found))
}

fn int(value: u32) -> Value {
    Value::Int(i64::from(value))
}

fn property(heap: &mut Heap, value: &Property) -> Result<Value, Error> {
    Ok(match value {
        Property::String(found) => text(heap, found)?,
        Property::Int(found) => Value::Int(*found),
        Property::Float(found) => Value::Float(*found),
        Property::Bool(found) => Value::Bool(*found),
        Property::Color(found) => color(heap, *found)?,
        Property::File(found) => path(heap, found)?,
        Property::Object(found) => int(*found),
        Property::Class(_, nested) => properties(heap, nested)?,
    })
}

fn properties(heap: &mut Heap, found: &Properties) -> Result<Value, Error> {
    let mut items = Vec::with_capacity(found.len() * 2);
    for (key, value) in found {
        items.push(text(heap, key)?);
        items.push(property(heap, value)?);
    }
    make_map(heap, &items)
}

fn image(heap: &mut Heap, found: Option<&Image>) -> Result<Value, Error> {
    let Some(found) = found else {
        return Ok(Value::Nil);
    };
    let source = path(heap, &found.source)?;
    map_value(
        heap,
        &[
            ("source", source),
            ("width", int(found.width)),
            ("height", int(found.height)),
        ],
    )
}

fn tile(heap: &mut Heap, found: Tile) -> Result<Value, Error> {
    map_value(
        heap,
        &[
            ("gid", int(found.gid)),
            ("flip-horizontal", Value::Bool(found.flip_horizontal)),
            ("flip-vertical", Value::Bool(found.flip_vertical)),
            ("flip-diagonal", Value::Bool(found.flip_diagonal)),
        ],
    )
}

fn points(heap: &mut Heap, found: &[Vec2]) -> Result<Value, Error> {
    let mut items = Vec::with_capacity(found.len());
    for spot in found {
        items.push(point_value(heap, *spot)?);
    }
    make_vector(heap, &items)
}

fn text_box(heap: &mut Heap, found: &Text) -> Result<Value, Error> {
    let content = text(heap, &found.content)?;
    let family = text(heap, &found.font_family)?;
    let paint = color(heap, found.color)?;
    let horizontal: &str = found.horizontal.into();
    let vertical: &str = found.vertical.into();
    map_value(
        heap,
        &[
            ("content", content),
            ("font-family", family),
            ("pixel-size", int(found.pixel_size)),
            ("wrap", Value::Bool(found.wrap)),
            ("color", paint),
            ("bold", Value::Bool(found.bold)),
            ("italic", Value::Bool(found.italic)),
            ("align", Value::keyword(horizontal)),
            ("valign", Value::keyword(vertical)),
        ],
    )
}

fn object(heap: &mut Heap, found: &Object) -> Result<Value, Error> {
    let (shape, outline, written) = match &found.shape {
        Shape::Rect => ("rect", Value::Nil, Value::Nil),
        Shape::Ellipse => ("ellipse", Value::Nil, Value::Nil),
        Shape::Point => ("point", Value::Nil, Value::Nil),
        Shape::Polygon(corners) => ("polygon", points(heap, corners)?, Value::Nil),
        Shape::Polyline(corners) => ("polyline", points(heap, corners)?, Value::Nil),
        Shape::Text(written) => ("text", Value::Nil, text_box(heap, written)?),
    };
    let name = text(heap, &found.name)?;
    let class = text(heap, &found.class)?;
    let placed = match found.tile {
        Some(found) => tile(heap, found)?,
        None => Value::Nil,
    };
    let props = properties(heap, &found.properties)?;
    map_value(
        heap,
        &[
            ("id", int(found.id)),
            ("name", name),
            ("class", class),
            ("x", Value::Float(found.x)),
            ("y", Value::Float(found.y)),
            ("width", Value::Float(found.width)),
            ("height", Value::Float(found.height)),
            ("rotation", Value::Float(found.rotation)),
            ("visible", Value::Bool(found.visible)),
            ("shape", Value::keyword(shape)),
            ("points", outline),
            ("text", written),
            ("tile", placed),
            ("properties", props),
        ],
    )
}

fn objects(heap: &mut Heap, found: &[Object]) -> Result<Value, Error> {
    let mut items = Vec::with_capacity(found.len());
    for each in found {
        items.push(object(heap, each)?);
    }
    make_vector(heap, &items)
}

fn layers(heap: &mut Heap, found: &[Layer], map: &Map) -> Result<Value, Error> {
    let mut items = Vec::with_capacity(found.len());
    for each in found {
        items.push(layer(heap, each, map)?);
    }
    make_vector(heap, &items)
}

fn layer(heap: &mut Heap, found: &Layer, map: &Map) -> Result<Value, Error> {
    let name = text(heap, &found.name)?;
    let class = text(heap, &found.class)?;
    let offset = point_value(heap, found.offset)?;
    let parallax = point_value(heap, found.parallax)?;
    let tint = optional_color(heap, found.tint)?;
    let props = properties(heap, &found.properties)?;
    let mut pairs = vec![
        ("id", int(found.id)),
        ("name", name),
        ("class", class),
        ("visible", Value::Bool(found.visible)),
        ("opacity", Value::Float(found.opacity)),
        ("offset", offset),
        ("parallax", parallax),
        ("tint", tint),
        ("properties", props),
    ];
    match &found.kind {
        LayerKind::Tiles(tiles) => {
            let data: Vec<Value> = tiles.data.iter().map(|raw| int(*raw)).collect();
            let data = make_vector(heap, &data)?;
            let size = make_vector(heap, &[int(map.tile_width), int(map.tile_height)])?;
            pairs.extend([
                ("kind", Value::keyword("tiles")),
                ("x", Value::Int(tiles.x)),
                ("y", Value::Int(tiles.y)),
                ("width", int(tiles.width)),
                ("height", int(tiles.height)),
                ("data", data),
                ("tile-size", size),
            ]);
        }
        LayerKind::Objects(found) => {
            let found = objects(heap, found)?;
            pairs.extend([("kind", Value::keyword("objects")), ("objects", found)]);
        }
        LayerKind::Image {
            image: picture,
            repeat_x,
            repeat_y,
        } => {
            let picture = image(heap, picture.as_ref())?;
            pairs.extend([
                ("kind", Value::keyword("image")),
                ("image", picture),
                ("repeat-x", Value::Bool(*repeat_x)),
                ("repeat-y", Value::Bool(*repeat_y)),
            ]);
        }
        LayerKind::Group(children) => {
            let children = layers(heap, children, map)?;
            pairs.extend([("kind", Value::keyword("group")), ("layers", children)]);
        }
    }
    map_value(heap, &pairs)
}

fn info(heap: &mut Heap, found: &TileInfo) -> Result<Value, Error> {
    let class = text(heap, &found.class)?;
    let props = properties(heap, &found.properties)?;
    let picture = image(heap, found.image.as_ref())?;
    let frames = match &found.animation {
        Some(animation) => {
            let mut items = Vec::with_capacity(animation.frames.len());
            for frame in &animation.frames {
                items.push(map_value(
                    heap,
                    &[
                        ("tile", Value::Int(i64::try_from(frame.region).unwrap_or(0))),
                        ("seconds", Value::Float(frame.seconds)),
                    ],
                )?);
            }
            make_vector(heap, &items)?
        }
        None => Value::Nil,
    };
    let collision = objects(heap, &found.collision)?;
    map_value(
        heap,
        &[
            ("class", class),
            ("properties", props),
            ("image", picture),
            ("animation", frames),
            ("collision", collision),
        ],
    )
}

fn tileset(heap: &mut Heap, found: &Tileset) -> Result<Value, Error> {
    let name = text(heap, &found.name)?;
    let class = text(heap, &found.class)?;
    let source = match &found.source {
        Some(source) => path(heap, source)?,
        None => Value::Nil,
    };
    let offset = point_value(heap, found.offset)?;
    let picture = image(heap, found.image.as_ref())?;
    let props = properties(heap, &found.properties)?;
    let mut tiles = Vec::with_capacity(found.tiles.len() * 2);
    for (id, each) in &found.tiles {
        tiles.push(int(*id));
        tiles.push(info(heap, each)?);
    }
    let tiles = make_map(heap, &tiles)?;
    map_value(
        heap,
        &[
            ("first-gid", int(found.first_gid)),
            ("name", name),
            ("class", class),
            ("source", source),
            ("tile-width", int(found.tile_width)),
            ("tile-height", int(found.tile_height)),
            ("spacing", int(found.spacing)),
            ("margin", int(found.margin)),
            ("tile-count", int(found.tile_count)),
            ("columns", int(found.columns)),
            ("offset", offset),
            ("image", picture),
            ("properties", props),
            ("tiles", tiles),
        ],
    )
}

fn map(heap: &mut Heap, found: &Map) -> Result<Value, Error> {
    let source = path(heap, &found.path)?;
    let background = optional_color(heap, found.background)?;
    let class = text(heap, &found.class)?;
    let props = properties(heap, &found.properties)?;
    let mut sets = Vec::with_capacity(found.tilesets.len());
    for each in &found.tilesets {
        sets.push(tileset(heap, each)?);
    }
    let sets = make_vector(heap, &sets)?;
    let all = layers(heap, &found.layers, found)?;
    map_value(
        heap,
        &[
            ("path", source),
            ("width", int(found.width)),
            ("height", int(found.height)),
            ("tile-width", int(found.tile_width)),
            ("tile-height", int(found.tile_height)),
            ("infinite", Value::Bool(found.infinite)),
            ("background", background),
            ("class", class),
            ("properties", props),
            ("tilesets", sets),
            ("layers", all),
        ],
    )
}

fn items(heap: &Heap, value: Option<Value>) -> Vec<Value> {
    value
        .and_then(|value| heap.elements(value))
        .unwrap_or_default()
}

fn named(heap: &Heap, value: Value, wanted: &str) -> bool {
    field(heap, value, "name")
        .and_then(|name| heap.text(name))
        .is_some_and(|name| name == wanted)
}

fn every_layer(heap: &Heap, value: Value, into: &mut Vec<Value>) {
    for each in items(heap, field(heap, value, "layers")) {
        into.push(each);
        every_layer(heap, each, into);
    }
}

fn every_object(heap: &Heap, level: Value) -> Vec<Value> {
    let mut all = Vec::new();
    every_layer(heap, level, &mut all);
    all.into_iter()
        .flat_map(|each| items(heap, field(heap, each, "objects")))
        .collect()
}

fn wanted_name<'a>(heap: &'a Heap, name: &str, value: Value) -> Result<&'a str, Error> {
    heap.text(value)
        .ok_or_else(|| expected(heap, name, "a name", value))
}

fn whole(heap: &Heap, value: Value, key: &str, default: i64) -> i64 {
    match field(heap, value, key) {
        Some(Value::Int(found)) => found,
        _ => default,
    }
}

fn tile_at(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let name = "tiled/tile";
    let layer = arguments[0];
    let (Value::Int(column), Value::Int(row)) = (arguments[1], arguments[2]) else {
        return Err(expected(heap, name, "a whole column and row", arguments[1]));
    };
    let (column, row) = (
        column - whole(heap, layer, "x", 0),
        row - whole(heap, layer, "y", 0),
    );
    let (width, height) = (
        whole(heap, layer, "width", 0),
        whole(heap, layer, "height", 0),
    );
    if column < 0 || row < 0 || column >= width || row >= height {
        return Ok(Value::Nil);
    }
    let data = field(heap, layer, "data")
        .and_then(|data| heap.vector_of(data))
        .ok_or_else(|| expected(heap, name, "a tile layer from tiled/load", layer))?;
    let index = usize::try_from(row * width + column).unwrap_or(usize::MAX);
    let raw = match data.get(heap, index) {
        Some(Value::Int(raw)) => u32::try_from(raw).unwrap_or(0),
        _ => 0,
    };
    match Tile::from_raw(raw) {
        Some(found) => tile(heap, found),
        None => Ok(Value::Nil),
    }
}

fn region(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let Value::Int(gid) = arguments[1] else {
        return Err(expected(heap, "tiled/region", "a tile id", arguments[1]));
    };
    let gid = gid & 0x0FFF_FFFF;
    let owner = items(heap, field(heap, arguments[0], "tilesets"))
        .into_iter()
        .filter(|set| whole(heap, *set, "first-gid", i64::MAX) <= gid)
        .max_by_key(|set| whole(heap, *set, "first-gid", 0));
    let Some(set) = owner.filter(|_| gid > 0) else {
        return Ok(Value::Nil);
    };
    let local = gid - whole(heap, set, "first-gid", 0);
    let own = field(heap, set, "tiles")
        .and_then(|tiles| heap.map_of(tiles))
        .and_then(|tiles| tiles.get(heap, Value::Int(local)))
        .and_then(|info| field(heap, info, "image"))
        .filter(|picture| *picture != Value::Nil);
    let (picture, area) = if let Some(picture) = own {
        let width = whole(heap, picture, "width", 0);
        let height = whole(heap, picture, "height", 0);
        (picture, Rect::new(0.0, 0.0, real(width), real(height)))
    } else {
        let columns = whole(heap, set, "columns", 1).max(1);
        let margin = whole(heap, set, "margin", 0);
        let spacing = whole(heap, set, "spacing", 0);
        let width = whole(heap, set, "tile-width", 0);
        let height = whole(heap, set, "tile-height", 0);
        let x = margin + (local % columns) * (width + spacing);
        let y = margin + (local / columns) * (height + spacing);
        let picture = field(heap, set, "image").unwrap_or(Value::Nil);
        (
            picture,
            Rect::new(real(x), real(y), real(width), real(height)),
        )
    };
    let source = field(heap, picture, "source").unwrap_or(Value::Nil);
    let area = rect_value(heap, area)?;
    map_value(heap, &[("image", source), ("rect", area)])
}

fn real(value: i64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "tile coordinates are far below 2^52"
    )]
    let real = value as f64;
    real
}

pub(crate) fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "load")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            let wanted = heap.text(arguments[0]).map(str::to_owned).ok_or_else(|| {
                expected(heap, "tiled/load", "the path of a .tmx file", arguments[0])
            })?;
            let loaded = Map::load(&wanted).map_err(|error| Error::new(error.to_string()))?;
            map(heap, &loaded)
        });
    isolate
        .native(MODULE, "layer")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let wanted = wanted_name(heap, "tiled/layer", arguments[1])?.to_owned();
            let mut all = Vec::new();
            every_layer(heap, arguments[0], &mut all);
            Ok(all
                .into_iter()
                .find(|each| named(heap, *each, &wanted))
                .unwrap_or(Value::Nil))
        });
    isolate
        .native(MODULE, "object")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let wanted = wanted_name(heap, "tiled/object", arguments[1])?.to_owned();
            Ok(every_object(heap, arguments[0])
                .into_iter()
                .find(|each| named(heap, *each, &wanted))
                .unwrap_or(Value::Nil))
        });
    isolate
        .native(MODULE, "objects")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let all = every_object(heap, arguments[0]);
            make_vector(heap, &all)
        });
    isolate
        .native(MODULE, "tile")
        .arity(3, Some(3))
        .function(tile_at);
    isolate
        .native(MODULE, "region")
        .arity(2, Some(2))
        .function(region);
}
