use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use crate::editor::Editor;
use crate::view::shapes::{Paint, Shape};

pub const INPUTS: usize = 16;

const PRELUDE: &str = "struct Frame {
    origin: vec2<f32>,
    size: vec2<f32>,
    scale: f32,
    time: f32,
    inputs: array<vec4<f32>, 4>,
}

@group(0) @binding(0) var<uniform> frame: Frame;

fn input(index: i32) -> f32 {
    return frame.inputs[index / 4][index % 4];
}

@vertex
fn oxmacs_vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

@fragment
fn oxmacs_fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return shade((position.xy - frame.origin) / frame.scale);
}
";

#[derive(Debug, Clone, PartialEq)]
pub struct Shader {
    pub source: Arc<str>,
    pub key: u64,
}

impl Shader {
    pub fn new(shade: &str) -> Self {
        let source = format!("{PRELUDE}\n{shade}");
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        Self {
            source: source.into(),
            key: hasher.finish(),
        }
    }

    pub fn line_of(source_line: usize) -> Option<usize> {
        source_line.checked_sub(PRELUDE.lines().count() + 1)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Backdrop {
    pub shader: Shader,
    pub time: f32,
    pub inputs: [f32; INPUTS],
}

impl Backdrop {
    pub fn uniforms(&self, origin: (f32, f32), size: (f32, f32), scale: f32) -> Vec<u8> {
        [
            origin.0, origin.1, size.0, size.1, scale, self.time, 0.0, 0.0,
        ]
        .into_iter()
        .chain(self.inputs)
        .flat_map(f32::to_ne_bytes)
        .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Takeover {
    pub background: Paint,
    pub backdrop: Option<Backdrop>,
    pub shapes: Vec<Shape>,
}

pub fn current(editor: &Editor) -> Option<&Takeover> {
    editor.plugin_state::<Takeover>()
}

#[cfg(test)]
mod tests {
    use super::{Backdrop, INPUTS, Shader};

    #[test]
    fn shaders_wrap_their_shade_function_and_map_lines_back() {
        let shader =
            Shader::new("fn shade(at: vec2<f32>) -> vec4<f32> {\n    return vec4<f32>(1.0);\n}");
        let lines: Vec<&str> = shader.source.lines().collect();
        let first = lines
            .iter()
            .position(|line| line.starts_with("fn shade"))
            .unwrap();

        assert_eq!(Shader::line_of(first + 1), Some(1));
        assert_eq!(Shader::line_of(first + 2), Some(2));
        assert_eq!(Shader::line_of(1), None);
        assert_ne!(shader.key, Shader::new("fn shade() {}").key);
    }

    #[test]
    fn uniforms_pack_the_frame_with_padding_before_the_inputs() {
        let mut inputs = [0.0; INPUTS];
        inputs[0] = 7.0;
        let backdrop = Backdrop {
            shader: Shader::new(""),
            time: 2.5,
            inputs,
        };

        let bytes = backdrop.uniforms((1.0, 2.0), (300.0, 200.0), 2.0);
        let floats: Vec<f32> = bytes
            .chunks(4)
            .map(|chunk| f32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect();

        assert_eq!(bytes.len(), 96);
        assert_eq!(
            &floats[..9],
            [1.0, 2.0, 300.0, 200.0, 2.0, 2.5, 0.0, 0.0, 7.0]
        );
    }
}
