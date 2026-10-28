use std::fs;
use std::process::Command;

fn ox() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ox"))
}

fn printed(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

#[test]
fn ox_check_prints_findings_like_rustc_and_fails_on_errors() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.ox");
    fs::write(&bad, "(def (area w h) (* w w))\n(frobnicate 1)\n").unwrap();
    fs::write(dir.path().join("good.ox"), "(def (ok) 1)\n").unwrap();

    let output = ox().arg("check").arg(dir.path()).output().unwrap();

    let shown = printed(&output);
    assert_eq!(output.status.code(), Some(1), "{shown}");
    let path = bad.display();
    assert!(
        shown.contains(&format!(
            "warning[unused]: h is never used; call it _h if that's on purpose\n --> {path}:1:14\n  |\n1 | (def (area w h) (* w w))\n  |              ^\n"
        )),
        "{shown}"
    );
    assert!(
        shown.contains(&format!(
            "error[undefined]: frobnicate is not defined\n --> {path}:2:2\n  |\n2 | (frobnicate 1)\n  |  ^^^^^^^^^^\n"
        )),
        "{shown}"
    );
    assert!(
        shown.ends_with("Checked 2 files: 1 error, 1 warning\n"),
        "{shown}"
    );
}

#[test]
fn warnings_alone_pass_unless_checking_is_strict() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("warn.ox");
    fs::write(&file, "(let [x 1] 2)\n").unwrap();

    assert_eq!(
        ox().arg("check").arg(&file).output().unwrap().status.code(),
        Some(0)
    );
    assert_eq!(
        ox().args(["check", "--strict"])
            .arg(&file)
            .output()
            .unwrap()
            .status
            .code(),
        Some(1)
    );
    fs::write(&file, "(let [_x 1] 2)\n").unwrap();
    let clean = ox()
        .args(["check", "--strict"])
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(clean.status.code(), Some(0));
    assert_eq!(printed(&clean), "Checked 1 file: no problems\n");
}

#[test]
fn findings_print_as_json_or_github_annotations() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("bad.ox");
    fs::write(&file, "\n(frobnicate 1)\n").unwrap();
    let path = file.display().to_string();

    let json = ox()
        .args(["check", "--format", "json"])
        .arg(&file)
        .output()
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&printed(&json)).unwrap();
    assert_eq!(
        parsed,
        serde_json::json!([{
            "file": path, "line": 2, "column": 2, "end_line": 2, "end_column": 12,
            "severity": "error", "code": "undefined", "message": "frobnicate is not defined"
        }])
    );

    let github = ox()
        .args(["check", "--format", "github"])
        .arg(&file)
        .output()
        .unwrap();
    let escaped = path
        .replace('%', "%25")
        .replace(':', "%3A")
        .replace(',', "%2C");
    assert_eq!(
        printed(&github),
        format!(
            "::error file={escaped},line=2,col=2,endLine=2,endColumn=12,title=ox check (undefined)::frobnicate is not defined\n"
        )
    );
    assert_eq!(github.status.code(), Some(1));
}

#[test]
fn plugin_files_and_init_ox_are_checked_with_the_editor_known() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = dir.path().join("greeter");
    fs::create_dir(&plugin).unwrap();
    fs::write(
        plugin.join("plugin.ox"),
        "(plugin greeter\n  :commands [greet]\n  :entry \"greeter.ox\")\n",
    )
    .unwrap();
    fs::write(
        plugin.join("greeter.ox"),
        "(module (import (ox ui :only [text]) (greeter words :as words)))\n(defcommand greet \"Say hello\" (message (words/hello (setting :name))))\n(def (view) (text \"hi\"))\n",
    )
    .unwrap();
    fs::write(
        plugin.join("words.ox"),
        "(module (export hello))\n(def (hello name) (str \"Hello \" name))\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("init.ox"),
        "(use-plugin greeter :config {name: \"Ada\"})\n(bind-key \"C-c g\" 'greet)\n",
    )
    .unwrap();
    let plain = dir.path().join("plain.ox");
    fs::write(&plain, "(message \"hi\")\n").unwrap();

    let output = ox().arg("check").arg(dir.path()).output().unwrap();

    let shown = printed(&output);
    assert_eq!(output.status.code(), Some(1), "{shown}");
    assert_eq!(
        shown,
        format!(
            "error[undefined]: message is not defined\n --> {}:1:2\n  |\n1 | (message \"hi\")\n  |  ^^^^^^^\n\nChecked 5 files: 1 error\n",
            plain.display()
        )
    );
}
