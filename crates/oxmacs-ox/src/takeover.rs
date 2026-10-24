use std::time::{Duration, Instant};

use ox::heap::{Heap, Hold};
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::editor::Editor;

use crate::log;
use crate::shapes::{self, Ink, Paint, Shape};

const FASTEST: i64 = 30;

#[derive(Debug)]
pub struct Takeover {
    draw: Hold,
    on_end: Option<Hold>,
    started: Option<Instant>,
    drawn: Option<Instant>,
    every: Duration,
    pub background: Paint,
    pub shapes: Vec<Shape>,
}

#[derive(Debug, Default)]
struct Endings(Vec<Hold>);

pub fn current(editor: &Editor) -> Option<&Takeover> {
    editor.plugin_state::<Takeover>()
}

fn end(heap: &mut Heap, editor: &mut Editor) {
    let Some(takeover) = editor.take_plugin_state::<Takeover>() else {
        return;
    };
    heap.release(takeover.draw);
    if let Some(on_end) = takeover.on_end {
        let mut endings = editor.take_plugin_state::<Endings>().unwrap_or_default();
        endings.0.push(on_end);
        editor.set_plugin_state(endings);
    }
}

fn take_over(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let mut background = Paint {
        ink: Ink::Background,
        alpha: 1.0,
    };
    let mut on_end = None;
    let mut every = Duration::from_millis(1000 / FASTEST.unsigned_abs());
    for pair in arguments[1..].chunks(2) {
        match pair {
            [Value::Keyword(option), value] if option.name() == "background" => {
                background = shapes::paint(heap, *value)?;
            }
            [Value::Keyword(option), value] if option.name() == "on-end" => {
                on_end = Some(*value);
            }
            [Value::Keyword(option), value] if option.name() == "fps" => {
                let Value::Int(fps @ 1..=FASTEST) = *value else {
                    return Err(Error::new(format!(
                        "take-over :fps must be a whole number from 1 to {FASTEST}, got {}",
                        print::brief(heap, *value)
                    )));
                };
                every = Duration::from_millis(1000 / fps.unsigned_abs());
            }
            _ => return Err(Error::new("take-over takes :background, :on-end and :fps")),
        }
    }
    end(heap, editor);
    let takeover = Takeover {
        draw: heap.hold(arguments[0]),
        on_end: on_end.map(|function| heap.hold(function)),
        started: None,
        drawn: None,
        every,
        background,
        shapes: Vec::new(),
    };
    editor.set_plugin_state(takeover);
    Ok(Value::Nil)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("take-over", 1, None, take_over);
    isolate.define_host_native::<Editor>("end-take-over", 0, Some(0), |heap, editor, _| {
        end(heap, editor);
        Ok(Value::Nil)
    });
}

pub fn finish(isolate: &mut Isolate, editor: &mut Editor) {
    end(isolate.heap_mut(), editor);
}

pub fn run_endings(isolate: &mut Isolate, editor: &mut Editor) {
    let Some(endings) = editor.take_plugin_state::<Endings>() else {
        return;
    };
    for hold in endings.0 {
        if let Some(function) = isolate.held(hold)
            && let Err(error) = isolate.spawn(function, &[])
        {
            log::report(editor, &error, None);
        }
        isolate.release(hold);
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "frame sizes and times are small"
)]
fn frame_map(
    isolate: &mut Isolate,
    elapsed: Duration,
    size: (f32, f32),
    font_size: f32,
) -> Result<Value, Error> {
    let whole = |number: f32| Value::Int(number.floor() as i64);
    let font = if font_size.fract() == 0.0 {
        whole(font_size)
    } else {
        Value::Float(f64::from(font_size))
    };
    isolate.heap_mut().map_from(&[
        Value::keyword("time"),
        Value::Int(i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)),
        Value::keyword("width"),
        whole(size.0),
        Value::keyword("height"),
        whole(size.1),
        Value::keyword("font-size"),
        font,
    ])
}

pub fn frame(
    isolate: &mut Isolate,
    editor: &mut Editor,
    now: Instant,
    size: (f32, f32),
    font_size: f32,
) {
    let Some(takeover) = editor.plugin_state_mut::<Takeover>() else {
        return;
    };
    if takeover
        .drawn
        .is_some_and(|drawn| now.saturating_duration_since(drawn) < takeover.every)
    {
        return;
    }
    let started = *takeover.started.get_or_insert(now);
    takeover.drawn = Some(now);
    let draw = takeover.draw;
    let drawn = isolate
        .held(draw)
        .ok_or_else(|| Error::new("The takeover lost its drawing function"))
        .and_then(|function| {
            let frame = frame_map(
                isolate,
                now.saturating_duration_since(started),
                size,
                font_size,
            )?;
            let value = isolate.within(crate::QUICK, |isolate| {
                isolate.call_with(editor, function, &[frame])
            })?;
            shapes::shapes(isolate.heap(), value)
        });
    match drawn {
        Ok(shapes) => {
            if let Some(takeover) = editor.plugin_state_mut::<Takeover>() {
                takeover.shapes = shapes;
            }
        }
        Err(error) => {
            log::report(editor, &error, None);
            finish(isolate, editor);
        }
    }
}
#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use oxmacs_core::editor::Editor;
    use oxmacs_core::theme::Highlight;

    use super::current;
    use crate::Ox;
    use crate::shapes::{Ink, Paint, Shape};
    use crate::testing::settle;
    use crate::views::{Node, ViewBuffer};

    fn paint(ink: Ink, alpha: f32) -> Paint {
        Paint { ink, alpha }
    }

    const DRAW: &str = r##"(def frames (atom []))
