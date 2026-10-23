use std::time::{Duration, Instant};

use ox::Isolate;
use ox::task::Wake;
use ox::value::Value;

fn finished(isolate: &mut Isolate) -> Vec<Result<String, String>> {
    let report = isolate.run(Duration::from_millis(50));
    report
        .finished
        .into_iter()
        .map(|(_, result)| {
            result
                .map(|value| isolate.write(value))
                .map_err(|error| error.message)
        })
        .collect()
}

#[test]
fn forms_can_be_evaluated_as_tasks_one_after_another() {
    let mut isolate = Isolate::new();

    assert_eq!(
        isolate.spawn_eval("scratch.ox", "(module (import (ox string :as s)))"),
        Ok(None)
    );
    let defined = isolate
        .spawn_eval("scratch.ox", "(def (shout x) (s/upper x))")
        .unwrap();
    assert!(defined.is_some());
    assert_eq!(finished(&mut isolate), [Ok("shout".to_owned())]);
    isolate.spawn_eval("scratch.ox", "(shout \"hi\")").unwrap();
    assert_eq!(finished(&mut isolate), [Ok("\"HI\"".to_owned())]);
    isolate.spawn_eval("scratch.ox", "(/ 1 0)").unwrap();
    assert_eq!(finished(&mut isolate), [Err("Division by zero".to_owned())]);
    assert!(isolate.spawn_eval("scratch.ox", "(let [x] x)").is_err());
}

#[test]
fn forms_can_be_evaluated_inside_a_module() {
    let mut isolate = Isolate::new();
    isolate
        .eval_module_with(&mut (), "shapes", "shapes.ox", "(def (area w h) (* w h))")
        .unwrap();

    isolate
        .spawn_eval_in(&mut (), "shapes", "shapes.ox", "(area 2 3)")
        .unwrap();
    assert_eq!(finished(&mut isolate), [Ok("6".to_owned())]);
    isolate.spawn_eval("scratch.ox", "(area 2 3)").unwrap();
    assert_eq!(
        finished(&mut isolate),
        [Err("area is not defined".to_owned())]
    );
}

#[test]
fn tracebacks_of_tasks_show_only_code_with_a_source() {
    let mut isolate = Isolate::new();
    isolate
        .eval_named("job.ox", "(def (fail) (+ 1 (/ 1 0)))")
        .unwrap();

    isolate.spawn_eval("scratch.ox", "(fail)").unwrap();
    isolate.eval("(spawn (fail))").unwrap();
    let report = isolate.run(Duration::from_millis(50));

    let places: Vec<Vec<String>> = report
        .finished
        .iter()
        .filter_map(|(_, result)| result.as_ref().err())
        .chain(&report.unhandled)
        .map(|error| error.traceback.iter().map(ToString::to_string).collect())
        .collect();
    assert_eq!(
        places,
        [
            vec!["fail at job.ox:1:18", "top level at scratch.ox:1:1"],
            vec!["fail at job.ox:1:18"],
        ]
    );
}

#[test]
fn sleeping_tasks_say_when_they_wake_next() {
    let mut isolate = Isolate::new();
    assert_eq!(isolate.next_wake(), Wake::Never);

    isolate
        .eval("(def gate (promise)) (spawn (await gate)) (spawn (sleep 5000)) (spawn (sleep 9000))")
        .unwrap();
    let started = Instant::now();
    let _ = isolate.run(Duration::from_millis(10));
    let ran = Instant::now();
    let Wake::At(wake) = isolate.next_wake() else {
        panic!("{:?}", isolate.next_wake());
    };
    assert!(wake >= started + Duration::from_secs(5) && wake <= ran + Duration::from_secs(5));

    isolate.eval("(def (spin) (spin)) (spawn (spin))").unwrap();
    assert_eq!(isolate.next_wake(), Wake::Soon);
}

#[test]
fn a_runaway_evaluation_can_be_cancelled() {
    let mut isolate = Isolate::new();
    isolate.eval("(def (spin) (spin))").unwrap();

    let task = isolate.spawn_eval("scratch.ox", "(spin)").unwrap().unwrap();
    assert_eq!(finished(&mut isolate), []);
    assert!(isolate.runnable() > 0);

    assert!(isolate.cancel(task));
    let _ = isolate.run(Duration::from_millis(10));
    assert_eq!(isolate.runnable(), 0);
    assert_eq!(isolate.eval("(+ 1 2)"), Ok(Value::Int(3)));
}
