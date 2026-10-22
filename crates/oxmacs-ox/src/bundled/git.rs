use std::fs;
use std::path::Path;
use std::process::Command;

use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::editor::Editor;
use oxmacs_core::theme::Highlight;

use crate::Ox;
use crate::buffers::LinesBuffer;
use crate::testing::{at_point, press, question, settle};

fn git(directory: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    let output = command.args(args).current_dir(directory).output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

const CONFIG: &str =
    "[user]\n\tname = Ada Lovelace\n\temail = ada@example.com\n[commit]\n\tgpgsign = false\n";

fn configure(directory: &Path) {
    let config = directory.join(".git").join("config");
    let mut text = fs::read_to_string(&config).unwrap();
    text.push_str(CONFIG);
    text = text.replace("autocrlf = true", "autocrlf = false");
    if !text.contains("autocrlf") {
        text.push_str("[core]\n\tautocrlf = false\n");
    }
    fs::write(config, text).unwrap();
}

fn init(directory: &Path) {
    git(directory, &["init", "-q", "-b", "main"]);
    configure(directory);
}

fn repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n").unwrap();
    fs::write(dir.path().join("b.txt"), "bee\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "First commit"]);
    dir
}

struct Session {
    _home: tempfile::TempDir,
    editor: Editor,
    ox: Ox,
}

fn session(directory: &Path, init_ox: &str) -> Session {
    let home = tempfile::tempdir().unwrap();
    fs::write(home.path().join("init.ox"), init_ox).unwrap();
    let mut editor = Editor::new();
    editor.set_home_directory(None);
    editor.set_working_directory(directory.to_path_buf());
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    Session {
        _home: home,
        editor,
        ox,
    }
}

fn opened(directory: &Path) -> Session {
    session(directory, "")
}

fn changed() -> (tempfile::TempDir, Session) {
    let dir = repository();
    fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\n").unwrap();
    fs::write(dir.path().join("c.txt"), "new\n").unwrap();
    fs::write(dir.path().join("d.txt"), "staged\n").unwrap();
    git(dir.path(), &["add", "d.txt"]);
    let session = opened(dir.path());
    (dir, session)
}

impl Session {
    fn press(&mut self, notation: &str) {
        press(&mut self.editor, &mut self.ox, notation);
    }

    fn lines(&self) -> Vec<String> {
        self.editor
            .buffer()
            .contents()
            .split('\n')
            .map(str::to_owned)
            .collect()
    }

    fn shows(&self, line: &str) -> bool {
        self.lines().iter().any(|shown| shown == line)
    }

    fn at_point(&self) -> String {
        at_point(&self.editor)
    }

    fn go_to(&mut self, text: &str) {
        let line = self
            .lines()
            .iter()
            .position(|line| line == text)
            .unwrap_or_else(|| panic!("{text:?} is not in {:?}", self.lines()));
        let point = self.editor.buffer().line_start(line);
        self.editor.buffer_mut().set_point(point);
    }

    fn pills(&self, line: usize) -> Vec<(std::ops::Range<usize>, Highlight)> {
        let id = self.editor.buffer().id();
        let (_, view) = self.editor.applet_buffer::<LinesBuffer>(id).unwrap();
        view.pills(line)
    }
}

fn folder_name(dir: &Path) -> String {
    dir.file_name().unwrap().to_string_lossy().into_owned()
}

#[test]
fn c_x_g_shows_the_status_with_point_on_the_first_file() {
    let (dir, mut session) = changed();

    session.press("C-x g");

    assert_eq!(
        session.editor.buffer().name(),
        format!("git: {}", folder_name(dir.path()))
    );
    assert!(session.editor.buffer().is_read_only());
    assert_eq!(session.at_point(), "  ?  c.txt  +1");
    let shown = session.lines();
    assert!(shown[0].starts_with("Head:     main  "), "{shown:?}");
    assert!(shown[0].ends_with(" First commit"), "{shown:?}");
    assert!(session.shows("  M  a.txt  +1 -1"), "{shown:?}");
    assert!(session.shows("  A  d.txt  +1"), "{shown:?}");
    assert_eq!(session.editor.default_directory(), dir.path());
}

#[test]
fn outside_a_repository_c_x_g_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = opened(dir.path());

    session.press("C-x g");

    assert_eq!(
        session.editor.message(),
        Some("Not inside a Git repository")
    );
    assert_eq!(session.editor.buffer().name(), "*scratch*");
}

