use std::io::Write;
use std::process::{Command, Stdio};

use oxmacs_core::external::r::{HELPER, PROFILE_VARIABLE};
use oxmacs_core::external::tools::{self, Probe};

fn session(input: &str) -> String {
    let Probe::Found { path, .. } = tools::probe_r(None) else {
        panic!("R is not installed");
    };
    let directory = tempfile::tempdir().unwrap();
    let helper = directory.path().join("helper.R");
    std::fs::write(&helper, HELPER).unwrap();
    let mut child = Command::new(path)
        .args(tools::r_arguments())
        .env(PROFILE_VARIABLE, &helper)
        .current_dir(directory.path())
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
    let output = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&output.stdout).replace('\r', "")
}

#[test]
#[ignore = "runs the R installed on this machine"]
fn help_comes_back_as_markers_and_leaves_the_last_value_alone() {
    let output = session(
        "42\ngetOption(\"oxmacs\")$help(\"mean\")\n.Last.value\ngetOption(\"oxmacs\")$help(\"nosuchtopicxyz\")\n?median\nq()\n",
    );

    assert!(output.contains("\u{1}help\u{2}mean "), "{output:?}");
    assert_eq!(output.matches("[1] 42\n").count(), 2, "{output:?}");
    assert!(
        output.contains("\u{1}message\u{2}No R help for nosuchtopicxyz\u{3}"),
        "{output:?}"
    );
    assert!(output.contains("\u{1}help\u{2}median "), "{output:?}");
}
