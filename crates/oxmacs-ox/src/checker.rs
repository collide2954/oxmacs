use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use ox::Isolate;
use ox::check::{Diagnostic, Severity};
use ox::span::line_column;
use oxmacs_core::editor::Editor;
use oxmacs_core::external::diagnostics::{Diagnostic as Found, Severity as Level};
use oxmacs_core::text::BufferId;

pub const CHECK_PAUSE: Duration = Duration::from_millis(500);

struct Job {
    path: PathBuf,
    source: String,
    editor: bool,
}

struct Done {
    path: PathBuf,
    source: String,
    found: Vec<Diagnostic>,
}

#[derive(Debug)]
struct Worker {
    jobs: Sender<Job>,
    done: Receiver<Done>,
}

#[derive(Debug, Default)]
pub(crate) struct Checker {
    worker: Option<Worker>,
    busy: bool,
    checked: Vec<(BufferId, u64)>,
}

fn serve(jobs: &Receiver<Job>, done: &Sender<Done>) {
    let mut editor = None;
    let mut plain = None;
    for job in jobs {
        let isolate = if job.editor {
            editor.get_or_insert_with(crate::checking_isolate)
        } else {
            plain.get_or_insert_with(Isolate::new)
        };
        for root in check_roots(&job.path) {
            isolate.add_root(root);
        }
        let found = isolate.check(&job.path.display().to_string(), &job.source);
        let finished = Done {
            path: job.path,
            source: job.source,
            found,
        };
        if done.send(finished).is_err() {
            return;
        }
    }
}

impl Worker {
    fn start() -> Option<Self> {
        let (jobs, waiting) = mpsc::channel();
        let (finished, done) = mpsc::channel();
        thread::Builder::new()
            .name("oxmacs ox checker".to_owned())
            .spawn(move || serve(&waiting, &finished))
            .ok()?;
        Some(Self { jobs, done })
    }
}

pub fn editor_code(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "init.ox")
        || path
            .parent()
            .is_some_and(|folder| folder.join("plugin.ox").is_file())
}

pub fn check_roots(path: &Path) -> Vec<PathBuf> {
    let Some(folder) = path.parent() else {
        return Vec::new();
    };
    let mut roots = vec![folder.to_path_buf()];
    if folder.join("plugin.ox").is_file()
        && let Some(plugins) = folder.parent()
    {
        roots.push(plugins.to_path_buf());
    }
    roots
}

fn due(editor: &Editor) -> Option<(BufferId, u64, PathBuf)> {
    let buffer = editor.buffer();
    let path = buffer.file()?.path.clone();
    path.extension()
        .is_some_and(|extension| extension == "ox")
        .then(|| (buffer.id(), buffer.revision(), path))
}

fn found(path: &Path, source: &str, diagnostic: Diagnostic) -> Found {
    Found {
        path: path.to_owned(),
        start: line_column(source, diagnostic.span.start),
        end: line_column(source, diagnostic.span.end),
        severity: match diagnostic.severity {
            Severity::Error => Level::Error,
            Severity::Warning => Level::Warning,
        },
        message: diagnostic.message,
    }
}

impl Checker {
    pub(crate) fn wait(&self, editor: &Editor) -> Option<Duration> {
        if self.busy {
            return None;
        }
        let (id, revision, _) = due(editor)?;
        (!self.checked.contains(&(id, revision))).then_some(CHECK_PAUSE)
    }

    pub(crate) fn busy(&self) -> bool {
        self.busy
    }

