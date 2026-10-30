use ox_lang::Isolate;
use ox_lang::portable::Portable;
use ox_lang::value::Value;

fn doubling() -> Isolate {
    let mut isolate = Isolate::new();
    isolate
        .native("ox", "double")
        .arity(1, Some(1))
        .suspends()
        .function(|heap, arguments| match arguments[0] {
            Value::Int(number) => heap.background("double", move || Ok(Portable::Int(number * 2))),
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
fn suspending_natives_give_their_value_instead_of_a_promise() {
    let mut isolate = doubling();

    assert_eq!(shown(&mut isolate, "(double 4)"), "8");
    assert_eq!(shown(&mut isolate, "(-> 5 double double)"), "20");
    assert_eq!(
        shown(&mut isolate, "(def (twice n) (double n)) (twice 7)"),
        "14"
    );
    assert_eq!(
        shown(&mut isolate, "[(promise? (double 1)) (await (double 2))]"),
        "[false 4]"
    );
}

#[test]
fn spawn_still_runs_suspending_calls_side_by_side() {
    let mut isolate = doubling();

    assert_eq!(
        shown(
            &mut isolate,
            "(let [a (spawn (double 1)) b (spawn (double 2))] [(promise? a) (await a) (await b)])"
        ),
        "[true 2 4]"
    );
}

#[test]
fn atomically_blocks_say_they_cannot_wait_for_one() {
    let mut isolate = doubling();

    let error = isolate.eval("(atomically (double 1))").unwrap_err();

    assert_eq!(
        error.message,
        "atomically blocks can't wait; await outside the block"
    );
}
