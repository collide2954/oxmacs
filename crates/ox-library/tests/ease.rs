use ox_library::ease::Ease;

use crate::support::evaluated;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn every_ease_starts_at_zero_and_ends_at_one() {
    for ease in Ease::all() {
        assert!(close(ease.apply(0.0), 0.0), "{ease:?} at 0");
        assert!(close(ease.apply(1.0), 1.0), "{ease:?} at 1");
    }
}

#[test]
fn eases_follow_the_usual_curves() {
    assert_eq!(Ease::Linear.apply(0.3), 0.3);
    assert_eq!(Ease::QuadIn.apply(0.5), 0.25);
    assert_eq!(Ease::QuadOut.apply(0.5), 0.75);
    assert_eq!(Ease::CubicInOut.apply(0.25), 0.0625);
    assert!(close(Ease::SineInOut.apply(0.5), 0.5));
    assert!(close(Ease::BounceOut.apply(0.5), 0.765_625));
    assert!(Ease::BackIn.apply(0.3) < 0.0);
    assert!((1..10).any(|step| Ease::ElasticOut.apply(f64::from(step) / 10.0) > 1.0));
}

#[test]
fn eases_clamp_time_and_tween_between_values() {
    assert_eq!(Ease::QuadIn.apply(2.0), 1.0);
    assert_eq!(Ease::QuadIn.apply(-1.0), 0.0);
    assert_eq!(Ease::QuadIn.between(10.0, 20.0, 0.5), 12.5);
}

#[test]
fn eases_have_kebab_case_names() {
    assert_eq!("quad-in".parse::<Ease>(), Ok(Ease::QuadIn));
    assert_eq!("elastic-in-out".parse::<Ease>(), Ok(Ease::ElasticInOut));
    assert_eq!(Ease::BounceOut.name(), "bounce-out");
    assert!("wobble".parse::<Ease>().is_err());
    assert_eq!(Ease::all().count(), 31);
}

fn ease(source: &str) -> String {
    evaluated("(ox ease)", source)
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
