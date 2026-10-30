use std::cell::RefCell;
use std::num::NonZero;
use std::sync::Arc;
use std::time::Duration;

use ox_lang::Isolate;
use ox_lang::build::{make_map, make_vector};
use ox_lang::error::Error;
use ox_lang::heap::Heap;
use ox_lang::native::{count, expected, number};
use ox_lang::value::Value;
use ox_synth::{Engine, Handle, Node, Note, RATE};
use rodio::{ChannelCount, SampleRate, Source};

use crate::api::games::audio::{Audio, Capture};

const MODULE: &str = "ox/synth";
pub const DOCS: &str = oxmacs_api::SOUND_DOCS;
const SCOPE_POINTS: usize = 256;

struct Stream {
    engine: Engine,
    frame: [f32; 2],
    right: bool,
}

impl Iterator for Stream {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.right {
            self.right = false;
            return Some(self.frame[1]);
        }
        self.frame = self.engine.next_frame();
        self.right = true;
        Some(self.frame[0])
    }
}

impl Source for Stream {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        NonZero::<u16>::MIN.saturating_add(1)
    }

    fn sample_rate(&self) -> SampleRate {
        NonZero::new(RATE).unwrap_or(NonZero::<u32>::MIN)
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

struct Speaker {
    handle: Handle,
    _audio: Audio,
}

thread_local! {
    static SPEAKER: RefCell<Option<Speaker>> = const { RefCell::new(None) };
}

fn connect(mut audio: Audio) -> Speaker {
    let (engine, handle) = Engine::new(RATE);
    audio.stream(Stream {
        engine,
        frame: [0.0; 2],
        right: false,
    });
    Speaker {
        handle,
        _audio: audio,
    }
}

fn device() -> Result<Audio, Error> {
    if cfg!(test) {
        return Ok(Audio::silent());
    }
    Audio::open().map_err(|error| {
        Error::new(format!(
            "can't play sound: {error}; synth/save writes WAV files instead"
        ))
    })
}

fn speaking<T>(use_it: impl FnOnce(&Handle) -> T) -> Result<T, Error> {
    SPEAKER.with(|speaker| -> Result<T, Error> {
        let mut speaker = speaker.borrow_mut();
        if speaker.is_none() {
            *speaker = Some(connect(device()?));
        }
        let found = speaker
            .as_ref()
            .ok_or_else(|| Error::new("can't play sound"))?;
        Ok(use_it(&found.handle))
    })
}

fn listening<T>(use_it: impl FnOnce(&Handle) -> T) -> Option<T> {
    SPEAKER.with(|speaker| speaker.borrow().as_ref().map(|found| use_it(&found.handle)))
}

pub fn capture() -> Capture {
    let (audio, captured) = Audio::capture(2, RATE);
    SPEAKER.with(|speaker| *speaker.borrow_mut() = Some(connect(audio)));
    captured
}

pub fn silence() {
    listening(Handle::stop_all);
}

fn amount(heap: &Heap, name: &str, value: Value, default: f32) -> Result<f32, Error> {
    match value {
        Value::Nil => Ok(default),
        value => Ok(narrow(number(heap, name, value)?)),
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "sound values need far less precision than an f64 holds"
)]
fn narrow(value: f64) -> f32 {
    value as f32
}

fn voice_of(heap: &Heap, name: &str, value: Value) -> Result<u64, Error> {
    match value {
        Value::Int(id) => {
            u64::try_from(id).map_err(|_| expected(heap, name, "a voice from synth/play", value))
        }
        _ => Err(expected(heap, name, "a voice from synth/play", value)),
    }
}

fn control_name(heap: &Heap, name: &str, value: Value) -> Result<String, Error> {
    match value {
        Value::Keyword(key) => Ok(key.name().to_owned()),
        _ => heap
            .text(value)
            .map(str::to_owned)
            .ok_or_else(|| expected(heap, name, "a control name such as :cutoff", value)),
    }
}

fn play(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let name = "synth/play";
    let sound = match ox_synth::sample_of(heap, arguments[0]) {
        Some(sample) => Arc::new(Node::Sampler {
            sample: (*sample).clone(),
            root: None,
            looping: false,
            start: 0.0,
        }),
        None => ox_synth::sound_of(heap, arguments[0])
            .ok_or_else(|| expected(heap, name, "a sound or a sample", arguments[0]))?,
    };
    let pitch = match arguments[1] {
        Value::Nil => 60.0,
        value => ox_synth::midi(heap, name, value)?,
    };
    let length = match arguments[3] {
        Value::Nil => None,
        value => Some(seconds_to_frames(amount(heap, name, value, 0.0)?)),
    };
    let note = Note {
        pitch: ox_synth::notes::frequency(pitch),
        velocity: amount(heap, name, arguments[2], 0.8)?,
        length,
    };
    let id = speaking(|handle| handle.play(sound, note))?;
    Ok(Value::Int(i64::try_from(id).unwrap_or(i64::MAX)))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the time is clamped to zero or more and rounded first"
)]
fn seconds_to_frames(seconds: f32) -> usize {
    (seconds.max(0.0) * 48_000.0).round() as usize
}

