use std::process::Command;

fn oxmacs(args: &[&str]) -> (Option<i32>, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_oxmacs"))
        .args(args)
        .output()
        .unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn version_and_help_print_without_opening_a_window() {
    let (code, out, _) = oxmacs(&["--version"]);
    assert_eq!(code, Some(0));
    assert_eq!(out.trim(), format!("oxmacs {}", env!("CARGO_PKG_VERSION")));

    let (code, out, _) = oxmacs(&["--help"]);
    assert_eq!(code, Some(0));
    assert!(out.contains("-q, --no-init"), "{out}");
}

#[test]
fn unknown_options_fail_and_point_at_the_help() {
    let (code, out, err) = oxmacs(&["--frobnicate"]);

    assert_eq!(code, Some(2));
    assert_eq!(out, "");
    assert!(err.contains("unknown option --frobnicate"), "{err}");
    assert!(err.contains("oxmacs --help"), "{err}");
}
