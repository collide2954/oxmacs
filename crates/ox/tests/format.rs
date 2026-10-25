use std::fs;

use ox::Isolate;
use ox::format::{format, format_with, indentation};
use ox::forms::Form;

fn formatted(source: &str) -> String {
    format(source).unwrap_or_else(|problem| panic!("{problem}"))
}

#[test]
fn macros_declare_their_form_and_every_tool_reads_it() {
    let mut isolate = Isolate::new();
    isolate
        .eval("(defmacro ^{indent: 1 defines: 0} defthing [name & body] `(def ~name (do ~@body)))")
        .unwrap();

    let forms = isolate.forms("user");

    assert_eq!(
        forms.get("defthing"),
        Some(Form {
            indent: Some(1),
            defines: Some(0)
        })
    );
    assert_eq!(forms.get("when").and_then(|form| form.indent), Some(1));
    assert_eq!(forms.get("let").and_then(|form| form.indent), Some(1));
    assert!(forms.knows("->"));
    assert!(!forms.knows("map"));
}

#[test]
fn a_files_forms_come_from_its_imports_and_its_own_macros() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("kit.ox"),
        "(defmacro ^{indent: 1} with-kit [k & body] `(do ~@body))\n(defmacro ^{indent: 0} grouped [& body] `(do ~@body))",
    )
    .unwrap();
    let mut isolate = Isolate::new();
    isolate.add_root(dir.path());
    let source = "(module (import (kit :as k :only [grouped])))\n(defmacro ^{indent: 2 defines: 0} defpair [a b & body] nil)\n(k/grouped 1\n2)\n(grouped 1\n2)\n(defpair x y\n1)\n";

    let forms = isolate.forms_for("sample.ox", source);

    assert_eq!(
        forms.get("k/with-kit").and_then(|form| form.indent),
        Some(1)
    );
    assert_eq!(forms.get("grouped").and_then(|form| form.indent), Some(0));
    assert_eq!(forms.get("defpair").and_then(|form| form.defines), Some(0));
    assert_eq!(
        format_with(source, &forms).unwrap(),
        "(module (import (kit :as k :only [grouped])))\n(defmacro ^{indent: 2 defines: 0} defpair [a b & body] nil)\n(k/grouped 1\n  2)\n(grouped 1\n  2)\n(defpair x y\n  1)\n"
    );
}

#[test]
fn metadata_keeps_its_place_when_lines_above_change() {
    assert_eq!(
        formatted("(f\nx)\n(def ^:shadow (g)\n1)\n(defmacro ^{indent: 1} m [x]\nx)\n"),
        "(f\n x)\n(def ^:shadow (g)\n  1)\n(defmacro ^{indent: 1} m [x]\n  x)\n"
    );
}

#[test]
fn bodies_indent_two_and_arguments_align_with_the_first() {
    let source = "(def (area w h)
(* w
h))
(when ok
(println 1)
    (println 2))
(foo bar
baz)
(foo
bar)
(let [x 1
y 2]
x)
";

    assert_eq!(
        formatted(source),
        "(def (area w h)
  (* w
     h))
(when ok
  (println 1)
  (println 2))
(foo bar
     baz)
(foo
 bar)
(let [x 1
      y 2]
  x)
"
    );
}

#[test]
fn collections_align_their_items() {
    let source = "[1
2]
{a 1
b 2}
#{x
y}
[
1]
'(a
b)
";

    assert_eq!(
        formatted(source),
        "[1
 2]
{a 1
 b 2}
#{x
  y}
[
 1]
'(a
  b)
"
    );
}

#[test]
fn spacing_closers_and_blank_lines_are_tidied() {
    assert_eq!(
        formatted("\n\n(f  ( g x ) )(h)\n\n\n\n(i\n  )\n(j ; note\n)"),
        "(f  (g x)) (h)\n\n(i)\n(j ; note\n )\n"
    );
}

#[test]
fn unicode_characters_stay_whole() {
    let source = "(def backspace \\u{8})\n(def snow [\\u{2603} \\a])\n";

    assert_eq!(formatted(source), source);
}

#[test]
fn block_comments_keep_their_text_and_what_follows_is_formatted() {
    let source =
        "(def x 1)\n#| outer (unbalanced\n   #| inner |# still \"quote\n|#\n(def   y\n2)\n";

    assert_eq!(
        formatted(source),
        "(def x 1)\n#| outer (unbalanced\n   #| inner |# still \"quote\n|#\n(def   y\n  2)\n"
    );
}

#[test]
fn strings_comments_and_characters_keep_their_text() {
    let source = "(def text \"a  ( b
   c\")
  ; comment  ( x
(f \\( ;  note )
x)
(g #r\"raw ( \"
y)
";

    assert_eq!(
        formatted(source),
        "(def text \"a  ( b
   c\")
; comment  ( x
(f \\( ;  note )
   x)
(g #r\"raw ( \"
   y)
"
    );
}

#[test]
fn interpolated_strings_holding_strings_stay_whole() {
    let source = "(message \"{(if x \"Trash\" \"Delete\")} {{literal}} {n}\")
(f \"a {(g \"(\")}
b\"
  c)
";

    assert_eq!(
        formatted(source),
        "(message \"{(if x \"Trash\" \"Delete\")} {{literal}} {n}\")
(f \"a {(g \"(\")}
b\"
   c)
"
    );
}

#[test]
fn definitions_and_binding_macros_take_bodies() {
    let source = "(defcommand greet \"Say hello\"
(message \"hi\"))
(fn step [i]
(inc i))
(match x
[1 :one])
(cond
(even? n) :even)
(try (risky)
(catch error e
nil))
(defsomething name
body)
(with-thing x
body)
(loop [i 0]
(again i))
(each [x xs]
(f x))
(case x
[1 :one])
";

    assert_eq!(
        formatted(source),
        "(defcommand greet \"Say hello\"
  (message \"hi\"))
(fn step [i]
  (inc i))
(match x
  [1 :one])
(cond
  (even? n) :even)
(try (risky)
  (catch error e
    nil))
(defsomething name
  body)
(with-thing x
  body)
(loop [i 0]
  (again i))
(each [x xs]
  (f x))
(case x
  [1 :one])
"
    );
}

#[test]
fn formatting_twice_changes_nothing_more() {
    let once = formatted("(a (b\nc)\n[d\ne])\n(f\n  g)\n");

    assert_eq!(formatted(&once), once);
}

#[test]
fn unreadable_code_is_left_alone() {
    let problem = format("(def x (+ 1 2)\n").unwrap_err();

    assert!(problem.contains("never closed"), "{problem}");
}

#[test]
fn the_editor_asks_for_the_indentation_of_the_next_line() {
    assert_eq!(indentation("(def (f x)\n"), Some(2));
    assert_eq!(indentation("(foo bar\n"), Some(5));
    assert_eq!(indentation("(foo\n"), Some(1));
    assert_eq!(indentation("(let [a 1\n"), Some(6));
    assert_eq!(indentation("(f x)\n"), Some(0));
    assert_eq!(indentation("(f \"open\n"), None);
    assert_eq!(indentation("  (when x\n"), Some(4));
}
