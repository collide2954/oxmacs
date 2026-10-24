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
fn appending_adds_to_the_end_and_makes_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("log.txt");
    let mut isolate = Isolate::new();

    let done = evaluated(
        &mut isolate,
        &format!(
            "(await (file/append {path} \"one\\n\")) (await (file/append {path} \"two\\n\"))",
            path = literal(&path)
        ),
    );

    assert_eq!(done, "nil");
    assert_eq!(fs::read_to_string(&path).unwrap(), "one\ntwo\n");
}

#[test]
fn files_travel_as_base64_whatever_their_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("bytes.bin");
    let copy = dir.path().join("copy.bin");
    let bytes = [0_u8, 159, 146, 150, 255, 10];
    fs::write(&source, bytes).unwrap();
    let mut isolate = Isolate::new();

    let encoded = evaluated(
        &mut isolate,
        &format!(
            "(let [encoded (await (file/read-base64 {}))] (await (file/write-base64 {} encoded)) encoded)",
            literal(&source),
            literal(&copy)
        ),
    );

    assert_eq!(encoded, r#""AJ+Slv8K""#);
    assert_eq!(fs::read(&copy).unwrap(), bytes);
    assert_eq!(
        evaluated(
            &mut isolate,
            &format!(
                "(await (file/write-base64 {} \"#not base64#\"))",
                literal(&copy)
            )
        ),
        "error: file/write-base64 got text that isn't base64"
    );
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}

#[cfg(windows)]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, at)
}

#[test]
fn links_in_listings_say_where_they_point() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("real.txt");
    fs::write(&target, "hello").unwrap();
    if link(&target, &dir.path().join("shortcut")).is_err() {
        return;
    }
    let mut isolate = Isolate::new();

    let listed = evaluated(
        &mut isolate,
        &format!(
            "(map (fn [entry] [entry.name entry.link (if entry.target (= entry.target {}) nil)]) (await (file/list {})))",
            literal(&target),
            literal(dir.path())
        ),
    );

    assert_eq!(listed, r#"[["real.txt" false nil] ["shortcut" true true]]"#);
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
    assert_eq!(
        evaluated(&mut isolate, "(path/temp)"),
        format!("{:?}", std::env::temp_dir().display().to_string())
    );
}