fn controls(heap: &mut Heap) -> Result<Value, Error> {
    let Some((shown, automated)) = listening(|handle| (handle.controls(), handle.automated()))
    else {
        return make_vector(heap, &[]);
    };
    let mut items = Vec::with_capacity(shown.len());
    for control in shown {
        let entries = [
            Value::keyword("name"),
            Value::keyword(&control.name),
            Value::keyword("value"),
            Value::Float(f64::from(control.value)),
            Value::keyword("default"),
            Value::Float(f64::from(control.default)),
            Value::keyword("low"),
            Value::Float(f64::from(control.low)),
            Value::keyword("high"),
            Value::Float(f64::from(control.high)),
            Value::keyword("automated"),
            Value::Bool(automated.contains(&control.name)),
        ];
        items.push(make_map(heap, &entries)?);
    }
    make_vector(heap, &items)
}

fn install_playing(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "play")
        .arity(1, Some(1))
        .keywords(&["note", "velocity", "seconds"])
        .outside()
        .function(play);
    isolate
        .native(MODULE, "release")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            let voice = voice_of(heap, "synth/release", arguments[0])?;
            listening(|handle| handle.release(voice));
            Ok(Value::Nil)
        });
    isolate
        .native(MODULE, "stop")
        .arity(0, Some(1))
        .outside()
        .function(|heap, arguments| {
            match arguments.first().copied().unwrap_or(Value::Nil) {
                Value::Nil => listening(Handle::stop_all),
                value => {
                    let voice = voice_of(heap, "synth/stop", value)?;
                    listening(|handle| handle.stop(voice))
                }
            };
            Ok(Value::Nil)
        });
    isolate
        .native(MODULE, "loop")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            let song = ox_synth::song_of(heap, arguments[0]).ok_or_else(|| {
                expected(heap, "synth/loop", "a song from synth/song", arguments[0])
            })?;
            speaking(|handle| handle.loop_song(song))?;
            Ok(Value::Nil)
        });
}

fn install_controls(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "set")
        .arity(2, Some(2))
        .outside()
        .function(|heap, arguments| {
            let name = "synth/set";
            let control = control_name(heap, name, arguments[0])?;
            let value = narrow(number(heap, name, arguments[1])?);
            speaking(|handle| handle.set(&control, value))?;
            Ok(Value::Nil)
        });
    isolate
        .native(MODULE, "controls")
        .function(|heap, _| controls(heap));
    isolate
        .native(MODULE, "automate")
        .arity(2, Some(2))
        .outside()
        .function(|heap, arguments| {
            let name = "synth/automate";
            if arguments[1] == Value::Nil {
                let control = control_name(heap, name, arguments[0])?;
                listening(|handle| handle.forget(control.into()));
                return Ok(Value::Nil);
            }
            let automation = ox_synth::automation(heap, name, arguments[0], arguments[1])?;
            speaking(|handle| handle.automate(automation))?;
            Ok(Value::Nil)
        });
}

