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
fn ox_migrate_updates_old_files_in_place_and_check_only_reports() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.ox");
    let current = dir.path().join("current.ox");
    fs::write(&old, "(def (moved p) (with p :x 1))\n").unwrap();
    fs::write(&current, "(def (moved p) (assoc p :x 1))\n").unwrap();
    let folder = dir.path().to_str().unwrap();

    let (code, printed, _) = ox(&["migrate", "--check", folder]);
    assert_eq!(code, 1);
    assert!(printed.contains("old.ox"), "{printed}");
    assert!(!printed.contains("current.ox"), "{printed}");
    assert_eq!(
        fs::read_to_string(&old).unwrap(),
        "(def (moved p) (with p :x 1))\n"
    );

    let (code, printed, _) = ox(&["migrate", folder]);
    assert_eq!(code, 0);
    assert!(printed.contains("Migrated 1 of 2 files"), "{printed}");
    assert_eq!(
        fs::read_to_string(&old).unwrap(),
        "(def (moved p) (assoc p :x 1))\n"
    );

    let (code, _, _) = ox(&["migrate", "--check", folder]);
    assert_eq!(code, 0);
}

#[test]
fn ox_migrate_leaves_unreadable_files_alone_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let broken = dir.path().join("broken.ox");
    fs::write(&broken, "(with x\n").unwrap();

    let (code, _, problem) = ox(&["migrate", broken.to_str().unwrap()]);

    assert_eq!(code, 2);
    assert!(problem.contains("broken.ox:1:1"), "{problem}");
    assert_eq!(fs::read_to_string(&broken).unwrap(), "(with x\n");
}
