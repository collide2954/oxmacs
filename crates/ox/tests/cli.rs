use std::fs;
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn ox() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ox"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

fn repl(input: &str) -> Output {
    let mut child = ox()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn files_run_and_print() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hello.ox");
    fs::write(
        &path,
        "(def (square x) (* x x))\n(println (square 7))\n(print \"done\")",
    )
    .unwrap();

    let output = ox().arg(&path).output().unwrap();

    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "49\ndone");
}

#[test]
fn files_wait_for_their_tasks_and_fail_when_one_raises() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.ox");
    fs::write(
        &path,
        "(spawn (do (sleep 30) (println \"late\")))\n(println \"main done\")",
    )
    .unwrap();

    let output = ox().arg(&path).output().unwrap();

    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "main done\nlate\n");

    fs::write(
        &path,
        "(spawn (do (sleep 10) (raise (error \"boom in a task\"))))\n(println \"main\")",
    )
    .unwrap();
    let failing = ox().arg(&path).output().unwrap();

    assert_eq!(failing.status.code(), Some(1));
    assert!(
        text(&failing.stderr).contains("boom in a task"),
        "{}",
        text(&failing.stderr)
    );
}

#[test]
fn files_may_start_with_a_byte_order_mark() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notepad.ox");
    fs::write(&path, "\u{feff}(println (+ 1 2))").unwrap();

    let output = ox().arg(&path).output().unwrap();

    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "3\n");
}

#[test]
fn help_lists_the_subcommands_and_version_prints_the_version() {
    for flag in ["--help", "-h", "help"] {
        let output = ox().arg(flag).output().unwrap();
        assert!(output.status.success(), "{flag}: {}", text(&output.stderr));
        let shown = text(&output.stdout);
        for subcommand in [
            "ox check",
            "ox fmt",
            "ox test",
            "ox profile",
            "ox perf bench",
            "ox perf sample",
            "ox lsp",
        ] {
            assert!(
                shown.contains(subcommand),
                "{flag} doesn't mention {subcommand}: {shown}"
            );
        }
    }

    let output = ox().arg("--version").output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        text(&output.stdout),
        format!("ox {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn errors_name_the_file_line_and_column() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.ox");
    fs::write(&path, "(def x 1)\n  (/ x 0)").unwrap();

    let output = ox().arg(&path).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stderr),
        format!("{}:2:3: Division by zero\n", path.display())
    );
}

#[test]
fn errors_inside_functions_print_each_frame() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deep.ox");
    fs::write(&path, "(def (f)\n  (/ 1 0))\n(f)\n").unwrap();

    let output = ox().arg(&path).output().unwrap();

    let file = path.display();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stderr),
        format!(
            "{file}:2:3: Division by zero\n  in f at {file}:2:3\n  in top level at {file}:3:1\n"
        )
    );
}

#[test]
fn errors_in_code_passed_through_a_macro_point_at_that_code() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wrapped.ox");
    fs::write(
        &path,
        "(defmacro wrap [& body] `(fn [] ~@body))\n(def run (wrap 1\n  (/ 1 0)))\n(run)\n",
    )
    .unwrap();

    let output = ox().arg(&path).output().unwrap();

    let shown = text(&output.stderr);
    assert!(
        shown.starts_with(&format!("{}:3:3: Division by zero", path.display())),
        "{shown}"
    );
}

#[test]
fn missing_files_are_reported() {
    let output = ox().arg("no-such-file.ox").output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).starts_with("no-such-file.ox: "),
        "{}",
        text(&output.stderr)
    );
}

#[test]
fn the_repl_prints_each_result_and_waits_for_closing_brackets() {
    let output = repl("(+ 1 2)\n(def x 5)\n\n(* x\n x)\n\"s\"\n(/ 1 0)\n");

    assert!(output.status.success());
    assert_eq!(
        text(&output.stdout),
        "ox> 3\nox> x\nox> ox> ... 25\nox> \"s\"\nox> ox> \n"
    );
    assert_eq!(text(&output.stderr), "error: Division by zero\n");
}

const TESTED: &str = "(def (add a b) (+ a b))
(println \"loaded\")

(test \"add sums its arguments\"
  (expect (= (add 1 2) 3)))

(test \"add knows arithmetic\"
  (expect (= (add 1 2) 4)))

(test \"add works with literals\"
  (expect (> 1 2)))
";

#[test]
fn running_a_file_skips_its_tests() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("math.ox");
    fs::write(&path, TESTED).unwrap();

    let output = ox().arg(&path).output().unwrap();

    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "loaded\n");
}

