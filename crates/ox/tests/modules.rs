use std::fs;
use std::path::Path;

use ox::Isolate;

fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, text).unwrap();
}

fn evaluated(isolate: &mut Isolate, source: &str) -> String {
    match isolate.eval(source) {
        Ok(value) => isolate.write(value),
        Err(error) => format!("error: {}", error.message),
    }
}

fn with_root(files: &[(&str, &str)]) -> (tempfile::TempDir, Isolate) {
    let dir = tempfile::tempdir().unwrap();
    for (name, text) in files {
        write(dir.path(), name, text);
    }
    let mut isolate = Isolate::new();
    isolate.add_root(dir.path());
    (dir, isolate)
}

#[test]
fn modules_export_what_their_header_lists() {
    let (_dir, mut isolate) = with_root(&[(
        "geometry/shapes.ox",
        "(module (export area))\n(def (helper r) (* r r))\n(def (area r) (* 3 (helper r)))",
    )]);

    assert_eq!(
        evaluated(
            &mut isolate,
            "(module (import (geometry shapes))) (shapes/area 2)"
        ),
        "12"
    );
    assert_eq!(
        evaluated(&mut isolate, "(shapes/helper 2)"),
        "error: shapes/helper is not defined"
    );
    assert_eq!(
        evaluated(&mut isolate, "(area 2)"),
        "error: area is not defined"
    );
}

#[test]
fn imports_rename_modules_and_bring_in_bare_names() {
    let (_dir, mut isolate) = with_root(&[(
        "text/tools.ox",
        "(def (shout s) (str s \"!\")) (def (whisper s) (str s \"...\"))",
    )]);

    assert_eq!(
        evaluated(
            &mut isolate,
            "(module (import (text tools :as t :only [shout]))) [(shout \"a\") (t/whisper \"b\")]"
        ),
        "[\"a!\" \"b...\"]"
    );
    assert_eq!(
        evaluated(&mut isolate, "(whisper \"c\")"),
        "error: whisper is not defined"
    );
}

#[test]
fn modules_keep_their_own_names() {
    let (_dir, mut isolate) = with_root(&[
        ("a.ox", "(def x :a) (def (get-x) x)"),
        ("b.ox", "(def x :b) (def (get-x) x)"),
    ]);

    assert_eq!(
        evaluated(
            &mut isolate,
            "(module (import (a) (b))) [(a/get-x) (b/get-x)]"
        ),
        "[:a :b]"
    );
    assert_eq!(
        evaluated(&mut isolate, "(def x :user) [x (a/get-x)]"),
        "[:user :a]"
    );
}

#[test]
fn macros_belong_to_their_module_and_use_its_names() {
    let (_dir, mut isolate) = with_root(&[(
        "lib.ox",
        "(module (export twice))\n(def (helper x) (* x 2))\n(defmacro twice [x] `(helper ~x))",
    )]);

    assert_eq!(
        evaluated(
            &mut isolate,
            "(module (import (lib :only [twice]))) (twice 21)"
        ),
        "42"
    );
    assert_eq!(
        evaluated(&mut isolate, "(def (helper x) :user-helper) (twice 1)"),
        "2"
    );
    assert_eq!(evaluated(&mut isolate, "(lib/twice 5)"), "10");
}

#[test]
fn packages_import_only_the_packages_they_require() {
    let (_dir, mut isolate) = with_root(&[
        ("text/words.ox", "(def hello \"hi\")"),
        (
            "notes/view.ox",
            "(module (import (text words)))\n(def shown words/hello)",
        ),
        (
            "notes/list.ox",
            "(module (import (notes view)))\n(def shown view/shown)",
        ),
        (
            "mail/view.ox",
            "(module (import (text words)))\n(def shown words/hello)",
        ),
    ]);
    isolate.limit_imports("text", &[]);
    isolate.limit_imports("notes", &["text".to_owned()]);
    isolate.limit_imports("mail", &[]);

    assert_eq!(
        evaluated(&mut isolate, "(module (import (notes list))) list/shown"),
        "\"hi\""
    );
    assert_eq!(
        evaluated(&mut isolate, "(module (import (mail view))) view/shown"),
        "error: mail/view imports text/words without requiring text; add text to mail's :requires"
    );
}

