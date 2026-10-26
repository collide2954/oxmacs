use std::cell::Cell;
use std::rc::Rc;

use ox_lang::value::Value;
use ox_lang::{Error, Isolate};

struct Counter {
    hits: Cell<i64>,
}

struct Watched(Rc<Cell<bool>>);

impl Drop for Watched {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

fn counting() -> Isolate {
    let mut isolate = Isolate::new();
    isolate
        .native("ox", "hit!")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let counter = heap
                .foreign_of::<Counter>(arguments[0])
                .ok_or_else(|| Error::new("hit! expects a counter"))?;
            counter.hits.set(counter.hits.get() + 1);
            Ok(Value::Int(counter.hits.get()))
        });
    isolate
}

#[test]
fn natives_get_the_rust_value_back() {
    let mut isolate = counting();
    let counter = isolate
        .foreign("counter", Counter { hits: Cell::new(0) })
        .unwrap();
    isolate.define("counter", counter);

    let hits = isolate.eval("(hit! counter) (hit! counter)").unwrap();

    assert_eq!(hits, Value::Int(2));
    let shown = isolate.eval("[counter (= counter counter)]").unwrap();
    assert_eq!(isolate.write(shown), "[#<counter> true]");
}

#[test]
fn natives_reject_values_of_another_type() {
    let mut isolate = counting();
    let other = isolate
        .foreign("label", "not a counter".to_owned())
        .unwrap();
    isolate.define("label", other);

    let error = isolate.eval("(hit! label)").unwrap_err();

    assert_eq!(error.message, "hit! expects a counter");
}

#[test]
fn rust_values_are_dropped_when_collected() {
    let mut isolate = Isolate::new();
    let dropped = Rc::new(Cell::new(false));
    let kept = Rc::new(Cell::new(false));
    isolate
        .foreign("watched", Watched(Rc::clone(&dropped)))
        .unwrap();
    let held = isolate
        .foreign("watched", Watched(Rc::clone(&kept)))
        .unwrap();
    let rooted = isolate.root(held);

    isolate.collect();

    assert!(dropped.get());
    assert!(!kept.get());
    drop(rooted);
    isolate.collect();
    assert!(kept.get());
}

#[test]
fn the_host_calls_ox_functions_with_rust_values() {
    let mut isolate = counting();
    let counter = isolate
        .foreign(
            "counter",
            Counter {
                hits: Cell::new(10),
            },
        )
        .unwrap();
    let twice = isolate.eval("(fn [c] (hit! c) (hit! c))").unwrap();

    assert_eq!(isolate.call(twice, &[counter]), Ok(Value::Int(12)));
}

#[test]
fn local_rust_values_stay_in_their_isolate() {
    let mut isolate = Isolate::new();
    let local = isolate.foreign("window", 5_u32).unwrap();
    isolate.define("window", local);

    let error = isolate.eval("(send (chan) window)").unwrap_err();

    assert_eq!(error.message, "#<window> can't be sent through a channel");
}
