use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};

use crate::controls::{Controls, Shown};
use crate::graph::Sound;
use crate::song::{Automation, Song, Track};
use crate::units::{Boxed, Compiler, Context, Frame, Rng};
use crate::voice::{Note, Voice};
use crate::{float, len_float, notes, samples};

pub const RATE: u32 = 48_000;
const BLOCK: usize = 64;
const SCOPE: usize = 2048;
const TAIL_SECONDS: f32 = 1.0;

#[derive(Debug)]
pub enum Command {
    Play { id: u64, sound: Sound, note: Note },
    Release(u64),
    Stop(u64),
    StopAll,
    Loop(Arc<Song>),
    StopSong,
    Automate(Automation),
    Forget(Arc<str>),
}

#[derive(Debug, Default)]
pub struct Meter {
    scope: Mutex<Vec<f32>>,
    level: AtomicU32,
    bar: AtomicU32,
    tempo: AtomicU32,
    playing: AtomicBool,
    automated: Mutex<Vec<Arc<str>>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub bar: f32,
    pub tempo: f32,
}

#[derive(Debug)]
pub struct Handle {
    commands: Sender<Command>,
    controls: Arc<Controls>,
    meter: Arc<Meter>,
    next: AtomicU64,
}

impl Handle {
    fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    pub fn play(&self, sound: Sound, note: Note) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.send(Command::Play { id, sound, note });
        id
    }

    pub fn release(&self, id: u64) {
        self.send(Command::Release(id));
    }

    pub fn stop(&self, id: u64) {
        self.send(Command::Stop(id));
    }

    pub fn stop_all(&self) {
        self.send(Command::StopAll);
    }

    pub fn loop_song(&self, song: Arc<Song>) {
        self.send(Command::Loop(song));
    }

    pub fn stop_song(&self) {
        self.send(Command::StopSong);
    }

    pub fn automate(&self, automation: Automation) {
        self.send(Command::Automate(automation));
    }

    pub fn forget(&self, control: Arc<str>) {
        self.send(Command::Forget(control));
    }

    pub fn set(&self, name: &str, value: f32) {
        self.controls.set(name, value);
    }

    pub fn controls(&self) -> Vec<Shown> {
        self.controls.list()
    }

    pub fn position(&self) -> Option<Position> {
        self.meter
            .playing
            .load(Ordering::Relaxed)
            .then(|| Position {
                bar: f32::from_bits(self.meter.bar.load(Ordering::Relaxed)),
                tempo: f32::from_bits(self.meter.tempo.load(Ordering::Relaxed)),
            })
    }

    pub fn scope(&self) -> Vec<f32> {
        self.meter
            .scope
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn automated(&self) -> Vec<Arc<str>> {
        self.meter
            .automated
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.meter.level.load(Ordering::Relaxed))
    }
}

struct Player {
    track: Track,
    starts: Vec<(usize, Note, Vec<f32>)>,
    length: usize,
    next: usize,
    voices: Vec<Voice>,
    effects: Option<Boxed>,
    quiet: usize,
    key: Arc<AtomicU32>,
}

impl fmt::Debug for Player {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Player")
            .field("track", &self.track.name)
            .field("voices", &self.voices.len())
            .finish_non_exhaustive()
    }
}

impl Player {
    fn timed(track: &Track, bar: usize) -> (Vec<(usize, Note, Vec<f32>)>, usize) {
        let mut starts: Vec<(usize, Note, Vec<f32>)> = track
            .pattern
            .events
            .iter()
            .map(|event| {
                let note = Note {
                    pitch: 0.0,
                    velocity: event.velocity,
                    length: Some(samples(event.length * len_float(bar)).max(1)),
                };
                (
                    samples(event.start * len_float(bar)),
                    note,
                    event.pitches.clone(),
                )
            })
            .collect();
        starts.sort_by_key(|(start, ..)| *start);
        (starts, (track.pattern.bars * bar).max(1))
    }

    fn retime(&mut self, track: Track, bar: usize, local: usize) {
        let (starts, length) = Self::timed(&track, bar);
        self.track = track;
        self.starts = starts;
        self.length = length;
        self.next = self
            .starts
            .iter()
            .position(|(start, ..)| *start >= local % self.length)
            .unwrap_or(self.starts.len());
    }

    fn trigger(&mut self, global: usize, controls: &Controls, rate: f32, seed: &mut u64) {
        let local = global % self.length;
        if local == 0 {
            self.next = 0;
        }
        while let Some((start, note, pitches)) = self.starts.get(self.next) {
            if *start != local {
                break;
            }
            for pitch in pitches {
                *seed += 1;
                let note = Note {
                    pitch: notes::frequency(*pitch),
                    ..*note
                };
                self.voices
                    .push(Voice::new(&self.track.sound, note, controls, rate, *seed));
            }
            self.next += 1;
        }
    }

