use std::path::Path;
use std::sync::Arc;

use ox_lang::Isolate;
use ox_lang::build::make_map;
use ox_lang::error::{Error, ErrorKind};
use ox_lang::heap::Heap;
use ox_lang::native::{expected, number, text};
use ox_lang::value::Value;

use crate::graph::{Color, Knob, Node, Pass, Sound, Stages, Wave, constant};
use crate::sample::Sample;
use crate::song::{Automation, Kind, Pattern, Song, Track, pattern};
use crate::{
    MODULE, Note, RATE, float, len_float, notes, render_song, render_sound, samples, single, wav,
};

const SOUND: &str = "synth/sound";
const SAMPLE: &str = "synth/sample";
const PATTERN: &str = "synth/pattern";
const TRACK: &str = "synth/track";
const SONG: &str = "synth/song";

fn arg(arguments: &[Value], at: usize) -> Value {
    arguments.get(at).copied().unwrap_or(Value::Nil)
}

fn shown(name: &str) -> String {
    format!("synth/{name}")
}

fn amount(heap: &Heap, name: &str, value: Value, default: f32) -> Result<f32, Error> {
    match value {
        Value::Nil => Ok(default),
        value => Ok(single(number(heap, name, value)?)),
    }
}

pub fn sound_of(heap: &Heap, value: Value) -> Option<Sound> {
    match value {
        Value::Int(_) | Value::Float(_) => {
            number(heap, "", value).ok().map(|n| constant(single(n)))
        }
        _ => heap.shared_of::<Node>(value),
    }
}

fn sound(heap: &Heap, name: &str, value: Value) -> Result<Sound, Error> {
    sound_of(heap, value).ok_or_else(|| expected(heap, name, "a sound or a number", value))
}

fn or_sound(heap: &Heap, name: &str, value: Value, default: f32) -> Result<Sound, Error> {
    match value {
        Value::Nil => Ok(constant(default)),
        value => sound(heap, name, value),
    }
}

fn wrap(heap: &mut Heap, node: Node) -> Result<Value, Error> {
    heap.shared(SOUND, Arc::new(node)).map_err(Error::from)
}

pub fn sample_of(heap: &Heap, value: Value) -> Option<Arc<Sample>> {
    heap.shared_of::<Sample>(value)
}

fn sample(heap: &Heap, name: &str, value: Value) -> Result<Arc<Sample>, Error> {
    sample_of(heap, value).ok_or_else(|| expected(heap, name, "a sample", value))
}

fn wrap_sample(heap: &mut Heap, sample: Sample) -> Result<Value, Error> {
    heap.shared(SAMPLE, Arc::new(sample)).map_err(Error::from)
}

pub fn song_of(heap: &Heap, value: Value) -> Option<Arc<Song>> {
    heap.shared_of::<Song>(value)
}

fn name_of(heap: &Heap, value: Value) -> Option<String> {
    match value {
        Value::Keyword(name) => Some(name.name().to_owned()),
        _ => heap.text(value).map(str::to_owned),
    }
}

pub fn midi(heap: &Heap, name: &str, value: Value) -> Result<f32, Error> {
    match value {
        Value::Int(_) | Value::Float(_) => Ok(single(number(heap, name, value)?)),
        _ => name_of(heap, value)
            .and_then(|note| notes::midi(&note))
            .ok_or_else(|| expected(heap, name, "a note such as :a4 or a MIDI number", value)),
    }
}

fn knob(heap: &Heap, arguments: &[Value]) -> Result<Knob, Error> {
    let name = "synth/control";
    let label = name_of(heap, arguments[0])
        .ok_or_else(|| expected(heap, name, "a name such as :cutoff", arguments[0]))?;
    let default = single(number(heap, name, arguments[1])?);
    let low = amount(heap, name, arguments[2], default.min(0.0))?;
    let high = amount(
        heap,
        name,
        arguments[3],
        if default > 1.0 { default * 2.0 } else { 1.0 },
    )?;
    Ok(Knob {
        name: label.into(),
        default,
        low,
        high,
    })
}

