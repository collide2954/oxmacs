use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use oxmacs_core::editor::{Editor, ServerSpec};
use oxmacs_core::external::runner::{Delivery, Runner};
use oxmacs_script::Ox;

const OX_LSP: ServerSpec = ServerSpec {
    name: "ox lsp",
    languages: &[("Ox", "ox")],
    programs: &[(env!("CARGO_BIN_EXE_ox"), &["lsp"])],
    install: "cargo install --path crates/ox",
    runs: true,
};

struct Host {
    runner: Runner,
    deliveries: Receiver<Delivery>,
}

impl Host {
    fn new() -> Self {
        let (sender, deliveries) = mpsc::channel();
        Self {
            runner: Runner::new(move |delivery| {
                let _ = sender.send(delivery);
            }),
            deliveries,
        }
    }

    fn perform(&mut self, editor: &mut Editor) {
        for effect in editor.take_effects() {
            self.runner.perform(editor, effect);
        }
    }

    fn until(&mut self, editor: &mut Editor, done: impl Fn(&Editor) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(60);
        self.perform(editor);
        while !done(editor) {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "gave up: {}", editor.mode_line());
            if let Ok(delivery) = self.deliveries.recv_timeout(left) {
                self.runner.deliver(editor, delivery);
            }
            self.perform(editor);
        }
    }
}

#[test]
fn oxmacs_runs_ox_lsp_and_shows_its_findings() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("init.ox"), "").unwrap();
    let file = home.path().join("broken.ox");
    std::fs::write(&file, "(def x 1)\n(undefined-thing x)\n").unwrap();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    editor.add_language_servers(&[OX_LSP]);
    let mut host = Host::new();

    assert!(editor.open_file(&file));
    host.until(&mut editor, |editor| editor.mode_line().contains("E1"));

    assert!(
        editor.mode_line().contains("ox lsp"),
        "{}",
        editor.mode_line()
    );
    editor.shutdown_servers();
    host.until(&mut editor, |editor| {
        editor.mode_line().contains("ox lsp stopped")
    });
}
