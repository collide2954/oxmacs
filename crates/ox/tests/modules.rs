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
