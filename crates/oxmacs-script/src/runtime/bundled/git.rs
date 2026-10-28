use std::fs;
use std::path::Path;

use oxmacs_core::editor::Editor;
use oxmacs_core::special_buffer::SpecialBuffer;
use oxmacs_core::theme::Highlight;

use crate::Ox;
use crate::api::buffers::LinesBuffer;
use crate::testing::{at_point, git, press, question, settle};

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
    fs::write(
        home.path().join("init.ox"),
        format!("(use-plugin git)\n{init_ox}"),
    )
    .unwrap();
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
        let (_, view) = self.editor.special_buffer::<LinesBuffer>(id).unwrap();
        view.pills(line)
    }

    fn resize(&mut self, columns: usize) {
        for window in self.editor.tiling().windows() {
            self.editor.resize_pane(window, 30, columns);
        }
        self.editor.prepare_display();
        settle(&mut self.editor, &mut self.ox);
    }

    fn pane(&self) -> String {
        let id = self
            .editor
            .buffer_names()
            .iter()
            .find(|name| name.starts_with("git diff: "))
            .and_then(|name| self.editor.buffer_named(name))
            .expect("a diff pane");
        self.editor.buffer_by_id(id).unwrap().contents()
    }

    fn evaluated(&mut self, source: &str) -> String {
        let source = format!(
            "(module (import (git pane :as pane) (git diff :as diff) (git commit :as commit))) {source}"
        );
        match self.ox.eval(&mut self.editor, &source) {
            Ok(shown) => shown,
            Err(error) => format!("error: {}", error.message),
        }
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
fn c_x_g_in_a_subfolder_shows_the_whole_repository_by_the_path_you_came_from() {
    let dir = repository();
    let inner = dir.path().join("docs").join("notes");
    fs::create_dir_all(&inner).unwrap();
    let mut session = opened(&inner);

    session.press("C-x g");

    assert_eq!(
        session.editor.buffer().name(),
        format!("git: {}", folder_name(dir.path()))
    );
    assert_eq!(session.editor.default_directory(), dir.path());
    assert!(
        session
            .editor
            .watched_directories()
            .contains(&dir.path().join(".git"))
    );
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
    let (dir, mut session) = edited("(use-plugin git) (settings {delete-to-trash: false})");
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

const SAMPLE: &str = r#"(diff/parse "@@ -1,3 +1,4 @@\n one\n-two\n+TWO\n three\n+four\n")"#;

fn rendered(layout: &str, width: usize) -> Session {
    let dir = tempfile::tempdir().unwrap();
    let mut session = opened(dir.path());
    let shown = session.evaluated(&format!(
        r#"(let [out (pane/render {{section: :unstaged path: "a.txt" counted: [2 1]}} {SAMPLE} {layout} {width})] (open-buffer "*pane*" out.lines) (count out.lines))"#
    ));
    assert!(!shown.starts_with("error"), "{shown}");
    session
}

#[test]
fn unified_diffs_number_both_sides() {
    let session = rendered(":unified", 80);

    assert_eq!(
        session.lines(),
        [
            "Unstaged  a.txt  +2 -1",
            "",
            "@@ -1,3 +1,4 @@",
            "1 1  one",
            "2   -two",
            "  2 +TWO",
            "3 3  three",
            "  4 +four",
        ]
    );
    assert_eq!(session.pills(0), [(0..8, Highlight::Link)]);
    assert_eq!(
        session.editor.line_style(4).background,
        Some(Highlight::Removed)
    );
    assert_eq!(
        session.editor.line_style(5).background,
        Some(Highlight::Added)
    );
}

#[test]
fn split_diffs_put_old_and_new_side_by_side() {
    let session = rendered(":split", 40);

    assert_eq!(
        session.lines()[3..],
        [
            "1 one              │ 1 one             ",
            "2 two              │ 2 TWO             ",
            "3 three            │ 3 three           ",
            "                   │ 4 four            ",
        ]
    );
    assert_eq!(
        session.pills(4),
        [(0..18, Highlight::Error), (21..39, Highlight::Success)]
    );
    assert_eq!(session.pills(3), []);
}

#[test]
fn long_lines_are_cut_short_in_split_diffs() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = opened(dir.path());
    session.evaluated(&format!(
        r#"(let [out (pane/render {{section: :unstaged path: "a.txt" counted: [1 1]}} (diff/parse "@@ -1 +1 @@\n-a\n+{}\n") :split 40)] (open-buffer "*pane*" out.lines))"#,
        "x".repeat(50)
    ));

    assert!(
        session.lines()[3].ends_with(&format!("{}…", "x".repeat(15))),
        "{:?}",
        session.lines()
    );
}

#[test]
fn removed_and_added_runs_pair_up_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = opened(dir.path());

    let pairs = session.evaluated(
        r#"(pane/pairs (first (diff/parse "@@ -1,4 +1,5 @@\n a\n-b\n-c\n+B\n d\n+E\n+F\n")))"#,
    );

    assert_eq!(pairs, "[[0 0] [1 3] [2 nil] [4 4] [nil 5] [nil 6]]");
}