fn oscillator(heap: &mut Heap, wave: Wave, arguments: &[Value]) -> Result<Value, Error> {
    let name = shown(match wave {
        Wave::Sine => "sine",
        Wave::Saw => "saw",
        Wave::Square => "square",
        Wave::Triangle => "triangle",
    });
    let frequency = match arg(arguments, 0) {
        Value::Nil => Arc::new(Node::Pitch),
        value => sound(heap, &name, value)?,
    };
    let detune = amount(heap, &name, arg(arguments, 1), 0.0)?;
    let octave = amount(heap, &name, arg(arguments, 2), 0.0)?;
    let width = or_sound(heap, &name, arg(arguments, 3), 0.5)?;
    let ratio = (octave + detune / 1200.0).exp2();
    let frequency = if (ratio - 1.0).abs() < f32::EPSILON {
        frequency
    } else {
        Arc::new(Node::Product(vec![frequency, constant(ratio)]))
    };
    wrap(
        heap,
        Node::Oscillator {
            wave,
            frequency,
            width,
        },
    )
}

fn sounds(heap: &Heap, name: &str, values: &[Value]) -> Result<Vec<Sound>, Error> {
    values
        .iter()
        .map(|value| sound(heap, name, *value))
        .collect()
}

fn filter(heap: &mut Heap, pass: Pass, arguments: &[Value]) -> Result<Value, Error> {
    let name = shown(match pass {
        Pass::Low => "lowpass",
        Pass::High => "highpass",
        Pass::Band => "bandpass",
        Pass::Notch => "notch",
    });
    let node = Node::Filter {
        pass,
        input: sound(heap, &name, arguments[0])?,
        cutoff: sound(heap, &name, arguments[1])?,
        resonance: or_sound(heap, &name, arguments[2], 0.0)?,
    };
    wrap(heap, node)
}

fn kind_pattern(
    heap: &mut Heap,
    name: &str,
    kind: Kind,
    text_value: Value,
    velocity: f32,
) -> Result<Value, Error> {
    let written = text(heap, name, text_value)?;
    let found = pattern(written, kind, velocity)
        .map_err(|problem| Error::new(format!("{name} {problem}")))?;
    heap.shared(PATTERN, Arc::new(found)).map_err(Error::from)
}

