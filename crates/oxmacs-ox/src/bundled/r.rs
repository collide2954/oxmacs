use std::path::{Path, PathBuf};

use oxmacs_core::editor::{Editor, Effect, Job, JobId};
use oxmacs_core::syntax::Language;
use oxmacs_core::text::Buffer;

use crate::Ox;
use crate::testing::{home_with, press, settle};

mod data;
mod debug;
mod definitions;
mod help;
mod jobs;
mod keys;
mod language;
mod markdown;
mod packages;
mod plots;
mod sessions;
mod smarts;

pub(super) const PROMPT: &str = "\u{1}>\u{2}\u{3}";
pub(super) const CONTINUE: &str = "\u{1}+\u{2}\u{3}";
pub(super) const CONSOLE: &str = "*R:analysis*";
pub(super) const PLOTS: &str = "*R:analysis plots*";

pub(super) struct R {
    pub home: tempfile::TempDir,
    pub editor: Editor,
    pub ox: Ox,
}

impl R {
    pub fn press(&mut self, notation: &str) {
        press(&mut self.editor, &mut self.ox, notation);
    }

    pub fn typed(&mut self, text: &str) {
        crate::testing::typed(&mut self.editor, &mut self.ox, text);
    }

    pub fn output(&mut self, job: JobId, text: &str) {
        self.editor.session_output(job, text);
        settle(&mut self.editor, &mut self.ox);
    }

    pub fn eval(&mut self, source: &str) -> String {
        match self.ox.eval(&mut self.editor, source) {
            Ok(shown) => shown,
            Err(error) => format!("error: {}", error.message),
        }
    }

    pub fn logged(&self) -> String {
        self.editor
            .buffer_named(crate::log::LOG)
            .and_then(|id| self.editor.buffer_by_id(id))
            .map(Buffer::contents)
            .unwrap_or_default()
    }

    pub fn program(&self) -> PathBuf {
        program(self.home.path())
    }

    pub fn contents(&self) -> String {
        self.editor.buffer().contents()
    }

    pub fn lines(&self) -> Vec<String> {
        self.contents().lines().map(str::to_owned).collect()
    }

    pub fn sessions(&mut self) -> Vec<Job> {
        self.editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::StartSession(job) => Some(job),
                _ => None,
            })
            .collect()
    }

    pub fn started(&mut self) -> Vec<JobId> {
        self.sessions().into_iter().map(|job| job.id).collect()
    }

    pub fn sent(&mut self) -> Vec<String> {
        self.sends().into_iter().map(|(_, text)| text).collect()
    }

    pub fn sends(&mut self) -> Vec<(JobId, String)> {
        self.editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Send(job, text) => Some((job, text)),
                _ => None,
            })
            .collect()
    }

    pub fn again(self) -> R {
        let R { home, .. } = self;
        let mut editor = Editor::new();
        editor.set_working_directory(folder());
        let mut ox = Ox::new();
        ox.start(&mut editor, Some(home.path()));
        settle(&mut editor, &mut ox);
        R { home, editor, ox }
    }

    pub fn running(&mut self) -> JobId {
        self.press("C-c C-z");
        let job = self.started()[0];
        self.output(job, PROMPT);
        self.press("C-c C-z");
        self.editor.take_effects();
        job
    }

    pub fn visit(&mut self, name: &str) {
        self.press("C-x b");
        self.typed(name);
        self.press("RET");
    }

    pub fn run(&mut self, command: &str) {
        self.press("M-x");
        self.typed(command);
        self.press("RET");
    }
}

pub(super) fn ox_text(text: &str) -> String {
    format!("{text:?}").replace('{', "{{").replace('}', "}}")
}

pub(super) fn folder() -> PathBuf {
    std::env::temp_dir().join("analysis")
}

fn program(home: &Path) -> PathBuf {
    home.join("R-bin")
}

pub(super) fn arguments() -> Vec<String> {
    let arguments: &[&str] = if cfg!(windows) {
        &["--ess", "--no-save", "--no-restore", "--quiet"]
    } else {
        &[
            "--no-save",
            "--no-restore",
            "--quiet",
            "--interactive",
            "--no-readline",
        ]
    };
    arguments
        .iter()
        .map(|argument| (*argument).to_owned())
        .collect()
}

pub(super) fn with_program(program: Option<&str>, editor: Editor) -> R {
    let home = home_with(&[]);
    let configured = program.map_or_else(
        || {
            let path = self::program(home.path());
            std::fs::write(&path, "").unwrap();
            path.display().to_string()
        },
        str::to_owned,
    );
    std::fs::write(
        home.path().join("init.ox"),
        format!("(use-plugin r :config {{program: {configured:?}}})"),
    )
    .unwrap();
    let mut editor = editor;
    editor.set_working_directory(folder());
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    R { home, editor, ox }
}

pub(super) fn started() -> R {
    with_program(None, Editor::new())
}

pub(super) fn script(text: &str, point: usize) -> R {
    let mut buffer = Buffer::with_text("t", text);
    buffer.set_point(point);
    let mut editor = Editor::with_buffer(buffer);
    editor.set_major_mode(Some(Language::R));
    with_program(None, editor)
}
