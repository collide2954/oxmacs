use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use rodio::mixer::{Mixer, MixerSource};
use rodio::source::Speed;
use rodio::{ChannelCount, Decoder, DeviceSinkBuilder, MixerDeviceSink, SampleRate, Source};

const CHUNK_FRAMES: usize = 512;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("can't open the sound device: {0}")]
    Device(#[from] rodio::DeviceSinkError),
    #[error("can't read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("can't decode {}: {source}", path.display())]
    Decode {
        path: PathBuf,
        source: rodio::decoder::DecoderError,
    },
    #[error("a sound needs at least one channel and a sample rate above zero")]
    Format,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    channels: ChannelCount,
    rate: SampleRate,
    samples: Arc<[f32]>,
}

struct Samples {
    samples: Arc<[f32]>,
    at: usize,
    channels: ChannelCount,
    rate: SampleRate,
    looping: bool,
    duration: Duration,
}

impl Iterator for Samples {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.at >= self.samples.len() {
            if !self.looping || self.samples.is_empty() {
                return None;
            }
            self.at = 0;
        }
        let sample = self.samples[self.at];
        self.at += 1;
        Some(sample)
    }
}

impl Source for Samples {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        (!self.looping).then_some(self.duration)
    }
}

#[derive(Debug)]
struct Controls {
    volume: AtomicU32,
    pitch: AtomicU32,
    paused: AtomicBool,
    stopped: AtomicBool,
    done: AtomicBool,
}

impl Controls {
    fn new(pitch: f32) -> Self {
        Self {
            volume: AtomicU32::new(1.0_f32.to_bits()),
            pitch: AtomicU32::new(pitch.to_bits()),
            paused: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            done: AtomicBool::new(false),
        }
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    fn pitch(&self) -> f32 {
        f32::from_bits(self.pitch.load(Ordering::Relaxed))
    }
}

struct Controlled<S> {
    inner: Speed<S>,
    controls: Arc<Controls>,
    emitted: usize,
    chunk: usize,
    holding: bool,
}

impl<S: Source> Controlled<S> {
    fn new(inner: S, controls: Arc<Controls>) -> Self {
        let chunk = CHUNK_FRAMES * usize::from(inner.channels().get());
        Self {
            inner: inner.speed(controls.pitch()),
            controls,
            emitted: 0,
            chunk,
            holding: false,
        }
    }

    fn finish(&self) -> Option<f32> {
        self.controls.done.store(true, Ordering::Relaxed);
        None
    }
}

impl<S: Source> Iterator for Controlled<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.controls.stopped.load(Ordering::Relaxed) {
            return self.finish();
        }
        let channels = usize::from(self.inner.channels().get());
        if self.emitted % channels == 0 {
            self.holding = self.controls.paused.load(Ordering::Relaxed);
        }
        let sample = if self.holding {
            0.0
        } else {
            match self.inner.next() {
                Some(sample) => sample * self.controls.volume(),
                None => return self.finish(),
            }
        };
        self.emitted += 1;
        if self.emitted % self.chunk == 0 {
            self.inner.set_factor(self.controls.pitch());
        }
        Some(sample)
    }
}