pub fn automation(
    heap: &Heap,
    name: &str,
    control: Value,
    points: Value,
) -> Result<Automation, Error> {
    let control = name_of(heap, control)
        .ok_or_else(|| expected(heap, name, "a control name such as :swell", control))?;
    let wrong = |value| expected(heap, name, "points as [bar value] pairs", value);
    let pairs = heap.elements(points).ok_or_else(|| wrong(points))?;
    let mut found = Vec::with_capacity(pairs.len());
    for pair in pairs {
        match heap.elements(pair).as_deref() {
            Some([bar, value]) => found.push((
                single(number(heap, name, *bar)?),
                single(number(heap, name, *value)?),
            )),
            _ => return Err(wrong(pair)),
        }
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(Automation {
        control: control.into(),
        points: found,
    })
}

const SONG_OPTIONS: [&str; 4] = ["tempo", "meter", "bars", "automate"];

fn song(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let name = "synth/song";
    let mut options = [Value::Nil; 4];
    let mut tracks = Vec::new();
    let mut given = arguments.iter();
    while let Some(value) = given.next() {
        if let Value::Keyword(key) = value {
            let slot = SONG_OPTIONS
                .iter()
                .position(|known| *known == key.name())
                .ok_or_else(|| {
                    Error::new(format!("synth/song has no option :{}", key.name()))
                        .of(ErrorKind::Arity)
                })?;
            options[slot] = given.next().copied().unwrap_or(Value::Nil);
            continue;
        }
        let track = heap
            .shared_of::<Track>(*value)
            .ok_or_else(|| expected(heap, name, "tracks from synth/track", *value))?;
        tracks.push((*track).clone());
    }
    let [tempo, meter, bars, automate] = options;
    let tempo = amount(heap, name, tempo, 120.0)?;
    let meter = amount(heap, name, meter, 4.0)?;
    if tempo <= 0.0 || meter <= 0.0 {
        return Err(Error::new("synth/song needs a :tempo and :meter above 0").of(ErrorKind::Range));
    }
    let bars = match bars {
        Value::Nil => None,
        value => Some(samples(single(number(heap, name, value)?)).max(1)),
    };
    let mut automations = Vec::new();
    if automate != Value::Nil {
        let entries = heap.entries(automate).ok_or_else(|| {
            expected(
                heap,
                name,
                "a map of controls to points for :automate",
                automate,
            )
        })?;
        for (control, points) in entries {
            automations.push(automation(heap, name, control, points)?);
        }
    }
    let made = Song {
        tempo,
        meter,
        tracks,
        bars,
        automations,
    };
    heap.shared(SONG, Arc::new(made)).map_err(Error::from)
}

fn rendered(heap: &Heap, name: &str, arguments: &[Value]) -> Result<Sample, Error> {
    let thing = arguments[0];
    if let Some(found) = sample_of(heap, thing) {
        return Ok((*found).clone());
    }
    if let Some(found) = song_of(heap, thing) {
        let bars = amount(heap, name, arguments[2], found.length())?;
        return Ok(Sample::from_interleaved(
            RATE,
            2,
            &render_song(&found, bars, RATE),
        ));
    }
    let made = sound(heap, name, thing)?;
    let seconds = amount(heap, name, arguments[1], 1.0)?;
    let pitch = match arguments[3] {
        Value::Nil => 60.0,
        value => midi(heap, name, value)?,
    };
    let note = Note {
        pitch: notes::frequency(pitch),
        velocity: amount(heap, name, arguments[4], 0.8)?,
        length: Some(samples(seconds * float(u64::from(RATE)))),
    };
    Ok(Sample::from_interleaved(
        RATE,
        2,
        &render_sound(&made, note, RATE),
    ))
}

fn info(heap: &mut Heap, found: &Sample) -> Result<Value, Error> {
    let items = [
        Value::keyword("seconds"),
        Value::Float(f64::from(found.seconds())),
        Value::keyword("rate"),
        Value::Int(i64::from(found.rate)),
        Value::keyword("peak"),
        Value::Float(f64::from(found.peak())),
        Value::keyword("rms"),
        Value::Float(f64::from(found.rms())),
    ];
    make_map(heap, &items)
}

const OSCILLATOR: [&str; 2] = ["detune", "octave"];
const SQUARE: [&str; 3] = ["detune", "octave", "width"];
const RENDER: [&str; 4] = ["seconds", "bars", "note", "velocity"];

fn install_sources(isolate: &mut Isolate) {
    for (name, wave) in [
        ("sine", Wave::Sine),
        ("saw", Wave::Saw),
        ("triangle", Wave::Triangle),
    ] {
        isolate
            .native(MODULE, name)
            .arity(0, Some(1))
            .keywords(&OSCILLATOR)
            .function(move |heap, arguments| oscillator(heap, wave, arguments));
    }
    isolate
        .native(MODULE, "square")
        .arity(0, Some(1))
        .keywords(&SQUARE)
        .function(|heap, arguments| oscillator(heap, Wave::Square, arguments));
    isolate
        .native(MODULE, "noise")
        .arity(0, Some(1))
        .function(|heap, arguments| {
            let color = match name_of(heap, arg(arguments, 0)).as_deref() {
                None | Some("white") => Color::White,
                Some("pink") => Color::Pink,
                Some("brown") => Color::Brown,
                Some(_) => {
                    return Err(expected(
                        heap,
                        "synth/noise",
                        ":white, :pink or :brown",
                        arguments[0],
                    ));
                }
            };
            wrap(heap, Node::Noise(color))
        });
    for (name, node) in [
        ("pitch", Node::Pitch),
        ("gate", Node::Gate),
        ("velocity", Node::Velocity),
        ("input", Node::Input),
    ] {
        isolate
            .native(MODULE, name)
            .function(move |heap, _| wrap(heap, node.clone()));
    }
}

fn install_modulation(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "control")
        .arity(2, Some(2))
        .keywords(&["low", "high"])
        .function(|heap, arguments| {
            let found = knob(heap, arguments)?;
            wrap(heap, Node::Control(found))
        });
    isolate
        .native(MODULE, "lfo")
        .arity(3, Some(3))
        .keywords(&["wave"])
        .function(|heap, arguments| {
            let name = "synth/lfo";
            let wave = match name_of(heap, arguments[3]).as_deref() {
                None | Some("sine") => Wave::Sine,
                Some("triangle") => Wave::Triangle,
                Some("saw") => Wave::Saw,
                Some("square") => Wave::Square,
                Some(_) => return Err(expected(heap, name, "a :wave such as :sine", arguments[3])),
            };
            let node = Node::Range {
                input: Arc::new(Node::Oscillator {
                    wave,
                    frequency: sound(heap, name, arguments[0])?,
                    width: constant(0.5),
                }),
                low: sound(heap, name, arguments[1])?,
                high: sound(heap, name, arguments[2])?,
            };
            wrap(heap, node)
        });
    isolate
        .native(MODULE, "sampler")
        .arity(1, Some(1))
        .keywords(&["root", "loop", "start"])
        .function(|heap, arguments| {
            let name = "synth/sampler";
            let node = Node::Sampler {
                sample: (*sample(heap, name, arguments[0])?).clone(),
                root: root(heap, name, arguments[1])?,
                looping: arguments[2].truthy(),
                start: amount(heap, name, arguments[3], 0.0)?,
            };
            wrap(heap, node)
        });
    isolate
        .native(MODULE, "grains")
        .arity(1, Some(1))
        .keywords(&["size", "density", "position", "spread", "root"])
        .function(|heap, arguments| {
            let name = "synth/grains";
            let node = Node::Grains {
                sample: (*sample(heap, name, arguments[0])?).clone(),
                size: amount(heap, name, arguments[1], 0.12)?.max(0.005),
                density: amount(heap, name, arguments[2], 20.0)?.max(0.1),
                position: or_sound(heap, name, arguments[3], 0.5)?,
                spread: amount(heap, name, arguments[4], 0.05)?.clamp(0.0, 1.0),
                root: root(heap, name, arguments[5])?,
            };
            wrap(heap, node)
        });
}