#[test]
fn ox_test_runs_a_plugin_folders_tests_and_skips_what_needs_the_editor() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = dir.path().join("plugins").join("hello");
    fs::create_dir_all(&plugin).unwrap();
    fs::write(
        plugin.join("plugin.ox"),
        "(plugin hello :version \"0.1.0\" :commands [hello])",
    )
    .unwrap();
    fs::write(
        plugin.join("hello.ox"),
        "(module (import (hello words)))\n(defcommand hello \"Say hello\" (message words/greeting))\n(test \"never runs\" (expect false))",
    )
    .unwrap();
    fs::write(
        plugin.join("words.ox"),
        "(def greeting \"Hello\")\n(test \"greets\" (expect (= greeting \"Hello\")))",
    )
    .unwrap();
    fs::write(
        plugin.join("shout.ox"),
        "(module (import (hello words)))\n(def (shout) (str words/greeting \"!\"))\n(test \"shouts\" (expect (= (shout) \"Hello!\")))",
    )
    .unwrap();

    let output = ox().arg("test").arg(&plugin).output().unwrap();

    let shown = text(&output.stdout);
    assert!(output.status.success(), "{shown}");
    assert!(shown.contains("ok    greets\n"), "{shown}");
    assert!(shown.contains("ok    shouts\n"), "{shown}");
    assert!(
        shown.contains(&format!(
            "skip  {}: it needs the editor\n",
            plugin.join("hello.ox").display()
        )),
        "{shown}"
    );
    assert!(shown.ends_with("2 tests passed, 1 skipped\n"), "{shown}");
}

#[test]
fn files_in_a_plugin_folder_import_its_modules() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = dir.path().join("plugins").join("hello");
    fs::create_dir_all(&plugin).unwrap();
    fs::write(
        plugin.join("plugin.ox"),
        "(plugin hello :version \"0.1.0\")",
    )
    .unwrap();
    fs::write(plugin.join("words.ox"), "(def greeting \"Hello\")").unwrap();
    let say = plugin.join("say.ox");
    fs::write(
        &say,
        "(module (import (hello words)))\n(println words/greeting)",
    )
    .unwrap();

    let output = ox().arg(&say).output().unwrap();

    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "Hello\n");
}

#[test]
fn ox_test_fails_a_test_whose_tasks_raise() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("tasks.ox"),
        "(test \"leaves a task that fails\" (spawn (do (sleep 10) (raise (error \"late boom\")))) (expect true))\n(test \"waits for its task\" (def seen (atom nil)) (spawn (reset! seen :done)) (expect true))",
    )
    .unwrap();

    let output = ox().arg("test").arg(dir.path()).output().unwrap();

    let shown = text(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{shown}");
    assert!(
        shown.contains("FAIL  leaves a task that fails\n"),
        "{shown}"
    );
    assert!(shown.contains("late boom"), "{shown}");
    assert!(shown.contains("ok    waits for its task\n"), "{shown}");
}

#[test]
fn ox_test_runs_each_test_and_explains_failures() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("math.ox");
    fs::write(&path, TESTED).unwrap();
    fs::write(
        dir.path().join("other.ox"),
        "(test \"strings join\" (expect (= (str \"a\" \"b\") \"ab\")))",
    )
    .unwrap();

    let output = ox().arg("test").arg(dir.path()).output().unwrap();

    let shown = text(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{shown}");
    assert!(shown.contains("ok    add sums its arguments\n"), "{shown}");
    assert!(shown.contains("FAIL  add knows arithmetic\n"), "{shown}");
    assert!(
        shown.contains("      (= (add 1 2) 4)\n        (add 1 2) => 3\n"),
        "{shown}"
    );
    assert!(shown.contains(&format!("{}:8:", path.display())), "{shown}");
    assert!(shown.contains("      (> 1 2)\n"), "{shown}");
    assert!(shown.contains("ok    strings join\n"), "{shown}");
    assert!(shown.ends_with("4 tests, 2 failed\n"), "{shown}");

    fs::remove_file(&path).unwrap();
    let passing = ox().arg("test").arg(dir.path()).output().unwrap();
    assert!(passing.status.success());
    assert!(text(&passing.stdout).ends_with("1 test passed\n"));
}

#[test]
fn subcommands_refuse_unknown_options_and_missing_paths() {
    let unknown = ox().args(["check", "--frob", "x.ox"]).output().unwrap();
    assert_eq!(unknown.status.code(), Some(2));
    assert!(
        text(&unknown.stderr).starts_with("ox check: unknown option --frob\nusage: ox check "),
        "{}",
        text(&unknown.stderr)
    );

    let missing = ox().args(["test", "no-such-folder"]).output().unwrap();
    assert_eq!(missing.status.code(), Some(2));
    assert!(
        text(&missing.stderr).starts_with("ox test: no-such-folder doesn't exist\n"),
        "{}",
        text(&missing.stderr)
    );
}

#[test]
fn a_double_dash_lets_a_path_start_with_a_dash() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("-odd.ox"), "(def x 1)\n").unwrap();

    let output = ox()
        .current_dir(dir.path())
        .args(["check", "--", "-odd.ox"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", text(&output.stderr));
    assert!(
        text(&output.stdout).contains("Checked 1 file"),
        "{}",
        text(&output.stdout)
    );
}