#[test]
fn n_and_p_move_between_sections_and_files() {
    let (_dir, mut session) = changed();
    session.press("C-x g");

    for (keys, expected) in [
        ("n", "Unstaged changes (1)"),
        ("n", "  M  a.txt  +1 -1"),
        ("n", "Staged changes (1)"),
        ("p", "  M  a.txt  +1 -1"),
        ("p p p", "Untracked files (1)"),
    ] {
        session.press(keys);
        assert_eq!(session.at_point(), expected, "{keys}");
    }
}

#[test]
fn tab_opens_a_file_diff_and_folds_hunks_and_sections() {
    let (_dir, mut session) = changed();
    session.press("C-x g n n");

    session.press("TAB");
    let shown = session.lines();
    let file = shown
        .iter()
        .position(|line| line.contains("a.txt"))
        .unwrap();
    assert_eq!(
        shown[file + 1..file + 6],
        ["@@ -1,3 +1,3 @@", " one", "-two", "+TWO", " three"]
    );
    assert_eq!(session.at_point(), "  M  a.txt  +1 -1");
    assert_eq!(
        session.editor.line_style(file + 3).background,
        Some(Highlight::Removed)
    );
    assert_eq!(
        session.editor.line_style(file + 4).background,
        Some(Highlight::Added)
    );
    assert_eq!(session.editor.line_style(file + 2).background, None);

    session.press("n TAB");
    assert_eq!(session.at_point(), "@@ -1,3 +1,3 @@");
    assert_eq!(session.lines()[file + 2], "");

    session.press("TAB C-n C-n TAB");
    assert_eq!(session.at_point(), "@@ -1,3 +1,3 @@");
    assert_eq!(session.lines()[file + 2], "");

    session.press("p p TAB");
    assert_eq!(session.at_point(), "Unstaged changes (1) …");
    assert!(!session.lines().iter().any(|line| line.contains("a.txt")));
    assert!(!session.editor.buffer().is_modified());
}

#[test]
fn return_visits_the_file_at_the_line_shown() {
    let (dir, mut session) = changed();
    session.press("C-x g n n TAB n C-n C-n C-n");
    assert_eq!(session.at_point(), "+TWO");

    session.press("RET");

    assert_eq!(
        session.editor.buffer().file().unwrap().path,
        dir.path().join("a.txt")
    );
    let buffer = session.editor.buffer();
    assert_eq!(buffer.line_of(buffer.point()), 1);
}

#[test]
fn g_reads_the_repository_again_keeping_point_on_its_file() {
    let (dir, mut session) = changed();
    session.press("C-x g n n");
    fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();

    session.press("g");

    assert!(session.shows("  M  b.txt  +1 -1"), "{:?}", session.lines());
    assert_eq!(session.at_point(), "  M  a.txt  +1 -1");
}

#[test]
fn c_x_g_again_reuses_and_refreshes_the_buffer() {
    let (dir, mut session) = changed();
    session.press("C-x g");
    session.editor.open_file(&dir.path().join("b.txt"));
    fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();

    session.press("C-x g");

    assert!(session.editor.buffer().name().starts_with("git: "));
    assert!(session.shows("  M  b.txt  +1 -1"), "{:?}", session.lines());
    assert_eq!(
        session
            .editor
            .buffer_names()
            .iter()
            .filter(|name| name.starts_with("git: "))
            .count(),
        1
    );
}

#[test]
fn saving_a_file_refreshes_the_status() {
    let (dir, mut session) = changed();
    session.press("C-x g");
    session.editor.open_file(&dir.path().join("b.txt"));

    session.press("x C-x C-s");
    session.press("C-x b RET");

    assert!(
        session.editor.buffer().name().starts_with("git: "),
        "{} {:?} {:?}",
        session.editor.buffer().name(),
        session.editor.buffer_names(),
        session.editor.message()
    );
    assert!(session.shows("  M  b.txt  +1 -1"), "{:?}", session.lines());
}

#[test]
fn the_status_follows_changes_on_disk() {
    let (dir, mut session) = changed();
    session.press("C-x g");
    assert!(
        session
            .editor
            .watched_directories()
            .contains(&dir.path().join(".git"))
    );

    fs::write(dir.path().join("e.txt"), "later\n").unwrap();
    session.editor.check_disk_changes();
    settle(&mut session.editor, &mut session.ox);

    assert!(
        session.shows("Untracked files (2)"),
        "{:?}",
        session.lines()
    );
}

