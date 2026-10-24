use ox::heap::Heap;
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::editor::{Arrangement, Axis, Editor};

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

pub fn install(isolate: &mut Isolate) {
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
