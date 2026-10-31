use oxmacs_core::editor::Editor;

use crate::Ox;
use crate::testing::settle;

fn started() -> (tempfile::TempDir, Editor, Ox) {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("init.ox"), "(use-plugin wsl)").unwrap();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    (home, editor, ox)
}

#[test]
fn the_wsl_commands_exist_before_the_plugin_loads() {
    let (_home, editor, _) = started();

    for command in [
        "wsl",
        "wsl-shell",
        "wsl-run",
        "wsl-find-file",
        "wsl-copy-linux-path",
    ] {
        assert!(editor.command_named(command).is_some(), "{command}");
    }
}

#[test]
fn loading_it_runs_programs_in_distro_folders_inside_the_distro() {
    let (home, mut editor, mut ox) = started();
    let file = home.path().join("notes.txt");
    std::fs::write(&file, "x").unwrap();
    editor.open_file(&file);

    let id = editor.command_named("wsl-copy-linux-path").unwrap();
    editor.execute(id);
    settle(&mut editor, &mut ox);

    let shown = editor.message().unwrap_or_default().to_owned();
    assert!(shown.starts_with("Copied "), "{shown}");
    let inside = oxmacs_process::Launch::new("cargo")
        .directory(r"\\wsl.localhost\Fedora\home\ana")
        .distro();
    assert_eq!(inside.map(|place| place.path), Some("/home/ana".to_owned()));
}
