use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use oxmacs_core::editor::{Editor, Effect, JobId};
use oxmacs_core::external::process::{self, Event, Handle};
use oxmacs_core::input::KeySequence;
use oxmacs_ox::views::{Node, ViewBuffer};
use oxmacs_ox::{Ox, Pace};

struct Live {
    _home: tempfile::TempDir,
    folder: tempfile::TempDir,
    editor: Editor,
    ox: Ox,
    jobs: Vec<(JobId, Handle)>,
    sender: Sender<(JobId, Event)>,
    events: Receiver<(JobId, Event)>,
}

impl Live {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("init.ox"), "(use-plugin r)").unwrap();
        let folder = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        editor.set_working_directory(folder.path().to_path_buf());
        let mut ox = Ox::new();
        ox.start(&mut editor, Some(home.path()));
        let (sender, events) = mpsc::channel();
        Self {
            _home: home,
            folder,
            editor,
            ox,
            jobs: Vec::new(),
            sender,
            events,
        }
    }

    fn step(&mut self) {
        for effect in self.editor.take_effects() {
            match effect {
                Effect::StartSession(job) => {
                    let sender = self.sender.clone();
                    let id = job.id;
                    let handle = process::start_session(&job, move |event| {
                        let _ = sender.send((id, event));
                    })
                    .unwrap();
                    self.jobs.push((id, handle));
                }
                Effect::Send(job, text) => {
                    if let Some((_, handle)) = self.jobs.iter().find(|(known, _)| *known == job) {
                        handle.send(&text).unwrap();
                    }
                }
                _ => {}
            }
        }
        while let Ok((job, event)) = self.events.try_recv() {
            match event {
                Event::Output(text) => self.editor.session_output(job, &text),
                Event::Line(line) => self.editor.job_output(job, &line),
                Event::Finished(outcome) => self.editor.job_finished(job, &outcome),
            }
        }
        while self.ox.tick(&mut self.editor) == Pace::Running {}
    }

    fn until(&mut self, what: &str, done: impl Fn(&Editor) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while !done(&self.editor) {
            assert!(Instant::now() < deadline, "gave up waiting for {what}");
            self.step();
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn press(&mut self, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            self.editor.handle_key(*key);
            self.step();
        }
    }

    fn typed(&mut self, text: &str) {
        for character in text.chars() {
            self.press(&if character == ' ' {
                "SPC".to_owned()
            } else {
                character.to_string()
            });
        }
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        for (_, handle) in &self.jobs {
            handle.kill();
        }
    }
}

fn contents(editor: &Editor, name: &str) -> String {
    editor
        .buffer_named(name)
        .and_then(|id| editor.buffer_by_id(id))
        .map(oxmacs_core::text::Buffer::contents)
        .unwrap_or_default()
}

fn has_image(node: &Node) -> bool {
    match node {
        Node::Image { .. } => true,
        Node::Column { children, .. } | Node::Row { children, .. } => {
            children.iter().any(has_image)
        }
        Node::Scrollable(child) => has_image(child),
        _ => false,
    }
}

fn plotted(editor: &Editor, name: &str) -> bool {
    editor
        .buffer_named(name)
        .and_then(|id| editor.plugin_buffer::<ViewBuffer>(id))
        .is_some_and(|(_, view)| has_image(&view.node))
}

#[test]
#[ignore = "runs the R installed on this machine"]
fn a_script_runs_in_r_and_its_plots_help_and_objects_show() {
    let mut live = Live::new();
    let script = live.folder.path().join("analysis.R");
    std::fs::write(&script, "x <- 1:10\nplot(x)\n").unwrap();
    live.editor.open_file(&script);
    let folder = live
        .folder
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let console = format!("*R:{folder}*");
    let plots = format!("*R:{folder} plots*");

    live.press("C-RET");
    live.until("the first statement", |editor| {
        contents(editor, &console).ends_with("> x <- 1:10\n> ")
    });
    live.press("C-RET");
    live.until("the plot", |editor| plotted(editor, &plots));

    live.press("C-c C-v");
    live.typed("median");
    live.press("RET");
    live.until("the help", |editor| {
        contents(editor, "*R help*").contains("Median Value")
    });

    let id = live.editor.buffer_named(&console).unwrap();
    live.editor.show_buffer(id);
    live.press("M->");
    live.typed("summary(x)");
    live.press("RET");
    live.until("the summary", |editor| {
        contents(editor, &console).contains("Median")
    });
    assert!(
        !contents(&live.editor, &console).contains('\u{1}'),
        "{}",
        contents(&live.editor, &console)
    );
}
