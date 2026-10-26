use crate::editor::Editor;
use crate::theme::Highlight;

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
pub struct Overlays(pub Vec<(String, Overlay)>);

pub fn shown(editor: &Editor) -> Vec<&Overlay> {
    editor
        .plugin_state::<Overlays>()
        .map(|overlays| overlays.0.iter().map(|(_, overlay)| overlay).collect())
        .unwrap_or_default()
}
