use std::time::{Duration, Instant};

use oxmacs_core::editor::Editor;
use oxmacs_core::theme::Highlight;
use oxmacs_core::view::UiEvent;

use crate::testing::{press, settle};
use crate::{Ox, api::idle};
use oxmacs_core::view::shapes::{Ink, Shape};
use oxmacs_core::view::takeover;

const IMPORTS: &str =
    "(module (import (screensaver starfield :as stars) (screensaver matrix :as rain)))";

const KINDS: [&str; 29] = [
    "starfield",
    "matrix",
    "pond",
    "fireworks",
    "harmonograph",
    "aurora",
    "clock",
    "flow",
    "orbits",
    "snowfall",
    "tunnel",
    "lava",
    "constellations",
    "plasma",
    "bubbles",
    "kaleidoscope",
    "wireframes",
    "city",
    "pendulums",
    "tree",
    "ocean",
    "glass",
    "galaxy",
    "ants",
    "campfire",
    "sorting",
    "storm",
    "fireflies",
    "mandelbrot",
];

fn started(init: &str) -> (tempfile::TempDir, Editor, Ox) {
    crate::testing::started(&format!("(use-plugin screensaver)\n{init}"))
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
    ox.event(&mut editor, UiEvent::EndTakeOver);
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
    assert_eq!(offered, KINDS);
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
fn stars_stay_on_screen() {
    let (_home, mut editor, mut ox) = started("");

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

#[test]
fn every_screensaver_starts_and_keeps_drawing_through_its_first_minute() {
    let (_home, mut editor, mut ox) = started("");

    for kind in KINDS {
        for millis in [0, 700, 3500, 9000, 31_000, 61_000] {
            let drawn = ox.eval(
                &mut editor,
                &format!(
                    "(module (import (screensaver savers :as savers))) (count ((get (find (fn [s] (= s.kind :{kind})) savers/savers) :draw) {{time: {millis} width: 960 height: 540 font-size: 16}}))"
                ),
            );
            let shapes: usize = drawn
                .unwrap_or_else(|error| panic!("{kind} at {millis} ms: {}", error.message))
                .parse()
                .unwrap();
            assert!(
                millis < 3000 || shapes > 0,
                "{kind} drew nothing at {millis} ms"
            );
        }
        ox.eval(
            &mut editor,
            &format!("(module (import (screensaver savers :as savers))) (savers/start :{kind})"),
        )
        .unwrap();
        assert!(takeover::current(&editor).is_some(), "{kind} didn't start");
        ox.event(&mut editor, UiEvent::EndTakeOver);
        settle(&mut editor, &mut ox);
    }
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
