use std::fs;
use std::process::Command;

const BUSY: &str = "(def (leaf n) (reduce + 0 (range n)))
(def (branch) (leaf 200) (leaf 200))
(def (repeat-branch k) (when (> k 0) (branch) (repeat-branch (- k 1))))
(def (sorting) (sort (vec (reverse (range 20000)))))";

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
