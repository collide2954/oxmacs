use ox_lang::native::{expected, field, number};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::BTreeSet;

use ox_lang::Isolate;
use ox_lang::build::make_vector;
use ox_lang::error::Error;
use ox_lang::heap::Heap;
use ox_lang::symbol::Sym;
use ox_lang::value::Value;

use super::{map_value, point, point_value, rect, rect_value};
use crate::collide::{Circle, Grid, Moved, move_in_tiles, penetration, slide, sweep};
use crate::rect::Rect;
use crate::vec::Vec2;

const MODULE: &str = "rodeo/collide";
const KIND: &str = "collision-grid";
const FLAGS: i64 = 0xF000_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Key {
    Int(i64),
    Word(Sym),
}

impl Ord for Key {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a.cmp(b),
            (Self::Int(_), Self::Word(_)) => Ordering::Less,
            (Self::Word(_), Self::Int(_)) => Ordering::Greater,
            (Self::Word(a), Self::Word(b)) => a.name().cmp(b.name()),
        }
    }
}

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Key {
    fn read(heap: &Heap, name: &str, value: Value) -> Result<Self, Error> {
        match value {
            Value::Int(number) => Ok(Self::Int(number)),
            Value::Keyword(word) => Ok(Self::Word(word)),
            _ => Err(expected(
                heap,
                name,
                "an id: a whole number or a keyword",
                value,
            )),
        }
    }

    fn value(self) -> Value {
        match self {
            Self::Int(number) => Value::Int(number),
            Self::Word(word) => Value::Keyword(word),
        }
    }
}

type Handle = RefCell<Grid<Key>>;

fn grid<'a>(heap: &'a Heap, name: &str, value: Value) -> Result<&'a Handle, Error> {
    heap.foreign_of::<Handle>(value)
        .ok_or_else(|| expected(heap, name, "a grid from collide/grid", value))
}

enum Body {
    Box(Rect),
    Round(Circle),
}

fn body(heap: &Heap, name: &str, value: Value) -> Result<Body, Error> {
    let what = "a rect as [x y w h], a circle as [x y r], or a map of either";
    let circle = |x: Value, y: Value, r: Value| -> Result<Body, Error> {
        Ok(Body::Round(Circle::new(
            Vec2::new(number(heap, name, x)?, number(heap, name, y)?),
            number(heap, name, r)?,
        )))
    };
    if let Some(items) = heap.vector_of(value).map(|vector| vector.items(heap)) {
        return match items.as_slice() {
            [x, y, r] => circle(*x, *y, *r),
            [_, _, _, _] => Ok(Body::Box(rect(heap, name, value)?)),
            _ => Err(expected(heap, name, what, value)),
        };
    }
    match (
        field(heap, value, "x"),
        field(heap, value, "y"),
        field(heap, value, "radius"),
    ) {
        (Some(x), Some(y), Some(r)) => circle(x, y, r),
        _ => rect(heap, name, value)
            .map(Body::Box)
            .map_err(|_| expected(heap, name, what, value)),
    }
}

fn overlaps(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let a = body(heap, "collide/overlaps?", arguments[0])?;
    let b = body(heap, "collide/overlaps?", arguments[1])?;
    Ok(Value::Bool(match (a, b) {
        (Body::Box(a), Body::Box(b)) => a.overlaps(b),
        (Body::Round(a), Body::Round(b)) => a.overlaps(b),
        (Body::Round(round), Body::Box(area)) | (Body::Box(area), Body::Round(round)) => {
            round.overlaps_rect(area)
        }
    }))
}

fn moved<I>(
    heap: &mut Heap,
    moved: &Moved<I>,
    key: &str,
    index: impl Fn(&mut Heap, &I) -> Result<Value, Error>,
) -> Result<Value, Error> {
    let mut contacts = Vec::with_capacity(moved.contacts.len());
    for contact in &moved.contacts {
        let at = index(heap, &contact.index)?;
        let normal = point_value(heap, contact.normal)?;
        contacts.push(map_value(heap, &[(key, at), ("normal", normal)])?);
    }
    let contacts = make_vector(heap, &contacts)?;
    let area = rect_value(heap, moved.rect)?;
    map_value(
        heap,
        &[
            ("rect", area),
            ("floor", Value::Bool(moved.on_floor())),
            ("wall", Value::Bool(moved.on_wall())),
            ("ceiling", Value::Bool(moved.on_ceiling())),
            ("contacts", contacts),
        ],
    )
}

fn sliding(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let name = "collide/slide";
    let moving = rect(heap, name, arguments[0])?;
    let delta = point(heap, name, arguments[1])?;
    let solids = heap
        .elements(arguments[2])
        .ok_or_else(|| expected(heap, name, "a vector of rects", arguments[2]))?
        .into_iter()
        .map(|solid| rect(heap, name, solid))
        .collect::<Result<Vec<_>, _>>()?;
    let result = slide(moving, delta, &solids);
    moved(heap, &result, "index", |_, index| {
        Ok(Value::Int(i64::try_from(*index).unwrap_or(i64::MAX)))
    })
}

fn gids(heap: &Heap, name: &str, value: Option<Value>) -> Result<Option<BTreeSet<i64>>, Error> {
    let Some(value) = value.filter(|value| *value != Value::Nil) else {
        return Ok(None);
    };
    let items = heap
        .elements(value)
        .ok_or_else(|| expected(heap, name, "a vector of tile ids", value))?;
    items
        .into_iter()
        .map(|item| match item {
            Value::Int(gid) => Ok(gid),
            _ => Err(expected(heap, name, "a tile id", item)),
        })
        .collect::<Result<BTreeSet<_>, _>>()
        .map(Some)
}