impl<S: Source> Source for Controlled<S> {
    fn current_span_len(&self) -> Option<usize> {
        let left = self.chunk - self.emitted % self.chunk;
        Some(
            self.inner
                .current_span_len()
                .map_or(left, |inner| inner.min(left)),
        )
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

fn duration_of(frames: usize, rate: SampleRate) -> Duration {
    let nanoseconds =
        u128::try_from(frames).unwrap_or(u128::MAX) * 1_000_000_000 / u128::from(rate.get());
    Duration::from_nanos(u64::try_from(nanoseconds).unwrap_or(u64::MAX))
}

fn file(path: &Path) -> Result<(BufReader<File>, u64), Error> {
    let failed = |source| Error::Read {
        path: path.to_owned(),
        source,
    };
    let file = File::open(path).map_err(failed)?;
    let length = file.metadata().map_err(failed)?.len();
    Ok((BufReader::new(file), length))
}

fn hint(path: &Path) -> &str {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
}

fn undecodable(path: &Path) -> impl FnOnce(rodio::decoder::DecoderError) -> Error + '_ {
    move |source| Error::Decode {
        path: path.to_owned(),
        source,
    }
}

impl Sound {
    pub fn from_samples(channels: u16, rate: u32, samples: Vec<f32>) -> Result<Self, Error> {
        Ok(Self {
            channels: NonZero::new(channels).ok_or(Error::Format)?,
            rate: NonZero::new(rate).ok_or(Error::Format)?,
            samples: samples.into(),
        })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let (data, length) = file(path)?;
        let decoder = Decoder::builder()
            .with_data(data)
            .with_byte_len(length)
            .with_hint(hint(path))
            .build()
            .map_err(undecodable(path))?;
        let channels = decoder.channels();
        let rate = decoder.sample_rate();
        Ok(Self {
            channels,
            rate,
            samples: decoder.collect::<Vec<f32>>().into(),
        })
    }

    pub fn channels(&self) -> u16 {
        self.channels.get()
    }

    pub fn rate(&self) -> u32 {
        self.rate.get()
    }

    pub fn duration(&self) -> Duration {
        duration_of(
            self.samples.len() / usize::from(self.channels.get()),
            self.rate,
        )
    }