#[test]
fn upstreams_stashes_and_recent_commits_are_listed() {
    let origin = repository();
    let dir = tempfile::tempdir().unwrap();
    git(
        dir.path(),
        &["clone", "-q", &origin.path().display().to_string(), "."],
    );
    configure(dir.path());
    fs::write(dir.path().join("b.txt"), "bee\nbuzz\n").unwrap();
    git(dir.path(), &["commit", "-q", "-am", "Second commit"]);
    fs::write(dir.path().join("a.txt"), "changed\n").unwrap();
    git(dir.path(), &["stash", "push", "-q", "-m", "wip"]);
    let mut session = opened(dir.path());

    session.press("C-x g");

    let shown = session.lines();
    assert!(shown[0].ends_with(" Second commit"), "{shown:?}");
    assert_eq!(shown[1], "Upstream: origin/main  ↑1 ↓0");
    assert_eq!(session.pills(0)[0], (10..14, Highlight::Link));
    assert_eq!(session.pills(1), [(10..21, Highlight::Function)]);
    assert!(session.shows("Stashes (1)"), "{shown:?}");
    assert!(session.shows("  stash@{0}  On main: wip"), "{shown:?}");
    let recent = shown
        .iter()
        .position(|line| line == "Recent commits")
        .unwrap();
    assert!(
        shown[recent + 1].contains("  Second commit  Ada Lovelace  just now"),
        "{shown:?}"
    );
    assert!(
        shown[recent + 2].contains("  First commit   Ada Lovelace  just now"),
        "{shown:?}"
    );
}

#[test]
fn renames_and_new_repositories_read_well() {
    let dir = repository();
    git(dir.path(), &["mv", "b.txt", "bee.txt"]);
    let mut session = opened(dir.path());
    session.press("C-x g");
    assert!(
        session.shows("  R  bee.txt ← b.txt"),
        "{:?}",
        session.lines()
    );

    let fresh = tempfile::tempdir().unwrap();
    init(fresh.path());
    fs::write(fresh.path().join("a.txt"), "a\n").unwrap();
    git(fresh.path(), &["add", "a.txt"]);
    let mut session = opened(fresh.path());
    session.press("C-x g");
    assert_eq!(session.lines()[0], "Head:     main  No commits yet");
    assert_eq!(session.at_point(), "  A  a.txt  +1");
}

#[test]
fn status_letters_and_counts_have_their_faces() {
    let (_dir, mut session) = changed();
    session.press("C-x g");

    session.go_to("  M  a.txt  +1 -1");
    let line = session
        .editor
        .buffer()
        .line_of(session.editor.buffer().point());
    assert_eq!(session.pills(line), [(2..3, Highlight::Warning)]);
    let spans = session.editor.line_style(line).spans;
    assert!(spans.contains(&(12..14, Highlight::Success)), "{spans:?}");
    assert!(spans.contains(&(15..17, Highlight::Error)), "{spans:?}");
}

fn staged(directory: &Path, path: &str) -> String {
    git(directory, &["show", &format!(":{path}")])
}

fn edited(init_ox: &str) -> (tempfile::TempDir, Session) {
    let dir = repository();
    fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\nfour\n").unwrap();
    fs::write(dir.path().join("c.txt"), "new\nlines\n").unwrap();
    let session = session(dir.path(), init_ox);
    (dir, session)
}

#[test]
fn s_stages_the_file_at_point() {
    let (dir, mut session) = edited("");
    session.press("C-x g n n");
    assert_eq!(session.at_point(), "  M  a.txt  +2 -1");

    session.press("s");

    assert_eq!(staged(dir.path(), "a.txt"), "one\nTWO\nthree\nfour\n");
    assert!(session.shows("Staged changes (1)"), "{:?}", session.lines());
    assert!(!session.shows("Unstaged changes (1)"));
}

#[test]
fn s_on_untracked_files_adds_them() {
    let (dir, mut session) = edited("");
    session.press("C-x g");

    session.press("s");

    assert_eq!(staged(dir.path(), "c.txt"), "new\nlines\n");
    assert!(session.shows("  A  c.txt  +2"), "{:?}", session.lines());
}

#[test]
fn s_on_a_section_stages_everything_in_it() {
    let (dir, mut session) = edited("");
    fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();
    session.press("C-x g n");
    assert_eq!(session.at_point(), "Unstaged changes (2)");

    session.press("s");

    assert!(session.shows("Staged changes (2)"), "{:?}", session.lines());
    assert!(session.shows("Untracked files (1)"));

    fs::write(dir.path().join("e.txt"), "e\n").unwrap();
    session.press("g");
    session.go_to("Untracked files (2)");
    session.press("s");
    assert!(session.shows("Staged changes (4)"), "{:?}", session.lines());
    assert!(
        !session
            .lines()
            .iter()
            .any(|line| line.starts_with("Untracked"))
    );
}

