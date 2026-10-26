use std::sync::Arc;

use ox_lang::heap::Heap;
use ox_lang::native::given;
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::theme::Highlight;

use crate::view::views::{self, Node, Picture, Press};

const UI: &str = "ox/ui";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub italic: bool,
    pub bold: bool,
    pub code: bool,
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Body,
    Heading(u8),
    Quote,
    Item { marker: String, depth: u8 },
    Pre,
    Center,
    Verse,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Text { kind: Kind, runs: Vec<Run> },
    Image { picture: Picture, alt: String },
    Rule,
}

impl Block {
    pub fn text(&self) -> String {
        match self {
            Self::Text { runs, .. } => runs.iter().map(|run| run.text.as_str()).collect(),
            Self::Image { alt, .. } => alt.clone(),
            Self::Rule => String::new(),
        }
    }
}

#[derive(Debug)]
pub struct Document {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone)]
pub struct Handle(pub Arc<Document>);

impl PartialEq for Handle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum At {
    Anchor(usize, usize),
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typeface {
    Serif,
    Sans,
    Mono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spread {
    Auto,
    Single,
    Double,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    pub from: (usize, usize),
    pub to: (usize, usize),
    pub face: Highlight,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pages {
    pub document: Handle,
    pub at: At,
    pub typeface: Typeface,
    pub size: f32,
    pub spread: Spread,
    pub marks: Vec<Mark>,
    pub header: (String, String),
    pub on_pages: Option<Press>,
    pub on_turn: Option<Press>,
    pub on_select: Option<Press>,
    pub on_link: Option<Press>,
}

impl Pages {
    pub(crate) fn presses(&mut self) -> [&mut Option<Press>; 4] {
        [
            &mut self.on_pages,
            &mut self.on_turn,
            &mut self.on_select,
            &mut self.on_link,
        ]
    }
}

fn wrong(heap: &Heap, name: &str, wanted: &str, value: Value) -> Error {
    Error::new(format!(
        "{name} expects {wanted}, got {}",
        print::brief(heap, value)
    ))
}

fn field(heap: &Heap, map: Value, name: &str) -> Value {
    heap.entries(map)
        .unwrap_or_default()
        .into_iter()
        .find(|(key, _)| matches!(key, Value::Keyword(found) if found.name() == name))
        .map_or(Value::Nil, |(_, value)| value)
}

fn count(heap: &Heap, name: &str, value: Value) -> Result<usize, Error> {
    match value {
        Value::Int(number) => {
            usize::try_from(number).map_err(|_| wrong(heap, name, "a whole number", value))
        }
        _ => Err(wrong(heap, name, "a whole number", value)),
    }
}

fn run(heap: &Heap, value: Value) -> Result<Run, Error> {
    if let Some(text) = heap.text(value) {
        return Ok(Run {
            text: text.to_owned(),
            ..Run::default()
        });
    }
    if heap.entries(value).is_none() {
        return Err(wrong(heap, "ui/document", "runs of text or maps", value));
    }
    let text = field(heap, value, "text");
    let link = field(heap, value, "link");
    Ok(Run {
        text: heap
            .text(text)
            .ok_or_else(|| wrong(heap, "ui/document", "a run's :text as a string", text))?
            .to_owned(),
        italic: field(heap, value, "italic").truthy(),
        bold: field(heap, value, "bold").truthy(),
        code: field(heap, value, "code").truthy(),
        link: (link != Value::Nil).then(|| print::display(heap, link)),
    })
}

fn runs(heap: &Heap, value: Value) -> Result<Vec<Run>, Error> {
    heap.elements(value)
        .filter(|_| heap.text(value).is_none())
        .ok_or_else(|| wrong(heap, "ui/document", "a vector of :runs", value))?
        .into_iter()
        .map(|item| run(heap, item))
        .collect()
}

fn block(heap: &Heap, value: Value) -> Result<Block, Error> {
    let kind = field(heap, value, "kind");
    let Value::Keyword(kind) = kind else {
        return Err(wrong(heap, "ui/document", "blocks with a :kind", value));
    };
    let text = |kind: Kind| -> Result<Block, Error> {
        Ok(Block::Text {
            kind,
            runs: runs(heap, field(heap, value, "runs"))?,
        })
    };
    match kind.name() {
        "paragraph" => text(Kind::Body),
        "quote" => text(Kind::Quote),
        "center" => text(Kind::Center),
        "verse" => text(Kind::Verse),
        "heading" => {
            let level = count(heap, "ui/document", field(heap, value, "level"))?;
            text(Kind::Heading(u8::try_from(level.clamp(1, 6)).unwrap_or(1)))
        }
        "item" => {
            let marker = print::display(heap, field(heap, value, "marker"));
            let depth = match field(heap, value, "depth") {
                Value::Nil => 0,
                given => count(heap, "ui/document", given)?,
            };
            text(Kind::Item {
                marker: if marker == "nil" {
                    "•".to_owned()
                } else {
                    marker
                },
                depth: u8::try_from(depth.min(8)).unwrap_or(0),
            })
        }
        "pre" => {
            let body = field(heap, value, "text");
            Ok(Block::Text {
                kind: Kind::Pre,
                runs: vec![Run {
                    text: heap
                        .text(body)
                        .ok_or_else(|| wrong(heap, "ui/document", "a :pre block's :text", body))?
                        .to_owned(),
                    code: true,
                    ..Run::default()
                }],
            })
        }
        "image" => {
            let image = field(heap, value, "image");
            let picture = ox_library::image::image_of(heap, image)
                .ok_or_else(|| wrong(heap, "ui/document", "an :image block's image", image))?;
            let alt = field(heap, value, "alt");
            Ok(Block::Image {
                picture: Picture(picture),
                alt: heap.text(alt).unwrap_or_default().to_owned(),
            })
        }
        "rule" => Ok(Block::Rule),
        other => Err(Error::new(format!(
            "ui/document doesn't know blocks of kind :{other}; use :paragraph, :heading, :quote, :item, :pre, :center, :verse, :image or :rule"
        ))),
    }
}

fn document(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let blocks = heap
        .elements(arguments[0])
        .ok_or_else(|| wrong(heap, "ui/document", "a vector of blocks", arguments[0]))?
        .into_iter()
        .map(|item| block(heap, item))
        .collect::<Result<Vec<Block>, Error>>()?;
    heap.foreign("document", Handle(Arc::new(Document { blocks })))
        .map_err(|full| Error::new(full.to_string()))
}

fn anchor(heap: &Heap, name: &str, value: Value) -> Result<(usize, usize), Error> {
    match heap.elements(value).as_deref() {
        Some([block, offset]) => Ok((count(heap, name, *block)?, count(heap, name, *offset)?)),
        _ => Err(wrong(heap, name, "[block character]", value)),
    }
}

fn marks(heap: &Heap, value: Value) -> Result<Vec<Mark>, Error> {
    heap.elements(value)
        .ok_or_else(|| wrong(heap, "ui/pages", "a vector of :marks", value))?
        .into_iter()
        .map(|item| {
            let face = field(heap, item, "face");
            Ok(Mark {
                from: anchor(heap, "ui/pages", field(heap, item, "from"))?,
                to: anchor(heap, "ui/pages", field(heap, item, "to"))?,
                face: match face {
                    Value::Keyword(name) => views::face_named(name.name()),
                    _ => None,
                }
                .ok_or_else(|| wrong(heap, "ui/pages", "a mark's :face", face))?,
            })
        })
        .collect()
}

fn choice<T: Copy>(
    heap: &Heap,
    option: &str,
    value: Value,
    known: &[(&str, T)],
) -> Result<T, Error> {
    match value {
        Value::Keyword(name) => known
            .iter()
            .find(|(known, _)| *known == name.name())
            .map(|(_, chosen)| *chosen),
        _ => None,
    }
    .ok_or_else(|| {
        let names: Vec<String> = known.iter().map(|(name, _)| format!(":{name}")).collect();
        wrong(
            heap,
            "ui/pages",
            &format!(":{option} as {}", names.join(" or ")),
            value,
        )
    })
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "text sizes are small numbers"
)]
fn size(heap: &Heap, value: Value) -> Result<f32, Error> {
    match value {
        Value::Int(number) => Ok(number as f32),
        Value::Float(number) => Ok(number as f32),
        _ => Err(wrong(heap, "ui/pages", "a :size in pixels", value)),
    }
}

fn pages(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let document = heap
        .foreign_of::<Handle>(arguments[0])
        .cloned()
        .ok_or_else(|| wrong(heap, "ui/pages", "a ui/document", arguments[0]))?;
    let mut shown = Pages {
        document,
        at: At::Anchor(0, 0),
        typeface: Typeface::Serif,
        size: 18.0,
        spread: Spread::Auto,
        marks: Vec::new(),
        header: (String::new(), String::new()),
        on_pages: None,
        on_turn: None,
        on_select: None,
        on_link: None,
    };
    for (option, value) in given(&PAGES, &arguments[1..]) {
        match option {
            "at" => {
                shown.at = match value {
                    Value::Keyword(name) if name.name() == "end" => At::End,
                    _ => {
                        let (block, offset) = anchor(heap, "ui/pages", value)?;
                        At::Anchor(block, offset)
                    }
                };
            }
            "typeface" => {
                shown.typeface = choice(
                    heap,
                    "typeface",
                    value,
                    &[
                        ("serif", Typeface::Serif),
                        ("sans", Typeface::Sans),
                        ("mono", Typeface::Mono),
                    ],
                )?;
            }
            "spread" => {
                shown.spread = choice(
                    heap,
                    "spread",
                    value,
                    &[
                        ("auto", Spread::Auto),
                        ("single", Spread::Single),
                        ("double", Spread::Double),
                    ],
                )?;
            }
            "size" => shown.size = size(heap, value)?.clamp(6.0, 96.0),
            "marks" => shown.marks = marks(heap, value)?,
            "header" => {
                let parts = heap.elements(value).unwrap_or_default();
                let part = |at: usize| {
                    parts
                        .get(at)
                        .map(|part| print::display(heap, *part))
                        .unwrap_or_default()
                };
                shown.header = (part(0), part(1));
            }
            "on-pages" => shown.on_pages = Some(Press::Held(heap.hold(value))),
            "on-turn" => shown.on_turn = Some(Press::Held(heap.hold(value))),
            "on-select" => shown.on_select = Some(Press::Held(heap.hold(value))),
            _ => shown.on_link = Some(Press::Held(heap.hold(value))),
        }
    }
    heap.foreign("pages", Node::Pages(Box::new(shown)))
        .map_err(|full| Error::new(full.to_string()))
}

const PAGES: [&str; 10] = [
    "at",
    "typeface",
    "size",
    "spread",
    "marks",
    "header",
    "on-pages",
    "on-turn",
    "on-select",
    "on-link",
];

pub fn install(isolate: &mut Isolate) {
    isolate
        .native(UI, "document")
        .arity(1, Some(1))
        .function(document);
    isolate
        .native(UI, "pages")
        .arity(1, Some(1))
        .keywords(&PAGES)
        .function(pages);
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::theme::Highlight;

    use super::{At, Block, Kind, Mark, Run, Spread, Typeface};
    use crate::Ox;
    use crate::testing::settle;
    use crate::view::views::{Arg, Node, Press, ViewBuffer};

    const BOOK: &str = r##"(module (import (ox ui) (ox image)))
(def heard (atom []))
(def doc (ui/document [{kind: :heading level: 1 runs: ["One"]}
                       {kind: :paragraph runs: ["It " {text: "was" italic: true} " a " {text: "night" bold: true link: "#n"}]}
                       {kind: :item runs: ["x"] marker: "1." depth: 1}
                       {kind: :pre text: "code"}
                       {kind: :rule}
                       {kind: :verse runs: ["Tyger Tyger"]}
                       {kind: :image image: (image/new 2 2) alt: "dot"}]))
(open-view "*book*" (fn [] (ui/pages doc :at [1 3] :typeface :sans :size 20 :spread :double
                                         :marks [{from: [1 0] to: [1 2] face: :marked}]
                                         :header ["Book" "One"]
                                         :on-pages (fn [starts turn] (swap! heard conj [:pages starts turn]))
                                         :on-turn (fn [by] (swap! heard conj [:turn by])))))"##;

    fn shown(editor: &Editor) -> super::Pages {
        let id = editor.buffer_named("*book*").unwrap();
        match &editor.plugin_buffer::<ViewBuffer>(id).unwrap().1.node {
            Node::Pages(pages) => (**pages).clone(),
            other => panic!("not pages: {other:?}"),
        }
    }

    #[test]
    fn documents_hold_blocks_of_styled_runs_and_pages_show_them() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, BOOK).unwrap();
        settle(&mut editor, &mut ox);

        let pages = shown(&editor);
        let blocks = &pages.document.0.blocks;
        assert_eq!(blocks.len(), 7);
        assert_eq!(
            blocks[1],
            Block::Text {
                kind: Kind::Body,
                runs: vec![
                    Run {
                        text: "It ".to_owned(),
                        ..Run::default()
                    },
                    Run {
                        text: "was".to_owned(),
                        italic: true,
                        ..Run::default()
                    },
                    Run {
                        text: " a ".to_owned(),
                        ..Run::default()
                    },
                    Run {
                        text: "night".to_owned(),
                        bold: true,
                        link: Some("#n".to_owned()),
                        ..Run::default()
                    },
                ],
            }
        );
        assert_eq!(blocks[1].text(), "It was a night");
        assert!(
            matches!(&blocks[2], Block::Text { kind: Kind::Item { marker, depth: 1 }, .. } if marker == "1.")
        );
        assert!(matches!(
            &blocks[3],
            Block::Text {
                kind: Kind::Pre,
                ..
            }
        ));
        assert_eq!(blocks[4], Block::Rule);
        assert!(matches!(
            &blocks[5],
            Block::Text {
                kind: Kind::Verse,
                ..
            }
        ));
        assert!(matches!(&blocks[6], Block::Image { alt, .. } if alt == "dot"));
        assert_eq!(pages.at, At::Anchor(1, 3));
        assert_eq!(pages.typeface, Typeface::Sans);
        assert_eq!(pages.spread, Spread::Double);
        assert!((pages.size - 20.0).abs() < f32::EPSILON);
        assert_eq!(
            pages.marks,
            [Mark {
                from: (1, 0),
                to: (1, 2),
                face: Highlight::Marked
            }]
        );
        assert_eq!(pages.header, ("Book".to_owned(), "One".to_owned()));
        assert_eq!(pages.on_pages, Some(Press::Slot(0)));
        assert_eq!(pages.on_turn, Some(Press::Slot(1)));
        assert_eq!(pages.on_select, None);
    }

