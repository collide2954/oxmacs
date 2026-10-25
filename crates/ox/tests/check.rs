use std::fs;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use ox::Isolate;
use ox::check::{Diagnostic, Severity};

fn messages(found: &[Diagnostic]) -> Vec<&str> {
    found.iter().map(|found| found.message.as_str()).collect()
}

fn rewrite(path: &Path, text: &str, step: u64) {
    fs::write(path, text).unwrap();
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(1_000_000 + step))
        .unwrap();
}

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
    let mut isolate = Isolate::new();
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
fn modules_that_need_the_host_to_load_are_skipped_quietly() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("hosted.ox"),
        "(register-it \"x\")\n(def (hello) \"hi\")\n",
    )
    .unwrap();
    let mut isolate = Isolate::new();
    isolate.define_host_native::<String>("register-it", 1, Some(1), |_, _, _| {
        Ok(ox::value::Value::Nil)
    });
    isolate.add_root(dir.path());

    assert_eq!(
        isolate.check("sample.ox", "(module (import (hosted)))\n(hosted/hello)\n"),
        []
    );
}

#[test]
fn checking_that_takes_too_long_stops_with_one_warning() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("slow.ox"),
        "(def (forever) (forever))\n(forever)\n",
    )
    .unwrap();
    let mut isolate = Isolate::new();
    isolate.add_root(dir.path());
    let budget = Duration::from_millis(100);
    let hang = "(defmacro hang [] ((fn again [] (again))))\n(hang)\n(missing)\n";
    let slow = "(module (import (slow)))\n(missing)\n";

    for (source, at) in [(hang, "(hang)"), (slow, "(slow)")] {
        let found = isolate.check_within(budget, "sample.ox", source);
        assert_eq!(messages(&found), ["Checking stopped here after 100 ms"]);
        assert_eq!(found[0].severity, Severity::Warning);
        let span = found[0].span;
        assert_eq!(&source[span.start as usize..span.end as usize], at);
    }
}

#[test]
fn checking_again_loads_imports_that_changed_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let words = dir.path().join("words.ox");
    let letters = dir.path().join("letters.ox");
    fs::write(&letters, "(def (greet) \"hi\")\n").unwrap();
    fs::write(
        &words,
        "(module (import (letters)))\n(def hello letters/greet)\n",
    )
    .unwrap();
    let mut isolate = Isolate::new();
    isolate.add_root(dir.path());
    let source = "(module (import (words)))\n(words/hello)\n";
    assert_eq!(
        messages(&isolate.check("sample.ox", source)),
        Vec::<&str>::new()
    );

    rewrite(&letters, "(def (greet name) name)\n", 1);
    assert_eq!(
        messages(&isolate.check("sample.ox", source)),
        ["words/hello takes 1 argument, got 0"]
    );

    rewrite(&words, "(def (bye) 1)\n", 2);
    assert_eq!(
        messages(&isolate.check("sample.ox", source)),
        ["words/hello is not defined"]
    );
}

fn found(source: &str) -> Vec<(Severity, String, String)> {
    let mut isolate = Isolate::new();
    isolate
        .check("sample.ox", source)
        .into_iter()
        .map(|diagnostic: Diagnostic| {
            let text =
                source[diagnostic.span.start as usize..diagnostic.span.end as usize].to_owned();
            (diagnostic.severity, text, diagnostic.message)
        })
        .collect()
}