#[test]
fn u_unstages_files_and_sections() {
    let (dir, mut session) = edited("");
    fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();
    git(dir.path(), &["add", "a.txt", "b.txt"]);
    session.press("C-x g");
    session.go_to("  M  a.txt  +2 -1");

    session.press("u");
    assert_eq!(staged(dir.path(), "a.txt"), "one\ntwo\nthree\n");
    assert!(session.shows("Staged changes (1)"), "{:?}", session.lines());

    session.go_to("Staged changes (1)");
    session.press("u");
    assert_eq!(staged(dir.path(), "b.txt"), "bee\n");
    assert!(
        !session
            .lines()
            .iter()
            .any(|line| line.starts_with("Staged"))
    );
}

#[test]
fn u_in_a_new_repository_unstages_without_a_head() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    fs::write(dir.path().join("a.txt"), "a\n").unwrap();
    git(dir.path(), &["add", "a.txt"]);
    let mut session = opened(dir.path());
    session.press("C-x g");
    assert_eq!(session.at_point(), "  A  a.txt  +1");

    session.press("u");

    assert!(session.shows("  ?  a.txt  +1"), "{:?}", session.lines());
}

#[test]
fn k_discards_unstaged_changes_after_asking() {
    let (dir, mut session) = edited("");
    session.press("C-x g n n k");
    assert_eq!(
        question(&session.editor),
        Some("Discard changes to a.txt? (y or n) ".to_owned())
    );

    session.press("y");

    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\ntwo\nthree\n"
    );
    assert!(!session.shows("Unstaged changes (1)"));
}

#[test]
fn k_on_the_unstaged_section_discards_all_of_it() {
    let (dir, mut session) = edited("");
    fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();
    session.press("C-x g n k");
    assert_eq!(
        question(&session.editor),
        Some("Discard all unstaged changes? (y or n) ".to_owned())
    );

    session.press("y");

    assert_eq!(
        fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        "bee\n"
    );
    assert!(dir.path().join("c.txt").exists());
}

#[test]
fn k_on_untracked_files_deletes_or_trashes_them_after_asking() {
    let (dir, mut session) = edited("(use-plugin git :config {discard-to-trash: false})");
    session.press("C-x g k");
    assert_eq!(
        question(&session.editor),
        Some("Delete c.txt? (y or n) ".to_owned())
    );
    session.press("y");
    assert!(!dir.path().join("c.txt").exists());
    assert!(!session.shows("Untracked files (1)"));

    let (dir, mut session) = edited("");
    session.press("C-x g k");
    assert_eq!(
        question(&session.editor),
        Some("Move c.txt to the trash? (y or n) ".to_owned())
    );
    session.press("n");
    assert!(dir.path().join("c.txt").exists());
}

#[test]
fn answering_no_keeps_everything() {
    let (dir, mut session) = edited("");

    session.press("C-x g n n k n");

    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\nTWO\nthree\nfour\n"
    );
}

#[test]
fn staged_changes_must_be_unstaged_before_discarding() {
    let (dir, mut session) = edited("");
    git(dir.path(), &["add", "a.txt"]);
    session.press("C-x g");
    session.go_to("  M  a.txt  +2 -1");

    session.press("k");
    assert_eq!(session.editor.message(), Some("Unstage it first with u"));
    session.press("s");
    assert_eq!(session.editor.message(), Some("Already staged"));
}

#[test]
fn failures_show_what_git_said() {
    let (dir, mut session) = edited("");
    session.press("C-x g n n");
    fs::write(dir.path().join(".git").join("index.lock"), "").unwrap();

    session.press("s");

    let message = session.editor.message().unwrap_or_default().to_owned();
    assert!(message.starts_with("Git: "), "{message}");
}
#[test]
fn a_closed_status_is_forgotten_quietly() {
    let (dir, mut session) = changed();
    session.press("C-x g C-x k RET");
    assert!(!session.editor.buffer().name().starts_with("git: "));

    fs::write(dir.path().join("e.txt"), "later\n").unwrap();
    session.editor.check_disk_changes();
    settle(&mut session.editor, &mut session.ox);

    assert_eq!(session.editor.message(), None);
}