fn whole_field(
    heap: &Heap,
    name: &str,
    layer: Value,
    key: &str,
    default: Option<i64>,
) -> Result<i64, Error> {
    match field(heap, layer, key).or(default.map(Value::Int)) {
        Some(Value::Int(found)) => Ok(found),
        _ => Err(expected(
            heap,
            name,
            "a tile layer from tiled/load, with :data, :width and :height",
            layer,
        )),
    }
}

fn through_tiles(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let name = "collide/move-in-tiles";
    let moving = rect(heap, name, arguments[0])?;
    let delta = point(heap, name, arguments[1])?;
    let layer = arguments[2];
    let given = |value: Value| (value != Value::Nil).then_some(value);
    let size = given(arguments[3]).or_else(|| field(heap, layer, "tile-size"));
    let (only, except) = (given(arguments[4]), given(arguments[5]));
    let size = size
        .map(|value| point(heap, name, value))
        .transpose()?
        .ok_or_else(|| {
            Error::new("collide/move-in-tiles needs the layer's :tile-size or a :tile-size option")
        })?;
    let only = gids(heap, name, only)?;
    let except = gids(heap, name, except)?.unwrap_or_default();
    let width = whole_field(heap, name, layer, "width", None)?;
    let height = whole_field(heap, name, layer, "height", None)?;
    let left = whole_field(heap, name, layer, "x", Some(0))?;
    let top = whole_field(heap, name, layer, "y", Some(0))?;
    let data = field(heap, layer, "data")
        .and_then(|data| heap.vector_of(data))
        .ok_or_else(|| expected(heap, name, "a tile layer with :data", layer))?;
    let reader: &Heap = heap;
    let solid = |column: i64, row: i64| {
        let (column, row) = (column - left, row - top);
        if column < 0 || row < 0 || column >= width || row >= height {
            return false;
        }
        let Ok(index) = usize::try_from(row * width + column) else {
            return false;
        };
        let Some(Value::Int(raw)) = data.get(reader, index) else {
            return false;
        };
        let gid = raw & !FLAGS;
        gid != 0 && only.as_ref().is_none_or(|only| only.contains(&gid)) && !except.contains(&gid)
    };
    let result = move_in_tiles(moving, delta, size, solid);
    moved(heap, &result, "tile", |heap, (column, row)| {
        make_vector(heap, &[Value::Int(*column), Value::Int(*row)])
    })
}

fn ids(heap: &mut Heap, found: Vec<Key>) -> Result<Value, Error> {
    let values: Vec<Value> = found.into_iter().map(Key::value).collect();
    make_vector(heap, &values)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "overlaps?")
        .arity(2, Some(2))
        .function(overlaps);
    isolate
        .native(MODULE, "penetration")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let moving = rect(heap, "collide/penetration", arguments[0])?;
            let solid = rect(heap, "collide/penetration", arguments[1])?;
            match penetration(moving, solid) {
                Some(push) => point_value(heap, push),
                None => Ok(Value::Nil),
            }
        });
    isolate
        .native(MODULE, "sweep")
        .arity(3, Some(3))
        .function(|heap, arguments| {
            let moving = rect(heap, "collide/sweep", arguments[0])?;
            let delta = point(heap, "collide/sweep", arguments[1])?;
            let solid = rect(heap, "collide/sweep", arguments[2])?;
            let Some(hit) = sweep(moving, delta, solid) else {
                return Ok(Value::Nil);
            };
            let normal = point_value(heap, hit.normal)?;
            map_value(
                heap,
                &[("time", Value::Float(hit.time)), ("normal", normal)],
            )
        });
    isolate
        .native(MODULE, "slide")
        .arity(3, Some(3))
        .function(sliding);
    isolate
        .native(MODULE, "move-in-tiles")
        .arity(3, Some(3))
        .keywords(&["tile-size", "only", "except"])
        .function(through_tiles);
    isolate
        .native(MODULE, "grid")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let size = number(heap, "collide/grid", arguments[0])?;
            heap.foreign(KIND, RefCell::new(Grid::<Key>::new(size)))
                .map_err(Error::from)
        });
    isolate
        .native(MODULE, "insert!")
        .arity(3, Some(3))
        .function(|heap, arguments| {
            let id = Key::read(heap, "collide/insert!", arguments[1])?;
            let area = rect(heap, "collide/insert!", arguments[2])?;
            grid(heap, "collide/insert!", arguments[0])?
                .borrow_mut()
                .insert(id, area);
            Ok(arguments[0])
        });
    isolate
        .native(MODULE, "remove!")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let id = Key::read(heap, "collide/remove!", arguments[1])?;
            grid(heap, "collide/remove!", arguments[0])?
                .borrow_mut()
                .remove(id);
            Ok(arguments[0])
        });
    isolate
        .native(MODULE, "query")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let area = rect(heap, "collide/query", arguments[1])?;
            let found = grid(heap, "collide/query", arguments[0])?
                .borrow()
                .query(area);
            ids(heap, found)
        });
    isolate
        .native(MODULE, "at")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let spot = point(heap, "collide/at", arguments[1])?;
            let found = grid(heap, "collide/at", arguments[0])?.borrow().at(spot);
            ids(heap, found)
        });
    isolate
        .native(MODULE, "pairs")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let found = grid(heap, "collide/pairs", arguments[0])?.borrow().pairs();
            let mut pairs = Vec::with_capacity(found.len());
            for (a, b) in found {
                pairs.push(make_vector(heap, &[a.value(), b.value()])?);
            }
            make_vector(heap, &pairs)
        });
}
