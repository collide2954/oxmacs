use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use oxmacs_core::editor::Editor;

use crate::Ox;
use crate::testing::{Runner, press, settle, typed};

fn started_in(directory: Option<&Path>, init_ox: &str) -> (tempfile::TempDir, Editor, Ox) {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("init.ox"), init_ox).unwrap();
    let mut editor = Editor::new();
    if let Some(directory) = directory {
        editor.set_home_directory(None);
        editor.set_working_directory(directory.to_path_buf());
    }
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    (home, editor, ox)
}

fn started() -> (tempfile::TempDir, Editor, Ox) {
    started_in(None, "(use-plugin wsl)")
}

fn inside(distro: &str, script: &str) -> String {
    let output = std::process::Command::new("wsl.exe")
        .args(["-d", distro, "--exec", "/bin/sh", "-c", script])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn pump(editor: &mut Editor, ox: &mut Ox, done: impl Fn(&Editor) -> bool) -> bool {
    let mut runner = Runner::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        runner.step(editor);
        ox.tick(editor);
        if done(editor) {
            return true;
        }
        thread::sleep(Duration::from_millis(5));
    }
    false
}

fn run(editor: &mut Editor, ox: &mut Ox, command: &str) {
    let id = editor.command_named(command).unwrap();
    editor.execute(id);
    settle(editor, ox);
}

#[test]
fn the_wsl_commands_are_there_to_run() {
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

#[cfg(windows)]
#[test]
fn turning_it_on_runs_programs_in_distro_folders_inside_the_distro() {
    let (_home, mut editor, mut ox) = started();
    settle(&mut editor, &mut ox);

    let inside = oxmacs_process::Launch::new("cargo")
        .directory(r"\\wsl.localhost\Fedora\home\ana")
        .distro();
    assert_eq!(inside.map(|place| place.path), Some("/home/ana".to_owned()));
}

#[cfg(not(windows))]
#[test]
fn outside_windows_it_says_so_once_and_each_command_says_so_again() {
    let (home, mut editor, mut ox) = started();
    settle(&mut editor, &mut ox);
    let system = if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    };
    let notice = format!("WSL is part of Windows, so the wsl plugin has nothing to do on {system}");

    assert_eq!(editor.message(), Some(notice.as_str()));
    let file = home.path().join("notes.txt");
    std::fs::write(&file, "x").unwrap();
    editor.open_file(&file);
    editor.show_message("");
    run(&mut editor, &mut ox, "wsl-copy-linux-path");
    assert_eq!(editor.message(), Some(notice.as_str()));
    run(&mut editor, &mut ox, "wsl");
    assert_eq!(editor.buffer().name(), "*WSL*");
    assert!(
        editor.buffer().contents().contains(&notice),
        "{}",
        editor.buffer().contents()
    );
    let inside = oxmacs_process::Launch::new("cargo")
        .directory(r"\\wsl.localhost\Fedora\home\ana")
        .distro();
    assert_eq!(inside, None);
}

#[test]
#[ignore = "lists and works in the WSL distro named by OXMACS_WSL_DISTRO"]
fn with_wsl_2_the_page_lists_distributions_and_windows_paths_reach_them_under_mnt() {
    let Ok(distro) = std::env::var("OXMACS_WSL_DISTRO") else {
        return;
    };
    let (home, mut editor, mut ox) = started();
    settle(&mut editor, &mut ox);

    run(&mut editor, &mut ox, "wsl");
    let shown = editor.buffer().contents();
    assert!(shown.contains(&distro), "{shown}");
    assert!(!shown.contains("WSL 2 isn't installed"), "{shown}");
    let file = home.path().join("notes.txt");
    std::fs::write(&file, "x").unwrap();
    editor.open_file(&file);
    run(&mut editor, &mut ox, "wsl-copy-linux-path");
    let shown = editor.message().unwrap_or_default().to_owned();
    assert!(shown.starts_with("Copied /mnt/"), "{shown}");
    assert!(shown.ends_with("/notes.txt"), "{shown}");
}

#[test]
#[ignore = "checks the tools inside the WSL distro named by OXMACS_WSL_DISTRO"]
fn list_tools_checks_a_running_distribution_from_inside_it() {
    let Ok(distro) = std::env::var("OXMACS_WSL_DISTRO") else {
        return;
    };
    inside(&distro, "true");
    let (_home, mut editor, mut ox) = started();
    settle(&mut editor, &mut ox);

    let id = editor.command_named("list-tools").unwrap();
    editor.execute(id);
    let heading = format!("{distro} (WSL)");
    let checked = pump(&mut editor, &mut ox, |editor| {
        let shown = editor.buffer().contents();
        shown.contains(&heading) && shown.contains("  inotifywait")
    });

    let shown = editor.buffer().contents();
    assert!(checked, "{shown}");
    let row = shown
        .lines()
        .skip_while(|line| *line != heading)
        .find(|line| line.starts_with("  inotifywait"))
        .unwrap();
    assert!(
        row.contains("found /") || row.contains("i installs inotify-tools with"),
        "{row}"
    );
}

#[test]
#[ignore = "works in a repository inside the WSL distro named by OXMACS_WSL_DISTRO"]
fn git_status_and_commits_run_inside_the_distro_holding_the_repository() {
    let Ok(distro) = std::env::var("OXMACS_WSL_DISTRO") else {
        return;
    };
    let folder = format!("/tmp/oxmacs-git-{}", std::process::id());
    inside(
        &distro,
        &format!(
            "rm -rf {folder} && mkdir -p {folder} && cd {folder} && git init -q -b main \
             && git config user.name Ada && git config user.email ada@example.com \
             && git config commit.gpgsign false && printf 'one\\n' > a.txt && git add . \
             && git commit -q -m First && printf 'two\\n' >> a.txt \
             && printf '#!/bin/sh\\nuname -s > .git/ran-on\\n' > .git/hooks/post-commit \
             && chmod +x .git/hooks/post-commit"
        ),
    );
    let share = std::path::PathBuf::from(format!(
        r"\\wsl.localhost\{distro}{}",
        folder.replace('/', r"\")
    ));
    let (_home, mut editor, mut ox) =
        started_in(Some(&share), "(use-plugin wsl)\n(use-plugin git)");

    press(&mut editor, &mut ox, "C-x g");
    let shown = editor.buffer().contents();
    assert!(
        shown.starts_with("Head:     main  "),
        "{shown} {:?} {}",
        editor.message(),
        editor.buffer().name()
    );
    assert!(shown.contains("  M  a.txt  +1"), "{shown}");
    press(&mut editor, &mut ox, "s c c");
    typed(&mut editor, &mut ox, "Second line");
    press(&mut editor, &mut ox, "C-c C-c");

    let message = editor.message().unwrap_or_default().to_owned();
    assert!(message.starts_with("Committed "), "{message}");
    assert_eq!(
        inside(&distro, &format!("cd {folder} && git log -1 --format=%s")),
        "Second line\n"
    );
    assert_eq!(
        inside(&distro, &format!("cat {folder}/.git/ran-on")),
        "Linux\n"
    );
    inside(&distro, &format!("rm -rf {folder}"));
}
