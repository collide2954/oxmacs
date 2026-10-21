use std::collections::VecDeque;
use std::ops::Range;

use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::{Editor, Outcome};
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;

use crate::status::{Line, Rendered, Row};
use crate::view::GitState;

pub(crate) const NAME: &str = "*git process*";
const KEPT: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Ran {
    pub(crate) command: String,
    pub(crate) outcome: Outcome,
    pub(crate) output: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct ProcessView {
    scope: ScopeId,
    rendered: Rendered,
}

impl AppletBuffer for ProcessView {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn line_style(&self, line: usize) -> LineStyle {
        self.rendered.styles.get(line).cloned().unwrap_or_default()
    }

    fn pills(&self, line: usize) -> Vec<(Range<usize>, Highlight)> {
        self.rendered.pills.get(line).cloned().unwrap_or_default()
    }

    fn fixed_name(&self) -> bool {
        true
    }
}

pub(crate) fn command_line(args: &[String]) -> String {
    std::iter::once("git".to_owned())
        .chain(args.iter().map(|arg| {
            if arg.chars().any(char::is_whitespace) {
                format!("\"{arg}\"")
            } else {
                arg.clone()
            }
        }))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn keep(history: &mut VecDeque<Ran>, ran: Ran) {
    history.push_back(ran);
    while history.len() > KEPT {
        history.pop_front();
    }
}

fn ending(outcome: &Outcome) -> (String, Highlight) {
    match outcome {
        Outcome::Exited(Some(0)) => ("exit 0".to_owned(), Highlight::Success),
        Outcome::Exited(Some(code)) => (format!("exit {code}"), Highlight::Error),
        Outcome::Exited(None) => ("killed".to_owned(), Highlight::Error),
        Outcome::Failed(error) => (format!("failed: {error}"), Highlight::Error),
    }
}

pub(crate) fn render(history: &VecDeque<Ran>) -> Rendered {
    let mut out = Rendered::default();
    if history.is_empty() {
        let mut line = Line::default();
        line.push("No Git commands have run yet", Some(Highlight::Comment));
        out.add(Row::Info, line);
        return out;
    }
    for (index, ran) in history.iter().enumerate() {
        if index > 0 {
            out.add(Row::Blank, Line::default());
        }
        let mut line = Line::default();
        line.push(&ran.command, None);
        line.push("  ", None);
        let (text, face) = ending(&ran.outcome);
        line.pill(&text, face);
        out.add(Row::Info, line);
        for output in &ran.output {
            let mut line = Line::default();
            line.push("  ", None);
            line.push(output, Some(Highlight::Comment));
            out.add(Row::Info, line);
        }
    }
    out
}

pub(crate) fn record(editor: &mut Editor, ran: Ran) {
    if let Some(state) = editor.applet_state_mut::<GitState>() {
        keep(&mut state.history, ran);
    }
    if let Some(id) = editor.buffer_named(NAME) {
        redraw(editor, id);
    }
}

fn redraw(editor: &mut Editor, id: BufferId) {
    let rendered = editor
        .applet_state::<GitState>()
        .map(|state| render(&state.history))
        .unwrap_or_default();
    let Some((buffer, view)) = editor.applet_buffer_mut::<ProcessView>(id) else {
        return;
    };
    let newest = rendered
        .rows
        .iter()
        .rposition(|row| *row == Row::Blank)
        .map_or(0, |blank| blank + 1);
    buffer.set_text(&rendered.text);
    let start = buffer.line_start(newest);
    buffer.set_point(start);
    view.rendered = rendered;
}

fn process_buffer(editor: &mut Editor) -> Option<BufferId> {
    if let Some(id) = editor.buffer_named(NAME) {
        return Some(id);
    }
    let scope = editor
        .applet_state::<GitState>()
        .map(|state| state.process_scope)?;
    Some(editor.add_applet_buffer_with(NAME, |buffer| {
        buffer.set_read_only(true);
        ProcessView {
            scope,
            rendered: Rendered::default(),
        }
    }))
}

pub(crate) fn open(editor: &mut Editor) {
    let Some(id) = process_buffer(editor) else {
        return;
    };
    redraw(editor, id);
    editor.show_buffer(id);
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oxmacs_core::editor::Editor;

    use super::*;
    use crate::repo::fixtures::repository;
    use crate::testing::{at_point, installed, lines, press, run_jobs};

    fn ran(command: &str, outcome: Outcome, output: &[&str]) -> Ran {
        Ran {
            command: command.to_owned(),
            outcome,
            output: output.iter().map(|line| (*line).to_owned()).collect(),
        }
    }

    #[test]
    fn command_lines_quote_arguments_with_spaces() {
        let args: Vec<String> = ["commit", "-F", "C:/My Repo/.git/COMMIT_EDITMSG"]
            .into_iter()
            .map(str::to_owned)
            .collect();

        assert_eq!(
            command_line(&args),
            "git commit -F \"C:/My Repo/.git/COMMIT_EDITMSG\""
        );
    }

    #[test]
    fn each_command_shows_how_it_ended_and_its_output() {
        let history = VecDeque::from([
            ran("git add -- a.txt", Outcome::Exited(Some(0)), &[]),
            ran(
                "git apply --cached -- p.patch",
                Outcome::Exited(Some(1)),
                &["error: patch failed"],
            ),
            ran("git status", Outcome::Failed("not found".to_owned()), &[]),
        ]);

        let rendered = render(&history);

        assert_eq!(
            rendered.text.split('\n').collect::<Vec<_>>(),
            [
                "git add -- a.txt  exit 0",
                "",
                "git apply --cached -- p.patch  exit 1",
                "  error: patch failed",
                "",
                "git status  failed: not found",
            ]
        );
        assert_eq!(rendered.pills[0], [(18..24, Highlight::Success)]);
        assert_eq!(rendered.pills[2], [(31..37, Highlight::Error)]);
        assert_eq!(rendered.styles[3].spans, [(2..21, Highlight::Comment)]);
    }

    #[test]
    fn nothing_run_yet_says_so() {
        let rendered = render(&VecDeque::new());

        assert_eq!(rendered.text, "No Git commands have run yet");
    }

    #[test]
    fn only_the_last_fifty_commands_are_kept() {
        let mut history = VecDeque::new();

        for number in 0..55 {
            keep(
                &mut history,
                ran(&format!("git {number}"), Outcome::Exited(Some(0)), &[]),
            );
        }

        assert_eq!(history.len(), 50);
        assert_eq!(history.front().unwrap().command, "git 5");
        assert_eq!(history.back().unwrap().command, "git 54");
    }

    fn changed() -> (tempfile::TempDir, Editor) {
        let dir = repository();
        fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\n").unwrap();
        let mut editor = installed(dir.path());
        press(&mut editor, "C-x g");
        (dir, editor)
    }

    #[test]
    fn dollar_lists_the_commands_run_with_the_newest_last() {
        let (_dir, mut editor) = changed();
        press(&mut editor, "s");
        run_jobs(&mut editor);
        press(&mut editor, "u");
        run_jobs(&mut editor);

        press(&mut editor, "$");

        assert_eq!(editor.buffer().name(), NAME);
        assert!(editor.buffer().is_read_only());
        assert_eq!(
            lines(&editor),
            [
                "git add -- a.txt  exit 0",
                "",
                "git reset -q -- a.txt  exit 0"
            ]
        );
        assert_eq!(at_point(&editor), "git reset -q -- a.txt  exit 0");
    }

    #[test]
    fn failures_keep_what_git_said() {
        let (dir, mut editor) = changed();
        fs::write(dir.path().join(".git").join("index.lock"), "").unwrap();
        press(&mut editor, "s");
        run_jobs(&mut editor);

        press(&mut editor, "$");

        let shown = lines(&editor);
        assert!(shown[0].starts_with("git add -- a.txt  exit "), "{shown:?}");
        assert!(!shown[0].ends_with("exit 0"), "{shown:?}");
        assert!(
            shown[1..].iter().any(|line| line.contains("index.lock")),
            "{shown:?}"
        );
    }

    #[test]
    fn the_process_buffer_follows_new_commands() {
        let (_dir, mut editor) = changed();
        press(&mut editor, "$ q");
        assert!(editor.buffer().name().starts_with("git: "));

        press(&mut editor, "s");
        run_jobs(&mut editor);

        let id = editor.buffer_named(NAME).unwrap();
        let shown = editor.buffer_by_id(id).unwrap().contents();
        assert_eq!(shown, "git add -- a.txt  exit 0");
    }
}
