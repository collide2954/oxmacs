use ox_lang::Isolate;
use ox_lang::value::Value;

fn doubling() -> Isolate {
    let mut isolate = Isolate::new();
    isolate.define_native_awaiting("double", 1, Some(1), |_, arguments| match arguments[0] {
        Value::Int(number) => Ok(Value::Int(number * 2)),
        _ => Err(ox_lang::Error::new("double expects a whole number")),
    });
    isolate
}

fn shown(isolate: &mut Isolate, source: &str) -> String {
    let value = isolate
        .eval(source)
        .unwrap_or_else(|error| panic!("{source}: {}", error.message));
    isolate.write(value)
}

#[test]
fn awaiting_natives_wait_for_promises_they_are_given() {
    let mut isolate = doubling();

    assert_eq!(shown(&mut isolate, "(double 4)"), "8");
    assert_eq!(shown(&mut isolate, "(double (spawn (+ 20 1)))"), "42");
    assert_eq!(shown(&mut isolate, "(-> (spawn 5) double double)"), "20");
    assert_eq!(
        shown(
            &mut isolate,
            "(def (twice-later n) (double (spawn n))) (twice-later 7)"
        ),
        "14"
    );
    assert_eq!(
        shown(
            &mut isolate,
            "(let [p (promise)] (spawn (deliver p 3)) (double p))"
        ),
        "6"
    );
}

#[test]
fn failures_of_awaited_promises_reach_the_caller() {
    let mut isolate = doubling();

    let error = isolate.eval("(double (spawn (/ 1 0)))").unwrap_err();

    assert_eq!(error.message, "Division by zero");
    assert_eq!(
        shown(
            &mut isolate,
            "(try (double (spawn (raise (error \"nope\")))) (catch error e e.message))"
        ),
        "\"nope\""
    );
}

#[test]
fn other_natives_still_receive_the_promise_itself() {
    let mut isolate = doubling();

    assert_eq!(shown(&mut isolate, "(promise? (spawn 1))"), "true");
}