(def ended (atom false))
(take-over (fn [frame]
             (swap! frames conj [frame.time frame.width frame.height frame.font-size])
             [[:rect 0 0 frame.width 10 :error]
              [:circle 5.5 5 2 "#ff8800"]
              [:line 0 0 10 10 [:success 0.5] 2]
              [:text 1 2 "hi" :foreground 14]])
           :background :background
           :on-end (fn [] (reset! ended true)))"##;

    #[test]
    fn taking_over_draws_each_frame_with_its_size_and_time() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, DRAW).unwrap();
        let start = Instant::now();

        ox.frame(&mut editor, start, (800.0, 600.0), 14.0);

        let shown = current(&editor).unwrap();
        assert_eq!(shown.background, paint(Ink::Background, 1.0));
        assert_eq!(
            shown.shapes,
            [
                Shape::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 800.0,
                    height: 10.0,
                    paint: paint(Ink::Face(Highlight::Error), 1.0),
                },
                Shape::Circle {
                    x: 5.5,
                    y: 5.0,
                    radius: 2.0,
                    paint: paint(Ink::Rgb(255, 136, 0), 1.0),
                },
                Shape::Line {
                    from: (0.0, 0.0),
                    to: (10.0, 10.0),
                    width: 2.0,
                    paint: paint(Ink::Face(Highlight::Success), 0.5),
                },
                Shape::Text {
                    x: 1.0,
                    y: 2.0,
                    text: "hi".to_owned(),
                    size: Some(14.0),
                    paint: paint(Ink::Foreground, 1.0),
                },
            ]
        );

        ox.frame(
            &mut editor,
            start + Duration::from_millis(10),
            (800.0, 600.0),
            14.0,
        );
        ox.frame(
            &mut editor,
            start + Duration::from_millis(40),
            (640.0, 480.0),
            14.0,
        );
        assert_eq!(
            ox.eval(&mut editor, "@frames").unwrap(),
            "[[0 800 600 14] [40 640 480 14]]"
        );
    }

    #[test]
    fn takeovers_can_ask_for_fewer_frames() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(
            &mut editor,
            "(def frames (atom 0)) (take-over (fn [frame] (swap! frames inc) []) :fps 10)",
        )
        .unwrap();
        let start = Instant::now();

        for millis in [0, 40, 99, 100, 150] {
            ox.frame(
                &mut editor,
                start + Duration::from_millis(millis),
                (100.0, 100.0),
                14.0,
            );
        }

        assert_eq!(ox.eval(&mut editor, "@frames").unwrap(), "2");
        assert_eq!(
            ox.eval(&mut editor, "(take-over (fn [frame] []) :fps 0)")
                .unwrap_err()
                .to_string(),
            "take-over :fps must be a whole number from 1 to 30, got 0"
        );
    }

    #[test]
    fn input_or_ox_ends_the_takeover_and_tells_the_plugin() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, DRAW).unwrap();
        assert!(current(&editor).is_some());

        ox.end_take_over(&mut editor);
        settle(&mut editor, &mut ox);

        assert!(current(&editor).is_none());
        assert_eq!(ox.eval(&mut editor, "@ended").unwrap(), "true");

        ox.eval(&mut editor, DRAW).unwrap();
        ox.eval(&mut editor, "(end-take-over)").unwrap();
        settle(&mut editor, &mut ox);
        assert!(current(&editor).is_none());
        assert_eq!(ox.eval(&mut editor, "@ended").unwrap(), "true");
    }

    #[test]
    fn bad_drawings_are_logged_once_and_end_the_takeover() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, "(take-over (fn [frame] [[:hexagon 1 2]]))")
            .unwrap();

        ox.frame(&mut editor, Instant::now(), (100.0, 100.0), 14.0);

        assert!(current(&editor).is_none());
        let message = editor.message().unwrap_or_default().to_owned();
        assert!(message.contains("[:hexagon 1 2]"), "{message}");
    }

    #[test]
    fn views_can_show_a_drawing() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(
            &mut editor,
            r#"(module (import (ox ui :only [canvas])))
(open-view "*art*" (fn [] (canvas [[:circle 10 10 4 :link]] :height 40)))"#,
        )
        .unwrap();
        settle(&mut editor, &mut ox);

        let id = editor.buffer_named("*art*").unwrap();
        let (_, view) = editor.plugin_buffer::<ViewBuffer>(id).unwrap();
        assert_eq!(
            view.node,
            Node::Canvas {
                shapes: vec![Shape::Circle {
                    x: 10.0,
                    y: 10.0,
                    radius: 4.0,
                    paint: paint(Ink::Face(Highlight::Link), 1.0),
                }],
                height: Some(40.0),
                relative: false,
            }
        );

        ox.eval(
            &mut editor,
            r#"(module (import (ox ui :only [canvas])))
(open-view "*grid*" (fn [] (canvas [[:rect 0.5 0 0.25 10 :link]] :height 40 :relative true)))"#,
        )
        .unwrap();
        settle(&mut editor, &mut ox);
        let id = editor.buffer_named("*grid*").unwrap();
        let (_, view) = editor.plugin_buffer::<ViewBuffer>(id).unwrap();
        assert!(matches!(view.node, Node::Canvas { relative: true, .. }));
    }
}