    fn next(&mut self, rate: f32) -> Frame {
        let mut sum = [0.0; 2];
        for voice in &mut self.voices {
            let [left, right] = voice.frame();
            sum = [sum[0] + left, sum[1] + right];
        }
        self.voices.retain(|voice| !voice.done());
        let output = match &mut self.effects {
            Some(effects) => effects.next(&Context {
                rate,
                pitch: 0.0,
                gate: true,
                velocity: 1.0,
                input: sum,
            }),
            None => sum,
        };
        if self.voices.is_empty() && output[0].abs().max(output[1].abs()) < 1e-4 {
            self.quiet += 1;
        } else {
            self.quiet = 0;
        }
        let output = output.map(|value| value * self.track.gain);
        self.key.store(
            output[0].abs().max(output[1].abs()).to_bits(),
            Ordering::Relaxed,
        );
        output
    }
}

#[derive(Debug)]
struct Sequence {
    song: Arc<Song>,
    bar: usize,
    first: usize,
    position: usize,
    players: Vec<Player>,
}

impl Sequence {
    fn bars(&self) -> f32 {
        len_float(self.first) + len_float(self.position) / len_float(self.bar)
    }

    fn global(&self) -> usize {
        self.first * self.bar + self.position
    }
}

pub struct Engine {
    rate: f32,
    controls: Arc<Controls>,
    meter: Arc<Meter>,
    commands: Option<Receiver<Command>>,
    voices: Vec<(u64, Voice)>,
    sequence: Option<Sequence>,
    pending: Option<Arc<Song>>,
    fading: Vec<Player>,
    automations: Vec<Automation>,
    seed: u64,
    count: usize,
    scope: Vec<f32>,
    peak: f32,
    dirty: bool,
}

impl fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Engine")
            .field("rate", &self.rate)
            .field("voices", &self.voices.len())
            .field("playing", &self.sequence.is_some())
            .finish_non_exhaustive()
    }
}

impl Engine {
    pub fn offline(rate: u32, controls: Arc<Controls>) -> Self {
        Self {
            rate: float(u64::from(rate)),
            controls,
            meter: Arc::new(Meter::default()),
            commands: None,
            voices: Vec::new(),
            sequence: None,
            pending: None,
            fading: Vec::new(),
            automations: Vec::new(),
            seed: 0,
            count: 0,
            scope: Vec::with_capacity(BLOCK),
            peak: 0.0,
            dirty: false,
        }
    }

    pub fn new(rate: u32) -> (Self, Handle) {
        let (sender, receiver) = channel();
        let controls = Arc::new(Controls::default());
        let mut engine = Self::offline(rate, Arc::clone(&controls));
        engine.commands = Some(receiver);
        let handle = Handle {
            commands: sender,
            controls,
            meter: Arc::clone(&engine.meter),
            next: AtomicU64::new(1),
        };
        (engine, handle)
    }

    pub fn controls(&self) -> &Arc<Controls> {
        &self.controls
    }

    pub fn busy(&self) -> bool {
        !self.voices.is_empty() || self.sequence.is_some() || !self.fading.is_empty()
    }

    fn bar_samples(&self, song: &Song) -> usize {
        samples(song.seconds_per_bar() * self.rate).max(1)
    }

    fn players(&mut self, song: &Song, bar: usize, global: usize) -> Vec<Player> {
        let mut old: Vec<Player> = self
            .sequence
            .take()
            .map(|sequence| sequence.players)
            .unwrap_or_default();
        let mut players = Vec::new();
        for track in &song.tracks {
            let kept = old.iter().position(|player| {
                player.track.name == track.name && player.track.effects == track.effects
            });
            let mut player = match kept {
                Some(at) => old.remove(at),
                None => Player {
                    track: track.clone(),
                    starts: Vec::new(),
                    length: 1,
                    next: 0,
                    voices: Vec::new(),
                    effects: track.effects.as_ref().map(|effects| {
                        self.seed += 1;
                        Compiler {
                            controls: &self.controls,
                            rate: self.rate,
                            rng: Rng::new(self.seed),
                        }
                        .compile(effects)
                    }),
                    quiet: 0,
                    key: self.controls.key(&track.name),
                },
            };
            player.retime(track.clone(), bar, global);
            players.push(player);
        }
        self.fading.extend(old);
        players
    }

    fn start(&mut self, song: Arc<Song>, first: usize) {
        let bar = self.bar_samples(&song);
        self.dirty = true;
        let players = self.players(&song, bar, first * bar);
        self.sequence = Some(Sequence {
            song,
            bar,
            first,
            position: 0,
            players,
        });
    }

