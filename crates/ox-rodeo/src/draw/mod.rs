pub mod effects;
mod quad;
pub mod raster;
mod viewport;

pub use quad::Instance;
pub use viewport::Viewport;

use std::ops::Range;

use crate::spacetime::Scene;
use crate::texture::TextureId;
use ox_library::color::Color;
use ox_library::geometry::Rect;
use ox_library::geometry::Vec2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline {
    pub color: Color,
    pub width: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    pub texture: TextureId,
    pub region: Rect,
    pub position: Vec2,
    pub rotation: f64,
    pub scale: Vec2,
    pub origin: Vec2,
    pub flip_x: bool,
    pub flip_y: bool,
    pub tint: Color,
    pub flash: f32,
    pub outline: Option<Outline>,
    pub palette: Option<usize>,
    pub layer: i32,
}

impl Sprite {
    pub fn new(texture: TextureId, region: Rect) -> Self {
        Self {
            texture,
            region,
            position: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::new(1.0, 1.0),
            origin: Vec2::ZERO,
            flip_x: false,
            flip_y: false,
            tint: Color::WHITE,
            flash: 0.0,
            outline: None,
            palette: None,
            layer: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Rect {
        rect: Rect,
        stroke: Option<f64>,
    },
    Circle {
        center: Vec2,
        radius: f64,
        stroke: Option<f64>,
    },
    Line {
        from: Vec2,
        to: Vec2,
        width: f64,
    },
    Polygon {
        points: Vec<Vec2>,
        stroke: Option<f64>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub content: String,
    pub position: Vec2,
    pub size: f32,
    pub color: Color,
    pub font: Option<String>,
}

impl Text {
    pub fn new(content: impl Into<String>, position: Vec2) -> Self {
        Self {
            content: content.into(),
            position,
            size: 16.0,
            color: Color::WHITE,
            font: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Sprite(Sprite),
    Shape {
        shape: Shape,
        color: Color,
        layer: i32,
    },
    Text {
        text: Text,
        layer: i32,
    },
    BlackHole {
        rect: Rect,
        scene: Box<Scene>,
        layer: i32,
    },
}

impl Command {
    pub fn layer(&self) -> i32 {
        match self {
            Self::Sprite(sprite) => sprite.layer,
            Self::Shape { layer, .. }
            | Self::Text { layer, .. }
            | Self::BlackHole { layer, .. } => *layer,
        }
    }

    fn kind(&self) -> BatchKind {
        match self {
            Self::Sprite(sprite) => BatchKind::Sprites(sprite.texture),
            Self::Shape { .. } => BatchKind::Shapes,
            Self::Text { .. } => BatchKind::Text,
            Self::BlackHole { .. } => BatchKind::BlackHole,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BatchKind {
    Sprites(TextureId),
    Shapes,
    Text,
    BlackHole,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    pub layer: i32,
    pub kind: BatchKind,
    pub range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub from: Vec<Color>,
    pub to: Vec<Color>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrawList {
    commands: Vec<Command>,
    palettes: Vec<Palette>,
    pub background: Color,
    pub fade: Option<Color>,
}

impl Default for DrawList {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawList {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            palettes: Vec::new(),
            background: Color::BLACK,
            fade: None,
        }
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    pub fn palettes(&self) -> &[Palette] {
        &self.palettes
    }

    pub fn sprite(&mut self, sprite: Sprite) {
        self.commands.push(Command::Sprite(sprite));
    }

    pub fn shape(&mut self, shape: Shape, color: Color, layer: i32) {
        self.commands.push(Command::Shape {
            shape,
            color,
            layer,
        });
    }

    pub fn text(&mut self, text: Text, layer: i32) {
        self.commands.push(Command::Text { text, layer });
    }

    pub fn black_hole(&mut self, rect: Rect, scene: Scene, layer: i32) {
        self.commands.push(Command::BlackHole {
            rect,
            scene: Box::new(scene),
            layer,
        });
    }

    pub fn palette(&mut self, from: Vec<Color>, to: Vec<Color>) -> usize {
        self.palettes.push(Palette { from, to });
        self.palettes.len() - 1
    }

    pub fn clear(&mut self) {
        self.commands.clear();
        self.palettes.clear();
        self.fade = None;
    }

    pub fn finish(&mut self) -> Vec<Batch> {
        self.commands.sort_by_key(Command::layer);
        let mut batches: Vec<Batch> = Vec::new();
        for (index, command) in self.commands.iter().enumerate() {
            let (layer, kind) = (command.layer(), command.kind());
            match batches.last_mut() {
                Some(batch) if batch.layer == layer && batch.kind == kind => {
                    batch.range.end = index + 1;
                }
                _ => batches.push(Batch {
                    layer,
                    kind,
                    range: index..index + 1,
                }),
            }
        }
        batches
    }
}
