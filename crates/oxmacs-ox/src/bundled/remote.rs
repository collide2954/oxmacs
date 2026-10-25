use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use oxmacs_core::editor::Editor;

use crate::Ox;
use crate::testing::{press, settle, typed};

fn started(init: &str) -> (tempfile::TempDir, Editor, Ox) {
    let home = tempfile::tempdir().unwrap();
    fs::write(home.path().join("init.ox"), init).unwrap();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn run(editor: &mut Editor, ox: &mut Ox, command: &str) {
    let id = editor.command_named(command).unwrap();
    editor.execute(id);
    settle(editor, ox);
}

fn answer(editor: &mut Editor, ox: &mut Ox, text: &str) {
    typed(editor, ox, text);
    press(editor, ox, "RET");
}

fn eval(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
    match ox.eval(editor, source) {
        Ok(value) => value,
        Err(error) => panic!("{}", error.message),
    }
}

fn pump(editor: &mut Editor, ox: &mut Ox, done: impl Fn(&Editor) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        ox.tick(editor);
        if done(editor) {
            return true;
        }
        thread::sleep(Duration::from_millis(5));
    }
    false
}

fn go_to(editor: &mut Editor, text: &str) {
    let line = editor
        .buffer()
        .contents()
        .lines()
        .position(|line| line.contains(text))
        .unwrap_or_else(|| panic!("no line has {text:?}: {}", editor.buffer().contents()));
    let start = editor.buffer().line_start(line);
    editor.buffer_mut().set_point(start);
}

