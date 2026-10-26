use ox_lang::Isolate;

#[test]
fn docstrings_describe_functions_macros_and_values() {
    let mut isolate = Isolate::new();
    isolate
        .eval(
            "(def (area w h) \"Width times height.\" (* w h))
(def limit \"Most retries.\" 5)
(def (shout) \"Just a string\")
(defmacro twice [x] \"Runs x twice.\" `(do ~x ~x))",
        )
        .unwrap();

    assert_eq!(
        isolate
            .eval("[(area 2 3) limit (shout) (twice 1)]")
            .map(|value| isolate.write(value)),
        Ok("[6 5 \"Just a string\" 1]".to_owned())
    );
    assert_eq!(isolate.doc("area").as_deref(), Some("Width times height."));
    assert_eq!(isolate.doc("limit").as_deref(), Some("Most retries."));
    assert_eq!(isolate.doc("twice").as_deref(), Some("Runs x twice."));
    assert_eq!(isolate.doc("shout"), None);
    assert_eq!(isolate.doc("nowhere"), None);

    isolate.eval("(def (area w h) (* w h))").unwrap();
    assert_eq!(isolate.doc("area"), None);
}
