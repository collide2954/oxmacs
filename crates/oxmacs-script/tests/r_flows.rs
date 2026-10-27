use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use oxmacs_core::editor::Editor;
use oxmacs_core::external::runner::{Delivery, Runner};
use oxmacs_core::input::KeySequence;
use oxmacs_core::view::{Node, ViewBuffer};
use oxmacs_script::{Ox, Pace};

struct Live {
    _home: tempfile::TempDir,
    folder: tempfile::TempDir,
    editor: Editor,
    ox: Ox,
    runner: Runner,
    deliveries: Receiver<Delivery>,
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
        let (sender, deliveries) = mpsc::channel();
        Self {
            _home: home,
            folder,
            editor,
            ox,
            runner: Runner::new(move |delivery| {
                let _ = sender.send(delivery);
            }),
            deliveries,
        }
    }

    fn step(&mut self) {
        for effect in self.editor.take_effects() {
            self.runner.perform(&mut self.editor, effect);
        }
        while let Ok(delivery) = self.deliveries.try_recv() {
            self.runner.deliver(&mut self.editor, delivery);
        }
        while self.ox.tick(&mut self.editor) == Pace::Running {}
    }

    fn until(&mut self, what: &str, done: impl Fn(&Editor) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while !done(&self.editor) {
            assert!(
                Instant::now() < deadline,
                "gave up waiting for {what} in {}: {:?}",
                self.editor.buffer().name(),
                self.editor
                    .buffer_ids()
                    .into_iter()
                    .filter_map(|id| self.editor.buffer_by_id(id))
                    .filter(|buffer| buffer.name().starts_with("*R"))
                    .map(oxmacs_core::text::Buffer::contents)
                    .collect::<Vec<_>>()
            );
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
        self.runner.shutdown(&mut self.editor);
    }
}

fn contents(editor: &Editor, name: &str) -> String {
    editor
        .buffer_named(name)
        .and_then(|id| editor.buffer_by_id(id))
        .map(oxmacs_core::text::Buffer::contents)
        .unwrap_or_default()
}

fn has_image(node: &Node<usize>) -> bool {
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
        .and_then(|id| editor.special_buffer::<ViewBuffer>(id))
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
    live.press("TAB RET");
    live.until("the linked help", |editor| {
        contents(editor, "*R help*").contains("Sample Quantiles")
    });
    live.press("l");
    assert!(contents(&live.editor, "*R help*").contains("Median Value"));

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

#[test]
#[ignore = "runs the R and the languageserver package installed on this machine"]
fn the_languageserver_finds_problems_in_r_scripts_without_a_session() {
    let mut live = Live::new();
    let script = live.folder.path().join("lint.R");
    std::fs::write(&script, "x = 1\n").unwrap();
    live.editor.open_file(&script);

    live.until("the languageserver's findings", |editor| {
        !editor
            .pane()
            .line_decorations(0, Instant::now())
            .waves
            .is_empty()
    });

    assert!(
        live.editor.mode_line().contains("languageserver"),
        "{}",
        live.editor.mode_line()
    );
    assert!(
        live.editor
            .processes()
            .iter()
            .all(|process| process.kind == "language server")
    );
}

#[test]
#[ignore = "runs the R installed on this machine"]
fn scripts_run_as_background_jobs_apart_from_the_session() {
    let mut live = Live::new();
    let script = live.folder.path().join("job.R");
    std::fs::write(&script, "cat('job says', 6 * 7, '\\n')\n").unwrap();
    live.editor.open_file(&script);

    live.press("M-x");
    live.typed("r-run-job");
    live.press("RET");
    live.until("the job", |editor| {
        contents(editor, "*R job: job.R*").ends_with("Finished\n")
    });

    assert!(
        contents(&live.editor, "*R job: job.R*").contains("job says 42"),
        "{}",
        contents(&live.editor, "*R job: job.R*")
    );
    assert!(
        live.editor
            .processes()
            .iter()
            .all(|process| process.status.finished())
    );
}

#[test]
#[ignore = "runs the R installed on this machine"]
fn the_debugger_follows_r_through_the_source() {
    let mut live = Live::new();
    std::fs::write(
        live.folder.path().join("double.R"),
        "double <- function(x) {\n  y <- x * 2\n  y\n}\n",
    )
    .unwrap();
    let run = live.folder.path().join("run.R");
    std::fs::write(&run, "source(\"double.R\")\ndebugonce(double)\ndouble(2)\n").unwrap();
    live.editor.open_file(&run);
    let at = |editor: &Editor| {
        let buffer = editor.buffer();
        (buffer.name().to_owned(), buffer.line_of(buffer.point()))
    };

    live.press("C-RET C-RET C-RET");
    live.until("the first debug line", |editor| {
        at(editor) == ("double.R".to_owned(), 0)
    });
    live.press("<f10>");
    live.until("the next line", |editor| {
        at(editor) == ("double.R".to_owned(), 1)
    });
    live.press("S-<f8>");
    let folder = live
        .folder
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let console = format!("*R:{folder}*");
    live.until("the prompt", |editor| {
        contents(editor, &console).ends_with("Q\n> ")
    });
}
