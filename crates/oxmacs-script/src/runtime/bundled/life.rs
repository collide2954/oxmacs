use std::thread;
use std::time::{Duration, Instant};

use oxmacs_core::editor::Editor;
use oxmacs_core::input::KeySequence;

use crate::Ox;
use crate::testing::{press, settle, started};
use oxmacs_core::view::shapes::Shape;
use oxmacs_core::view::{Node, ViewBuffer};

fn until(editor: &mut Editor, ox: &mut Ox, done: impl Fn(&Editor) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(editor) && Instant::now() < deadline {
        ox.tick(editor);
        thread::sleep(Duration::from_millis(1));
    }
}

fn walk(node: &Node<usize>, texts: &mut Vec<String>, cells: &mut usize) {
    match node {
        Node::Column { children, .. } | Node::Row { children, .. } => {
            for child in children {
                walk(child, texts, cells);
            }
        }
        Node::Text { text, .. } => texts.push(text.clone()),
        Node::Scrollable(child) => walk(child, texts, cells),
        Node::Canvas { shapes, .. } => {
            *cells += shapes
                .iter()
                .filter(|shape| matches!(shape, Shape::Rect { .. }))
                .count();
        }
        Node::Button { .. }
        | Node::Space { .. }
        | Node::Image { .. }
        | Node::Pages(_)
        | Node::Pdf(_) => {}
    }
}

fn shown(editor: &Editor) -> (String, usize) {
    let id = editor.buffer_named("*Life*").unwrap();
    let (_, view) = editor.plugin_buffer::<ViewBuffer>(id).unwrap();
    let mut texts = Vec::new();
    let mut cells = 0;
    walk(&view.node, &mut texts, &mut cells);
    (texts.join(" | "), cells)
}

fn generation(editor: &Editor) -> usize {
    let (text, _) = shown(editor);
    text.split_whitespace()
        .nth(1)
        .and_then(|number| number.parse().ok())
        .unwrap()
}

#[test]
fn life_is_off_until_use_plugin_turns_it_on() {
    let (_home, editor, _ox) = started("");
    assert!(editor.command_named("life").is_none());

    let (_home, editor, _ox) = started("(use-plugin life)");
    assert!(editor.command_named("life").is_some());
}

#[test]
fn life_draws_generations_and_takes_keys() {
    let (_home, mut editor, mut ox) =
        started("(use-plugin life :config {speed: 600 pattern: :glider})");

    let id = editor.command_named("life").unwrap();
    editor.execute(id);
    until(&mut editor, &mut ox, |editor| {
        editor.buffer_named("*Life*").is_some() && shown(editor).0.starts_with("Generation")
    });
    assert_eq!(editor.buffer().name(), "*Life*");
    let (text, cells) = shown(&editor);
    assert!(text.starts_with("Generation 0   5 alive"), "{text}");
    assert_eq!(cells, 5);

    until(&mut editor, &mut ox, |editor| generation(editor) >= 1);
    assert!(generation(&editor) >= 1);

    press(&mut editor, &mut ox, "SPC");
    let paused = generation(&editor);
    assert!(shown(&editor).0.contains("Paused"));
    thread::sleep(Duration::from_millis(700));
    settle(&mut editor, &mut ox);
    assert_eq!(generation(&editor), paused);

    press(&mut editor, &mut ox, "n");
    assert_eq!(generation(&editor), paused + 1);
    assert_eq!(shown(&editor).1, 5);

    let restart: KeySequence = "r".parse().unwrap();
    editor.handle_key(restart.keys()[0]);
    until(&mut editor, &mut ox, |editor| generation(editor) == 0);
    assert_eq!(generation(&editor), 0);
    assert!(!shown(&editor).0.contains("Paused"));

    press(&mut editor, &mut ox, "q");
    assert_eq!(editor.buffer_named("*Life*"), None);
}
