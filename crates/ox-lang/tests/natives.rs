use std::cell::Cell;
use std::rc::Rc;

use ox_lang::build::make_vector;
use ox_lang::value::Value;
use ox_lang::{Isolate, describe::Kind};

fn labels() -> Isolate {
    let mut isolate = Isolate::new();
    isolate
        .native("ox", "label")
        .arity(1, Some(1))
        .keywords(&["size", "face"])
        .function(make_vector);
    isolate
        .native("ox", "column")
        .arity(0, None)
        .keywords(&["spacing"])
        .function(make_vector);
    isolate
}

fn evaluated(isolate: &mut Isolate, source: &str) -> String {
    match isolate.eval(source) {
        Ok(value) => isolate.write(value),
        Err(error) => format!("error: {}", error.message),
    }
}

#[test]
fn keywords_reach_a_native_in_the_order_it_declared_them() {
    let mut isolate = labels();

    assert_eq!(
        evaluated(&mut isolate, r#"(label "x" :face :bold)"#),
        r#"["x" nil :bold]"#
    );
    assert_eq!(
        evaluated(&mut isolate, r#"(label "x" :face :bold :size 2)"#),
        r#"["x" 2 :bold]"#
    );
    assert_eq!(
        evaluated(&mut isolate, "(column 1 2 3 :spacing 4)"),
        "[4 1 2 3]"
    );
    assert_eq!(evaluated(&mut isolate, "(column)"), "[nil]");
}

#[test]
fn natives_and_functions_refuse_wrong_keywords_the_same_way() {
    let mut isolate = labels();
    isolate
        .eval(r#"(def (shout text {size 1 face nil}) text)"#)
        .unwrap();

    for name in ["label", "shout"] {
        assert_eq!(
            evaluated(&mut isolate, &format!(r#"({name} "x" :color 1)"#)),
            format!("error: {name} has no keyword parameter :color")
        );
        assert_eq!(
            evaluated(&mut isolate, &format!(r#"({name} "x" :size)"#)),
            format!("error: {name} was given :size without a value")
        );
    }
    assert_eq!(
        evaluated(&mut isolate, "(label)"),
        "error: label takes 1 argument, got 0"
    );
}

#[test]
fn natives_that_reach_outside_never_run_while_checking() {
    let erased = Rc::new(Cell::new(0));
    let mut isolate = Isolate::new();
    let counted = Rc::clone(&erased);
    isolate
        .native("ox/disk", "erase")
        .arity(0, Some(0))
        .outside()
        .function(move |_, _| {
            counted.set(counted.get() + 1);
            Ok(Value::Nil)
        });
    let source = "(module (import (ox disk)))\n(defmacro wipe [] (disk/erase) nil)\n(wipe)";

    assert_eq!(isolate.check("erase.ox", source), []);
    assert_eq!(erased.get(), 0);
    isolate.eval(source).unwrap();
    assert_eq!(erased.get(), 1);
}

#[test]
fn a_native_says_which_keywords_it_takes() {
    let isolate = labels();

    let label = isolate.describe("user", "label").unwrap();

    assert_eq!(label.kind, Kind::Native);
    assert!(label.signature.contains("size"), "{}", label.signature);
}
