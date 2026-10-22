use std::time::Duration;

use ox::Isolate;
use ox::value::Value;

#[derive(Debug, Default)]
struct Log(Vec<String>);

fn noting() -> Isolate {
    let mut isolate = Isolate::new();
    isolate.define_host_native::<Log>("note", 1, Some(1), |heap, log, arguments| {
        log.0.push(ox::value::print::display(heap, arguments[0]));
        Ok(Value::Nil)
    });
    isolate
}

#[test]
fn host_natives_reach_the_host_lent_to_eval() {
    let mut isolate = noting();
    let mut log = Log::default();

    isolate
        .eval_with(&mut log, r#"(note "first") (note [1 2])"#)
        .unwrap();

    assert_eq!(log.0, ["first", "[1 2]"]);
}

#[test]
fn host_natives_refuse_to_run_without_their_host() {
    let mut isolate = noting();

    let error = isolate.eval("(note 1)").unwrap_err();

    assert_eq!(error.message, "note isn't available here");
}

#[test]
fn calls_and_tasks_get_the_host_too() {
    let mut isolate = noting();
    let mut log = Log::default();
    let later = isolate.eval("(fn [x] (note x))").unwrap();

    isolate
        .call_with(&mut log, later, &[Value::Int(1)])
        .unwrap();
    isolate.spawn(later, &[Value::Int(2)]).unwrap();
    isolate.eval_with(&mut log, "(spawn (note 3))").unwrap();
    let report = isolate.run_with(&mut log, Duration::from_millis(20));

    assert_eq!(report.pending, 0);
    assert_eq!(log.0, ["1", "2", "3"]);
}

#[test]
fn held_values_survive_collection_until_released() {
    let mut isolate = Isolate::new();
    let text = isolate.eval(r#"(str "held " "text")"#).unwrap();

    let hold = isolate.hold(text);
    isolate.collect();
    isolate
        .eval("(def junk (map (fn [i] (str i)) (range 1000))) (def junk nil)")
        .unwrap();
    isolate.collect();

    let back = isolate.release(hold).unwrap();
    assert_eq!(isolate.write(back), r#""held text""#);
    assert_eq!(isolate.release(hold), None);
}
