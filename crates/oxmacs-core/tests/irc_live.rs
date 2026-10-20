use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use oxmacs_core::command::Command;
use oxmacs_core::config::Config;
use oxmacs_core::editor::{Editor, Effect};
use oxmacs_core::external::net::{self, Connection, Event};
use oxmacs_core::input::KeySequence;

const CHANNEL: &str = "#oxmacs-test";
const PATIENCE: Duration = Duration::from_secs(90);
const STEP: Duration = Duration::from_millis(200);

fn press(editor: &mut Editor, notation: &str) {
    let keys: KeySequence = notation.parse().unwrap();
    for key in keys.keys() {
        editor.handle_key(*key);
    }
}

fn type_text(editor: &mut Editor, text: &str) {
    for c in text.chars() {
        let notation = if c == ' ' {
            "SPC".to_owned()
        } else {
            c.to_string()
        };
        press(editor, &notation);
    }
}

fn perform(editor: &mut Editor, connection: &mut Option<Connection>, events: &mpsc::Sender<Event>) {
    for effect in editor.take_effects() {
        match effect {
            Effect::Connect(_, endpoint) => {
                let sender = events.clone();
                *connection = Some(net::connect(&endpoint, move |event| {
                    let _ = sender.send(event);
                }));
            }
            Effect::Transmit(_, lines) => {
                for line in lines {
                    println!(">> {line}");
                    if let Some(connection) = connection {
                        connection.send(&line);
                    }
                }
            }
            Effect::Disconnect(_) => {
                if let Some(connection) = connection {
                    connection.close();
                }
            }
            _ => {}
        }
    }
}

fn show(editor: &mut Editor, buffer: &str) {
    press(editor, "C-x b");
    type_text(editor, buffer);
    press(editor, "RET");
    println!("==== {buffer} ====\n{}", editor.buffer().contents());
}

#[test]
#[ignore = "connects to Libera.Chat; run once, courteously"]
fn a_short_courteous_session_on_libera() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs() % 10_000);
    let nick = format!("oxmx-{suffix:04}");
    let config = Config::parse(&format!(
        "[[irc-server]]\nname = \"libera\"\nhost = \"irc.libera.chat\"\nnick = \"{nick}\"\nrealname = \"oxmacs test\"\nchannels = [\"{CHANNEL}\"]\n"
    ))
    .unwrap();
    let mut editor = Editor::new();
    editor.apply_config(&config);
    let (sender, received) = mpsc::channel();
    let mut connection = None;

    editor.execute(Command::Irc);
    perform(&mut editor, &mut connection, &sender);

    let start = Instant::now();
    let mut said = None;
    let mut quit = false;
    let mut closed = false;
    while start.elapsed() < PATIENCE && !closed {
        if let Ok(event) = received.recv_timeout(STEP) {
            match &event {
                Event::Line(line) => println!("<< {line}"),
                other => println!("-- {other:?}"),
            }
            closed = matches!(event, Event::Closed(_));
            editor.connection_event(oxmacs_core::editor::ConnectionId(0), event);
        }
        perform(&mut editor, &mut connection, &sender);
        let joined = editor.buffer_names().contains(&CHANNEL);
        if joined && said.is_none() {
            said = Some(Instant::now());
            show(&mut editor, CHANNEL);
            type_text(
                &mut editor,
                "Testing the oxmacs IRC client: one message, then leaving. Sorry for the noise!",
            );
            press(&mut editor, "RET");
            perform(&mut editor, &mut connection, &sender);
        }
        if !quit && said.is_some_and(|said| said.elapsed() > Duration::from_secs(5)) {
            quit = true;
            show(&mut editor, CHANNEL);
            type_text(&mut editor, "/quit oxmacs test done");
            press(&mut editor, "RET");
            perform(&mut editor, &mut connection, &sender);
        }
    }
    show(&mut editor, "libera");
    show(&mut editor, CHANNEL);
    assert!(said.is_some(), "never joined {CHANNEL}");
    assert!(closed, "the connection did not close");
}
