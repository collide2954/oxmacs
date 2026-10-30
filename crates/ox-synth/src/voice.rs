use std::fmt;

use crate::controls::Controls;
use crate::graph::Sound;
use crate::samples;
use crate::units::{Boxed, Compiler, Context, Frame, Rng};

const QUIET: f32 = 1e-4;
const FADE_SECONDS: f32 = 0.01;

pub struct Voice {
    unit: Boxed,
    context: Context,
    held: Option<usize>,
    gated: bool,
    fade: f32,
    fading: f32,
    quiet: usize,
    enough: usize,
}

impl fmt::Debug for Voice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Voice")
            .field("pitch", &self.context.pitch)
            .field("gate", &self.context.gate)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Note {
    pub pitch: f32,
    pub velocity: f32,
    pub length: Option<usize>,
}

impl Voice {
    pub fn new(sound: &Sound, note: Note, controls: &Controls, rate: f32, seed: u64) -> Self {
        let mut compiler = Compiler {
            controls,
            rate,
            rng: Rng::new(seed),
        };
        Self {
            unit: compiler.compile(sound),
            context: Context {
                rate,
                pitch: note.pitch,
                gate: true,
                velocity: note.velocity,
                input: [0.0; 2],
            },
            held: note.length,
            gated: sound.gated() || sound.plays_out(),
            fade: 1.0,
            fading: 1.0 / (FADE_SECONDS * rate),
            quiet: 0,
            enough: samples(0.05 * rate),
        }
    }

    pub fn release(&mut self) {
        self.context.gate = false;
        self.held = None;
    }

    pub fn frame(&mut self) -> Frame {
        match &mut self.held {
            Some(0) => self.release(),
            Some(left) => *left -= 1,
            None => {}
        }
        let output = self.unit.next(&self.context);
        if !self.context.gate {
            if !self.gated {
                self.fade = (self.fade - self.fading).max(0.0);
            }
            if output[0].abs().max(output[1].abs()) * self.fade < QUIET {
                self.quiet += 1;
            } else {
                self.quiet = 0;
            }
        }
        let level = self.context.velocity * self.fade;
        output.map(|value| value * level)
    }

    pub fn done(&self) -> bool {
        !self.context.gate && (self.fade <= 0.0 || self.quiet > self.enough)
    }
}