#[test]
fn a_module_can_be_imported_twice_with_different_options() {
    let mut isolate = Isolate::new();

    assert_eq!(
        evaluated(
            &mut isolate,
            "(module (import (ox string :as s) (ox string :only [upper]))) [(upper \"a\") (s/lower \"B\")]"
        ),
        "[\"A\" \"b\"]"
    );
}

#[test]
fn a_module_that_failed_to_load_fails_again_on_the_next_import() {
    let (dir, mut isolate) = with_root(&[("shaky.ox", "(def ok 1)\n(/ 1 0)\n(def later 2)")]);

    for _ in 0..2 {
        assert_eq!(
            evaluated(&mut isolate, "(module (import (shaky))) shaky/ok"),
            "error: Division by zero"
        );
    }

    write(dir.path(), "shaky.ox", "(def ok 1)\n(def later 2)");
    assert_eq!(
        evaluated(
            &mut isolate,
            "(module (import (shaky))) [shaky/ok shaky/later]"
        ),
        "[1 2]"
    );
}

#[test]
fn import_cycles_are_named() {
    let (_dir, mut isolate) = with_root(&[
        ("a.ox", "(module (import (b)))"),
        ("b.ox", "(module (import (a)))"),
    ]);

    assert_eq!(
        evaluated(&mut isolate, "(module (import (a)))"),
        "error: Import cycle: a → b → a"
    );
}

#[test]
fn missing_modules_are_reported() {
    let (_dir, mut isolate) = with_root(&[]);

    assert_eq!(
        evaluated(&mut isolate, "(module (import (nowhere)))"),
        "error: No module named nowhere"
    );
}

#[test]
fn reloading_a_module_redefines_it_in_place() {
    let (dir, mut isolate) = with_root(&[(
        "counter.ox",
        "(defonce hits (atom 0)) (def (greet) \"hello\")",
    )]);
    assert_eq!(
        evaluated(
            &mut isolate,
            "(module (import (counter))) (swap! counter/hits inc) (counter/greet)"
        ),
        "\"hello\""
    );

    write(
        dir.path(),
        "counter.ox",
        "(defonce hits (atom 0)) (def (greet) \"hi\")",
    );
    isolate.load("counter").unwrap();

    assert_eq!(evaluated(&mut isolate, "(counter/greet)"), "\"hi\"");
    assert_eq!(evaluated(&mut isolate, "@counter/hits"), "1");
}

#[test]
fn modules_can_come_from_memory_ahead_of_files() {
    let (_dir, mut isolate) = with_root(&[("kit/words.ox", "(def hello \"from disk\")")]);
    isolate.add_source("kit/words", "kit/words.ox", "(def hello \"from memory\")");

    isolate
        .eval_module_with(
            &mut (),
            "kit",
            "kit.ox",
            "(module (import (kit words))) (def greeting words/hello)",
        )
        .unwrap();

    assert_eq!(
        evaluated(&mut isolate, "(module (import (kit))) kit/greeting"),
        "\"from memory\""
    );
}

#[test]
fn hosts_can_look_inside_collections() {
    let mut isolate = Isolate::new();
    let list = isolate.eval("'(a :b \"c\")").unwrap();
    let map = isolate.eval("{x: 1 y: [2 3]}").unwrap();
    let heap = isolate.heap();

    let items: Vec<String> = heap
        .elements(list)
        .unwrap()
        .into_iter()
        .map(|item| isolate.write(item))
        .collect();
    let entries: Vec<(String, String)> = heap
        .entries(map)
        .unwrap()
        .into_iter()
        .map(|(key, value)| (isolate.write(key), isolate.write(value)))
        .collect();

    assert_eq!(items, ["a", ":b", "\"c\""]);
    assert_eq!(
        entries,
        [
            (":x".to_owned(), "1".to_owned()),
            (":y".to_owned(), "[2 3]".to_owned())
        ]
    );
    assert_eq!(heap.elements(map), None);
    assert_eq!(heap.entries(list), None);
}
