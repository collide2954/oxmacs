use crate::support::evaluated;

fn rect(source: &str) -> String {
    evaluated("(rodeo rect)", source)
}

#[test]
fn rects_know_their_parts() {
    assert_eq!(rect("(rect/center [10 20 30 40])"), "[25.0 40.0]");
    assert_eq!(rect("(rect/position [10 20 30 40])"), "[10.0 20.0]");
    assert_eq!(rect("(rect/size {x: 10 y: 20 w: 30 h: 40})"), "[30.0 40.0]");
    assert_eq!(
        rect(
            "[(rect/left [10 20 30 40]) (rect/top [10 20 30 40]) (rect/right [10 20 30 40]) (rect/bottom [10 20 30 40])]"
        ),
        "[10.0 20.0 40.0 60.0]"
    );
    assert_eq!(
        rect("(rect/centered [25 40] [30 40])"),
        "[10.0 20.0 30.0 40.0]"
    );
}

#[test]
fn rects_overlap_contain_and_combine() {
    assert_eq!(rect("(rect/overlaps? [0 0 10 10] [5 5 10 10])"), "true");
    assert_eq!(rect("(rect/overlaps? [0 0 10 10] [10 0 10 10])"), "false");
    assert_eq!(rect("(rect/contains? [0 0 10 10] [0 0])"), "true");
    assert_eq!(rect("(rect/contains? [0 0 10 10] [10 5])"), "false");
    assert_eq!(
        rect("(rect/intersection [0 0 10 10] [5 5 10 10])"),
        "[5.0 5.0 5.0 5.0]"
    );
    assert_eq!(rect("(rect/intersection [0 0 1 1] [5 5 1 1])"), "nil");
    assert_eq!(
        rect("(rect/union [0 0 10 10] [20 -5 5 5])"),
        "[0.0 -5.0 25.0 15.0]"
    );
}

#[test]
fn rects_move_grow_and_clamp() {
    assert_eq!(
        rect("(rect/translate [0 0 10 10] [2 3])"),
        "[2.0 3.0 10.0 10.0]"
    );
    assert_eq!(rect("(rect/expand [0 0 10 10] 2)"), "[-2.0 -2.0 14.0 14.0]");
    assert_eq!(rect("(rect/clamp [0 0 10 10] [15 -4])"), "[10.0 0.0]");
    assert_eq!(
        rect("(rect/center [1 2 3])"),
        "error: rect/center expects a rect as [x y w h] or a map of :x, :y, :w and :h, got [1 2 3]"
    );
}
