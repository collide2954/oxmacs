use std::time::{Duration, Instant};

use oxmacs_core::editor::Editor;
use oxmacs_core::theme::Highlight;

use crate::shapes::{Ink, Shape};
use crate::testing::{press, settle};
use crate::{Ox, idle, takeover};

const IMPORTS: &str =
    "(module (import (screensaver starfield :as stars) (screensaver matrix :as rain)))";

fn started(init: &str) -> (tempfile::TempDir, Editor, Ox) {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join("init.ox"),
        format!("(use-plugin screensaver)\n{init}"),
    )
    .unwrap();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn drawn(editor: &mut Editor, ox: &mut Ox, at: Instant) -> Vec<Shape> {
    ox.frame(editor, at, (800.0, 600.0), 18.0);
    takeover::current(editor).unwrap().shapes.clone()
}

fn evaluated(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
    match ox.eval(editor, &format!("{IMPORTS} {source}")) {
        Ok(shown) => shown,
        Err(error) => format!("error: {}", error.message),
    }
}

#[test]
fn screensavers_stay_off_until_an_idle_time_is_set() {
    let (_home, editor, _) = started("");
    assert!(editor.command_named("screensaver-preview").is_some());
    assert_eq!(idle::next(&editor, Instant::now()), None);

    let (_home, editor, _) = started("(use-plugin screensaver :config {idle: 300})");
    assert_eq!(
        idle::next(&editor, Instant::now()),
        Some(Duration::from_secs(300))
    );
}

#[test]
fn going_idle_starts_the_chosen_screensaver_until_input() {
    let (_home, mut editor, mut ox) =
        started("(use-plugin screensaver :config {idle: 60 kind: :matrix})");
    let since = Instant::now();

    ox.idle(&mut editor, since, since + Duration::from_secs(61));
    settle(&mut editor, &mut ox);

    let shapes = drawn(&mut editor, &mut ox, since);
    assert!(
        shapes
            .iter()
            .any(|shape| matches!(shape, Shape::Text { .. }))
    );
    assert!(
        shapes
            .iter()
            .all(|shape| matches!(shape, Shape::Text { .. }))
    );
    ox.end_take_over(&mut editor);
    settle(&mut editor, &mut ox);
    assert!(takeover::current(&editor).is_none());
}

#[test]
fn the_preview_offers_each_screensaver_by_name() {
    let (_home, mut editor, mut ox) = started("");
    let id = editor.command_named("screensaver-preview").unwrap();

    editor.execute(id);
    settle(&mut editor, &mut ox);
    let offered: Vec<String> = editor
        .completions()
        .into_iter()
        .map(|completion| completion.label)
        .collect();
    assert_eq!(offered, ["starfield", "matrix"]);
    press(&mut editor, &mut ox, "s t RET");

    let start = Instant::now();
    let first = drawn(&mut editor, &mut ox, start);
    assert!(
        first
            .iter()
            .any(|shape| matches!(shape, Shape::Circle { .. }))
    );
    let later = drawn(&mut editor, &mut ox, start + Duration::from_millis(500));
    assert_ne!(first, later);

    editor.execute(id);
    settle(&mut editor, &mut ox);
    press(&mut editor, &mut ox, "x y z M-RET");
    assert_eq!(
        editor.message(),
        Some("There is no screensaver named \"xyz\"")
    );
}

#[test]
fn stars_rush_toward_you_and_stay_on_screen() {
    let (_home, mut editor, mut ox) = started("");

    assert_eq!(
        evaluated(
            &mut editor,
            &mut ox,
            "(every? (fn [i] (let [a (stars/star i 2.0) b (stars/star i 2.05)] (or (< b.z a.z) (< a.z 0.02)))) (range 100))"
        ),
        "true"
    );
    let shapes = drawn_at(&mut editor, &mut ox, "stars", 1500);
    let circles: Vec<(f32, f32)> = shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Circle { x, y, .. } => Some((*x, *y)),
            _ => None,
        })
        .collect();
    assert!(circles.len() > 100, "{}", circles.len());
    assert!(
        circles
            .iter()
            .all(|(x, y)| (0.0..=800.0).contains(x) && (0.0..=600.0).contains(y))
    );
}

#[test]
fn rain_falls_in_columns_with_bright_heads_and_fading_trails() {
    let (_home, mut editor, mut ox) = started("");

    assert_eq!(
        evaluated(
            &mut editor,
            &mut ox,
            "(every? (fn [c] (let [a (rain/drop c 1.0 40) b (rain/drop c 1.1 40)] (or (>= b.head a.head) (<= b.head 2)))) (range 40))"
        ),
        "true"
    );
    let shapes = drawn_at(&mut editor, &mut ox, "rain", 4000);
    let mut heads = 0;
    let mut trails = 0;
    for shape in &shapes {
        let Shape::Text {
            text, paint, size, ..
        } = shape
        else {
            panic!("{shape:?}");
        };
        assert_eq!(text.chars().count(), 1, "{text}");
        assert_eq!(*size, Some(18.0));
        match paint.ink {
            Ink::Foreground => heads += 1,
            Ink::Face(Highlight::Success) => trails += 1,
            ink => panic!("{ink:?}"),
        }
    }
    assert!(heads > 0 && trails > heads, "{heads} {trails}");
}

fn drawn_at(editor: &mut Editor, ox: &mut Ox, module: &str, time: u32) -> Vec<Shape> {
    evaluated(
        editor,
        ox,
        &format!(
            "(take-over (fn [frame] ({module}/draw {{time: {time} width: frame.width height: frame.height font-size: frame.font-size}})))"
        ),
    );
    drawn(editor, ox, Instant::now())
}