fn root(heap: &Heap, name: &str, value: Value) -> Result<Option<f32>, Error> {
    match value {
        Value::Nil => Ok(None),
        value => Ok(Some(notes::frequency(midi(heap, name, value)?))),
    }
}

fn install_shaping(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "mix")
        .arity(1, None)
        .function(|heap, arguments| {
            let node = Node::Mix(sounds(heap, "synth/mix", arguments)?);
            wrap(heap, node)
        });
    isolate
        .native(MODULE, "mul")
        .arity(1, None)
        .function(|heap, arguments| {
            let node = Node::Product(sounds(heap, "synth/mul", arguments)?);
            wrap(heap, node)
        });
    isolate
        .native(MODULE, "gain")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let name = "synth/gain";
            if let Some(found) = sample_of(heap, arguments[0]) {
                let level = amount(heap, name, arguments[1], 1.0)?;
                return wrap_sample(heap, found.scaled(level));
            }
            let node = Node::Product(sounds(heap, name, &arguments[..2])?);
            wrap(heap, node)
        });
    isolate
        .native(MODULE, "range")
        .arity(3, Some(3))
        .function(|heap, arguments| {
            let name = "synth/range";
            let node = Node::Range {
                input: sound(heap, name, arguments[0])?,
                low: sound(heap, name, arguments[1])?,
                high: sound(heap, name, arguments[2])?,
            };
            wrap(heap, node)
        });
    isolate
        .native(MODULE, "envelope")
        .arity(1, Some(1))
        .keywords(&["attack", "decay", "sustain", "release"])
        .function(|heap, arguments| {
            let name = "synth/envelope";
            let node = Node::Envelope {
                input: sound(heap, name, arguments[0])?,
                stages: Stages {
                    attack: amount(heap, name, arguments[1], 0.01)?,
                    decay: amount(heap, name, arguments[2], 0.2)?,
                    sustain: amount(heap, name, arguments[3], 0.8)?.clamp(0.0, 1.0),
                    release: amount(heap, name, arguments[4], 0.3)?,
                },
            };
            wrap(heap, node)
        });
    for (name, pass) in [
        ("lowpass", Pass::Low),
        ("highpass", Pass::High),
        ("bandpass", Pass::Band),
        ("notch", Pass::Notch),
    ] {
        isolate
            .native(MODULE, name)
            .arity(2, Some(2))
            .keywords(&["resonance"])
            .function(move |heap, arguments| filter(heap, pass, arguments));
    }
    isolate
        .native(MODULE, "pan")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let name = "synth/pan";
            let node = Node::Pan {
                input: sound(heap, name, arguments[0])?,
                position: sound(heap, name, arguments[1])?,
            };
            wrap(heap, node)
        });
}

