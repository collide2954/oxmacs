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

#[test]
#[ignore = "runs the R installed on this machine"]
fn output_and_help_fit_the_width_r_is_given() {
    let output = session(
        "getOption(\"oxmacs\")$width(40L)\n1:30\ngetOption(\"oxmacs\")$help(\"mean\")\nq()\n",
    );

    assert!(
        output.contains("\n[13]") || output.contains("\n[12]"),
        "{output:?}"
    );
    let help = output
        .split_once("\u{1}help\u{2}")
        .and_then(|(_, rest)| rest.split_once('\u{3}'))
        .map(|(help, _)| help.replace("_\u{8}", ""))
        .unwrap();
    let widest = help.lines().map(|line| line.chars().count()).max().unwrap();
    assert!(widest <= 42, "{widest}: {help}");
}

#[test]
#[ignore = "runs the R installed on this machine"]
fn completions_come_back_with_where_they_start() {
    let output = session(
        "mydata <- data.frame(alpha = 1, beta = 2)\ngetOption(\"oxmacs\")$complete(\"mydata$a\", 8L)\ngetOption(\"oxmacs\")$complete(\"x <- pri\", 8L)\ngetOption(\"oxmacs\")$complete(\"library(stat\", 12L)\nq()\n",
    );

    assert!(
        output.contains("\u{1}complete\u{2}0\tmydata$alpha\u{3}"),
        "{output:?}"
    );
    assert!(
        output.contains("\u{1}complete\u{2}5\tprincomp\tprint\t"),
        "{output:?}"
    );
    assert!(
        output.contains("\u{1}complete\u{2}8\tstats\tstats4"),
        "{output:?}"
    );
}
