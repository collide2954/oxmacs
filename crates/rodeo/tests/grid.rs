use rodeo::collide::Grid;
use rodeo::rect::Rect;
use rodeo::vec::Vec2;

fn filled() -> Grid<u32> {
    let mut grid = Grid::new(32.0);
    grid.insert(3, Rect::new(0.0, 0.0, 10.0, 10.0));
    grid.insert(1, Rect::new(5.0, 5.0, 10.0, 10.0));
    grid.insert(2, Rect::new(100.0, 100.0, 50.0, 50.0));
    grid.insert(4, Rect::new(-40.0, -40.0, 8.0, 8.0));
    grid
}

#[test]
fn queries_find_what_overlaps_an_area_in_id_order() {
    let grid = filled();
    assert_eq!(grid.len(), 4);
    assert_eq!(grid.query(Rect::new(0.0, 0.0, 20.0, 20.0)), [1, 3]);
    assert_eq!(
        grid.query(Rect::new(10.0, 15.0, 5.0, 5.0)),
        Vec::<u32>::new()
    );
    assert_eq!(grid.query(Rect::new(140.0, 140.0, 100.0, 100.0)), [2]);
    assert_eq!(grid.query(Rect::new(-50.0, -50.0, 15.0, 15.0)), [4]);
    assert_eq!(grid.at(Vec2::new(7.0, 7.0)), [1, 3]);
}

#[test]
fn moving_and_removing_update_the_cells() {
    let mut grid = filled();
    grid.insert(3, Rect::new(200.0, 0.0, 10.0, 10.0));
    assert_eq!(grid.query(Rect::new(0.0, 0.0, 5.0, 5.0)), Vec::<u32>::new());
    assert_eq!(grid.query(Rect::new(195.0, 0.0, 10.0, 10.0)), [3]);
    assert_eq!(grid.rect(3), Some(Rect::new(200.0, 0.0, 10.0, 10.0)));
    assert_eq!(grid.remove(3), Some(Rect::new(200.0, 0.0, 10.0, 10.0)));
    assert_eq!(grid.remove(3), None);
    assert_eq!(
        grid.query(Rect::new(195.0, 0.0, 10.0, 10.0)),
        Vec::<u32>::new()
    );
    assert_eq!(grid.len(), 3);
}

#[test]
fn pairs_list_each_overlap_once() {
    let mut grid = filled();
    grid.insert(5, Rect::new(8.0, 8.0, 40.0, 40.0));
    assert_eq!(grid.pairs(), [(1, 3), (1, 5), (3, 5)]);
    grid.clear();
    assert!(grid.is_empty());
}
