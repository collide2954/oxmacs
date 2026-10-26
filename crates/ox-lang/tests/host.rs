use std::time::Duration;

use ox_lang::Isolate;
use ox_lang::value::Value;

#[derive(Debug, Default)]
struct Log(Vec<String>);

fn noting() -> Isolate {
    let mut isolate = Isolate::new();
    isolate.define_host_native::<Log>("note", 1, Some(1), |heap, log, arguments| {
        log.0
            .push(ox_lang::value::print::display(heap, arguments[0]));
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

#[test]
fn natives_can_hand_slow_work_to_a_thread_that_answers_with_json() {
    let mut isolate = Isolate::new();
    isolate.define_native("slow-lookup", 1, Some(1), |heap, arguments| {
        let wanted = ox_lang::value::print::display(heap, arguments[0]);
        heap.background_json("slow-lookup", move || {
            if wanted == "missing" {
                return Err(format!("No {wanted} here"));
            }
            Ok(serde_json::json!({"name": wanted, "tags": ["a", "b"], "size": 2, "ratio": 0.5, "gone": null}))
        })
    });

    let found = isolate
        .eval(r#"(let [found (await (slow-lookup "mail"))] [found.name found.tags found.size found.ratio found.gone (try (await (slow-lookup "missing")) (catch error e e.message))])"#)
        .unwrap();

    assert_eq!(
        isolate.write(found),
        r#"["mail" ["a" "b"] 2 0.5 nil "No missing here"]"#
    );
}

#[test]
fn natives_that_panic_raise_an_error_and_leave_the_isolate_working() {
    let mut isolate = Isolate::new();
    isolate.define_native("boom", 0, Some(0), |_, _| panic!("oh no"));

    let error = isolate.eval("(boom)").unwrap_err();

    assert_eq!(error.message, "boom crashed: oh no");
    let caught = isolate
        .eval("(try (boom) (catch error e e.message))")
        .unwrap();
    assert_eq!(isolate.write(caught), r#""boom crashed: oh no""#);
    let sum = isolate.eval("(+ 1 2)").unwrap();
    assert_eq!(isolate.write(sum), "3");
}
