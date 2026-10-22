use std::fs;
use std::path::Path;

use ox::Isolate;

fn evaluated(isolate: &mut Isolate, source: &str) -> String {
    let source = format!("(module (import (ox file) (ox path))) {source}");
    match isolate.eval(&source) {
        Ok(value) => isolate.write(value),
        Err(error) => format!("error: {}", error.message),
    }
}

fn literal(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

#[test]
fn directories_are_listed_by_name_with_their_details() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("b.txt"), "hello").unwrap();
    fs::write(dir.path().join(".hidden"), "").unwrap();
    fs::create_dir(dir.path().join("a")).unwrap();
    let mut isolate = Isolate::new();

    let listed = evaluated(
        &mut isolate,
        &format!(
            "(map (fn [entry] [entry.name entry.dir entry.size entry.hidden (= entry.path (path/join {} entry.name)) (> entry.modified 0)]) (await (file/list {})))",
            literal(dir.path()),
            literal(dir.path())
        ),
    );

    assert_eq!(
        listed,
        r#"[[".hidden" false 0 true true true] ["a" true nil false true true] ["b.txt" false 5 false true true]]"#
    );
}

#[test]
fn files_are_written_read_renamed_copied_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let note = literal(&dir.path().join("note.txt"));
    let moved = literal(&dir.path().join("moved.txt"));
    let nested = literal(&dir.path().join("x").join("y"));
    let inside = literal(&dir.path().join("x").join("y").join("inner.txt"));
    let copied = literal(&dir.path().join("copy"));
    let copied_inside = literal(&dir.path().join("copy").join("y").join("inner.txt"));
    let mut isolate = Isolate::new();

    let results = evaluated(
        &mut isolate,
        &format!(
            "(await (file/write {note} \"hi\"))
             (def read-back (await (file/read {note})))
             (await (file/rename {note} {moved}))
             (def after-rename [(await (file/exists? {note})) (await (file/exists? {moved}))])
             (await (file/make-dir {nested}))
             (await (file/write {inside} \"deep\"))
             (await (file/copy (path/parent {nested}) {copied}))
             (def copy-read (await (file/read {copied_inside})))
             (await (file/remove {copied}))
             [read-back after-rename copy-read (await (file/exists? {copied}))]"
        ),
    );

    assert_eq!(results, r#"["hi" [false true] "deep" false]"#);
}

#[test]
fn failures_say_what_went_wrong() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.txt");
    let mut isolate = Isolate::new();

    let failed = evaluated(
        &mut isolate,
        &format!(
            "(try (await (file/read {})) (catch error e e.message))",
            literal(&missing)
        ),
    );

    let expected = format!("\"Couldn't read {}: ", missing.display()).replace('\\', "\\\\");
    assert!(failed.starts_with(&expected), "{failed}");
}

#[test]
fn paths_are_joined_and_taken_apart() {
    let mut isolate = Isolate::new();
    let joined = Path::new("a").join("b").join("c.txt");
    let joined = joined.display().to_string();

    assert_eq!(
        evaluated(
            &mut isolate,
            &format!(
                "(let [file {}] [(path/name file) (path/stem file) (path/extension file) (= (path/parent file) (path/join \"a\" \"b\")) (= file (path/join \"a\" \"b\" \"c.txt\"))])",
                format_args!("{joined:?}")
            )
        ),
        r#"["c.txt" "c" "txt" true true]"#
    );
    assert_eq!(
        evaluated(
            &mut isolate,
            "[(path/extension \"Makefile\") (path/parent \"/\") (= (path/expand \"~/x\") (path/join (path/home) \"x\")) (path/absolute? \"relative\")]"
        ),
        "[nil nil true false]"
    );
}