    pub(crate) fn run(&mut self, editor: &Editor, rested: Duration) {
        if rested < CHECK_PAUSE || self.busy {
            return;
        }
        let Some((id, revision, path)) = due(editor) else {
            return;
        };
        if self.checked.contains(&(id, revision)) {
            return;
        }
        self.checked.retain(|(checked, _)| *checked != id);
        self.checked.push((id, revision));
        let job = Job {
            editor: editor_code(&path),
            path,
            source: editor.buffer().contents(),
        };
        if self.worker.is_none() {
            self.worker = Worker::start();
        }
        self.busy = self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.jobs.send(job).is_ok());
        if !self.busy {
            self.worker = None;
        }
    }

    pub(crate) fn collect(&mut self, editor: &mut Editor) {
        let Some(worker) = &self.worker else {
            return;
        };
        match worker.done.try_recv() {
            Ok(done) => {
                self.busy = false;
                let findings = done
                    .found
                    .into_iter()
                    .map(|diagnostic| found(&done.path, &done.source, diagnostic))
                    .collect();
                editor.replace_diagnostics(&done.path, findings);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.busy = false;
                self.worker = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use oxmacs_core::editor::Editor;
    use oxmacs_core::external::diagnostics::Severity;

    use super::CHECK_PAUSE;
    use crate::{Ox, Pace};

    const UNUSED: &str = "h is never used; call it _h if that's on purpose";

    fn settle(editor: &mut Editor, ox: &mut Ox) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while ox.pace() != Pace::Idle && Instant::now() < deadline {
            ox.tick(editor);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn visiting(files: &[(&str, &str)], name: &str) -> (tempfile::TempDir, Editor, Ox) {
        let home = tempfile::tempdir().unwrap();
        for (file, text) in files {
            let path = home.path().join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let mut editor = Editor::new();
        let mut ox = Ox::with_bundled(&[]);
        ox.start(&mut editor, Some(home.path()));
        editor.open_file(&home.path().join(name));
        (home, editor, ox)
    }

    fn rest(editor: &mut Editor, ox: &mut Ox, since: Instant, millis: u64) {
        ox.idle(editor, since, since + Duration::from_millis(millis));
    }

    #[test]
    fn ox_files_are_checked_after_a_pause_in_typing() {
        let (_home, mut editor, mut ox) = visiting(
            &[("scratch.ox", "(def (area w h) (* w w))\n(aera 1 2)\n")],
            "scratch.ox",
        );
        let since = Instant::now();

        assert_eq!(ox.idle_wait(&editor, since), Some(CHECK_PAUSE));
        rest(&mut editor, &mut ox, since, 100);
        assert_eq!(editor.pane().line_diagnostics(1), []);
        rest(&mut editor, &mut ox, since, 600);
        settle(&mut editor, &mut ox);
        assert_eq!(
            editor.pane().line_diagnostics(0),
            [(13..14, Severity::Warning)]
        );
        assert_eq!(editor.pane().line_diagnostics(1), [(1..5, Severity::Error)]);
        assert_eq!(editor.pane().status().lint.as_deref(), Some("E1 W1"));
        assert_eq!(ox.idle_wait(&editor, since), None);
        editor.buffer_mut().set_point(13);
        assert_eq!(editor.diagnostic_at_point(), Some(UNUSED));

        editor.buffer_mut().delete_range(26..30);
        editor.buffer_mut().insert_at(26, "area");
        let later = since + Duration::from_secs(1);
        assert_eq!(ox.idle_wait(&editor, later), Some(CHECK_PAUSE));
        rest(&mut editor, &mut ox, later, 600);
        settle(&mut editor, &mut ox);
        assert_eq!(editor.pane().line_diagnostics(1), []);
        assert_eq!(editor.pane().status().lint.as_deref(), Some("W1"));
    }

    #[test]
    fn plugin_files_are_checked_knowing_the_editor() {
        let (_home, mut editor, mut ox) = visiting(
            &[
                (
                    "plugins/greeter/plugin.ox",
                    "(plugin greeter :entry \"greeter.ox\")",
                ),
                (
                    "plugins/greeter/greeter.ox",
                    "(module (import (greeter words)))\n(message words/hello)\n(mesage 1)\n",
                ),
                ("plugins/greeter/words.ox", "(def hello \"Hello\")"),
            ],
            "plugins/greeter/greeter.ox",
        );

        rest(&mut editor, &mut ox, Instant::now(), 600);
        settle(&mut editor, &mut ox);

        assert_eq!(editor.pane().line_diagnostics(1), []);
        assert_eq!(editor.pane().line_diagnostics(2), [(1..7, Severity::Error)]);
    }

    #[test]
    fn checking_runs_beside_the_editor_and_reports_when_it_is_done() {
        let (_home, mut editor, mut ox) = visiting(
            &[(
                "scratch.ox",
                "(defmacro hang [] ((fn again [] (again))))\n(hang)\n",
            )],
            "scratch.ox",
        );
        let started = Instant::now();

        rest(&mut editor, &mut ox, started, 600);

        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(ox.pace(), Pace::Waiting);
        assert_eq!(editor.pane().line_diagnostics(1), []);
        settle(&mut editor, &mut ox);
        assert_eq!(
            editor.pane().line_diagnostics(1),
            [(0..6, Severity::Warning)]
        );
    }
}
