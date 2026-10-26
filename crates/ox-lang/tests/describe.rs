use ox_lang::describe::{Description, Kind};
use ox_lang::{Isolate, Stubs};

const SHAPES: &str = "(module (import (ox string :as s)))
(def (area w h) \"Width times height.\" (* w h))
(def (greet name (greeting \"Hi\") & more {loud false}) name)
(defmacro twice [x] \"Runs x twice.\" `(do ~x ~x))
(def limit 5)
(defrecord Point [x y])
(defonce counter (atom 0))";

fn shapes() -> Isolate {
    let mut isolate = Isolate::new();
    isolate.eval_named("shapes.ox", SHAPES).unwrap();
    isolate
}

fn described(isolate: &mut Isolate, name: &str) -> Description {
    isolate
        .describe("user", name)
        .unwrap_or_else(|| panic!("{name} is not described"))
}

fn place(description: &Description) -> Option<(String, usize, usize)> {
    description.place.as_ref().map(|place| {
        let (line, column) = place.line_column();
        (place.file().to_owned(), line, column)
    })
}

#[test]
fn definitions_describe_their_signature_doc_and_place() {
    let mut isolate = shapes();

    let area = described(&mut isolate, "area");
    assert_eq!(area.kind, Kind::Function);
    assert_eq!(area.signature, "(area w h)");
    assert_eq!(area.doc.as_deref(), Some("Width times height."));
    assert_eq!(place(&area), Some(("shapes.ox".to_owned(), 2, 1)));

    let greet = described(&mut isolate, "greet");
    assert_eq!(
        greet.signature,
        "(greet name (greeting \"Hi\") & more {loud false})"
    );
    assert_eq!(greet.doc, None);

    let twice = described(&mut isolate, "twice");
    assert_eq!(twice.kind, Kind::Macro);
    assert_eq!(twice.signature, "(twice x)");
    assert_eq!(twice.doc.as_deref(), Some("Runs x twice."));
    assert_eq!(place(&twice), Some(("shapes.ox".to_owned(), 4, 1)));

    let limit = described(&mut isolate, "limit");
    assert_eq!(limit.kind, Kind::Value);
    assert_eq!(limit.signature, "limit");
    assert_eq!(place(&limit), Some(("shapes.ox".to_owned(), 5, 1)));

    let point = described(&mut isolate, "Point");
    assert_eq!(point.kind, Kind::Record);
    assert_eq!(point.signature, "(Point x y)");
    assert_eq!(place(&point), Some(("shapes.ox".to_owned(), 6, 1)));

    assert_eq!(
        place(&described(&mut isolate, "counter")),
        Some(("shapes.ox".to_owned(), 7, 1))
    );
    assert!(isolate.describe("user", "nowhere").is_none());
}

#[test]
fn natives_and_special_forms_are_described_from_the_library_docs() {
    let mut isolate = shapes();

    let upper = described(&mut isolate, "s/upper");
    assert_eq!(upper.kind, Kind::Native);
    assert_eq!(upper.signature, "(s/upper text)");
    assert!(upper.doc.is_some());
    assert_eq!(upper.place, None);

    let println = described(&mut isolate, "println");
    assert_eq!(println.kind, Kind::Native);
    assert_eq!(println.signature, "(println & values)");

    let branch = described(&mut isolate, "if");
    assert_eq!(branch.kind, Kind::Special);
    assert_eq!(branch.signature, "(if test then (else))");
}

#[test]
fn stubs_from_a_docs_table_keep_their_arity_and_docs_but_refuse_to_run() {
    let mut isolate = Isolate::new();
    isolate.define_stubs(
        "(greet name (loud)) \"Greets name.\"\n(ui/label text {size 1}) \"A label.\"\n(special (never x) \"Not a native.\")",
        Stubs::All,
    );

    assert_eq!(
        isolate.eval("(greet \"a\")").unwrap_err().message,
        "greet isn't available here"
    );
    assert_eq!(
        isolate
            .eval("(module (import (ox ui))) (ui/label \"x\" :size 2)")
            .unwrap_err()
            .message,
        "ui/label isn't available here"
    );
    let found: Vec<String> = isolate
        .check("sample.ox", "(greet)\n(greet 1 2 3)")
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect();
    assert_eq!(
        found,
        [
            "greet takes 1 to 2 arguments, got 0",
            "greet takes 1 to 2 arguments, got 3"
        ]
    );
    let greet = described(&mut isolate, "greet");
    assert_eq!(greet.signature, "(greet name (loud))");
    assert_eq!(greet.doc.as_deref(), Some("Greets name."));
    assert!(isolate.eval("never").is_err());
}

#[test]
fn library_constants_are_described_with_their_docs() {
    let mut isolate = Isolate::new();
    isolate.eval("(module (import (ox math)))").unwrap();

    let pi = described(&mut isolate, "math/pi");

    assert_eq!(pi.kind, Kind::Value);
    assert_eq!(pi.signature, "math/pi");
    assert!(
        pi.doc.as_deref().is_some_and(|doc| doc.contains("circle")),
        "{:?}",
        pi.doc
    );
}

#[test]
fn error_types_and_made_functions_describe_their_parameters() {
    let mut isolate = shapes();
    isolate.eval("(def always (constantly 1))").unwrap();

    let error = described(&mut isolate, "type-error");
    assert_eq!(error.kind, Kind::Record);
    assert_eq!(
        error.signature,
        "(type-error (message \"type-error\") (traceback nil))"
    );
    let always = described(&mut isolate, "always");
    assert_eq!(always.kind, Kind::Function);
    assert_eq!(always.signature, "(always & _)");
    assert_eq!(always.doc, None);
    isolate.eval("(def add (partial + 1))").unwrap();
    assert_eq!(described(&mut isolate, "add").signature, "(add & more)");
}

#[test]
fn the_core_macros_are_built_in_and_described_from_the_library_docs() {
    let mut isolate = shapes();

    let when = described(&mut isolate, "when");
    assert_eq!(when.kind, Kind::Macro);
    assert_eq!(when.signature, "(when test & body)");
    assert_eq!(
        when.doc.as_deref(),
        Some("Evaluates body when test is truthy; otherwise nil.")
    );
    assert_eq!(when.place, None);
    let if_let = described(&mut isolate, "if-let");
    assert_eq!(if_let.signature, "(if-let binding then & otherwise)");
}

#[test]
fn redefining_a_name_moves_its_place() {
    let mut isolate = shapes();

    isolate
        .eval_named("other.ox", "\n\n(def (area w h) (* w h 1))")
        .unwrap();

    let area = described(&mut isolate, "area");
    assert_eq!(place(&area), Some(("other.ox".to_owned(), 3, 1)));
    assert_eq!(area.doc, None);
}

#[test]
fn completions_offer_what_a_module_can_see() {
    let isolate = shapes();

    assert_eq!(isolate.completions("user", "are"), ["area"]);
    assert_eq!(isolate.completions("user", "lim"), ["limit"]);
    assert_eq!(isolate.completions("user", "s/upp"), ["s/upper"]);
    assert_eq!(
        isolate.completions("user", "defr"),
        ["defrecord", "defrule", "defrules"]
    );
    assert_eq!(isolate.completions("user", "when"), ["when", "when-let"]);
    assert_eq!(isolate.completions("user", "#"), Vec::<String>::new());
    assert_eq!(isolate.completions("ox", "are"), Vec::<String>::new());
}

#[test]
fn tests_named_after_a_function_are_its_examples() {
    let mut isolate = shapes();
    isolate
        .eval_named(
            "shapes_test.ox",
            "(test \"area of a square\" (expect (= (area 2 2) 4)))
(test \"areas add up\" (expect true))
(test \"area\"
  (expect (= (area 1 5) 5)))",
        )
        .unwrap();

    assert_eq!(
        isolate.examples("area"),
        [
            "(test \"area of a square\" (expect (= (area 2 2) 4)))",
            "(test \"area\"\n  (expect (= (area 1 5) 5)))",
        ]
    );
    assert_eq!(isolate.examples("limit"), Vec::<String>::new());
}

#[test]
fn the_whole_library_is_documented_and_its_docs_match_the_arities() {
    let isolate = Isolate::new();

    let problems = isolate.undocumented();

    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_reference_lists_every_library_module_with_its_documented_names() {
    let mut isolate = Isolate::new();

    let reference = isolate.reference();
    let module = |name: &str| {
        reference
            .iter()
            .find(|docs| docs.module == name)
            .unwrap_or_else(|| panic!("no module {name}"))
    };
    let entry = |module_name: &str, name: &str| {
        module(module_name)
            .entries
            .iter()
            .find(|entry| entry.name == name)
            .unwrap_or_else(|| panic!("no {name} in {module_name}"))
            .clone()
    };

    assert_eq!(reference[0].module, "ox");
    assert_eq!(entry("ox", "map").kind, Kind::Native);
    assert_eq!(entry("ox", "map").signature, "(map f coll & colls)");
    assert!(entry("ox", "map").doc.is_some());
    assert_eq!(entry("ox", "+").kind, Kind::Native);
    assert_eq!(entry("ox", "if").kind, Kind::Special);
    assert_eq!(entry("ox", "when").kind, Kind::Macro);
    assert_eq!(entry("ox/string", "split").kind, Kind::Native);
    let names: Vec<&str> = module("ox")
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
    assert!(names.iter().all(|name| !name.starts_with('#')));
    assert!(
        reference
            .iter()
            .all(|docs| docs.module == "ox" || docs.module.starts_with("ox/"))
    );

    let value = isolate.docs_value(&reference).unwrap();
    let shown = isolate.write(value);
    assert!(
        shown.starts_with("[{module: \"ox\" entries: [{name: "),
        "{}",
        &shown[..80]
    );
    assert!(
        shown.contains("{name: \"split\" kind: :native signature: \"(split "),
        "split is missing"
    );
}
