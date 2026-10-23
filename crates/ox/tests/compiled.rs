use ox::Isolate;

fn ops(source: &str) -> Vec<String> {
    let mut isolate = Isolate::new();
    let value = isolate.eval(source).unwrap();
    let closure = isolate.heap_mut().closure_of(value).unwrap();
    closure
        .code
        .ops
        .iter()
        .map(|op| format!("{op:?}"))
        .collect()
}

#[test]
fn arithmetic_on_two_arguments_is_one_instruction() {
    let compiled = ops("(fn [a b] (+ a b))");

    assert_eq!(compiled.len(), 2, "{compiled:#?}");
    assert!(compiled[0].starts_with("Prim"), "{compiled:#?}");
}

#[test]
fn a_counting_loop_needs_few_instructions() {
    let compiled = ops("(fn walk [i n acc] (if (< i n) (walk (+ i 1) n (+ acc i)) acc))");

    assert!(compiled.len() <= 12, "{compiled:#?}");
}
