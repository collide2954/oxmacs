use crate::editor::Editor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabBar {
    pub tabs: Vec<String>,
    pub selected: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabEvent {
    Select(usize),
    Close(usize),
    New,
    Move { from: usize, to: usize },
}

pub fn shown(editor: &Editor) -> Option<&TabBar> {
    editor.plugin_state::<TabBar>()
}
