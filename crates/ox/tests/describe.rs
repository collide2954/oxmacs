use ox::Isolate;
use ox::describe::{Description, Kind};

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
