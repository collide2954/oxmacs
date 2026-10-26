use super::cell;
use super::sweep::{Contact, Moved, slide};
use crate::rect::Rect;
use crate::vec::Vec2;

fn edge(index: i64, size: f64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "tile indexes stay far below 2^52"
    )]
    let index = index as f64;
    index * size
}

pub fn move_in_tiles(
    moving: Rect,
    delta: Vec2,
    tile: Vec2,
    solid: impl Fn(i64, i64) -> bool,
) -> Moved<(i64, i64)> {
    let reach = moving.union(moving.translated(delta));
    let columns = cell(reach.left(), tile.x)..=cell(reach.right(), tile.x);
    let rows = cell(reach.top(), tile.y)..=cell(reach.bottom(), tile.y);
    let mut places = Vec::new();
    let mut rects = Vec::new();
    for row in rows {
        for column in columns.clone() {
            if solid(column, row) {
                places.push((column, row));
                rects.push(Rect::new(
                    edge(column, tile.x),
                    edge(row, tile.y),
                    tile.x,
                    tile.y,
                ));
            }
        }
    }
    let moved = slide(moving, delta, &rects);
    Moved {
        rect: moved.rect,
        contacts: moved
            .contacts
            .into_iter()
            .map(|contact| Contact {
                index: places[contact.index],
                normal: contact.normal,
            })
            .collect(),
    }
}
