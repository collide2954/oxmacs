use ox::heap::Heap;
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::editor::{Arrangement, Axis, Edge, Editor};

fn made(made: Result<Value, ox::heap::HeapFull>) -> Result<Value, Error> {
    made.map_err(|full| Error::new(full.to_string()))
}

fn shown(heap: &mut Heap, editor: &Editor, arrangement: &Arrangement) -> Result<Value, Error> {
    match arrangement {
        Arrangement::Buffer(id) => {
            let name = editor
                .buffer_by_id(*id)
                .map(|buffer| buffer.name().to_owned())
                .unwrap_or_default();
            made(heap.string(&name))
        }
        Arrangement::Split { axis, parts } => {
            let mut items = vec![Value::keyword(match axis {
                Axis::Rows => "rows",
                Axis::Columns => "columns",
            })];
            for (weight, part) in parts {
                let part = shown(heap, editor, part)?;
                let weight = (f64::from(*weight) * 10_000.0).round() / 10_000.0;
                items.push(made(heap.vector_from(&[Value::Float(weight), part]))?);
            }
            made(heap.vector_from(&items))
        }
    }
}

fn wrong(heap: &Heap, value: Value) -> Error {
    Error::new(format!(
        "arrange-windows expects a buffer name or [:rows …] or [:columns …] of parts, got {}",
        print::brief(heap, value)
    ))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "window weights are small fractions"
)]
fn weight(value: Value) -> Option<f32> {
    match value {
        Value::Int(number) => Some(number as f32),
        Value::Float(number) => Some(number as f32),
        _ => None,
    }
}

fn read(heap: &Heap, editor: &Editor, value: Value) -> Result<Arrangement, Error> {
    if let Some(name) = heap.text(value) {
        return editor
            .buffer_named(name)
            .map(Arrangement::Buffer)
            .ok_or_else(|| {
                Error::new(format!("arrange-windows: there is no buffer named {name}"))
            });
    }
    let items = heap.elements(value).ok_or_else(|| wrong(heap, value))?;
    let axis = match items.first() {
        Some(Value::Keyword(name)) if name.name() == "rows" => Axis::Rows,
        Some(Value::Keyword(name)) if name.name() == "columns" => Axis::Columns,
        _ => return Err(wrong(heap, value)),
    };
    let mut parts = Vec::new();
    for item in &items[1..] {
        let weighted = heap
            .text(*item)
            .is_none()
            .then(|| heap.elements(*item))
            .flatten()
            .and_then(|pair| match pair.as_slice() {
                [given, part] => weight(*given).map(|given| (given, *part)),
                _ => None,
            });
        match weighted {
            Some((given, part)) => parts.push((given, read(heap, editor, part)?)),
            None => parts.push((f32::NAN, read(heap, editor, *item)?)),
        }
    }
    if parts.is_empty() {
        return Err(wrong(heap, value));
    }
    let missing: Vec<usize> = (0..parts.len())
        .filter(|at| parts[*at].0.is_nan())
        .collect();
    let given: f32 = parts
        .iter()
        .filter(|(weight, _)| !weight.is_nan())
        .map(|(weight, _)| weight)
        .sum();
    let share =
        (1.0 - given).max(0.0) / f32::from(u16::try_from(missing.len().max(1)).unwrap_or(u16::MAX));
    for at in missing {
        parts[at].0 = share;
    }
    Ok(Arrangement::Split { axis, parts })
}

fn show_side(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = crate::handles::name(heap, editor, "show-side-window", arguments[0])?;
    let buffer = editor
        .buffer_named(&name)
        .ok_or_else(|| Error::new(format!("show-side-window: there is no buffer named {name}")))?;
    let (mut edge, mut columns, mut select) = (Edge::Left, 30, false);
    for pair in arguments[1..].chunks(2) {
        match pair {
            [Value::Keyword(option), value] if option.name() == "edge" => {
                edge = match value {
                    Value::Keyword(side) if side.name() == "left" => Edge::Left,
                    Value::Keyword(side) if side.name() == "right" => Edge::Right,
                    _ => {
                        return Err(Error::new(format!(
                            "show-side-window's :edge is :left or :right, got {}",
                            print::brief(heap, *value)
                        )));
                    }
                };
            }
            [Value::Keyword(option), Value::Int(width)] if option.name() == "width" => {
                columns = usize::try_from(*width).unwrap_or(1).max(1);
            }
            [Value::Keyword(option), value] if option.name() == "select" => {
                select = value.truthy();
            }
            _ => {
                return Err(Error::new(
                    "show-side-window takes :edge, :width as a whole number and :select",
                ));
            }
        }
    }
    let window = editor.show_side(buffer, edge, columns);
    if select {
        editor.select_window(window);
    }
    Ok(Value::Nil)
}