#[test]
fn undefined_names_are_errors_that_suggest_a_close_name() {
    let source = "(module (import (ox string :as s)))
(def (area w h) (* w h))
(def (later) (helper 1))
(def (helper x) (s/upper (str x)))
(def (bad y) (helpr y))
(let [p {x: 1}] p.x)
(match [1 2] [[a b] (+ a b)])
(try (raise (error \"x\")) (catch error e e.message))
(def (named) ((fn again [n] (if (> n 0) (again (- n 1)) n)) 3))
(when true (unknown-thing))
'(not checked here)
(s/nope \"x\")
(def (keywords {size 3}) size)";

    assert_eq!(
        found(source),
        [
            (
                Severity::Error,
                "helpr".to_owned(),
                "helpr is not defined; did you mean helper?".to_owned()
            ),
            (
                Severity::Error,
                "unknown-thing".to_owned(),
                "unknown-thing is not defined".to_owned()
            ),
            (
                Severity::Error,
                "s/nope".to_owned(),
                "s/nope is not defined".to_owned()
            ),
        ]
    );
}

#[test]
fn escaped_regex_patterns_and_brace_heavy_strings_suggest_literals() {
    let source = r#"(module (import (ox regex) (ox regex :as re :only [split])))
(regex/find "^(\\d{{4}})$" "2026")
(re/matches? "a+" "aa")
(split "\\s+" "a b")
(regex/find #"\d+" "1")
(regex/find "\"[^\"]*\"" "a")
(str "{{\"a\": 1, \"b\": {{}}}}")
(str "{{one}} only" "{{a}} \n {{b}}")
(def x 1)
(regex/find "{x}\\d" "1")"#;
    let pattern =
        r#"this pattern escapes backslashes or braces; a regex literal, #"…", needs no escapes"#;

    assert_eq!(
        found(source),
        [
            (
                Severity::Warning,
                r#""^(\\d{{4}})$""#.to_owned(),
                pattern.to_owned()
            ),
            (
                Severity::Warning,
                r#""\\s+""#.to_owned(),
                pattern.to_owned()
            ),
            (
                Severity::Warning,
                r#""{{\"a\": 1, \"b\": {{}}}}""#.to_owned(),
                r#"this string escapes braces 2 times; a raw string, #r"…", needs no escapes"#
                    .to_owned()
            ),
        ]
    );
}

#[test]
fn shadowing_a_built_in_warns_unless_marked_or_excluded() {
    let source = "(module (import (ox :exclude [count])))
(def (count _xs) 0)
(def ^:shadow (each _f _xs) nil)
(def (map _f _xs) nil)
(defmacro when [x] x)
(def first-thing 1)
(count (map 1 2))";
    let message = |name: &str| {
        format!(
            "{name} shadows the built-in {name}; mark it ^:shadow, or leave the built-in out with (import (ox :exclude [{name}]))"
        )
    };

    assert_eq!(
        found(source),
        [
            (Severity::Warning, "map".to_owned(), message("map")),
            (Severity::Warning, "when".to_owned(), message("when")),
        ]
    );
}

#[test]
fn checking_a_file_again_forgets_what_it_defined_before() {
    let mut isolate = Isolate::new();
    let modules = isolate.module_count();

    assert_eq!(
        isolate.check(
            "sample.ox",
            "(def (helper) 1)\n(defmacro twice [x] `(do ~x ~x))"
        ),
        []
    );
    let again = isolate.check("sample.ox", "(helper)\n(twice 1)");

    let messages: Vec<&str> = again.iter().map(|found| found.message.as_str()).collect();
    assert_eq!(messages, ["helper is not defined", "twice is not defined"]);
    assert_eq!(isolate.module_count(), modules + 1);
}

#[test]
fn checking_runs_nothing_but_macro_definitions() {
    let source = "(defmacro twice [x] `(do ~x ~x))
(def boom (raise (error \"ran\")))
(twice (frob))
(twice (println \"fine\"))";

    assert_eq!(
        found(source),
        [(
            Severity::Error,
            "frob".to_owned(),
            "frob is not defined".to_owned()
        )]
    );
}

#[test]
fn reader_errors_and_malformed_forms_are_reported_where_they_are() {
    let malformed = found("(def x 1)\n(let [y] y)\n");
    assert_eq!(malformed.len(), 1, "{malformed:?}");
    assert_eq!(malformed[0].0, Severity::Error);
    assert!(malformed[0].1.starts_with("(let"), "{malformed:?}");

    let unread = found("(def x 1)\n(def y\n");
    assert_eq!(unread.len(), 1, "{unread:?}");
    assert_eq!(unread[0].0, Severity::Error);
}

#[test]
fn calls_with_the_wrong_arguments_are_errors_worded_like_the_runtime() {
    let source = "(module (import (ox string)))
(def (area w h) (* w h))
(def (greet name (greeting \"Hi\")) (str greeting name))
(def (total & items) (reduce + 0 items))
(def (box {width 1 height 1}) (* width height))
(defrecord Point [x y])
(area 1 2)
(area 1)
(greet \"Ada\")
(greet \"Ada\" \"Hey\" \"extra\")
(total)
(box :width 3)
(box :depth 3)
(box :width)
(Point 1)
(inc 1 2)
(string/upper \"a\" \"b\")
(let [area (fn [x] x)] (area 1))";

    assert_eq!(
        found(source),
        [
            (
                Severity::Error,
                "area".to_owned(),
                "area takes 2 arguments, got 1".to_owned()
            ),
            (
                Severity::Error,
                "greet".to_owned(),
                "greet takes 1 to 2 arguments, got 3".to_owned()
            ),
            (
                Severity::Error,
                "box".to_owned(),
                "box has no keyword parameter :depth".to_owned()
            ),
            (
                Severity::Error,
                "box".to_owned(),
                "box was given :width without a value".to_owned()
            ),
            (
                Severity::Error,
                "Point".to_owned(),
                "Point takes 2 arguments, got 1".to_owned()
            ),
            (
                Severity::Error,
                "inc".to_owned(),
                "inc takes 1 argument, got 2".to_owned()
            ),
            (
                Severity::Error,
                "string/upper".to_owned(),
                "string/upper takes 1 argument, got 2".to_owned()
            ),
        ]
    );
}

#[test]
fn unused_bindings_and_imports_are_warnings_unless_named_with_an_underscore() {
    let source = "(module (import (ox string :as s) (ox path :as p) (ox regex :only [matches?])))
(def (area w h _unit) (* w w))
(let [a 1 b 2 _c 3] (+ a 1))
(fn [x] (s/upper \"a\"))
(match [1 2] [[first-one second-one] first-one])
(try (raise (error \"x\")) (catch error e 1))
(defmacro twice [form] `(do ~form ~form))
(twice (let [y 1] 2))";
    let warning =
        |text: &str, message: &str| (Severity::Warning, text.to_owned(), message.to_owned());
    let never = |name: &str| format!("{name} is never used; call it _{name} if that's on purpose");

    assert_eq!(
        found(source),
        [
            warning("(ox path :as p)", "(ox path) is imported but never used"),
            warning(
                "(ox regex :only [matches?])",
                "(ox regex) is imported but never used"
            ),
            warning("h", &never("h")),
            warning("b", &never("b")),
            warning("x", &never("x")),
            warning("second-one", &never("second-one")),
            warning("e", &never("e")),
            warning("y", &never("y")),
        ]
    );
}
