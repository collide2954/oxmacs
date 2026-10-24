use ox::heap::Heap;
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::editor::Editor;
use oxmacs_core::theme::Highlight;

use crate::views;

const NAME: &str = "set-overlay";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    BottomRight,
    BottomCenter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Plain,
    Face(Highlight),
    Keycap,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub text: String,
    pub look: Look,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub pieces: Vec<Piece>,
    pub opacity: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Overlay {
    pub place: Place,
    pub scale: f32,
    pub rows: Vec<Row>,
}

#[derive(Debug, Default)]
struct Overlays(Vec<(String, Overlay)>);

pub fn shown(editor: &Editor) -> Vec<&Overlay> {
    editor
        .plugin_state::<Overlays>()
        .map(|overlays| overlays.0.iter().map(|(_, overlay)| overlay).collect())
        .unwrap_or_default()
}

fn wrong(heap: &Heap, wanted: &str, value: Value) -> Error {
    Error::new(format!(
        "{NAME} {wanted}, got {}",
        print::brief(heap, value)
    ))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "opacities and scales are small numbers"
)]
fn number(value: Value) -> Option<f32> {
    match value {
        Value::Int(number) => Some(number as f32),
        Value::Float(number) => Some(number as f32),
        _ => None,
    }
}

fn piece(heap: &Heap, value: Value) -> Result<Piece, Error> {
    if let Some(text) = heap.text(value) {
        return Ok(Piece {
            text: text.to_owned(),
            look: Look::Plain,
        });
    }
    let pair = heap.elements(value).unwrap_or_default();
    let [text, Value::Keyword(face)] = pair[..] else {
        return Err(wrong(heap, "pieces are text or [text face]", value));
    };
    let Some(text) = heap.text(text) else {
        return Err(wrong(heap, "pieces are text or [text face]", value));
    };
    let look = match face.name() {
        "keycap" => Look::Keycap,
        name => Look::Face(
            views::face_named(name)
                .ok_or_else(|| Error::new(format!("{NAME} doesn't know the face :{name}")))?,
        ),
    };
    Ok(Piece {
        text: text.to_owned(),
        look,
    })
}

fn pieces(heap: &Heap, value: Value) -> Result<Vec<Piece>, Error> {
    heap.elements(value)
        .ok_or_else(|| {
            wrong(
                heap,
                "rows are text, vectors of pieces or maps of :pieces and :opacity",
                value,
            )
        })?
        .into_iter()
        .map(|item| piece(heap, item))
        .collect()
}

fn row(heap: &Heap, value: Value) -> Result<Row, Error> {
    if heap.text(value).is_some() {
        return Ok(Row {
            pieces: vec![piece(heap, value)?],
            opacity: 1.0,
        });
    }
    let Some(entries) = heap.entries(value) else {
        return Ok(Row {
            pieces: pieces(heap, value)?,
            opacity: 1.0,
        });
    };
    let mut row = Row {
        pieces: Vec::new(),
        opacity: 1.0,
    };
    for (key, given) in entries {
        match key {
            Value::Keyword(key) if key.name() == "pieces" => row.pieces = pieces(heap, given)?,
            Value::Keyword(key) if key.name() == "opacity" => {
                row.opacity = number(given)
                    .ok_or_else(|| wrong(heap, ":opacity must be a number", given))?
                    .clamp(0.0, 1.0);
            }
            _ => return Err(wrong(heap, "rows take :pieces and :opacity", key)),
        }
    }
    Ok(row)
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
    for pair in arguments[2..].chunks(2) {
        match pair {
            [Value::Keyword(option), value] if option.name() == "place" => {
                overlay.place = place(heap, *value)?;
            }
            [Value::Keyword(option), value] if option.name() == "scale" => {
                overlay.scale = number(*value)
                    .filter(|scale| *scale > 0.0)
                    .ok_or_else(|| wrong(heap, ":scale must be a number above 0", *value))?;
            }
            _ => return Err(Error::new(format!("{NAME} takes :place and :scale"))),
        }
    }
    let rows = heap
        .elements(arguments[1])
        .ok_or_else(|| wrong(heap, "takes a vector of rows", arguments[1]))?;
    overlay.rows = rows
        .into_iter()
        .map(|item| row(heap, item))
        .collect::<Result<_, _>>()?;
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
    isolate.define_host_native::<Editor>(NAME, 2, None, set_overlay);
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::theme::Highlight;

    use super::{Look, Overlay, Piece, Place, Row, shown};
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
            r#"(set-overlay :keys [{pieces: [["C-x" :keycap] ["C-s" :keycap] ["×2" :comment] "save-buffer"] opacity: 0.5} "plain"] :place :bottom-center :scale 1.5)
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
        assert!(shown(&editor).is_empty());
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
                r#"(set-overlay :x [["C-x" :keycap]])"#
            ),
            "set-overlay pieces are text or [text face], got :keycap"
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
            "set-overlay takes :place and :scale"
        );
        assert!(shown(&editor).is_empty());
    }
}
