mod commit;
mod diff;
mod history;
mod log;
mod mode_line;
mod pane;
mod process;
mod repo;
mod run;
mod stage;
mod status;
mod view;

use std::collections::VecDeque;

use oxmacs_core::applet::Applet;
use oxmacs_core::command::{Command, CommandId, CommandSpec, Scope, ScopeId};
use oxmacs_core::editor::Editor;
use oxmacs_gui::applet::GuiApplet;

use view::GitState;

struct Spec {
    name: &'static str,
    summary: &'static str,
    help: Option<&'static str>,
    keys: &'static [&'static str],
    run: fn(&mut Editor),
}

const GLOBAL: &[Spec] = &[Spec {
    name: "git-status",
    summary: "Show the Git status of the current repository",
    help: Some(
        "The status lists untracked, unstaged and staged files, stashes and recent commits. n and p move between sections and files, TAB shows or hides a file's changes, a hunk or a whole section, RET visits the file at the line shown and g reads the repository again. s stages and u unstages a file, section, hunk or the lines in the region, and k throws changes away after asking. The diff of the file at point shows beside the status, where t switches to side by side. c opens the commit menu; the message is written beside the staged diff and C-c C-c commits it. l opens the log menu. $ lists the Git commands run and what they printed. The status also refreshes itself when files are saved or the repository changes.",
    ),
    keys: &["C-x g"],
    run: view::open,
}];

const STATUS: &[Spec] = &[
    Spec {
        name: "git-next",
        summary: "Move to the next section, file or hunk",
        help: None,
        keys: &["n"],
        run: |editor| view::step(editor, true),
    },
    Spec {
        name: "git-previous",
        summary: "Move to the previous section, file or hunk",
        help: None,
        keys: &["p"],
        run: |editor| view::step(editor, false),
    },
    Spec {
        name: "git-toggle",
        summary: "Show or hide what is under this line",
        help: None,
        keys: &["TAB"],
        run: view::toggle,
    },
    Spec {
        name: "git-visit",
        summary: "Visit the file at the line shown",
        help: None,
        keys: &["RET"],
        run: view::visit,
    },
    Spec {
        name: "git-stage",
        summary: "Stage the file, hunk or selected lines",
        help: None,
        keys: &["s"],
        run: stage::stage,
    },
    Spec {
        name: "git-unstage",
        summary: "Unstage the file, hunk or selected lines",
        help: None,
        keys: &["u"],
        run: stage::unstage,
    },
    Spec {
        name: "git-discard",
        summary: "Throw away the changes to the file, hunk or selected lines",
        help: None,
        keys: &["k"],
        run: stage::discard,
    },
    Spec {
        name: "git-refresh",
        summary: "Read the repository again",
        help: None,
        keys: &["g"],
        run: view::refresh,
    },
    Spec {
        name: "git-commit",
        summary: "Commit, amend, extend or reword",
        help: None,
        keys: &["c"],
        run: commit::dispatch,
    },
    Spec {
        name: "git-process",
        summary: "Show the Git commands run and what they printed",
        help: None,
        keys: &["$"],
        run: process::open,
    },
    Spec {
        name: "git-log",
        summary: "Log the current branch, all branches or another one",
        help: None,
        keys: &["l"],
        run: log::dispatch,
    },
];

const LOG: &[Spec] = &[
    Spec {
        name: "git-log-next",
        summary: "Move to the next commit",
        help: None,
        keys: &["n"],
        run: |editor| log::step(editor, true),
    },
    Spec {
        name: "git-log-previous",
        summary: "Move to the previous commit",
        help: None,
        keys: &["p"],
        run: |editor| log::step(editor, false),
    },
    Spec {
        name: "git-log-refresh",
        summary: "Read the log again",
        help: None,
        keys: &["g"],
        run: log::refresh,
    },
];

const MESSAGE: &[Spec] = &[
    Spec {
        name: "git-commit-finish",
        summary: "Commit with this message",
        help: None,
        keys: &["C-c C-c"],
        run: commit::finish,
    },
    Spec {
        name: "git-commit-cancel",
        summary: "Leave without committing",
        help: None,
        keys: &["C-c C-k"],
        run: commit::cancel,
    },
    Spec {
        name: "git-commit-previous-message",
        summary: "Put an earlier commit message here",
        help: None,
        keys: &["M-p"],
        run: |editor| commit::recall(editor, true),
    },
    Spec {
        name: "git-commit-next-message",
        summary: "Put a later commit message here",
        help: None,
        keys: &["M-n"],
        run: |editor| commit::recall(editor, false),
    },
];

const DIFF: &[Spec] = &[
    Spec {
        name: "git-diff-next",
        summary: "Move to the next hunk",
        help: None,
        keys: &["n"],
        run: |editor| view::step(editor, true),
    },
    Spec {
        name: "git-diff-previous",
        summary: "Move to the previous hunk",
        help: None,
        keys: &["p"],
        run: |editor| view::step(editor, false),
    },
    Spec {
        name: "git-diff-stage",
        summary: "Stage the hunk or selected lines",
        help: None,
        keys: &["s"],
        run: stage::stage,
    },
    Spec {
        name: "git-diff-unstage",
        summary: "Unstage the hunk or selected lines",
        help: None,
        keys: &["u"],
        run: stage::unstage,
    },
    Spec {
        name: "git-diff-discard",
        summary: "Throw away the hunk or selected lines",
        help: None,
        keys: &["k"],
        run: stage::discard,
    },
    Spec {
        name: "git-diff-visit",
        summary: "Visit the file at the line shown",
        help: None,
        keys: &["RET"],
        run: view::visit,
    },
    Spec {
        name: "git-diff-toggle-layout",
        summary: "Switch between one column and side by side",
        help: None,
        keys: &["t"],
        run: pane::toggle_layout,
    },
    Spec {
        name: "git-diff-refresh",
        summary: "Read the repository again",
        help: None,
        keys: &["g"],
        run: pane::refresh,
    },
];

