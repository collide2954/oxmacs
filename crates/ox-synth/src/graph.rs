use std::sync::Arc;

use crate::sample::Sample;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wave {
    Sine,
    Saw,
    Square,
    Triangle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    White,
    Pink,
    Brown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    Low,
    High,
    Band,
    Notch,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stages {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Knob {
    pub name: Arc<str>,
    pub default: f32,
    pub low: f32,
    pub high: f32,
}

pub type Sound = Arc<Node>;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Constant(f32),
    Pitch,
    Gate,
    Velocity,
    Input,
    Control(Knob),
    Oscillator {
        wave: Wave,
        frequency: Sound,
        width: Sound,
    },
    Noise(Color),
    Mix(Vec<Sound>),
    Product(Vec<Sound>),
    Range {
        input: Sound,
        low: Sound,
        high: Sound,
    },
    Envelope {
        input: Sound,
        stages: Stages,
    },
    Filter {
        pass: Pass,
        input: Sound,
        cutoff: Sound,
        resonance: Sound,
    },
    Drive {
        input: Sound,
        amount: Sound,
    },
    Fold {
        input: Sound,
        amount: Sound,
    },
    Crush {
        input: Sound,
        bits: Sound,
        rate: Sound,
    },
    Compress {
        input: Sound,
        threshold: Sound,
        ratio: Sound,
        attack: f32,
        release: f32,
        key: Option<Arc<str>>,
    },
    Vowel {
        input: Sound,
        vowel: Sound,
    },
    Pan {
        input: Sound,
        position: Sound,
    },
    Delay {
        input: Sound,
        seconds: f32,
        feedback: Sound,
        mix: Sound,
        crossed: bool,
    },
    Reverb {
        input: Sound,
        size: Sound,
        damping: Sound,
        mix: Sound,
    },
    Chorus {
        input: Sound,
        rate: f32,
        depth: f32,
        mix: Sound,
    },
    Sampler {
        sample: Sample,
        root: Option<f32>,
        looping: bool,
        start: f32,
    },
    Grains {
        sample: Sample,
        size: f32,
        density: f32,
        position: Sound,
        spread: f32,
        root: Option<f32>,
    },
}

pub fn constant(value: f32) -> Sound {
    Arc::new(Node::Constant(value))
}

impl Node {
    fn inputs(&self) -> Vec<&Sound> {
        match self {
            Self::Constant(_)
            | Self::Pitch
            | Self::Gate
            | Self::Velocity
            | Self::Input
            | Self::Control(_)
            | Self::Noise(_)
            | Self::Sampler { .. } => Vec::new(),
            Self::Grains { position, .. } => vec![position],
            Self::Oscillator {
                frequency, width, ..
            } => vec![frequency, width],
            Self::Mix(sounds) | Self::Product(sounds) => sounds.iter().collect(),
            Self::Range { input, low, high } => vec![input, low, high],
            Self::Envelope { input, .. } => vec![input],
            Self::Filter {
                input,
                cutoff,
                resonance,
                ..
            } => vec![input, cutoff, resonance],
            Self::Drive { input, amount } | Self::Fold { input, amount } => vec![input, amount],
            Self::Crush { input, bits, rate } => vec![input, bits, rate],
            Self::Compress {
                input,
                threshold,
                ratio,
                ..
            } => vec![input, threshold, ratio],
            Self::Vowel { input, vowel } => vec![input, vowel],
            Self::Pan { input, position } => vec![input, position],
            Self::Delay {
                input,
                feedback,
                mix,
                ..
            } => vec![input, feedback, mix],
            Self::Reverb {
                input,
                size,
                damping,
                mix,
            } => vec![input, size, damping, mix],
            Self::Chorus { input, mix, .. } => vec![input, mix],
        }
    }

    pub fn any(&self, wanted: &impl Fn(&Self) -> bool) -> bool {
        wanted(self) || self.inputs().into_iter().any(|input| input.any(wanted))
    }

    pub fn gated(&self) -> bool {
        self.any(&|node| matches!(node, Self::Gate | Self::Envelope { .. }))
    }

    pub fn plays_out(&self) -> bool {
        self.any(&|node| matches!(node, Self::Sampler { looping: false, .. }))
    }

    pub fn knobs(&self) -> Vec<Knob> {
        let mut found = Vec::new();
        self.collect_knobs(&mut found);
        found
    }

    fn collect_knobs(&self, found: &mut Vec<Knob>) {
        if let Self::Control(knob) = self
            && !found.iter().any(|known: &Knob| known.name == knob.name)
        {
            found.push(knob.clone());
        }
        for input in self.inputs() {
            input.collect_knobs(found);
        }
    }
}
