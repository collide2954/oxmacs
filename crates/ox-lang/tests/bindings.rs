use std::fs;

use ox_lang::Isolate;
use ox_lang::check::{Binding, Occurrence, binding_at};

fn spans_like(source: &str, occurrences: &[Occurrence], at: &str, nth: usize) -> Vec<String> {
    let offset = source.match_indices(at).nth(nth).unwrap().0;
    let wanted = &binding_at(occurrences, offset).unwrap().binding;
    occurrences
        .iter()
        .filter(|occurrence| occurrence.binding == *wanted)
        .map(|occurrence| {
            let (start, end) = (occurrence.span.start as usize, occurrence.span.end as usize);
            format!("{start}:{}", &source[start..end])
        })
        .collect()
}

#[test]
fn a_global_and_a_parameter_with_the_same_name_are_told_apart() {
    let source = "(def (area w h) (* w h))\n(def (other area) (+ area 1))\n(area 1 2)\n";
    let found = Isolate::new().bindings("sample.ox", source);

    assert_eq!(spans_like(source, &found, "area", 3), ["6:area", "56:area"]);
    assert_eq!(
        spans_like(source, &found, "area", 1),
        ["37:area", "46:area"]
    );
    let defining: Vec<bool> = found
        .iter()
        .filter(|occurrence| occurrence.span.start == 6 || occurrence.span.start == 37)
        .map(|occurrence| occurrence.defines)
        .collect();
    assert_eq!(defining, [true, true]);
}

#[test]
fn names_from_an_imported_module_carry_that_module() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("geometry.ox"), "(def (area w h) (* w h))\n").unwrap();
    let mut isolate = Isolate::new();
    isolate.add_root(dir.path());
    let source = "(module (import (geometry :as g)))\n(g/area 1 2)\n(def area 5)\narea\n";

    let found = isolate.bindings("sample.ox", source);

    let at = |offset| binding_at(&found, offset).map(|occurrence| occurrence.binding.clone());
    assert_eq!(
        at(source.find("g/area").unwrap()),
        Some(Binding::Global {
            module: Some("geometry".to_owned()),
            name: "area".to_owned()
        })
    );
    assert_eq!(
        at(source.rfind("area").unwrap()),
        Some(Binding::Global {
            module: None,
            name: "area".to_owned()
        })
    );
}

#[test]
fn macro_arguments_are_found_once_and_inner_lets_shadow_outer_ones() {
    let source =
        "(defmacro twice [x] `(do ~x ~x))\n(def n 1)\n(twice n)\n(let [y 1] (let [y 2] y) y)\n";
    let found = Isolate::new().bindings("sample.ox", source);

    assert_eq!(spans_like(source, &found, "n", 1), ["38:n", "50:n"]);
    assert_eq!(spans_like(source, &found, "y", 1), ["70:y", "75:y"]);
    assert_eq!(spans_like(source, &found, "y", 0), ["59:y", "78:y"]);
}
