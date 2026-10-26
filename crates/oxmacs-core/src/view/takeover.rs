use crate::editor::Editor;
use crate::view::shapes::{Paint, Shape};

#[derive(Debug, Clone, PartialEq)]
pub struct Takeover {
    pub background: Paint,
    pub shapes: Vec<Shape>,
}

pub fn current(editor: &Editor) -> Option<&Takeover> {
    editor.plugin_state::<Takeover>()
}
