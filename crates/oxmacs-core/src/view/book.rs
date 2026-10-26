use std::sync::Arc;

use super::Picture;
use crate::theme::Highlight;

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
pub struct Pages<P> {
    pub document: Handle,
    pub at: At,
    pub typeface: Typeface,
    pub size: f32,
    pub spread: Spread,
    pub marks: Vec<Mark>,
    pub header: (String, String),
    pub on_pages: Option<P>,
    pub on_turn: Option<P>,
    pub on_select: Option<P>,
    pub on_link: Option<P>,
}

impl<P> Pages<P> {
    pub fn map<Q>(self, press: &mut impl FnMut(P) -> Q) -> Pages<Q> {
        let on_pages = self.on_pages.map(&mut *press);
        let on_turn = self.on_turn.map(&mut *press);
        let on_select = self.on_select.map(&mut *press);
        let on_link = self.on_link.map(&mut *press);
        Pages {
            document: self.document,
            at: self.at,
            typeface: self.typeface,
            size: self.size,
            spread: self.spread,
            marks: self.marks,
            header: self.header,
            on_pages,
            on_turn,
            on_select,
            on_link,
        }
    }
}