    #[test]
    fn page_callbacks_hear_their_arguments() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, BOOK).unwrap();
        settle(&mut editor, &mut ox);
        let id = editor.buffer_named("*book*").unwrap();

        ox.call(
            &mut editor,
            id,
            0,
            &[
                Arg::List(vec![
                    Arg::List(vec![Arg::Int(0), Arg::Int(0)]),
                    Arg::List(vec![Arg::Int(1), Arg::Int(9)]),
                ]),
                Arg::Int(2),
            ],
        );
        ox.call(&mut editor, id, 1, &[Arg::Int(-1)]);
        settle(&mut editor, &mut ox);

        assert_eq!(
            ox.eval(&mut editor, "@heard").unwrap(),
            "[[:pages [[0 0] [1 9]] 2] [:turn -1]]"
        );
    }

    #[test]
    fn mistakes_in_documents_are_explained() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        let error = |ox: &mut Ox, editor: &mut Editor, source: &str| {
            ox.eval(editor, &format!("(module (import (ox ui))) {source}"))
                .unwrap_err()
                .message
        };

        assert!(
            error(&mut ox, &mut editor, "(ui/document [{kind: :poem}])")
                .starts_with("ui/document doesn't know blocks of kind :poem")
        );
        assert!(
            error(
                &mut ox,
                &mut editor,
                "(ui/document [{kind: :paragraph runs: [5]}])"
            )
            .contains("runs of text or maps")
        );
        assert!(
            error(
                &mut ox,
                &mut editor,
                "(ui/pages (ui/document []) :spread :triple)"
            )
            .contains(":spread as :auto or :single or :double")
        );
    }
}
