use std::sync::Arc;

use crate::editor::Editor;
use crate::text::BufferId;
use crate::view::{Node, ViewBuffer};

#[derive(Debug, Clone, PartialEq)]
pub struct Caption {
    pub text: String,
    pub at: (f32, f32),
    pub size: f32,
    pub color: [u8; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Screen {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<[u8]>,
    pub generation: u64,
    pub smooth: bool,
    pub captions: Vec<Caption>,
    pub status: Option<String>,
}

impl Screen {
    pub fn blank((width, height): (u32, u32)) -> Self {
        let count = usize::try_from(u64::from(width) * u64::from(height) * 4).unwrap_or(0);
        Self {
            width,
            height,
            pixels: vec![0; count].into(),
            generation: 0,
            smooth: false,
            captions: Vec::new(),
            status: Some("Loading".to_owned()),
        }
    }
}

pub fn screen(editor: &Editor, buffer: BufferId) -> Option<&Screen> {
    match &editor.special_buffer::<ViewBuffer>(buffer)?.1.node {
        Node::Game(screen) => Some(screen),
        _ => None,
    }
}

pub fn screen_mut(editor: &mut Editor, buffer: BufferId) -> Option<&mut Screen> {
    match &mut editor.special_buffer_mut::<ViewBuffer>(buffer)?.1.node {
        Node::Game(screen) => Some(screen),
        _ => None,
    }
}
