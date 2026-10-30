use std::thread;
use std::time::{Duration, Instant};

use oxmacs_core::editor::Editor;
use oxmacs_core::view::{Arg, Node, UiEvent, ViewBuffer};

use crate::Ox;
use crate::api::sound;
use crate::testing::started;

const SONG: &str = r#"(module (import (ox synth :as s)))
(def pad (s/lowpass (s/saw) (s/control :cutoff 1200 :low 200 :high 6000)))
(s/loop (s/song :tempo 240 (s/track :pad pad (s/chords "Dm9 F"))))
"#;

fn until(editor: &mut Editor, ox: &mut Ox, mut done: impl FnMut(&mut Editor, &mut Ox) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(editor, ox) && Instant::now() < deadline {
        ox.tick(editor);
        thread::sleep(Duration::from_millis(2));
    }
}

fn workbench(editor: &Editor) -> Option<Node<usize>> {
    let id = editor.buffer_named("*bellow*")?;
    editor
        .special_buffer::<ViewBuffer>(id)
        .map(|(_, view)| view.node.clone())
}

fn sliders(node: &Node<usize>, found: &mut Vec<(f32, usize)>) {
    match node {
        Node::Column { children, .. } | Node::Row { children, .. } => {
            for child in children {
                sliders(child, found);
            }
        }
        Node::Slider {
            value,
            change: Some(slot),
            ..
        } => found.push((*value, *slot)),
        _ => {}
    }
}

fn run(editor: &mut Editor, name: &str) {
    let id = editor.command_named(name).unwrap();
    editor.execute(id);
}

#[test]
fn bellow_is_off_until_use_plugin_turns_it_on() {
    let (_home, editor, _ox) = started("");
    assert!(editor.command_named("bellow-play").is_none());

    let (_home, editor, _ox) = started("(use-plugin bellow)");
    assert!(editor.command_named("bellow-play").is_some());
}

#[test]
fn playing_a_song_shows_a_slider_for_each_control_that_moves_it() {
    let (home, mut editor, mut ox) = started("(use-plugin bellow)");
    let mut speaker = sound::capture();
    let path = home.path().join("song.ox");
    std::fs::write(&path, SONG).unwrap();
    editor.open_file(&path);

    run(&mut editor, "bellow-play");
    until(&mut editor, &mut ox, |editor, ox| {
        speaker.take(2 * 2400);
        ox.eval(
            editor,
            "(module (import (ox synth :as s))) (some? (s/position))",
        )
        .is_ok_and(|playing| playing == "true")
            && workbench(editor).is_some_and(|node| {
                let mut found = Vec::new();
                sliders(&node, &mut found);
                !found.is_empty()
            })
    });

    let mut found = Vec::new();
    sliders(&workbench(&editor).unwrap(), &mut found);
    assert_eq!(found.len(), 1);
    assert!((found[0].0 - 1200.0).abs() < 1.0);
    let id = editor.buffer_named("*bellow*").unwrap();
    ox.event(
        &mut editor,
        UiEvent::Call {
            buffer: id,
            slot: found[0].1,
            args: vec![Arg::Float(400.0)],
        },
    );
    let cutoff = |editor: &mut Editor, ox: &mut Ox| {
        ox.eval(
            editor,
            "(module (import (ox synth :as s))) (get (first (s/controls)) :value)",
        )
        .unwrap()
    };
    until(&mut editor, &mut ox, |editor, ox| {
        cutoff(editor, ox) == "400.0"
    });
    assert_eq!(cutoff(&mut editor, &mut ox), "400.0");

    run(&mut editor, "bellow-stop");
    until(&mut editor, &mut ox, |editor, ox| {
        speaker.take(2 * 2400);
        ox.eval(editor, "(module (import (ox synth :as s))) (s/position)")
            .is_ok_and(|position| position == "nil")
    });
    assert_eq!(editor.message(), Some("Stopped"));
}
