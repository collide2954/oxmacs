use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use oxmacs_core::editor::{Editor, Effect, ServerId, ServerSpec};
use oxmacs_core::external::lsp::{self, Event, Server};
use oxmacs_ox::Ox;

const OX_LSP: ServerSpec = ServerSpec {
    name: "ox lsp",
    languages: &[("Ox", "ox")],
    programs: &[(env!("CARGO_BIN_EXE_ox"), &["lsp"])],
    roots: &["init.ox"],
    install: "cargo install --path crates/ox-cli",
    runs: true,
};

struct Host {
    servers: Vec<(ServerId, Server)>,
    sender: mpsc::Sender<(ServerId, Event)>,
    events: Receiver<(ServerId, Event)>,
}

impl Host {
    fn new() -> Self {
        let (sender, events) = mpsc::channel();
        Self {
            servers: Vec::new(),
            sender,
            events,
        }
    }

    fn perform(&mut self, editor: &mut Editor) {
        for effect in editor.take_effects() {
            match effect {
                Effect::StartServer(id, launch) => {
                    let sender = self.sender.clone();
                    let server = lsp::start(&launch, move |event| {
                        let _ = sender.send((id, event));
                    })
                    .unwrap();
                    self.servers.push((id, server));
                }
                Effect::ToServer(id, body) => {
                    if let Some((_, server)) = self.servers.iter().find(|(known, _)| *known == id) {
                        server.send(&body).unwrap();
                    }
                }
                Effect::StopServer(id) => {
                    if let Some((_, server)) = self.servers.iter().find(|(known, _)| *known == id) {
                        server.kill();
                    }
                }
                _ => {}
            }
        }
    }

    fn until(&mut self, editor: &mut Editor, done: impl Fn(&Editor) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(60);
        self.perform(editor);
        while !done(editor) {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "gave up: {}", editor.mode_line());
            if let Ok((id, event)) = self.events.recv_timeout(left) {
                editor.server_event(id, event);
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
