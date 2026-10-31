use crate::support::evaluated;

#[test]
fn share_paths_and_wsl_names_say_where_they_are_inside_a_distro() {
    assert_eq!(
        evaluated(
            "(ox wsl)",
            r#"[(wsl/place "\\\\wsl.localhost\\Fedora\\home\\ana") (wsl/place "wsl:Fedora:/etc") (wsl/place "C:\\work")]"#
        ),
        r#"[{distro: "Fedora" path: "/home/ana"} {distro: "Fedora" path: "/etc"} nil]"#
    );
    assert_eq!(
        evaluated("(ox wsl)", r#"(wsl/share "Fedora" "/home/ana/x")"#),
        r#""\\\\wsl.localhost\\Fedora\\home\\ana\\x""#
    );
}

#[test]
fn files_that_route_into_a_distro_check_cleanly() {
    let mut isolate = ox_library::isolate();

    let found = isolate.check("user", "(module (import (ox wsl))) (wsl/route! false)");

    assert!(
        found
            .iter()
            .all(|diagnostic| diagnostic.severity != ox_lang::check::Severity::Error),
        "{found:?}"
    );
}
