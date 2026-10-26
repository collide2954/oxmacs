use ox_library::collide::{Circle, penetration};
use ox_library::geometry::Rect;
use ox_library::geometry::Vec2;

use crate::support::evaluated;

#[test]
fn circles_overlap_circles_rects_and_points() {
    let circle = Circle::new(Vec2::new(0.0, 0.0), 5.0);
    assert!(circle.overlaps(Circle::new(Vec2::new(8.0, 0.0), 4.0)));
    assert!(!circle.overlaps(Circle::new(Vec2::new(9.0, 0.0), 4.0)));
    assert!(circle.contains(Vec2::new(3.0, 3.0)));
    assert!(!circle.contains(Vec2::new(4.0, 4.0)));
    assert!(circle.overlaps_rect(Rect::new(4.0, -1.0, 10.0, 2.0)));
    assert!(!circle.overlaps_rect(Rect::new(4.0, 4.0, 10.0, 10.0)));
    assert!(circle.overlaps_rect(Rect::new(-1.0, -1.0, 2.0, 2.0)));
    assert_eq!(circle.bounds(), Rect::new(-5.0, -5.0, 10.0, 10.0));
}

#[test]
fn penetration_pushes_out_along_the_shallowest_axis() {
    let wall = Rect::new(10.0, 0.0, 10.0, 100.0);
    assert_eq!(
        penetration(Rect::new(8.0, 20.0, 4.0, 4.0), wall),
        Some(Vec2::new(-2.0, 0.0))
    );
    assert_eq!(
        penetration(Rect::new(19.0, 20.0, 4.0, 4.0), wall),
        Some(Vec2::new(1.0, 0.0))
    );
    let floor = Rect::new(0.0, 50.0, 100.0, 10.0);
    assert_eq!(
        penetration(Rect::new(20.0, 47.0, 4.0, 4.0), floor),
        Some(Vec2::new(0.0, -1.0))
    );
    assert_eq!(penetration(Rect::new(0.0, 0.0, 4.0, 4.0), floor), None);
}

fn collide(source: &str) -> String {
    evaluated("(ox collide)", source)
}

#[test]
fn shapes_overlap_rects_and_circles() {
    assert_eq!(
        collide("(collide/overlaps? [0 0 10 10] [5 5 10 10])"),
        "true"
    );
    assert_eq!(collide("(collide/overlaps? [0 0 5] [8 0 4])"), "true");
    assert_eq!(collide("(collide/overlaps? [0 0 5] [4 -1 10 2])"), "true");
    assert_eq!(
        collide("(collide/overlaps? [0 0 10 10] [20 20 2])"),
        "false"
    );
    assert_eq!(
        collide("(collide/overlaps? {x: 0 y: 0 radius: 5} {x: 3 y: 0 w: 1 h: 1})"),
        "true"
    );
    assert_eq!(
        collide("(collide/penetration [8 20 4 4] [10 0 10 100])"),
        "[-2.0 0.0]"
    );
    assert_eq!(
        collide("(collide/penetration [0 0 4 4] [10 0 10 100])"),
        "nil"
    );
}

#[test]
fn boxes_sweep_and_slide() {
    assert_eq!(
        collide(
            "(let [hit (collide/sweep [0 0 10 10] [20 0] [15 0 10 10])] [hit.time hit.normal])"
        ),
        "[0.25 [-1.0 0.0]]"
    );
    assert_eq!(
        collide("(collide/sweep [0 0 10 10] [4 0] [15 0 10 10])"),
        "nil"
    );
    assert_eq!(
        collide(
            "(let [moved (collide/slide [0 0 10 10] [5 3] [[0 10 100 10]])] [moved.rect moved.floor moved.wall moved.ceiling (get (first moved.contacts) :index) (get (first moved.contacts) :normal)])"
        ),
        "[[5.0 0.0 10.0 10.0] true false false 0 [0.0 -1.0]]"
    );
}

#[test]
fn boxes_move_through_tile_layers() {
    let layer =
        "{width: 4 height: 2 x: 0 y: 0 data: [0 0 0 0 1 1 2147483649 1] tile-size: [16 16]}";
    assert_eq!(
        collide(&format!(
            "(let [moved (collide/move-in-tiles [4 0 8 8] [0 40] {layer})] [moved.rect moved.floor (get (first moved.contacts) :tile)])"
        )),
        "[[4.0 8.0 8.0 8.0] true [0 1]]"
    );
    assert_eq!(
        collide(&format!(
            "(get (collide/move-in-tiles [4 0 8 8] [0 40] {layer} :except [1]) :rect)"
        )),
        "[4.0 40.0 8.0 8.0]"
    );
    assert_eq!(
        collide(&format!(
            "(get (collide/move-in-tiles [36 0 8 8] [0 40] {layer} :only [1]) :rect)"
        )),
        "[36.0 8.0 8.0 8.0]"
    );
}

#[test]
fn grids_find_what_is_near() {
    assert_eq!(
        collide(
            "(let [g (collide/grid 32)]
               (collide/insert! g 3 [0 0 10 10])
               (collide/insert! g :hero [5 5 10 10])
               (collide/insert! g 2 [100 100 5 5])
               (let [before [(collide/query g [0 0 20 20]) (collide/at g [7 7]) (collide/pairs g)]]
                 (collide/remove! g 3)
                 [before (collide/query g [0 0 20 20])]))"
        ),
        "[[[3 :hero] [3 :hero] [[3 :hero]]] [:hero]]"
    );
    assert_eq!(
        collide("(collide/query 5 [0 0 1 1])"),
        "error: collide/query expects a grid from collide/grid, got 5"
    );
}
