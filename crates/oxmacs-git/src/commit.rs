use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

use oxmacs_core::editor::{Action, Editor, Group, Switch, Transient};
use oxmacs_core::syntax::{Feature, Indentation, Language, LanguageSpec, Outline};
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;

use crate::pane;
use crate::run::{self, Plan};
use crate::status::Section;
use crate::view::{self, GitState, StatusView};

const SUMMARY: usize = 50;
const FILE: &str = "COMMIT_EDITMSG";
const HISTORY: usize = 30;

static SPEC: LanguageSpec = LanguageSpec {
    name: "Git commit",
    extensions: &[],
    file_names: &[FILE],
    aliases: &[],
    line_comment: Some("#"),
    pairs: &[],
    indentation: Indentation::KEEP,
    grammar: None,
    grammars: &[],
    outline: Outline::NONE,
    features: &[Feature::Highlighting],
};

pub(crate) const LANGUAGE: Language = Language::new(&SPEC);

pub(crate) static COMMIT: Transient = Transient {
    title: "Commit",
    groups: &[
        Group {
            title: "Arguments",
            switches: &[
                Switch {
                    key: 'a',
                    flag: "--all",
                    summary: "Stage all modified and deleted files",
                },
                Switch {
                    key: 'e',
                    flag: "--allow-empty",
                    summary: "Allow a commit without changes",
                },
                Switch {
                    key: 'n',
                    flag: "--no-verify",
                    summary: "Skip the commit hooks",
                },
                Switch {
                    key: 's',
                    flag: "--signoff",
                    summary: "Add a Signed-off-by line",
                },
            ],
            actions: &[],
        },
        Group {
            title: "Create",
            switches: &[],
            actions: &[Action {
                key: 'c',
                summary: "Commit",
                run: |editor, flags| write(editor, Kind::Create, flags),
            }],
        },
        Group {
            title: "Edit HEAD",
            switches: &[],
            actions: &[
                Action {
                    key: 'a',
                    summary: "Amend",
                    run: |editor, flags| write(editor, Kind::Amend, flags),
                },
                Action {
                    key: 'e',
                    summary: "Extend",
                    run: extend,
                },
                Action {
                    key: 'w',
                    summary: "Reword",
                    run: |editor, flags| write(editor, Kind::Reword, flags),
                },
            ],
        },
    ],
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Create,
    Amend,
    Reword,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pending {
    pub(crate) root: PathBuf,
    pub(crate) file: PathBuf,
    pub(crate) args: Vec<String>,
    pub(crate) history: Vec<String>,
    pub(crate) recalled: Option<usize>,
}

pub(crate) fn dispatch(editor: &mut Editor) {
    if view::current(editor).is_none() {
        return;
    }
    editor.open_transient(&COMMIT);
}

fn head_message(root: &Path) -> String {
    crate::repo::messages(root, 1)
        .into_iter()
        .next()
        .unwrap_or_default()
}

fn template(message: &str, view: &StatusView) -> String {
    let mut lines = vec![
        message.trim_end().to_owned(),
        String::new(),
        "# Write the message above; lines starting with # are left out.".to_owned(),
        "# C-c C-c commits and C-c C-k cancels.".to_owned(),
    ];
    if let Some(branch) = &view.status.snapshot.branch {
        lines.extend(["#".to_owned(), format!("# On branch {branch}")]);
    }
    let staged = view.status.files(Section::Staged);
    if !staged.is_empty() {
        lines.push("# Changes to be committed:".to_owned());
        lines.extend(staged.iter().map(|change| format!("#   {}", change.path)));
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn write(editor: &mut Editor, kind: Kind, flags: &[&'static str]) {
    let Some((_, view)) = view::current(editor) else {
        return;
    };
    let born = view.status.snapshot.head.is_some();
    let nothing = view.status.snapshot.staged.is_empty()
        && !flags.contains(&"--all")
        && !flags.contains(&"--allow-empty");
    if kind == Kind::Create && nothing {
        editor.show_message("Nothing staged to commit; stage changes with s or use -a");
        return;
    }
    if kind != Kind::Create && !born {
        editor.show_message("There is no commit to change yet");
        return;
    }
    let root = view.root.clone();
    let file = view.metadata.join(FILE);
    let root_for_pane = root.clone();
    let message = if kind == Kind::Create {
        String::new()
    } else {
        head_message(&root)
    };
    let text = template(&message, view);
    if let Err(error) = fs::write(&file, text) {
        editor.show_message(format!("Could not write {FILE}: {error}"));
        return;
    }
    let mut args: Vec<String> = ["commit", "--cleanup=strip", "-F"]
        .into_iter()
        .map(str::to_owned)
        .chain([file.display().to_string()])
        .chain(flags.iter().map(|flag| (*flag).to_owned()))
        .collect();
    match kind {
        Kind::Create => {}
        Kind::Amend => args.push("--amend".to_owned()),
        Kind::Reword => args.extend(["--amend".to_owned(), "--only".to_owned()]),
    }
    let history = crate::repo::messages(&root, HISTORY);
    if let Some(state) = editor.applet_state_mut::<GitState>() {
        state.pending.retain(|pending| pending.file != file);
        state.pending.push(Pending {
            root,
            file: file.clone(),
            args,
            history,
            recalled: None,
        });
    }
    if editor.open_file(&file) {
        editor.buffer_mut().set_point(0);
        pane::show_staged(editor, &root_for_pane);
    }
}

fn extend(editor: &mut Editor, flags: &[&'static str]) {
    let Some((_, view)) = view::current(editor) else {
        return;
    };
    if view.status.snapshot.head.is_none() {
        editor.show_message("There is no commit to change yet");
        return;
    }
    let root = view.root.clone();
    let mut plan = Plan::default().step(
        ["commit", "--amend", "--no-edit"]
            .into_iter()
            .chain(flags.iter().copied()),
    );
    plan.done = Some(committed);
    run::run(editor, root, plan);
}

fn pending(editor: &Editor) -> Option<(BufferId, Pending)> {
    let buffer = editor.buffer();
    let path = buffer.file()?.path.clone();
    let found = editor
        .applet_state::<GitState>()?
        .pending
        .iter()
        .find(|pending| pending.file == path)?
        .clone();
    Some((buffer.id(), found))
}

fn message_text(text: &str) -> String {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

pub(crate) fn finish(editor: &mut Editor) {
    let Some((id, pending)) = pending(editor) else {
        editor.show_message("This buffer is not a commit message");
        return;
    };
    if message_text(&editor.buffer().contents()).is_empty() {
        editor.show_message("Write a commit message first");
        return;
    }
    if !editor.save_buffer(id) {
        return;
    }
    editor.show_message("Committing…");
    let mut plan = Plan::default().step(pending.args.clone());
    plan.done = Some(committed);
    run::run(editor, pending.root, plan);
}

fn forget(editor: &mut Editor, root: &Path) -> Option<PathBuf> {
    let state = editor.applet_state_mut::<GitState>()?;
    let at = state
        .pending
        .iter()
        .position(|pending| pending.root == root)?;
    Some(state.pending.remove(at).file)
}

fn back_to_status(editor: &mut Editor, root: &Path, file: Option<PathBuf>) {
    if let Some(id) = file.and_then(|file| editor.buffer_visiting(&file)) {
        editor.close_buffer(id);
    }
    if let Some(id) = editor.find_applet_buffer::<StatusView>(|view| view.root == root) {
        editor.show_buffer(id);
        pane::follow(editor);
    }
}

fn committed(editor: &mut Editor, root: &Path) {
    let file = forget(editor, root);
    back_to_status(editor, root, file);
    let head = editor
        .find_applet_buffer::<StatusView>(|view| view.root == root)
        .and_then(|id| editor.applet_buffer::<StatusView>(id))
        .and_then(|(_, view)| view.status.snapshot.head.clone());
    if let Some(head) = head {
        editor.show_message(format!("Committed {} {}", head.id, head.summary));
    }
}

pub(crate) fn cancel(editor: &mut Editor) {
    let Some((_, pending)) = pending(editor) else {
        editor.show_message("This buffer is not a commit message");
        return;
    };
    let file = forget(editor, &pending.root);
    back_to_status(editor, &pending.root, file);
    editor.show_message("Commit canceled");
}

pub(crate) fn recall(editor: &mut Editor, older: bool) {
    let Some((_, pending)) = pending(editor) else {
        return;
    };
    let count = pending.history.len();
    if count == 0 {
        editor.show_message("No earlier messages");
        return;
    }
    let next = match (pending.recalled, older) {
        (None, true) => 0,
        (None, false) => return,
        (Some(at), true) => (at + 1).min(count - 1),
        (Some(0), false) => {
            replace_message(editor, "");
            set_recalled(editor, &pending.file, None);
            return;
        }
        (Some(at), false) => at - 1,
    };
    replace_message(editor, &pending.history[next]);
    set_recalled(editor, &pending.file, Some(next));
}

fn set_recalled(editor: &mut Editor, file: &Path, recalled: Option<usize>) {
    if let Some(state) = editor.applet_state_mut::<GitState>()
        && let Some(pending) = state
            .pending
            .iter_mut()
            .find(|pending| pending.file == file)
    {
        pending.recalled = recalled;
    }
}

fn replace_message(editor: &mut Editor, message: &str) {
    let buffer = editor.buffer_mut();
    let text = buffer.contents();
    let end = if text.starts_with('#') {
        0
    } else {
        text.find("\n#")
            .map_or(buffer.len_chars(), |at| text[..=at].chars().count())
    };
    buffer.delete_range(0..end);
    buffer.insert_at(0, &format!("{}\n\n", message.trim_end()));
    buffer.set_point(0);
}

pub(crate) fn highlight(
    editor: &Editor,
    id: BufferId,
    line: usize,
) -> Vec<(Range<usize>, Highlight)> {
    let Some(buffer) = editor.buffer_by_id(id) else {
        return Vec::new();
    };
    let text = buffer.text_in(buffer.line_start(line)..buffer.line_end(line));
    let width = text.chars().count();
    if text.starts_with('#') {
        return vec![(0..width, Highlight::Comment)];
    }
    match line {
        0 if width > SUMMARY => vec![
            (0..SUMMARY, Highlight::Heading),
            (SUMMARY..width, Highlight::Warning),
        ],
        0 => vec![(0..width, Highlight::Heading)],
        1 if width > 0 => vec![(0..width, Highlight::Error)],
        _ => trailer(&text),
    }
}

fn trailer(text: &str) -> Vec<(Range<usize>, Highlight)> {
    let Some((key, _)) = text.split_once(": ") else {
        return Vec::new();
    };
    let looks = !key.is_empty()
        && key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-');
    if looks {
        vec![(0..key.chars().count() + 1, Highlight::Keyword)]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use oxmacs_core::editor::Editor;
    use oxmacs_core::theme::Highlight;

    use super::highlight;
    use crate::repo::fixtures::{git, repository};
    use crate::testing::{installed, lines, press, run_jobs, type_text};

    fn head(dir: &Path) -> String {
        git(dir, &["log", "-1", "--format=%B"])
            .trim_end()
            .to_owned()
    }

    fn count(dir: &Path) -> String {
        git(dir, &["rev-list", "--count", "HEAD"]).trim().to_owned()
    }

    fn staged_change() -> (tempfile::TempDir, Editor) {
        let dir = repository();
        fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\n").unwrap();
        git(dir.path(), &["add", "a.txt"]);
        let mut editor = installed(dir.path());
        press(&mut editor, "C-x g");
        (dir, editor)
    }

    #[test]
    fn c_c_writes_a_message_that_c_c_c_c_commits() {
        let (dir, mut editor) = staged_change();

        press(&mut editor, "c c");
        assert_eq!(editor.buffer().name(), "COMMIT_EDITMSG");
        assert_eq!(editor.buffer().point(), 0);
        assert!(
            editor
                .buffer()
                .contents()
                .contains("# Changes to be committed:\n#   a.txt\n")
        );
        type_text(&mut editor, "Shout the second line");
        press(&mut editor, "C-c C-c");
        run_jobs(&mut editor);

        assert_eq!(head(dir.path()), "Shout the second line");
        assert_eq!(count(dir.path()), "2");
        assert!(editor.buffer().name().starts_with("git: "));
        let message = editor.message().unwrap_or_default().to_owned();
        assert!(message.starts_with("Committed "), "{message}");
        assert!(message.ends_with(" Shout the second line"), "{message}");
        assert_eq!(editor.buffer_named("COMMIT_EDITMSG"), None);
        assert!(!lines(&editor).contains(&"Staged changes (1)".to_owned()));
    }

    #[test]
    fn the_message_shows_the_staged_changes_beside_it() {
        let (_dir, mut editor) = staged_change();

        press(&mut editor, "c c");

        let id = editor
            .find_applet_buffer::<crate::pane::DiffPane>(|_| true)
            .unwrap();
        assert!(editor.buffer_shown(id));
        let shown = editor.buffer_by_id(id).unwrap().contents();
        assert!(shown.starts_with("Staged  a.txt  +1 -1\n"), "{shown}");
    }

    #[test]
    fn the_commit_transient_shows_its_switches_and_actions() {
        let (_dir, mut editor) = staged_change();

        press(&mut editor, "c");

        let view = editor.transient().unwrap();
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
        let mut editor = installed(dir.path());
        press(&mut editor, "C-x g c c");

        assert_eq!(
            editor.message(),
            Some("Nothing staged to commit; stage changes with s or use -a")
        );
        assert!(editor.buffer().name().starts_with("git: "));
    }

    #[test]
    fn the_all_switch_commits_unstaged_changes_too() {
        let dir = repository();
        fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();
        let mut editor = installed(dir.path());
        press(&mut editor, "C-x g c - a c");
        type_text(&mut editor, "Buzz");
        press(&mut editor, "C-c C-c");
        run_jobs(&mut editor);

        assert_eq!(head(dir.path()), "Buzz");
        assert_eq!(git(dir.path(), &["show", "HEAD:b.txt"]), "bee bee\n");
    }

    #[test]
    fn c_c_c_k_cancels_without_committing() {
        let (dir, mut editor) = staged_change();
        press(&mut editor, "c c");
        type_text(&mut editor, "Never mind");

        press(&mut editor, "C-c C-k");
        run_jobs(&mut editor);

        assert_eq!(head(dir.path()), "First commit");
        assert!(editor.buffer().name().starts_with("git: "));
        assert_eq!(editor.message(), Some("Commit canceled"));
        assert_eq!(editor.buffer_named("COMMIT_EDITMSG"), None);
    }

    #[test]
    fn an_empty_message_is_refused() {
        let (dir, mut editor) = staged_change();
        press(&mut editor, "c c C-c C-c");
        run_jobs(&mut editor);

        assert_eq!(editor.message(), Some("Write a commit message first"));
        assert_eq!(count(dir.path()), "1");
        assert_eq!(editor.buffer().name(), "COMMIT_EDITMSG");
    }

    #[test]
    fn amending_starts_from_the_head_message() {
        let (dir, mut editor) = staged_change();

        press(&mut editor, "c a");
        assert!(editor.buffer().contents().starts_with("First commit\n"));
        press(&mut editor, "C-e");
        type_text(&mut editor, " again");
        press(&mut editor, "C-c C-c");
        run_jobs(&mut editor);

        assert_eq!(head(dir.path()), "First commit again");
        assert_eq!(count(dir.path()), "1");
        assert_eq!(
            git(dir.path(), &["show", "HEAD:a.txt"]),
            "one\nTWO\nthree\n"
        );
    }

    #[test]
    fn extending_adds_the_staged_changes_without_asking() {
        let (dir, mut editor) = staged_change();

        press(&mut editor, "c e");
        run_jobs(&mut editor);

        assert_eq!(head(dir.path()), "First commit");
        assert_eq!(count(dir.path()), "1");
        assert_eq!(
            git(dir.path(), &["show", "HEAD:a.txt"]),
            "one\nTWO\nthree\n"
        );
    }

    #[test]
    fn rewording_keeps_the_staged_changes_staged() {
        let (dir, mut editor) = staged_change();

        press(&mut editor, "c w C-e");
        type_text(&mut editor, "!");
        press(&mut editor, "C-c C-c");
        run_jobs(&mut editor);

        assert_eq!(head(dir.path()), "First commit!");
        assert_eq!(
            git(dir.path(), &["diff", "--cached", "--name-only"]),
            "a.txt\n"
        );
    }

    #[test]
    fn m_p_and_m_n_walk_through_earlier_messages() {
        let (_dir, mut editor) = staged_change();
        press(&mut editor, "c c M-p");
        assert!(editor.buffer().contents().starts_with("First commit\n"));

        press(&mut editor, "M-n");
        assert!(editor.buffer().contents().starts_with("\n\n# Write"));
    }

    #[test]
    fn long_summaries_and_comments_are_highlighted() {
        let (_dir, mut editor) = staged_change();
        press(&mut editor, "c c");
        type_text(&mut editor, &"x".repeat(55));
        let id = editor.buffer().id();

        assert_eq!(
            highlight(&editor, id, 0),
            [(0..50, Highlight::Heading), (50..55, Highlight::Warning)]
        );
        let comment = highlight(&editor, id, 2);
        assert_eq!(comment.len(), 1);
        assert_eq!(comment[0].1, Highlight::Comment);
    }

    #[test]
    fn trailers_name_their_key() {
        assert_eq!(
            super::trailer("Signed-off-by: Ada <ada@example.com>"),
            [(0..14, Highlight::Keyword)]
        );
        assert_eq!(super::trailer("Not a trailer: really"), []);
    }
}
