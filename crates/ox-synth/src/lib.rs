mod controls;
pub mod engine;
pub mod graph;
mod natives;
pub mod notes;
pub mod sample;
pub mod song;
pub mod units;
pub mod voice;
pub mod wav;

use std::sync::Arc;

pub use controls::{Controls, Shown};
pub use engine::{Command, Engine, Handle, Position, RATE};
pub use graph::{Node, Sound};
pub use natives::{automation, midi, sample_of, song_of, sound_of};
pub use sample::Sample;
pub use song::{Automation, Song};
pub use voice::Note;

pub const DOCS: &str = include_str!("docs.ox");
pub const MODULE: &str = "ox/synth";
const LONGEST_TAIL: f32 = 20.0;

pub fn install(isolate: &mut ox_lang::Isolate) {
    natives::install(isolate);
    isolate.add_native_docs(DOCS);
}

#[expect(
    clippy::cast_precision_loss,
    reason = "sample counts stay far below where f32 loses whole numbers that matter for timing"
)]
pub(crate) fn float(count: u64) -> f32 {
    count as f32
}

pub(crate) fn len_float(count: usize) -> f32 {
    float(u64::try_from(count).unwrap_or(u64::MAX))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to zero or more and rounded first"
)]
pub(crate) fn samples(value: f32) -> usize {
    value.max(0.0).round() as usize
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "sound values need far less precision than an f64 holds"
)]
pub(crate) fn single(value: f64) -> f32 {
    value as f32
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "the sample is clamped to the 16-bit range first"
)]
pub(crate) fn pcm(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

pub fn render_sound(sound: &Sound, note: Note, rate: u32) -> Vec<f32> {
    let mut engine = Engine::offline(rate, Arc::new(Controls::default()));
    engine.apply(Command::Play {
        id: 0,
        sound: Arc::clone(sound),
        note,
    });
    let held = note.length.unwrap_or(0);
    let longest = held + samples(LONGEST_TAIL * float(u64::from(rate)));
    let mut rendered = engine.render(held.max(1));
    while engine.busy() && rendered.len() / 2 < longest {
        rendered.extend(engine.render(1024));
    }
    rendered
}

pub fn render_song(song: &Arc<Song>, bars: f32, rate: u32) -> Vec<f32> {
    let mut engine = Engine::offline(rate, Arc::new(Controls::default()));
    engine.apply(Command::Loop(Arc::clone(song)));
    let length = samples(bars * song.seconds_per_bar() * float(u64::from(rate)));
    let mut rendered = engine.render(length);
    engine.apply(Command::StopSong);
    let longest = length + samples(LONGEST_TAIL * float(u64::from(rate)));
    while engine.busy() && rendered.len() / 2 < longest {
        rendered.extend(engine.render(1024));
    }
    rendered
}