fn slashed(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn with_box(folder: &Path) -> String {
    format!(
        r#"(use-plugin remote :config {{hosts: [{{name: "box" shell: ["sh" "-c" "cd '{}' && exec sh"]}}]}})"#,
        slashed(folder)
    )
}

fn browsing() -> (tempfile::TempDir, tempfile::TempDir, Editor, Ox) {
    let far = tempfile::tempdir().unwrap();
    fs::write(far.path().join("notes.txt"), "first line\nsecond line\n").unwrap();
    fs::create_dir(far.path().join("src")).unwrap();
    fs::write(far.path().join("src").join("main.txt"), "a needle here\n").unwrap();
    let (home, mut editor, mut ox) = started(&with_box(far.path()));

    run(&mut editor, &mut ox, "remote");
    assert_eq!(editor.buffer().name(), "*Remote*");
    go_to(&mut editor, "box");
    press(&mut editor, &mut ox, "RET");
    assert!(
        editor.buffer().name().starts_with("box:"),
        "{} {:?}",
        editor.buffer().name(),
        editor.message()
    );
    (far, home, editor, ox)
}

#[test]
fn saved_hosts_go_to_custom_ox_once_and_leave_init_ox_alone() {
    let (home, mut editor, mut ox) = started(";; mine\n");
    for (name, address) in [("web", "10.0.0.5"), ("db", "10.0.0.6")] {
        run(&mut editor, &mut ox, "remote-add-host");
        answer(&mut editor, &mut ox, name);
        answer(&mut editor, &mut ox, "ssh");
        answer(&mut editor, &mut ox, address);
        answer(&mut editor, &mut ox, "ana");
    }

    assert_eq!(
        fs::read_to_string(home.path().join("init.ox")).unwrap(),
        ";; mine\n"
    );
    let custom = fs::read_to_string(home.path().join("custom.ox")).unwrap();
    assert_eq!(
        custom.matches("(configure-plugin 'remote").count(),
        1,
        "{custom}"
    );
    assert!(
        custom.contains(r#""web""#) && custom.contains(r#""db""#),
        "{custom}"
    );
    assert_eq!(
        eval(
            &mut editor,
            &mut ox,
            "(map :name (plugin-setting 'remote :hosts))"
        ),
        r#"["web" "db"]"#
    );
}

#[test]
fn every_sftp_transfer_gets_a_file_of_its_own() {
    let (home, mut editor, mut ox) = started("");

    let found = eval(
        &mut editor,
        &mut ox,
        r#"(module (import (remote ops :as ops) (ox path)))
           (let [first (ops/transfer-file) second (ops/transfer-file)]
             [(= first second) (= (path/parent first) (path/join (oxmacs-home) "remote"))])"#,
    );

    assert_eq!(found, "[false true]", "{}", home.path().display());
}

#[test]
fn remote_files_open_locally_and_go_back_when_saved() {
    let (far, _home, mut editor, mut ox) = browsing();
    let listing = editor.buffer().contents();
    assert!(listing.contains("src/"), "{listing}");
    assert!(listing.contains("notes.txt"), "{listing}");

    go_to(&mut editor, "notes.txt");
    press(&mut editor, &mut ox, "RET");
    assert_eq!(editor.buffer().contents(), "first line\nsecond line\n");
    assert!(editor.buffer().file().is_some());

    press(&mut editor, &mut ox, "M->");
    typed(&mut editor, &mut ox, "third line");
    press(&mut editor, &mut ox, "C-x C-s");
    let sent = pump(&mut editor, &mut ox, |_| {
        fs::read_to_string(far.path().join("notes.txt"))
            .is_ok_and(|text| text.ends_with("third line"))
    });
    assert!(sent, "{:?}", editor.message());
}

#[test]
fn folders_are_managed_from_the_browser() {
    let (far, home, mut editor, mut ox) = browsing();
    let browser = editor.buffer().name().to_owned();

    press(&mut editor, &mut ox, "+");
    answer(&mut editor, &mut ox, "made");
    assert!(far.path().join("made").is_dir(), "{:?}", editor.message());

    go_to(&mut editor, "notes.txt");
    press(&mut editor, &mut ox, "R");
    press(&mut editor, &mut ox, "C-a C-k");
    answer(&mut editor, &mut ox, "renamed.txt");
    assert!(
        far.path().join("renamed.txt").is_file(),
        "{:?}",
        editor.message()
    );

    go_to(&mut editor, "made/");
    press(&mut editor, &mut ox, "D");
    press(&mut editor, &mut ox, "y");
    assert!(!far.path().join("made").exists(), "{:?}", editor.message());

    let local = home.path().join("picture.bin");
    fs::write(&local, [0_u8, 255, 10, 13, 128]).unwrap();
    press(&mut editor, &mut ox, "U");
    press(&mut editor, &mut ox, "C-a C-k");
    answer(&mut editor, &mut ox, &local.display().to_string());
    assert_eq!(
        fs::read(far.path().join("picture.bin")).unwrap(),
        [0_u8, 255, 10, 13, 128]
    );

    let landing = home.path().join("landing");
    fs::create_dir(&landing).unwrap();
    go_to(&mut editor, "picture.bin");
    press(&mut editor, &mut ox, "C");
    press(&mut editor, &mut ox, "C-a C-k");
    answer(
        &mut editor,
        &mut ox,
        &format!("{}{}", landing.display(), std::path::MAIN_SEPARATOR),
    );
    assert_eq!(
        fs::read(landing.join("picture.bin")).unwrap(),
        [0_u8, 255, 10, 13, 128],
        "{:?}",
        editor.message()
    );
    assert_eq!(editor.buffer().name(), browser);
}

#[test]
fn commands_run_and_searches_find_lines_on_the_host() {
    let (_far, _home, mut editor, mut ox) = browsing();

    press(&mut editor, &mut ox, "!");
    answer(&mut editor, &mut ox, "echo hello from the box");
    assert_eq!(editor.buffer().name(), "*remote output*");
    let output = editor.buffer().contents();
    assert!(output.contains("hello from the box"), "{output}");
    assert!(output.contains("Finished"), "{output}");

    press(&mut editor, &mut ox, "q");
    press(&mut editor, &mut ox, "/");
    answer(&mut editor, &mut ox, "needle");
    assert_eq!(editor.buffer().name(), "*remote search*");
    let found = editor.buffer().contents();
    assert!(found.contains("src/main.txt:1: a needle here"), "{found}");

    go_to(&mut editor, "main.txt");
    press(&mut editor, &mut ox, "RET");
    assert_eq!(editor.buffer().contents(), "a needle here\n");
}

#[test]
fn a_command_that_reads_input_gets_none_instead_of_the_shells_next_commands() {
    let (_far, _home, mut editor, mut ox) = browsing();

    press(&mut editor, &mut ox, "!");
    answer(&mut editor, &mut ox, "cat; echo after cat");
    let output = editor.buffer().contents();
    assert!(output.contains("after cat"), "{output}");
    assert!(output.contains("Finished"), "{output}");

    press(&mut editor, &mut ox, "q");
    press(&mut editor, &mut ox, "!");
    answer(&mut editor, &mut ox, "echo still here");
    let output = editor.buffer().contents();
    assert!(output.contains("still here"), "{output}");
}

#[test]
fn a_remote_shell_runs_what_is_typed() {
    let (_far, _home, mut editor, mut ox) = browsing();
    let keys = |editor: &mut Editor, ox: &mut Ox, notation: &str| {
        let keys: oxmacs_core::input::KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
            ox.tick(editor);
        }
    };

    keys(&mut editor, &mut ox, "s");
    assert!(pump(&mut editor, &mut ox, |editor| editor.buffer().name()
        == "*shell box*"));
    keys(
        &mut editor,
        &mut ox,
        "e c h o SPC s h e l l SPC s a y s SPC h i RET",
    );

    let answered = pump(&mut editor, &mut ox, |editor| {
        editor
            .buffer()
            .contents()
            .lines()
            .any(|line| line == "shell says hi")
    });
    assert!(answered, "{}", editor.buffer().contents());
}