fn side(heap: &mut Heap, editor: &mut Editor, _: &[Value]) -> Result<Value, Error> {
    let Some(side) = editor.side_window() else {
        return Ok(Value::Nil);
    };
    let name = editor
        .pane_of(side.window)
        .map(|pane| pane.buffer().name().to_owned())
        .unwrap_or_default();
    let buffer = made(heap.string(&name))?;
    let edge = Value::keyword(match side.edge {
        Edge::Left => "left",
        Edge::Right => "right",
    });
    heap.map_from(&[
        Value::keyword("buffer"),
        buffer,
        Value::keyword("edge"),
        edge,
        Value::keyword("width"),
        Value::Int(i64::try_from(side.columns).unwrap_or(i64::MAX)),
        Value::keyword("selected"),
        Value::Bool(editor.selected_window() == side.window),
    ])
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("show-side-window", 1, None, show_side);
    isolate.define_host_native::<Editor>("side-window", 0, Some(0), side);
    isolate.define_host_native::<Editor>("close-side-window", 0, Some(0), |_, editor, _| {
        editor.close_side();
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>("select-main-window", 0, Some(0), |_, editor, _| {
        editor.select_main_window();
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>("select-side-window", 0, Some(0), |_, editor, _| {
        editor.select_side_window();
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>("store-windows", 1, Some(1), |heap, editor, arguments| {
        editor.store_windows(&print::display(heap, arguments[0]));
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>(
        "recall-windows",
        1,
        Some(1),
        |heap, editor, arguments| {
            Ok(Value::Bool(
                editor.recall_windows(&print::display(heap, arguments[0])),
            ))
        },
    );
    isolate.define_host_native::<Editor>(
        "forget-windows",
        1,
        Some(1),
        |heap, editor, arguments| {
            editor.forget_windows(&print::display(heap, arguments[0]));
            Ok(Value::Nil)
        },
    );
    isolate.define_host_native::<Editor>("window-layout", 0, Some(0), |heap, editor, _| {
        let arrangement = editor.window_arrangement();
        shown(heap, editor, &arrangement)
    });
    isolate.define_host_native::<Editor>(
        "arrange-windows",
        1,
        Some(3),
        |heap, editor, arguments| {
            let arrangement = read(heap, editor, arguments[0])?;
            let select = match &arguments[1..] {
                [Value::Keyword(option), name] if option.name() == "select" => {
                    let name = print::display(heap, *name);
                    Some(editor.buffer_named(&name).ok_or_else(|| {
                        Error::new(format!("arrange-windows: there is no buffer named {name}"))
                    })?)
                }
                [] => None,
                _ => return Err(Error::new("arrange-windows takes :select")),
            };
            editor.arrange_windows(&arrangement, select);
            Ok(Value::Nil)
        },
    );
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

    use crate::Ox;

    fn eval(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
        match ox.eval(editor, source) {
            Ok(shown) => shown,
            Err(error) => format!("error: {}", error.message),
        }
    }

    #[test]
    fn side_windows_show_beside_the_others_and_close() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, r#"(open-buffer "*tree*" ["a"] :show false)"#)
            .unwrap();

        assert_eq!(eval(&mut editor, &mut ox, "(side-window)"), "nil");
        eval(
            &mut editor,
            &mut ox,
            r#"(show-side-window "*tree*" :edge :right :width 24 :select true)"#,
        );
        assert_eq!(editor.buffer().name(), "*tree*");
        assert_eq!(
            eval(&mut editor, &mut ox, "(side-window)"),
            r#"{buffer: "*tree*" edge: :right width: 24 selected: true}"#
        );
        eval(&mut editor, &mut ox, "(select-main-window)");
        assert_eq!(editor.buffer().name(), "*scratch*");
        eval(&mut editor, &mut ox, "(select-side-window)");
        assert_eq!(editor.buffer().name(), "*tree*");
        eval(&mut editor, &mut ox, "(close-side-window)");
        assert_eq!(eval(&mut editor, &mut ox, "(side-window)"), "nil");
        assert_eq!(editor.buffer().name(), "*scratch*");
        assert!(
            eval(
                &mut editor,
                &mut ox,
                r#"(show-side-window "*tree*" :edge :up)"#
            )
            .contains("show-side-window's :edge is :left or :right")
        );
    }

    #[test]
    fn window_layouts_are_stored_and_recalled_by_key() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, r#"(open-buffer "*a*" [] :show false)"#)
            .unwrap();
        eval(
            &mut editor,
            &mut ox,
            r#"(arrange-windows [:columns "*a*" "*scratch*"] :select "*scratch*")"#,
        );

        assert_eq!(eval(&mut editor, &mut ox, "(store-windows :one)"), "nil");
        eval(&mut editor, &mut ox, r#"(arrange-windows "*a*")"#);
        assert_eq!(eval(&mut editor, &mut ox, "(recall-windows :one)"), "true");

        assert_eq!(editor.buffer().name(), "*scratch*");
        assert_eq!(
            eval(&mut editor, &mut ox, "(window-layout)"),
            r#"[:columns [0.5 "*a*"] [0.5 "*scratch*"]]"#
        );
        assert_eq!(eval(&mut editor, &mut ox, "(recall-windows :one)"), "false");
        eval(
            &mut editor,
            &mut ox,
            "(store-windows :two) (forget-windows :two)",
        );
        assert_eq!(eval(&mut editor, &mut ox, "(recall-windows :two)"), "false");
    }

    #[test]
    fn windows_are_arranged_from_data_and_read_back_as_data() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(
            &mut editor,
            r#"(open-buffer "*a*" [] :show false) (open-buffer "*b*" [] :show false)"#,
        )
        .unwrap();

        eval(
            &mut editor,
            &mut ox,
            r#"(arrange-windows [:columns "*a*" [0.75 [:rows "*b*" "*scratch*"]]] :select "*b*")"#,
        );

        assert_eq!(editor.buffer().name(), "*b*");
        assert_eq!(
            eval(&mut editor, &mut ox, "(window-layout)"),
            r#"[:columns [0.25 "*a*"] [0.75 [:rows [0.5 "*b*"] [0.5 "*scratch*"]]]]"#
        );
        eval(&mut editor, &mut ox, r#"(arrange-windows "*a*")"#);
        assert_eq!(eval(&mut editor, &mut ox, "(window-layout)"), r#""*a*""#);
        assert!(
            eval(
                &mut editor,
                &mut ox,
                r#"(arrange-windows [:rows "*nope*"])"#
            )
            .contains("there is no buffer named *nope*")
        );
        assert!(
            eval(&mut editor, &mut ox, "(arrange-windows [:diagonal])")
                .contains("arrange-windows expects")
        );
    }
}