fn install_coloring(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "drive")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let name = "synth/drive";
            let node = Node::Drive {
                input: sound(heap, name, arguments[0])?,
                amount: sound(heap, name, arguments[1])?,
            };
            wrap(heap, node)
        });
    isolate
        .native(MODULE, "fold")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let name = "synth/fold";
            let node = Node::Fold {
                input: sound(heap, name, arguments[0])?,
                amount: sound(heap, name, arguments[1])?,
            };
            wrap(heap, node)
        });
    isolate
        .native(MODULE, "crush")
        .arity(1, Some(1))
        .keywords(&["bits", "rate"])
        .function(|heap, arguments| {
            let name = "synth/crush";
            let node = Node::Crush {
                input: sound(heap, name, arguments[0])?,
                bits: or_sound(heap, name, arguments[1], 8.0)?,
                rate: or_sound(heap, name, arguments[2], float(u64::from(RATE)))?,
            };
            wrap(heap, node)
        });
    isolate
        .native(MODULE, "vowel")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let name = "synth/vowel";
            let vowel = match arguments[1] {
                Value::Keyword(key) => ["a", "e", "i", "o", "u"]
                    .iter()
                    .position(|known| *known == key.name())
                    .map(|at| constant(len_float(at))),
                value => sound_of(heap, value),
            }
            .ok_or_else(|| {
                expected(
                    heap,
                    name,
                    ":a, :e, :i, :o, :u or a sound from 0 to 4",
                    arguments[1],
                )
            })?;
            let node = Node::Vowel {
                input: sound(heap, name, arguments[0])?,
                vowel,
            };
            wrap(heap, node)
        });
}

fn install_effects(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "delay")
        .arity(2, Some(2))
        .keywords(&["feedback", "mix", "cross"])
        .function(|heap, arguments| {
            let name = "synth/delay";
            let node = Node::Delay {
                input: sound(heap, name, arguments[0])?,
                seconds: single(number(heap, name, arguments[1])?).clamp(0.0, 10.0),
                feedback: or_sound(heap, name, arguments[2], 0.35)?,
                mix: or_sound(heap, name, arguments[3], 0.3)?,
                crossed: arguments[4].truthy(),
            };
            wrap(heap, node)
        });
    isolate
        .native(MODULE, "reverb")
        .arity(1, Some(1))
        .keywords(&["size", "damping", "mix"])
        .function(|heap, arguments| {
            let name = "synth/reverb";
            let node = Node::Reverb {
                input: sound(heap, name, arguments[0])?,
                size: or_sound(heap, name, arguments[1], 0.8)?,
                damping: or_sound(heap, name, arguments[2], 0.3)?,
                mix: or_sound(heap, name, arguments[3], 0.35)?,
            };
            wrap(heap, node)
        });
    isolate
        .native(MODULE, "compress")
        .arity(1, Some(1))
        .keywords(&["threshold", "ratio", "attack", "release", "key"])
        .function(|heap, arguments| {
            let name = "synth/compress";
            let key = match arguments[5] {
                Value::Nil => None,
                value => Some(
                    name_of(heap, value)
                        .ok_or_else(|| expected(heap, name, "a track name such as :kick", value))?
                        .into(),
                ),
            };
            let node = Node::Compress {
                input: sound(heap, name, arguments[0])?,
                threshold: or_sound(heap, name, arguments[1], 0.25)?,
                ratio: or_sound(heap, name, arguments[2], 4.0)?,
                attack: amount(heap, name, arguments[3], 0.005)?,
                release: amount(heap, name, arguments[4], 0.15)?,
                key,
            };
            wrap(heap, node)
        });
    isolate
        .native(MODULE, "chorus")
        .arity(1, Some(1))
        .keywords(&["rate", "depth", "mix"])
        .function(|heap, arguments| {
            let name = "synth/chorus";
            let node = Node::Chorus {
                input: sound(heap, name, arguments[0])?,
                rate: amount(heap, name, arguments[1], 0.5)?.max(0.0),
                depth: amount(heap, name, arguments[2], 0.004)?.clamp(0.0, 0.05),
                mix: or_sound(heap, name, arguments[3], 0.4)?,
            };
            wrap(heap, node)
        });
}

