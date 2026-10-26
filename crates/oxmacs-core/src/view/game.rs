use std::sync::Arc;

use crate::command::ScopeId;
use crate::editor::Editor;
use crate::plugin::PluginBuffer;
use crate::text::BufferId;

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

#[derive(Debug)]
pub struct GameBuffer {
    pub scope: ScopeId,
    pub screen: Screen,
}

impl PluginBuffer for GameBuffer {
    fn scope(&self) -> ScopeId {
        self.scope
    }
}

pub fn screen(editor: &Editor, buffer: BufferId) -> Option<&Screen> {
    editor
        .plugin_buffer::<GameBuffer>(buffer)
        .map(|(_, view)| &view.screen)
}
