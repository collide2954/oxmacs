pub mod book;
pub mod game;
pub mod overlays;
pub mod pdf;
pub mod shapes;
pub mod tabbar;
pub mod takeover;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::command::ScopeId;
use crate::special_buffer::SpecialBuffer;
use crate::text::BufferId;
use crate::theme::Highlight;
use shapes::Shape;

pub trait Pixels: fmt::Debug + Send + Sync {
    fn size(&self) -> (u32, u32);
    fn rgba(&self) -> &[u8];
    fn key(&self) -> usize;
    fn unused(&self) -> bool;
}

#[derive(Debug, Clone)]
pub struct Picture(pub Arc<dyn Pixels>);

impl Picture {
    pub fn key(&self) -> usize {
        self.0.key()
    }

    pub fn unused(&self) -> bool {
        self.0.unused()
    }

    pub fn size(&self) -> (u32, u32) {
        self.0.size()
    }

    pub fn rgba(&self) -> Vec<u8> {
        self.0.rgba().to_vec()
    }
}

impl PartialEq for Picture {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node<P> {
    Column {
        spacing: f32,
        children: Vec<Node<P>>,
    },
    Row {
        spacing: f32,
        children: Vec<Node<P>>,
    },
    Text {
        text: String,
        face: Option<Highlight>,
        size: Option<f32>,
    },
    Button {
        label: String,
        press: Option<P>,
    },
    Space {
        size: Option<f32>,
    },
    Scrollable(Box<Node<P>>),
    Canvas {
        shapes: Vec<Shape>,
        height: Option<f32>,
        relative: bool,
    },
    Image {
        picture: Picture,
        width: Option<f32>,
        height: Option<f32>,
        press: Option<P>,
        selected: bool,
    },
    Pages(Box<book::Pages<P>>),
    Pdf(Box<pdf::View<P>>),
    Game(Box<game::Screen>),
}

impl<P> Node<P> {
    pub fn map<Q>(self, press: &mut impl FnMut(P) -> Q) -> Node<Q> {
        match self {
            Self::Column { spacing, children } => Node::Column {
                spacing,
                children: children.into_iter().map(|child| child.map(press)).collect(),
            },
            Self::Row { spacing, children } => Node::Row {
                spacing,
                children: children.into_iter().map(|child| child.map(press)).collect(),
            },
            Self::Text { text, face, size } => Node::Text { text, face, size },
            Self::Button { label, press: on } => Node::Button {
                label,
                press: on.map(&mut *press),
            },
            Self::Space { size } => Node::Space { size },
            Self::Scrollable(child) => Node::Scrollable(Box::new(child.map(press))),
            Self::Canvas {
                shapes,
                height,
                relative,
            } => Node::Canvas {
                shapes,
                height,
                relative,
            },
            Self::Image {
                picture,
                width,
                height,
                press: on,
                selected,
            } => Node::Image {
                picture,
                width,
                height,
                press: on.map(&mut *press),
                selected,
            },
            Self::Pages(pages) => Node::Pages(Box::new(pages.map(press))),
            Self::Pdf(view) => Node::Pdf(Box::new(view.map(press))),
            Self::Game(screen) => Node::Game(screen),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    Press {
        buffer: BufferId,
        slot: usize,
    },
    Call {
        buffer: BufferId,
        slot: usize,
        args: Vec<Arg>,
    },
    Sized {
        buffer: BufferId,
        width: u32,
        height: u32,
    },
    Tab(tabbar::TabEvent),
    GameKey {
        name: String,
        down: bool,
    },
    EndTakeOver,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Nil,
    Int(i64),
    Text(String),
    List(Vec<Arg>),
}

#[derive(Debug)]
pub struct ViewBuffer {
    pub scope: ScopeId,
    pub file: Option<PathBuf>,
    pub watched: Option<PathBuf>,
    pub node: Node<usize>,
}

impl SpecialBuffer for ViewBuffer {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    fn watched(&self) -> Option<&Path> {
        self.watched.as_deref()
    }
}
