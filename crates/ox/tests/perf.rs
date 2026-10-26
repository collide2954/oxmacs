use std::fs;
use std::process::Command;

fn ox(arguments: &[&str]) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_ox"))
        .args(arguments)
        .output()
        .unwrap();
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    (
        output.status.success(),
        text(&output.stdout),
        text(&output.stderr),
    )
}

#[test]
fn ox_perf_benchmarks_a_file_and_counts_its_allocations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("suite.ox");
    fs::write(&path, "{grow: (fn [] (vec (range 100))) quiet: (fn [] 1)}").unwrap();
    let file = path.to_str().unwrap();

    let (succeeded, printed, problem) = ox(&["perf", "bench", file, "grow"]);

    assert!(succeeded, "{problem}");
    assert!(printed.starts_with("grow "), "{printed}");
    assert!(!printed.contains(" 0 allocs"), "{printed}");
    assert!(!printed.contains("quiet"), "{printed}");
}

#[test]
fn ox_perf_without_a_command_says_how_to_use_it() {
    let (succeeded, _, problem) = ox(&["perf"]);

    assert!(!succeeded);
    assert!(
        problem.starts_with("ox perf: which one?\nusage: ox perf bench FILE"),
        "{problem}"
    );
}