    fn source(&self, looping: bool) -> Samples {
        Samples {
            samples: Arc::clone(&self.samples),
            at: 0,
            channels: self.channels,
            rate: self.rate,
            looping,
            duration: self.duration(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Voice(u64);

impl Voice {
    pub fn id(self) -> u64 {
        self.0
    }

    pub fn from_id(id: u64) -> Self {
        Self(id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bus {
    Music,
    Effects,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayOptions {
    pub volume: f32,
    pub pitch: f32,
    pub looping: bool,
}

impl Default for PlayOptions {
    fn default() -> Self {
        Self {
            volume: 1.0,
            pitch: 1.0,
            looping: false,
        }
    }
}

struct Fade {
    from: f32,
    to: f32,
    elapsed: Duration,
    length: Duration,
    stop: bool,
}

struct Playing {
    controls: Arc<Controls>,
    bus: Bus,
    volume: f32,
    gain: f32,
    fade: Option<Fade>,
    left: Option<Duration>,
}

#[expect(
    dead_code,
    reason = "the device sink plays only while it is kept alive"
)]
enum Output {
    Device(MixerDeviceSink),
    Captured,
    Detached,
}

pub struct Capture {
    output: MixerSource,
}

impl fmt::Debug for Capture {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Capture").finish_non_exhaustive()
    }
}

impl Capture {
    pub fn take(&mut self, count: usize) -> Vec<f32> {
        (0..count)
            .map(|_| self.output.next().unwrap_or(0.0))
            .collect()
    }
}

pub struct Audio {
    output: Output,
    mixer: Mixer,
    master: f32,
    music_volume: f32,
    effects_volume: f32,
    voices: BTreeMap<Voice, Playing>,
    next: u64,
    music: Option<Voice>,
}

impl fmt::Debug for Audio {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Audio")
            .field("master", &self.master)
            .field("music", &self.music_volume)
            .field("effects", &self.effects_volume)
            .field("voices", &self.voices.len())
            .finish_non_exhaustive()
    }
}

const STEREO: ChannelCount = NonZero::<u16>::MIN.saturating_add(1);
const CD_RATE: SampleRate = NonZero::<u32>::MIN.saturating_add(44_099);

impl Audio {
    fn with(output: Output, mixer: Mixer) -> Self {
        Self {
            output,
            mixer,
            master: 1.0,
            music_volume: 1.0,
            effects_volume: 1.0,
            voices: BTreeMap::new(),
            next: 0,
            music: None,
        }
    }

    pub fn open() -> Result<Self, Error> {
        let mut sink = DeviceSinkBuilder::open_default_sink()?;
        sink.log_on_drop(false);
        let mixer = sink.mixer().clone();
        Ok(Self::with(Output::Device(sink), mixer))
    }

    pub fn silent() -> Self {
        let (mixer, _) = rodio::mixer::mixer(STEREO, CD_RATE);
        Self::with(Output::Detached, mixer)
    }

    pub fn capture(channels: u16, rate: u32) -> (Self, Capture) {
        let (mixer, output) = rodio::mixer::mixer(
            NonZero::new(channels).unwrap_or(STEREO),
            NonZero::new(rate).unwrap_or(CD_RATE),
        );
        (Self::with(Output::Captured, mixer), Capture { output })
    }

    fn bus_volume(&self, bus: Bus) -> f32 {
        match bus {
            Bus::Music => self.music_volume,
            Bus::Effects => self.effects_volume,
        }
    }

    fn apply(&self, playing: &Playing) {
        let volume = playing.volume * playing.gain * self.bus_volume(playing.bus) * self.master;
        playing
            .controls
            .volume
            .store(volume.to_bits(), Ordering::Relaxed);
    }

    fn apply_all(&self) {
        for playing in self.voices.values() {
            self.apply(playing);
        }
    }

    fn start(
        &mut self,
        bus: Bus,
        options: PlayOptions,
        length: Option<Duration>,
        fade: Option<Fade>,
        feed: impl FnOnce(&Mixer, Arc<Controls>),
    ) -> Voice {
        let pitch = if options.pitch > 0.0 {
            options.pitch
        } else {
            1.0
        };
        let playing = Playing {
            controls: Arc::new(Controls::new(pitch)),
            bus,
            volume: options.volume,
            gain: fade.as_ref().map_or(1.0, |fade| fade.from),
            fade,
            left: match self.output {
                Output::Detached if !options.looping => length.map(|length| length.div_f32(pitch)),
                _ => None,
            },
        };
        self.apply(&playing);
        feed(&self.mixer, Arc::clone(&playing.controls));
        let voice = Voice(self.next);
        self.next += 1;
        self.voices.insert(voice, playing);
        voice
    }

    pub fn play(&mut self, sound: &Sound, options: PlayOptions) -> Voice {
        let source = sound.source(options.looping);
        self.start(
            Bus::Effects,
            options,
            Some(sound.duration()),
            None,
            move |mixer, controls| mixer.add(Controlled::new(source, controls)),
        )
    }

    pub fn play_music(
        &mut self,
        path: impl AsRef<Path>,
        options: PlayOptions,
        fade: Duration,
    ) -> Result<Voice, Error> {
        let path = path.as_ref();
        let (data, length) = file(path)?;
        let builder = Decoder::builder()
            .with_data(data)
            .with_byte_len(length)
            .with_hint(hint(path))
            .with_seekable(true);
        let fading = (!fade.is_zero()).then_some(Fade {
            from: 0.0,
            to: 1.0,
            elapsed: Duration::ZERO,
            length: fade,
            stop: false,
        });
        let voice = if options.looping {
            let decoder = builder.build_looped().map_err(undecodable(path))?;
            self.start(Bus::Music, options, None, fading, move |mixer, controls| {
                mixer.add(Controlled::new(decoder, controls));
            })
        } else {
            let decoder = builder.build().map_err(undecodable(path))?;
            let total = decoder.total_duration();
            self.start(
                Bus::Music,
                options,
                total,
                fading,
                move |mixer, controls| {
                    mixer.add(Controlled::new(decoder, controls));
                },
            )
        };
        self.stop_music(fade);
        self.music = Some(voice);
        Ok(voice)
    }

    pub fn stop_music(&mut self, fade: Duration) {
        if let Some(voice) = self.music.take() {
            self.fade_out(voice, fade);
        }
    }

    pub fn fade_out(&mut self, voice: Voice, fade: Duration) {
        if fade.is_zero() {
            self.stop(voice);
        } else if let Some(playing) = self.voices.get_mut(&voice) {
            playing.fade = Some(Fade {
                from: playing.gain,
                to: 0.0,
                elapsed: Duration::ZERO,
                length: fade,
                stop: true,
            });
        }
    }

    pub fn music(&self) -> Option<Voice> {
        self.music.filter(|voice| self.voices.contains_key(voice))
    }

    pub fn stop(&mut self, voice: Voice) {
        if let Some(playing) = self.voices.remove(&voice) {
            playing.controls.stopped.store(true, Ordering::Relaxed);
        }
        if self.music == Some(voice) {
            self.music = None;
        }
    }

    pub fn stop_all(&mut self) {
        let voices: Vec<Voice> = self.voices.keys().copied().collect();
        for voice in voices {
            self.stop(voice);
        }
    }

    pub fn playing(&self, voice: Voice) -> bool {
        self.voices
            .get(&voice)
            .is_some_and(|playing| !playing.controls.done.load(Ordering::Relaxed))
    }

    fn controls(&self, voice: Voice) -> Option<&Controls> {
        self.voices.get(&voice).map(|playing| &*playing.controls)
    }

    pub fn paused(&self, voice: Voice) -> bool {
        self.controls(voice)
            .is_some_and(|controls| controls.paused.load(Ordering::Relaxed))
    }

    pub fn pause(&mut self, voice: Voice) {
        if let Some(controls) = self.controls(voice) {
            controls.paused.store(true, Ordering::Relaxed);
        }
    }

    pub fn resume(&mut self, voice: Voice) {
        if let Some(controls) = self.controls(voice) {
            controls.paused.store(false, Ordering::Relaxed);
        }
    }

    pub fn pause_all(&mut self) {
        for playing in self.voices.values() {
            playing.controls.paused.store(true, Ordering::Relaxed);
        }
    }

    pub fn resume_all(&mut self) {
        for playing in self.voices.values() {
            playing.controls.paused.store(false, Ordering::Relaxed);
        }
    }

    pub fn set_volume(&mut self, voice: Voice, volume: f32) {
        if let Some(playing) = self.voices.get_mut(&voice) {
            playing.volume = volume.max(0.0);
        }
        if let Some(playing) = self.voices.get(&voice) {
            self.apply(playing);
        }
    }

    pub fn set_pitch(&mut self, voice: Voice, pitch: f32) {
        if let Some(controls) = self.controls(voice)
            && pitch > 0.0
        {
            controls.pitch.store(pitch.to_bits(), Ordering::Relaxed);
        }
    }

    pub fn master(&self) -> f32 {
        self.master
    }

    pub fn set_master(&mut self, volume: f32) {
        self.master = volume.max(0.0);
        self.apply_all();
    }

    pub fn bus(&self, bus: Bus) -> f32 {
        self.bus_volume(bus)
    }

    pub fn set_bus(&mut self, bus: Bus, volume: f32) {
        let volume = volume.max(0.0);
        match bus {
            Bus::Music => self.music_volume = volume,
            Bus::Effects => self.effects_volume = volume,
        }
        self.apply_all();
    }

    pub fn voices(&self) -> usize {
        self.voices.len()
    }

    pub fn update(&mut self, elapsed: Duration) {
        let mut finished = Vec::new();
        for (voice, playing) in &mut self.voices {
            if let Some(fade) = &mut playing.fade {
                fade.elapsed += elapsed;
                let progress = if fade.length.is_zero() {
                    1.0
                } else {
                    (fade.elapsed.as_secs_f32() / fade.length.as_secs_f32()).min(1.0)
                };
                playing.gain = (fade.to - fade.from).mul_add(progress, fade.from);
                if progress >= 1.0 {
                    if fade.stop {
                        finished.push(*voice);
                    }
                    playing.fade = None;
                }
            }
            if let Some(left) = &mut playing.left {
                *left = left.saturating_sub(elapsed);
                if left.is_zero() {
                    finished.push(*voice);
                }
            }
            if playing.controls.done.load(Ordering::Relaxed) {
                finished.push(*voice);
            }
        }
        for voice in finished {
            self.stop(voice);
        }
        self.apply_all();
    }
}
