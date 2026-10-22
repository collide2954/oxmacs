use std::fs;
use std::process::Command;
use std::time::Duration;

use ox::Isolate;
use ox::profile::Kind;

const BUSY: &str = "(def (leaf n) (reduce + 0 (range n)))
(def (branch) (leaf 200) (leaf 200))
(def (repeat-branch k) (when (> k 0) (branch) (repeat-branch (- k 1))))
(def (sorting) (sort (vec (reverse (range 20000)))))";

#[test]
fn profiles_count_calls_and_time_each_path_through_the_code() {
    let mut isolate = Isolate::new();
    isolate.eval_named("busy.ox", BUSY).unwrap();
    assert!(!isolate.profiling());

    assert!(isolate.start_profiling());
    assert!(!isolate.start_profiling());
    isolate.eval("(repeat-branch 300) (sorting)").unwrap();
    let profile = isolate.stop_profiling().unwrap();

    assert!(!isolate.profiling());
    assert!(isolate.stop_profiling().is_none());
    let leaf = profile.function("leaf").unwrap();
    assert_eq!(leaf.calls, 600);
    assert_eq!(leaf.kind, Kind::Function);
    assert_eq!(leaf.module, "user");
    assert_eq!(
        leaf.place
            .as_ref()
            .map(|place| (place.file.as_str(), place.line)),
        Some(("busy.ox", 1))
    );
    assert!(leaf.total > Duration::ZERO);
    assert_eq!(profile.function("branch").unwrap().calls, 300);
    let path = profile
        .branch(&["user", "repeat-branch", "branch", "leaf"])
        .unwrap();
    assert!(path.total > Duration::ZERO);
    let sort = profile.branch(&["user", "sorting", "sort"]).unwrap();
    assert_eq!(sort.kind, Kind::Native);
    assert!(sort.total > Duration::ZERO);
    assert!(profile.busy <= profile.wall);
    assert!(profile.samples > 0);
}

#[test]
fn reports_show_the_tree_and_the_functions_by_self_time() {
    let mut isolate = Isolate::new();
    isolate.eval_named("busy.ox", BUSY).unwrap();
    isolate.start_profiling();
    isolate.eval("(repeat-branch 100)").unwrap();
    isolate.record_tick("frame", Duration::from_millis(12));
    isolate.record_tick("tick", Duration::from_millis(3));
    let profile = isolate.stop_profiling().unwrap();

    let report = profile.report();

    assert!(report.starts_with("Profile: "), "{report}");
    assert!(report.contains("\nSlowest ticks\n"), "{report}");
    assert!(report.contains("12.00 ms  frame"), "{report}");
    assert!(report.contains("\nCall tree"), "{report}");
    assert!(report.contains("\nFunctions by self time"), "{report}");
    assert!(report.contains("  repeat-branch"), "{report}");
    assert!(report.contains("busy.ox:2"), "{report}");
    assert_eq!(profile.ticks[0].kind, "frame");
    let mut isolate = Isolate::new();
    isolate.start_profiling();
    for millis in 0..3000 {
        isolate.record_tick("tick", Duration::from_micros(millis));
    }
    let profile = isolate.stop_profiling().unwrap();
    assert!(profile.ticks.len() <= 2000);
    assert_eq!(profile.ticks[0].took, Duration::from_micros(2999));
}

#[test]
fn ox_code_can_profile_a_piece_of_work_and_read_the_result() {
    let mut isolate = Isolate::new();
    isolate.eval_named("busy.ox", BUSY).unwrap();

    let read = isolate
        .eval(
            "(def found (profile (fn [] (branch))))
             [(get (find (fn [f] (= f.name \"leaf\")) found.functions) :calls)
              (string? found.report)
              (vector? found.tree)
              (number? found.busy)]",
        )
        .map(|value| isolate.write(value));

    assert_eq!(read, Ok("[2 true true true]".to_owned()));
    assert!(!isolate.profiling());
    assert_eq!(
        isolate
            .eval("(profile (fn [] (raise (error \"broke\"))))")
            .unwrap_err()
            .message,
        "broke"
    );
    assert!(!isolate.profiling());
}

#[test]
fn the_ox_command_profiles_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("busy.ox");
    fs::write(&path, format!("{BUSY}\n(repeat-branch 200)\n")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ox"))
        .arg("profile")
        .arg(&path)
        .output()
        .unwrap();

    let printed = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(output.status.success(), "{printed}");
    assert!(printed.starts_with("Profile: "), "{printed}");
    assert!(printed.contains("branch"), "{printed}");
}