#[test]
fn changed_words_stand_out_when_lines_share_others() {
    let mut session = rendered(":unified", 80);
    session.evaluated(
        r#"(let [out (pane/render {section: :unstaged path: "a.txt" counted: [1 1]} (diff/parse "@@ -1 +1 @@\n-the quick fox\n+the slow fox\n") :unified 80)] (open-buffer "*words*" out.lines))"#,
    );

    assert_eq!(session.lines()[3], "1   -the quick fox");
    assert_eq!(session.pills(3), [(9..14, Highlight::Error)]);
    assert_eq!(session.pills(4), [(9..13, Highlight::Success)]);
}

fn paned() -> (tempfile::TempDir, Session) {
    let dir = repository();
    fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\nfour\n").unwrap();
    fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();
    let mut session = opened(dir.path());
    session.resize(160);
    session.press("C-x g");
    (dir, session)
}

#[test]
fn the_status_opens_the_diff_beside_it_and_keeps_focus() {
    let (_dir, session) = paned();

    assert!(session.editor.buffer().name().starts_with("git: "));
    assert_eq!(session.editor.tiling().windows().len(), 2);
    assert!(
        session.pane().starts_with("Unstaged  a.txt  +2 -1\n"),
        "{}",
        session.pane()
    );
}

#[test]
fn the_diff_follows_the_file_at_point() {
    let (_dir, mut session) = paned();

    session.press("C-n");

    assert_eq!(session.at_point(), "  M  b.txt  +1 -1");
    assert!(
        session.pane().starts_with("Unstaged  b.txt  +1 -1\n"),
        "{}",
        session.pane()
    );
}

#[test]
fn t_in_the_diff_switches_to_side_by_side() {
    let (_dir, mut session) = paned();
    session.press("C-x o");

    session.press("t");
    assert!(session.pane().contains(" │ "), "{}", session.pane());

    session.press("t");
    assert!(!session.pane().contains(" │ "));
}

#[test]
fn s_in_the_diff_stages_and_the_diff_moves_with_the_file() {
    let (dir, mut session) = paned();
    session.press("C-x o");

    session.press("n s");

    assert_eq!(staged(dir.path(), "a.txt"), "one\nTWO\nthree\nfour\n");
    assert!(
        session.pane().starts_with("Staged  a.txt  +2 -1\n"),
        "{}",
        session.pane()
    );
}

#[test]
fn a_region_in_a_side_by_side_diff_stages_its_lines() {
    let (dir, mut session) = paned();
    session.press("C-x o");
    session.press("t n C-n C-n");
    assert!(
        session.at_point().starts_with("2 two"),
        "{}",
        session.at_point()
    );

    session.press("C-SPC C-e s");

    assert_eq!(staged(dir.path(), "a.txt"), "one\nTWO\nthree\n");
}

#[test]
fn side_by_side_diffs_refit_when_their_window_changes_width() {
    let (_dir, mut session) = paned();
    session.press("C-x o t");

    session.resize(50);

    let id = session.editor.buffer().id();
    let width = session.editor.buffer_width(id).unwrap();
    let longest = session
        .pane()
        .lines()
        .skip(3)
        .map(|line| line.chars().count())
        .max()
        .unwrap();
    assert!(longest <= width, "{longest} > {width}");
    assert!(longest + 4 > width, "{longest} is far below {width}");
}

#[test]
fn return_in_the_diff_visits_the_new_line() {
    let (dir, mut session) = paned();
    session.press("C-x o");

    session.press("n C-n C-n C-n RET");

    assert_eq!(
        session.editor.buffer().file().unwrap().path,
        dir.path().join("a.txt")
    );
    let buffer = session.editor.buffer();
    assert_eq!(buffer.line_of(buffer.point()), 1);
}

#[test]
fn s_on_a_hunk_line_stages_the_whole_hunk() {
    let (dir, mut session) = edited("");

    session.press("C-x g n n TAB n C-n");
    assert_eq!(session.at_point(), " one", "{:?}", session.lines());
    session.press("s");

    assert_eq!(
        staged(dir.path(), "a.txt"),
        "one\nTWO\nthree\nfour\n",
        "{:?} {:?}",
        session.editor.message(),
        session.lines()
    );
}