    pub fn apply(&mut self, command: Command) {
        self.dirty = true;
        match command {
            Command::Play { id, sound, note } => {
                self.seed += 1;
                let voice = Voice::new(&sound, note, &self.controls, self.rate, self.seed);
                self.voices.push((id, voice));
            }
            Command::Release(id) => {
                for (_, voice) in self.voices.iter_mut().filter(|(known, _)| *known == id) {
                    voice.release();
                }
            }
            Command::Stop(id) => self.voices.retain(|(known, _)| *known != id),
            Command::StopAll => {
                self.voices.clear();
                self.sequence = None;
                self.pending = None;
                self.fading.clear();
            }
            Command::Loop(song) => {
                if self.sequence.is_some() {
                    self.pending = Some(song);
                } else {
                    self.start(song, 0);
                }
            }
            Command::StopSong => {
                if let Some(sequence) = self.sequence.take() {
                    self.fading.extend(sequence.players);
                }
                self.pending = None;
            }
            Command::Automate(automation) => {
                self.automations
                    .retain(|known| known.control != automation.control);
                self.automations.push(automation);
            }
            Command::Forget(control) => self.automations.retain(|known| known.control != control),
        }
    }

    fn automate(&self) {
        let Some(sequence) = &self.sequence else {
            return;
        };
        let bar = sequence.bars();
        let live = self.automations.iter();
        let written = sequence.song.automations.iter().filter(|automation| {
            !self
                .automations
                .iter()
                .any(|known| known.control == automation.control)
        });
        for automation in live.chain(written) {
            if let Some(value) = automation.at(bar) {
                self.controls.set(&automation.control, value);
            }
        }
    }

    fn block(&mut self) {
        if let Some(commands) = &self.commands {
            let waiting: Vec<Command> = commands.try_iter().collect();
            for command in waiting {
                self.apply(command);
            }
        }
        self.automate();
        if self.dirty
            && let Ok(mut automated) = self.meter.automated.try_lock()
        {
            let written = self
                .sequence
                .iter()
                .flat_map(|sequence| &sequence.song.automations);
            let mut names: Vec<Arc<str>> = self
                .automations
                .iter()
                .chain(written)
                .map(|automation| Arc::clone(&automation.control))
                .collect();
            names.sort();
            names.dedup();
            *automated = names;
            self.dirty = false;
        }
        if let Ok(mut scope) = self.meter.scope.try_lock() {
            scope.extend(self.scope.drain(..));
            let excess = scope.len().saturating_sub(SCOPE);
            scope.drain(..excess);
        }
        self.meter
            .level
            .store(self.peak.to_bits(), Ordering::Relaxed);
        match &self.sequence {
            Some(sequence) => {
                self.meter.playing.store(true, Ordering::Relaxed);
                self.meter
                    .bar
                    .store(sequence.bars().to_bits(), Ordering::Relaxed);
                self.meter
                    .tempo
                    .store(sequence.song.tempo.to_bits(), Ordering::Relaxed);
            }
            None => self.meter.playing.store(false, Ordering::Relaxed),
        }
    }

    pub fn next_frame(&mut self) -> Frame {
        if self.count.is_multiple_of(BLOCK) {
            self.block();
        }
        self.count += 1;
        let mut output = [0.0; 2];
        let mut add = |frame: Frame| {
            output = [output[0] + frame[0], output[1] + frame[1]];
        };
        for (_, voice) in &mut self.voices {
            add(voice.frame());
        }
        self.voices.retain(|(_, voice)| !voice.done());
        if let Some(sequence) = &self.sequence
            && sequence.position.is_multiple_of(sequence.bar)
            && sequence.position > 0
            && let Some(song) = self.pending.take()
        {
            let first = sequence.first + sequence.position / sequence.bar;
            self.start(song, first);
        }
        let rate = self.rate;
        if let Some(sequence) = &mut self.sequence {
            let global = sequence.global();
            for player in &mut sequence.players {
                player.trigger(global, &self.controls, rate, &mut self.seed);
                add(player.next(rate));
            }
            sequence.position += 1;
        }
        let tail = samples(TAIL_SECONDS * rate);
        for player in &mut self.fading {
            add(player.next(rate));
        }
        self.fading.retain(|player| player.quiet < tail);
        let output = output.map(f32::tanh);
        let middle = f32::midpoint(output[0], output[1]);
        self.peak = (self.peak * 0.9995).max(middle.abs());
        self.scope.push(middle);
        output
    }

    pub fn render(&mut self, frames: usize) -> Vec<f32> {
        let mut rendered = Vec::with_capacity(frames * 2);
        for _ in 0..frames {
            rendered.extend(self.next_frame());
        }
        rendered
    }
}