fn install_songs(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "notes")
        .arity(1, Some(1))
        .keywords(&["velocity"])
        .function(|heap, arguments| {
            let velocity = amount(heap, "synth/notes", arguments[1], 0.8)?;
            kind_pattern(heap, "synth/notes", Kind::Notes, arguments[0], velocity)
        });
    isolate
        .native(MODULE, "chords")
        .arity(1, Some(1))
        .keywords(&["octave", "velocity"])
        .function(|heap, arguments| {
            let name = "synth/chords";
            let octave = match arguments[1] {
                Value::Nil => 3,
                Value::Int(octave) => i32::try_from(octave).unwrap_or(3),
                other => return Err(expected(heap, name, "a whole :octave", other)),
            };
            let velocity = amount(heap, name, arguments[2], 0.6)?;
            kind_pattern(heap, name, Kind::Chords { octave }, arguments[0], velocity)
        });
    isolate
        .native(MODULE, "beats")
        .arity(1, Some(1))
        .keywords(&["note", "velocity"])
        .function(|heap, arguments| {
            let name = "synth/beats";
            let note = match arguments[1] {
                Value::Nil => 60.0,
                value => midi(heap, name, value)?,
            };
            let velocity = amount(heap, name, arguments[2], 0.7)?;
            kind_pattern(heap, name, Kind::Beats { note }, arguments[0], velocity)
        });
    isolate
        .native(MODULE, "track")
        .arity(3, Some(3))
        .keywords(&["effects", "gain"])
        .function(|heap, arguments| {
            let name = "synth/track";
            let label = name_of(heap, arguments[0])
                .ok_or_else(|| expected(heap, name, "a track name such as :pad", arguments[0]))?;
            let found = heap.shared_of::<Pattern>(arguments[2]).ok_or_else(|| {
                expected(
                    heap,
                    name,
                    "a pattern from synth/notes, synth/chords or synth/beats",
                    arguments[2],
                )
            })?;
            let effects = match arguments[3] {
                Value::Nil => None,
                value => Some(sound(heap, name, value)?),
            };
            let made = Track {
                name: label.into(),
                sound: sound(heap, name, arguments[1])?,
                pattern: found,
                effects,
                gain: amount(heap, name, arguments[4], 1.0)?,
            };
            heap.shared(TRACK, Arc::new(made)).map_err(Error::from)
        });
    isolate.native(MODULE, "song").arity(0, None).function(song);
    isolate
        .native(MODULE, "frequency")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let pitch = midi(heap, "synth/frequency", arguments[0])?;
            Ok(Value::Float(f64::from(notes::frequency(pitch))))
        });
}

