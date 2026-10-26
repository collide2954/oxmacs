use crate::support::evaluated;

fn ease(source: &str) -> String {
    evaluated("(rodeo ease)", source)
}

#[test]
fn eases_shape_time() {
    assert_eq!(ease("(ease/apply :quad-in 0.5)"), "0.25");
    assert_eq!(ease("(ease/apply :linear 2)"), "1.0");
    assert_eq!(ease("(ease/apply \"quad-out\" 0.5)"), "0.75");
}

#[test]
fn eases_tween_numbers_and_points() {
    assert_eq!(ease("(ease/between :quad-in 10 20 0.5)"), "12.5");
    assert_eq!(
        ease("(ease/between :linear [0 0] [10 20] 0.25)"),
        "[2.5 5.0]"
    );
}

#[test]
fn eases_are_named() {
    assert_eq!(ease("(count (ease/names))"), "31");
    assert_eq!(ease("(first (ease/names))"), ":linear");
    assert_eq!(
        ease("(ease/apply :wobble 0.5)"),
        "error: ease/apply expects an ease such as :quad-in or :bounce-out, got :wobble"
    );
}