fn install_meters(isolate: &mut Isolate) {
    isolate.native(MODULE, "position").function(|heap, _| {
        match listening(Handle::position).flatten() {
            Some(position) => make_map(
                heap,
                &[
                    Value::keyword("bar"),
                    Value::Float(f64::from(position.bar)),
                    Value::keyword("tempo"),
                    Value::Float(f64::from(position.tempo)),
                ],
            ),
            None => Ok(Value::Nil),
        }
    });
    isolate
        .native(MODULE, "scope")
        .arity(0, Some(1))
        .function(|heap, arguments| {
            let wanted = match arguments.first().copied().unwrap_or(Value::Nil) {
                Value::Nil => SCOPE_POINTS,
                value => count(heap, "synth/scope", value)?.clamp(2, 4096),
            };
            let recent = listening(Handle::scope).unwrap_or_default();
            let points: Vec<Value> = (0..wanted)
                .map(|at| {
                    let index = at * recent.len() / wanted;
                    Value::Float(f64::from(recent.get(index).copied().unwrap_or(0.0)))
                })
                .collect();
            make_vector(heap, &points)
        });
    isolate.native(MODULE, "level").function(|_, _| {
        Ok(Value::Float(f64::from(
            listening(Handle::level).unwrap_or(0.0),
        )))
    });
}

pub fn install(isolate: &mut Isolate) {
    install_playing(isolate);
    install_controls(isolate);
    install_meters(isolate);
    isolate.add_native_docs(DOCS);
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

    use crate::Ox;

    fn evaluated(ox: &mut Ox, editor: &mut Editor, source: &str) -> String {
        ox.eval(
            editor,
            &format!("(module (import (ox synth :as s))) {source}"),
        )
        .unwrap_or_else(|error| format!("error: {}", error.message))
    }

    #[test]
    fn played_sounds_reach_the_speaker_and_stop() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        let mut speaker = super::capture();

        evaluated(
            &mut ox,
            &mut editor,
            "(s/play (s/sine) :note :a4 :seconds 0.2)",
        );
        let heard = speaker.take(2 * 4800);
        assert!(heard.iter().any(|sample| sample.abs() > 0.3));

        evaluated(&mut ox, &mut editor, "(s/play (s/sine) :note :a4) (s/stop)");
        speaker.take(2 * 4800);
        assert!(
            speaker
                .take(2 * 4800)
                .iter()
                .all(|sample| sample.abs() < 1e-3)
        );
    }

    #[test]
    fn looping_songs_report_their_bar_and_automated_controls() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        let mut speaker = super::capture();
        evaluated(
            &mut ox,
            &mut editor,
            r#"(s/loop (s/song :tempo 240
                 (s/track :pad (s/gain (s/sine) (s/control :swell 0.5)) (s/notes "a4 c5"))))
               (s/automate :swell [[0 0] [4 1]])"#,
        );

        speaker.take(2 * 72_000);

        let bar: f64 = evaluated(&mut ox, &mut editor, "(get (s/position) :bar)")
            .parse()
            .unwrap();
        assert!((bar - 1.5).abs() < 0.01, "{bar}");
        assert_eq!(
            evaluated(
                &mut ox,
                &mut editor,
                "(map (fn [c] [c.name c.automated]) (s/controls))"
            ),
            "[[:swell true]]"
        );
        let swell: f64 = evaluated(&mut ox, &mut editor, "(get (first (s/controls)) :value)")
            .parse()
            .unwrap();
        assert!((swell - 0.375).abs() < 0.01, "{swell}");
        assert_eq!(
            evaluated(&mut ox, &mut editor, "(count (s/scope 64))"),
            "64"
        );
    }
}
