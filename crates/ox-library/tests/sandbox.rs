use std::fs;

#[test]
fn checking_never_writes_files_or_runs_programs() {
    let dir = tempfile::tempdir().unwrap();
    let made = dir.path().join("made.txt");
    let shown = made.display().to_string().replace('\\', "/");
    fs::write(
        dir.path().join("writer.ox"),
        format!(
            "(module (import (ox file)))\n(file/write \"{shown}\" \"x\")\n(def (hello) \"hi\")\n"
        ),
    )
    .unwrap();
    let mut isolate = ox_library::isolate();
    isolate.add_root(dir.path());
    let source = format!(
        "(module (import (writer) (ox file) (ox process)))
(defmacro sneaky [] (file/write \"{shown}\" \"y\") 1)
(defmacro runs [] (process/run \"nothing-here\" []) 1)
(sneaky)
(runs)
(writer/hello)
"
    );

    assert_eq!(isolate.check("sample.ox", &source), []);
    assert!(!made.exists());

    isolate
        .eval(&format!(
            "(module (import (ox file)))\n(await (file/write \"{shown}\" \"z\"))"
        ))
        .unwrap();
    assert!(made.exists());
}

#[test]
fn the_library_is_documented_and_its_docs_match_the_arities() {
    let isolate = ox_library::isolate();

    let problems = isolate.undocumented();

    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn workers_get_the_library_too() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("hashing.ox"),
        "(module (import (ox crypto)))\n(def (digest text) (crypto/sha256 text))",
    )
    .unwrap();
    let mut isolate = ox_library::isolate();
    isolate.add_root(dir.path());

    let digest = isolate
        .eval("(await (worker 'hashing 'digest \"ox\"))")
        .unwrap();

    assert_eq!(isolate.write(digest).len(), 66);
}
