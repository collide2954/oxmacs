use crate::support::evaluated;

fn vec(source: &str) -> String {
    evaluated("(rodeo vec)", source)
}

#[test]
fn vectors_add_subtract_and_scale() {
    assert_eq!(vec("(vec/add [1 2] [3 4] [0.5 0.5])"), "[4.5 6.5]");
    assert_eq!(vec("(vec/add [1 2])"), "[1.0 2.0]");
    assert_eq!(vec("(vec/sub [5 5] [1 2])"), "[4.0 3.0]");
    assert_eq!(vec("(vec/scale [1 2] 3)"), "[3.0 6.0]");
    assert_eq!(vec("(vec/add {x: 1 y: 2} [1 1])"), "[2.0 3.0]");
}

#[test]
fn vectors_measure_and_turn() {
    assert_eq!(vec("(vec/length [3 4])"), "5.0");
    assert_eq!(vec("(vec/normalize [3 4])"), "[0.6 0.8]");
    assert_eq!(vec("(vec/dot [3 4] [2 1])"), "10.0");
    assert_eq!(vec("(vec/cross [3 4] [2 1])"), "-5.0");
    assert_eq!(vec("(vec/distance [0 0] [3 4])"), "5.0");
    assert_eq!(vec("(vec/angle [0 1])"), "1.5707963267948966");
    assert_eq!(vec("(vec/from-angle 0)"), "[1.0 0.0]");
    assert_eq!(vec("(vec/from-angle 0 2)"), "[2.0 0.0]");
    assert_eq!(vec("(vec/rotate [2 0] 0)"), "[2.0 0.0]");
    assert_eq!(vec("(vec/perpendicular [1 0])"), "[0.0 1.0]");
    assert_eq!(vec("(vec/radians 180)"), "3.141592653589793");
    assert_eq!(vec("(vec/degrees 3.141592653589793)"), "180.0");
}

#[test]
fn vectors_mix_clamp_reflect_and_approach() {
    assert_eq!(vec("(vec/lerp [0 0] [10 20] 0.25)"), "[2.5 5.0]");
    assert_eq!(vec("(vec/clamp-length [30 40] 5)"), "[3.0 4.0]");
    assert_eq!(vec("(vec/reflect [2 3] [0 -1])"), "[2.0 -3.0]");
    assert_eq!(vec("(vec/move-toward [0 0] [10 0] 4)"), "[4.0 0.0]");
}

#[test]
fn bad_vectors_explain_themselves() {
    assert_eq!(
        vec("(vec/add [1] [2 3])"),
        "error: vec/add expects a point as [x y] or a map of :x and :y, got [1]"
    );
    assert_eq!(
        vec("(vec/scale [1 2] :big)"),
        "error: vec/scale expects a number, got :big"
    );
}