fn register(editor: &mut Editor, scope: ScopeId, specs: &[Spec]) -> Vec<CommandId> {
    let mut added = Vec::new();
    for spec in specs {
        let command = editor.add_command(CommandSpec {
            name: spec.name,
            summary: spec.summary,
            help: spec.help,
            scope,
            run: spec.run,
        });
        for keys in spec.keys {
            editor.bind(scope, keys, command).ok();
        }
        added.push(command);
    }
    added
}

#[derive(Debug)]
pub struct Git;

impl Applet for Git {
    fn name(&self) -> &'static str {
        "git"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn opt_in(&self) -> bool {
        true
    }

    fn install(&self, editor: &mut Editor) {
        let scope = editor.add_scope("git status");
        let diff_scope = editor.add_scope("git diff");
        let process_scope = editor.add_scope("git process");
        let log_scope = editor.add_scope("git log");
        editor.set_applet_state(GitState {
            scope,
            diff_scope,
            process_scope,
            log_scope,
            patches: 0,
            pending: Vec::new(),
            mode_line: mode_line::Cache::default(),
            history: VecDeque::new(),
        });
        register(editor, Scope::Global.into(), GLOBAL);
        register(editor, scope, STATUS);
        register(editor, diff_scope, DIFF);
        register(editor, log_scope, LOG);
        let message = editor.add_scope("git commit");
        register(editor, message, MESSAGE);
        editor.add_language(commit::LANGUAGE, Some(message));
        editor.set_highlighter(commit::LANGUAGE, commit::highlight);
        editor.bind(scope, "q", Command::QuitWindow).ok();
        editor.bind(diff_scope, "q", Command::QuitWindow).ok();
        editor.bind(process_scope, "q", Command::QuitWindow).ok();
        editor.bind(log_scope, "q", Command::QuitWindow).ok();
        editor.on_command(pane::follow);
        editor.on_command(mode_line::notice);
        editor.on_display(pane::fit);
        editor.on_disk_change(view::refresh_all);
        editor.on_disk_change(mode_line::disk_changed);
        editor.on_saved(view::refresh_saved);
        editor.on_saved(mode_line::saved);
        editor.add_mode_line_segment(mode_line::segment);
    }
}

impl GuiApplet for Git {}

#[cfg(test)]
pub(crate) mod testing {
    use std::path::Path;

    use oxmacs_core::editor::{Editor, Effect, Job, Outcome};
    use oxmacs_core::input::KeySequence;

    use crate::Git;

    pub(crate) fn installed(directory: &Path) -> Editor {
        let mut editor = Editor::new();
        editor.install(&Git);
        editor.set_home_directory(None);
        editor.set_working_directory(directory.to_owned());
        editor
    }

    pub(crate) fn press(editor: &mut Editor, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
    }

    pub(crate) fn lines(editor: &Editor) -> Vec<String> {
        editor
            .buffer()
            .contents()
            .split('\n')
            .map(str::to_owned)
            .collect()
    }

    pub(crate) fn run_jobs(editor: &mut Editor) {
        loop {
            let jobs: Vec<Job> = editor
                .take_effects()
                .into_iter()
                .filter_map(|effect| match effect {
                    Effect::Spawn(job) => Some(job),
                    _ => None,
                })
                .collect();
            if jobs.is_empty() {
                return;
            }
            for job in jobs {
                let output = crate::repo::fixtures::isolated()
                    .args(&job.args)
                    .current_dir(&job.directory)
                    .envs(job.env.iter().map(|(key, value)| (key, value)))
                    .envs([
                        ("GIT_CONFIG_COUNT", "1"),
                        ("GIT_CONFIG_KEY_0", "core.autocrlf"),
                        ("GIT_CONFIG_VALUE_0", "false"),
                        ("GIT_AUTHOR_NAME", "Ada Lovelace"),
                        ("GIT_AUTHOR_EMAIL", "ada@example.com"),
                        ("GIT_COMMITTER_NAME", "Ada Lovelace"),
                        ("GIT_COMMITTER_EMAIL", "ada@example.com"),
                    ])
                    .output()
                    .unwrap();
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                for line in stdout.lines().chain(stderr.lines()) {
                    editor.job_output(job.id, line);
                }
                editor.job_finished(job.id, &Outcome::Exited(output.status.code()));
            }
        }
    }

    pub(crate) fn type_text(editor: &mut Editor, text: &str) {
        for character in text.chars() {
            let notation = match character {
                ' ' => "SPC".to_owned(),
                other => other.to_string(),
            };
            press(editor, &notation);
        }
    }

    pub(crate) fn at_point(editor: &Editor) -> String {
        let buffer = editor.buffer();
        let line = buffer.line_of(buffer.point());
        buffer.text_in(buffer.line_start(line)..buffer.line_end(line))
    }
}
