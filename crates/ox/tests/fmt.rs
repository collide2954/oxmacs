use std::fs;
use std::process::Command;

fn ox(arguments: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_ox"))
        .args(arguments)
        .output()
        .unwrap();
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    (
        output.status.code().unwrap_or(-1),
        text(&output.stdout),
        text(&output.stderr),
    )
}

#[test]
fn ox_fmt_formats_in_place_and_check_only_reports() {
    let dir = tempfile::tempdir().unwrap();
    let messy = dir.path().join("messy.ox");
    let tidy = dir.path().join("sub").join("tidy.ox");
    fs::create_dir_all(tidy.parent().unwrap()).unwrap();
    fs::write(&messy, "(def (f x)\nx)\n").unwrap();
    fs::write(&tidy, "(f 1)\n").unwrap();
    let folder = dir.path().to_str().unwrap();

    let (code, printed, _) = ox(&["fmt", "--check", folder]);
    assert_eq!(code, 1);
    assert!(printed.contains("messy.ox"), "{printed}");
    assert!(!printed.contains("tidy.ox"), "{printed}");
    assert_eq!(fs::read_to_string(&messy).unwrap(), "(def (f x)\nx)\n");

    let (code, printed, _) = ox(&["fmt", folder]);
    assert_eq!(code, 0);
    assert!(printed.contains("Formatted 1 of 2 files"), "{printed}");
    assert_eq!(fs::read_to_string(&messy).unwrap(), "(def (f x)\n  x)\n");

    let (code, _, _) = ox(&["fmt", "--check", folder]);
    assert_eq!(code, 0);
}

#[test]
fn ox_fmt_indents_with_the_forms_of_imported_macros_and_editor_macros() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("kit.ox"),
        "(defmacro ^{indent: 1} grouped [label & body] `(do ~@body))\n",
    )
    .unwrap();
    let using = dir.path().join("using.ox");
    fs::write(
        &using,
        "(module (import (kit :only [grouped])))\n(grouped \"a\"\n1)\n",
    )
    .unwrap();
    let plugin = dir.path().join("plugins").join("hello");
    fs::create_dir_all(&plugin).unwrap();
    fs::write(
        plugin.join("plugin.ox"),
        "(plugin hello\n:version \"0.1.0\")\n",
    )
    .unwrap();

    let (code, _, problem) = ox(&["fmt", dir.path().to_str().unwrap()]);

    assert_eq!(code, 0, "{problem}");
    assert_eq!(
        fs::read_to_string(&using).unwrap(),
        "(module (import (kit :only [grouped])))\n(grouped \"a\"\n  1)\n"
    );
    assert_eq!(
        fs::read_to_string(plugin.join("plugin.ox")).unwrap(),
        "(plugin hello\n  :version \"0.1.0\")\n"
    );
}

#[test]
fn ox_fmt_leaves_unreadable_files_alone_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let broken = dir.path().join("broken.ox");
    fs::write(&broken, "(def x\n").unwrap();

    let (code, _, problem) = ox(&["fmt", broken.to_str().unwrap()]);

    assert_eq!(code, 2);
    assert!(problem.contains("broken.ox:1:1"), "{problem}");
    assert_eq!(fs::read_to_string(&broken).unwrap(), "(def x\n");
}