#[test]
fn a_region_stages_only_its_lines() {
    let (dir, mut session) = edited("");
    session.press("C-x g n n TAB n C-n C-n C-n C-n C-n");
    assert_eq!(session.at_point(), "+four");

    session.press("C-SPC C-e s");

    assert_eq!(staged(dir.path(), "a.txt"), "one\ntwo\nthree\nfour\n");
    assert!(session.editor.buffer().selection().is_none());
}

#[test]
fn a_region_in_an_untracked_file_stages_part_of_it() {
    let (dir, mut session) = edited("");
    session.press("C-x g TAB n C-n");
    assert_eq!(session.at_point(), "+new");

    session.press("C-SPC C-e s");

    assert_eq!(staged(dir.path(), "c.txt"), "new\n");
}

#[test]
fn u_unstages_hunks_and_lines() {
    let (dir, mut session) = edited("");
    git(dir.path(), &["add", "a.txt"]);
    session.press("C-x g");
    session.go_to("  M  a.txt  +2 -1");

    session.press("TAB n C-n C-n C-SPC C-n C-n");
    let before = (session.at_point(), session.lines());
    session.press("u");
    assert_eq!(
        staged(dir.path(), "a.txt"),
        "one\ntwo\nthree\nfour\n",
        "{:?} {before:?}",
        session.editor.message()
    );

    session.go_to("  M  a.txt  +1");
    session.press("u");
    assert_eq!(staged(dir.path(), "a.txt"), "one\ntwo\nthree\n");
}

#[test]
fn k_on_selected_lines_discards_only_them() {
    let (dir, mut session) = edited("");
    session.press("C-x g n n TAB n C-n C-n C-n C-n C-n C-SPC C-e k");
    assert_eq!(
        question(&session.editor),
        Some("Discard the selected lines? (y or n) ".to_owned())
    );

    session.press("y");

    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\nTWO\nthree\n"
    );
}

fn head(dir: &Path) -> String {
    git(dir, &["log", "-1", "--format=%B"])
        .trim_end()
        .to_owned()
}

fn commits(dir: &Path) -> String {
    git(dir, &["rev-list", "--count", "HEAD"]).trim().to_owned()
}

fn staged_change() -> (tempfile::TempDir, Session) {
    let dir = repository();
    fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\n").unwrap();
    git(dir.path(), &["add", "a.txt"]);
    let mut session = opened(dir.path());
    session.press("C-x g");
    (dir, session)
}

impl Session {
    fn typed(&mut self, text: &str) {
        crate::testing::typed(&mut self.editor, &mut self.ox, text);
    }
}

#[test]
fn c_c_writes_a_message_that_c_c_c_c_commits() {
    let (dir, mut session) = staged_change();

    session.press("c c");
    assert_eq!(session.editor.buffer().name(), "COMMIT_EDITMSG");
    assert_eq!(session.editor.buffer().point(), 0);
    let text = session.editor.buffer().contents();
    assert!(
        text.contains("# Changes to be committed:\n#   a.txt\n"),
        "{text}"
    );
    session.typed("Shout the second line");
    session.press("C-c C-c");

    assert_eq!(head(dir.path()), "Shout the second line");
    assert_eq!(commits(dir.path()), "2");
    assert!(session.editor.buffer().name().starts_with("git: "));
    let message = session.editor.message().unwrap_or_default().to_owned();
    assert!(message.starts_with("Committed "), "{message}");
    assert!(message.ends_with(" Shout the second line"), "{message}");
    assert_eq!(session.editor.buffer_named("COMMIT_EDITMSG"), None);
    assert!(!session.shows("Staged changes (1)"));
}

#[test]
fn the_message_shows_the_staged_changes_beside_it() {
    let (_dir, mut session) = staged_change();

    session.press("c c");

    assert_eq!(session.editor.buffer().name(), "COMMIT_EDITMSG");
    let pane = session
        .editor
        .buffer_names()
        .iter()
        .find(|name| name.starts_with("git diff: "))
        .and_then(|name| session.editor.buffer_named(name))
        .unwrap();
    assert!(session.editor.buffer_shown(pane));
    assert!(
        session.pane().starts_with("Staged  a.txt  +1 -1\n"),
        "{}",
        session.pane()
    );
}

#[test]
fn the_commit_transient_shows_its_switches_and_actions() {
    let (_dir, mut session) = staged_change();

    session.press("c");

    let view = session.editor.transient().unwrap();
    assert_eq!(view.title, "Commit");
    let keys: Vec<String> = view
        .groups
        .iter()
        .flat_map(|group| group.entries.iter().map(|entry| entry.key.clone()))
        .collect();
    assert_eq!(keys, ["-a", "-e", "-n", "-s", "c", "a", "e", "w"]);
}