fn install_samples(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "load")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            let path = text(heap, "synth/load", arguments[0])?;
            let found = Sample::load(Path::new(path))
                .map_err(|problem| Error::new(format!("synth/load {problem}")))?;
            wrap_sample(heap, found)
        });
    isolate
        .native(MODULE, "render")
        .arity(1, Some(1))
        .keywords(&RENDER)
        .function(|heap, arguments| {
            let found = rendered(heap, "synth/render", arguments)?;
            wrap_sample(heap, found)
        });
    isolate
        .native(MODULE, "save")
        .arity(2, Some(2))
        .keywords(&RENDER)
        .outside()
        .function(|heap, arguments| {
            let name = "synth/save";
            let path = text(heap, name, arguments[1])?.to_owned();
            let mut shifted = vec![arguments[0]];
            shifted.extend_from_slice(&arguments[2..]);
            let found = rendered(heap, name, &shifted)?;
            wav::save(Path::new(&path), found.rate, &found.interleaved())
                .map_err(|error| Error::new(format!("synth/save can't write {path}: {error}")))?;
            heap.string(&path).map_err(Error::from)
        });
}

fn install_editing(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "slice")
        .arity(2, Some(3))
        .function(|heap, arguments| {
            let name = "synth/slice";
            let found = sample(heap, name, arguments[0])?;
            let from = amount(heap, name, arguments[1], 0.0)?;
            let to = match arg(arguments, 2) {
                Value::Nil => None,
                value => Some(amount(heap, name, value, 0.0)?),
            };
            wrap_sample(heap, found.slice(from, to))
        });
    isolate
        .native(MODULE, "reverse")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let found = sample(heap, "synth/reverse", arguments[0])?;
            wrap_sample(heap, found.reverse())
        });
    isolate
        .native(MODULE, "resample")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let name = "synth/resample";
            let found = sample(heap, name, arguments[0])?;
            let ratio = amount(heap, name, arguments[1], 1.0)?;
            if ratio <= 0.0 {
                return Err(Error::new("synth/resample needs a ratio above 0").of(ErrorKind::Range));
            }
            wrap_sample(heap, found.resampled(ratio))
        });
    isolate
        .native(MODULE, "fade")
        .arity(1, Some(1))
        .keywords(&["in", "out"])
        .function(|heap, arguments| {
            let name = "synth/fade";
            let found = sample(heap, name, arguments[0])?;
            let rising = amount(heap, name, arguments[1], 0.0)?;
            let falling = amount(heap, name, arguments[2], 0.0)?;
            wrap_sample(heap, found.faded(rising, falling))
        });
    isolate
        .native(MODULE, "normalize")
        .arity(1, Some(1))
        .keywords(&["peak"])
        .function(|heap, arguments| {
            let name = "synth/normalize";
            let found = sample(heap, name, arguments[0])?;
            let peak = amount(heap, name, arguments[1], 1.0)?;
            wrap_sample(heap, found.normalized(peak))
        });
    for (name, joined) in [("join", true), ("layer", false)] {
        isolate
            .native(MODULE, name)
            .arity(1, None)
            .function(move |heap, arguments| {
                let shown = shown(name);
                let parts = arguments
                    .iter()
                    .map(|value| sample(heap, &shown, *value).map(|found| (*found).clone()))
                    .collect::<Result<Vec<_>, _>>()?;
                let made = if joined {
                    Sample::joined(&parts)
                } else {
                    Sample::layered(&parts)
                };
                wrap_sample(heap, made)
            });
    }
    isolate
        .native(MODULE, "process")
        .arity(2, Some(2))
        .keywords(&["tail"])
        .function(|heap, arguments| {
            let name = "synth/process";
            let found = sample(heap, name, arguments[0])?;
            let effects = sound(heap, name, arguments[1])?;
            let tail = amount(heap, name, arguments[2], 2.0)?.clamp(0.0, 60.0);
            wrap_sample(heap, found.processed(&effects, tail))
        });
    isolate
        .native(MODULE, "info")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let found = sample(heap, "synth/info", arguments[0])?;
            info(heap, &found)
        });
}

pub(crate) fn install(isolate: &mut Isolate) {
    isolate.add_module(
        MODULE,
        "Sound from graphs of oscillators, noise, samples, envelopes, filters and effects, songs of patterned tracks, and WAV files",
    );
    install_sources(isolate);
    install_modulation(isolate);
    install_editing(isolate);
    install_shaping(isolate);
    install_coloring(isolate);
    install_effects(isolate);
    install_songs(isolate);
    install_samples(isolate);
}
