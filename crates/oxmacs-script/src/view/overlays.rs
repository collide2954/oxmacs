use ox_lang::heap::Heap;
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::Editor;
use oxmacs_core::view::overlays::{Overlay, Overlays, Place};

const NAME: &str = "set-overlay";

fn wrong(heap: &Heap, wanted: &str, value: Value) -> Error {
    Error::new(format!(
        "{NAME} {wanted}, got {}",
        print::brief(heap, value)
    ))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "scales are small numbers"
)]
fn scale(value: Value) -> Option<f32> {
    match value {
        Value::Int(number) => Some(number as f32),
        Value::Float(number) => Some(number as f32),
        _ => None,
    }
}

fn place(heap: &Heap, value: Value) -> Result<Place, Error> {
    match value {
        Value::Keyword(name) if name.name() == "bottom-right" => Ok(Place::BottomRight),
        Value::Keyword(name) if name.name() == "bottom-center" => Ok(Place::BottomCenter),
        _ => Err(wrong(
            heap,
            ":place is :bottom-right or :bottom-center",
            value,
        )),
    }
}

fn set_overlay(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let key = print::display(heap, arguments[0]);
    let mut overlay = Overlay {
        place: Place::BottomRight,
        scale: 1.0,
        rows: Vec::new(),
    };
    if arguments[2] != Value::Nil {
        overlay.place = place(heap, arguments[2])?;
    }
    if arguments[3] != Value::Nil {
        overlay.scale = scale(arguments[3])
            .filter(|scale| *scale > 0.0)
            .ok_or_else(|| wrong(heap, ":scale must be a number above 0", arguments[3]))?;
    }
    overlay.rows = if arguments[1] == Value::Nil {
        Vec::new()
    } else {
        crate::api::lines::lines(heap, NAME, arguments[1])?
            .iter()
            .map(crate::api::lines::Line::row)
            .collect()
    };
    let mut overlays = editor.take_plugin_state::<Overlays>().unwrap_or_default();
    let at = overlays.0.iter().position(|(known, _)| *known == key);
    match (at, overlay.rows.is_empty()) {
        (Some(at), true) => {
            overlays.0.remove(at);
        }
        (Some(at), false) => overlays.0[at].1 = overlay,
        (None, false) => overlays.0.push((key, overlay)),
        (None, true) => {}
    }
    editor.set_plugin_state(overlays);
    Ok(Value::Nil)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", NAME)
        .arity(2, Some(2))
        .keywords(&["place", "scale"])
        .host::<Editor>(set_overlay);
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::theme::Highlight;
    use oxmacs_core::view::overlays::{Look, Overlay, Piece, Place, Row, shown};

    use crate::Ox;

    fn piece(text: &str, look: Look) -> Piece {
        Piece {
            text: text.to_owned(),
            look,
        }
    }

    fn plain(text: &str) -> Row {
        Row {
            pieces: vec![piece(text, Look::Plain)],
            opacity: 1.0,
        }
    }

    #[test]
    fn overlays_float_rows_of_pieces_until_taken_away() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        ox.eval(
            &mut editor,
            r#"(set-overlay :keys [{line: [["C-x" nil :keycap] ["C-s" nil :keycap] ["×2" :comment] "save-buffer"] opacity: 0.5} "plain"] :place :bottom-center :scale 1.5)
(set-overlay :other [["hi" "there"]])"#,
        )
        .unwrap();

        assert_eq!(
            shown(&editor),
            [
                &Overlay {
                    place: Place::BottomCenter,
                    scale: 1.5,
                    rows: vec![
                        Row {
                            pieces: vec![
                                piece("C-x", Look::Keycap),
                                piece("C-s", Look::Keycap),
                                piece("×2", Look::Face(Highlight::Comment)),
                                piece("save-buffer", Look::Plain),
                            ],
                            opacity: 0.5,
                        },
                        plain("plain"),
                    ],
                },
                &Overlay {
                    place: Place::BottomRight,
                    scale: 1.0,
                    rows: vec![Row {
                        pieces: vec![piece("hi", Look::Plain), piece("there", Look::Plain)],
                        opacity: 1.0,
                    }],
                },
            ]
        );

        ox.eval(&mut editor, r#"(set-overlay :keys ["again"])"#)
            .unwrap();
        assert_eq!(
            shown(&editor)[0],
            &Overlay {
                place: Place::BottomRight,
                scale: 1.0,
                rows: vec![plain("again")],
            }
        );

        ox.eval(
            &mut editor,
            "(set-overlay :keys nil) (set-overlay :other [])",
        )
        .unwrap();
        assert_eq!(shown(&editor), Vec::<&Overlay>::new());
    }

    #[test]
    fn mistakes_in_overlays_are_explained() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        let error = |ox: &mut Ox, editor: &mut Editor, source: &str| {
            ox.eval(editor, source).unwrap_err().message
        };

        assert_eq!(
            error(&mut ox, &mut editor, r#"(set-overlay :x [[["a" :nope]]])"#),
            "set-overlay doesn't know the face :nope"
        );
        assert_eq!(
            error(
                &mut ox,
                &mut editor,
                r#"(set-overlay :x [{line: "a" opacity: :half}])"#
            ),
            "set-overlay expects a number from 0 to 1 for :opacity, got :half"
        );
        assert_eq!(
            error(
                &mut ox,
                &mut editor,
                r#"(set-overlay :x ["a"] :place :middle)"#
            ),
            "set-overlay :place is :bottom-right or :bottom-center, got :middle"
        );
        assert_eq!(
            error(&mut ox, &mut editor, r#"(set-overlay :x ["a"] :scale 0)"#),
            "set-overlay :scale must be a number above 0, got 0"
        );
        assert_eq!(
            error(&mut ox, &mut editor, r#"(set-overlay :x ["a"] :size 2)"#),
            "set-overlay has no keyword parameter :size"
        );
        assert_eq!(shown(&editor), Vec::<&Overlay>::new());
    }
}