#[test]
fn nothing_staged_says_so() {
    let dir = repository();
    let mut session = opened(dir.path());

    session.press("C-x g c c");

    assert_eq!(
        session.editor.message(),
        Some("Nothing staged to commit; stage changes with s or use -a")
    );
    assert!(session.editor.buffer().name().starts_with("git: "));
}

#[test]
fn the_all_switch_commits_unstaged_changes_too() {
    let dir = repository();
    fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();
    let mut session = opened(dir.path());

    session.press("C-x g c - a c");
    session.typed("Buzz");
    session.press("C-c C-c");

    assert_eq!(head(dir.path()), "Buzz");
    assert_eq!(git(dir.path(), &["show", "HEAD:b.txt"]), "bee bee\n");
}

#[test]
fn c_c_c_k_cancels_without_committing() {
    let (dir, mut session) = staged_change();
    session.press("c c");
    session.typed("Never mind");

    session.press("C-c C-k");

    assert_eq!(head(dir.path()), "First commit");
    assert!(session.editor.buffer().name().starts_with("git: "));
    assert_eq!(session.editor.message(), Some("Commit canceled"));
    assert_eq!(session.editor.buffer_named("COMMIT_EDITMSG"), None);
}

#[test]
fn an_empty_message_is_refused() {
    let (dir, mut session) = staged_change();

    session.press("c c C-c C-c");

    assert_eq!(
        session.editor.message(),
        Some("Write a commit message first")
    );
    assert_eq!(commits(dir.path()), "1");
    assert_eq!(session.editor.buffer().name(), "COMMIT_EDITMSG");
}

#[test]
fn amending_starts_from_the_head_message() {
    let (dir, mut session) = staged_change();

    session.press("c a");
    assert!(
        session
            .editor
            .buffer()
            .contents()
            .starts_with("First commit\n")
    );
    session.press("C-e");
    session.typed(" again");
    session.press("C-c C-c");

    assert_eq!(head(dir.path()), "First commit again");
    assert_eq!(commits(dir.path()), "1");
    assert_eq!(
        git(dir.path(), &["show", "HEAD:a.txt"]),
        "one\nTWO\nthree\n"
    );
}

#[test]
fn extending_adds_the_staged_changes_without_asking() {
    let (dir, mut session) = staged_change();

    session.press("c e");

    assert_eq!(head(dir.path()), "First commit");
    assert_eq!(commits(dir.path()), "1");
    assert_eq!(
        git(dir.path(), &["show", "HEAD:a.txt"]),
        "one\nTWO\nthree\n"
    );
}

#[test]
fn rewording_keeps_the_staged_changes_staged() {
    let (dir, mut session) = staged_change();

    session.press("c w C-e");
    session.typed("!");
    session.press("C-c C-c");

    assert_eq!(head(dir.path()), "First commit!");
    assert_eq!(
        git(dir.path(), &["diff", "--cached", "--name-only"]),
        "a.txt\n"
    );
}

#[test]
fn m_p_and_m_n_walk_through_earlier_messages() {
    let (_dir, mut session) = staged_change();

    session.press("c c M-p");
    assert!(
        session
            .editor
            .buffer()
            .contents()
            .starts_with("First commit\n")
    );

    session.press("M-n");
    assert!(
        session
            .editor
            .buffer()
            .contents()
            .starts_with("\n\n# Write")
    );
}

#[test]
fn long_summaries_comments_and_trailers_are_highlighted() {
    let (_dir, mut session) = staged_change();
    session.press("c c");

    session.typed(&"x".repeat(55));

    assert_eq!(
        session.editor.line_style(0).spans,
        [(0..50, Highlight::Heading), (50..55, Highlight::Warning)]
    );
    let comment = session.editor.line_style(2).spans;
    assert_eq!(comment.len(), 1);
    assert_eq!(comment[0].1, Highlight::Comment);
    assert_eq!(
        session.evaluated(
            r#"[(commit/highlight 3 "Signed-off-by: Ada <ada@example.com>") (commit/highlight 3 "Not a trailer: really")]"#
        ),
        "[[[0 14 :keyword]] []]"
    );
}

#[test]
fn commit_messages_opened_elsewhere_get_their_mode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("COMMIT_EDITMSG");
    fs::write(&path, format!("{}\n\n# comment\n", "x".repeat(55))).unwrap();
    let mut session = opened(dir.path());

    session.editor.open_file(&path);
    settle(&mut session.editor, &mut session.ox);

    assert_eq!(
        session
            .editor
            .language()
            .map(oxmacs_core::syntax::Language::name),
        Some("Git commit")
    );
    assert_eq!(
        session.editor.line_style(0).spans,
        [(0..50, Highlight::Heading), (50..55, Highlight::Warning)]
    );
}
