use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use rodio::{Decoder, Source};

use crate::controls::Controls;
use crate::graph::Sound;
use crate::units::{Compiler, Context, Frame, Rng};
use crate::{float, len_float, samples};

#[derive(Clone)]
pub struct Sample {
    pub rate: u32,
    pub frames: Arc<[Frame]>,
}

impl fmt::Debug for Sample {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Sample")
            .field("rate", &self.rate)
            .field("frames", &self.frames.len())
            .finish()
    }
}

impl PartialEq for Sample {
    fn eq(&self, other: &Self) -> bool {
        self.rate == other.rate && Arc::ptr_eq(&self.frames, &other.frames)
    }
}

impl Sample {
    pub fn new(rate: u32, frames: Vec<Frame>) -> Self {
        Self {
            rate: rate.max(1),
            frames: frames.into(),
        }
    }

    pub fn from_interleaved(rate: u32, channels: usize, data: &[f32]) -> Self {
        let channels = channels.max(1);
        let frames = data
            .chunks(channels)
            .map(|chunk| match chunk {
                [only] => [*only; 2],
                [left, right, ..] => [*left, *right],
                [] => [0.0; 2],
            })
            .collect();
        Self::new(rate, frames)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let shown = path.display();
        let file = File::open(path).map_err(|error| format!("can't read {shown}: {error}"))?;
        let length = file.metadata().map_or(0, |data| data.len());
        let mut builder = Decoder::builder()
            .with_data(BufReader::new(file))
            .with_byte_len(length);
        if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
            builder = builder.with_hint(extension);
        }
        let decoder = builder
            .build()
            .map_err(|error| format!("can't decode {shown}: {error}"))?;
        let channels = usize::from(decoder.channels().get());
        let rate = decoder.sample_rate().get();
        let data: Vec<f32> = decoder.collect();
        Ok(Self::from_interleaved(rate, channels, &data))
    }

    pub fn seconds(&self) -> f32 {
        len_float(self.frames.len()) / float(u64::from(self.rate))
    }

    pub fn interleaved(&self) -> Vec<f32> {
        self.frames.iter().flatten().copied().collect()
    }

    pub fn at(&self, position: f32) -> Frame {
        if position < 0.0 {
            return [0.0; 2];
        }
        let whole = samples(position.floor());
        let fraction = position - position.floor();
        let Some(first) = self.frames.get(whole) else {
            return [0.0; 2];
        };
        let second = self.frames.get(whole + 1).unwrap_or(&[0.0; 2]);
        [
            first[0] + (second[0] - first[0]) * fraction,
            first[1] + (second[1] - first[1]) * fraction,
        ]
    }

    fn frame_at(&self, seconds: f32) -> usize {
        samples(seconds * float(u64::from(self.rate))).min(self.frames.len())
    }

    #[must_use]
    pub fn slice(&self, from: f32, to: Option<f32>) -> Self {
        let start = self.frame_at(from);
        let end = to
            .map_or(self.frames.len(), |to| self.frame_at(to))
            .max(start);
        Self::new(self.rate, self.frames[start..end].to_vec())
    }

    #[must_use]
    pub fn reverse(&self) -> Self {
        Self::new(self.rate, self.frames.iter().rev().copied().collect())
    }

    #[must_use]
    pub fn scaled(&self, amount: f32) -> Self {
        Self::new(
            self.rate,
            self.frames
                .iter()
                .map(|frame| frame.map(|value| value * amount))
                .collect(),
        )
    }

    pub fn peak(&self) -> f32 {
        self.frames
            .iter()
            .flatten()
            .fold(0.0, |peak: f32, value| peak.max(value.abs()))
    }

    pub fn rms(&self) -> f32 {
        if self.frames.is_empty() {
            return 0.0;
        }
        let sum: f32 = self
            .frames
            .iter()
            .flatten()
            .map(|value| value * value)
            .sum();
        (sum / len_float(self.frames.len() * 2)).sqrt()
    }

    #[must_use]
    pub fn normalized(&self, peak: f32) -> Self {
        let loudest = self.peak();
        if loudest <= 0.0 {
            return self.clone();
        }
        self.scaled(peak / loudest)
    }

    #[must_use]
    pub fn faded(&self, fade_in: f32, fade_out: f32) -> Self {
        let rate = float(u64::from(self.rate));
        let rising = samples(fade_in * rate).max(1);
        let falling = samples(fade_out * rate).max(1);
        let count = self.frames.len();
        Self::new(
            self.rate,
            self.frames
                .iter()
                .enumerate()
                .map(|(at, frame)| {
                    let up = (len_float(at) / len_float(rising)).min(1.0);
                    let down = (len_float(count - at) / len_float(falling)).min(1.0);
                    frame.map(|value| value * up * down)
                })
                .collect(),
        )
    }

    #[must_use]
    pub fn resampled(&self, ratio: f32) -> Self {
        let ratio = ratio.max(1e-3);
        let count = samples(len_float(self.frames.len()) / ratio);
        Self::new(
            self.rate,
            (0..count)
                .map(|at| self.at(len_float(at) * ratio))
                .collect(),
        )
    }

    #[must_use]
    pub fn at_rate(&self, rate: u32) -> Self {
        if rate == self.rate {
            return self.clone();
        }
        let ratio = float(u64::from(self.rate)) / float(u64::from(rate));
        Self {
            rate,
            ..self.resampled(ratio)
        }
    }

    pub fn joined(parts: &[Self]) -> Self {
        let rate = parts.first().map_or(crate::RATE, |first| first.rate);
        let frames = parts
            .iter()
            .flat_map(|part| part.at_rate(rate).frames.to_vec())
            .collect();
        Self::new(rate, frames)
    }

    pub fn layered(parts: &[Self]) -> Self {
        let rate = parts.first().map_or(crate::RATE, |first| first.rate);
        let parts: Vec<Self> = parts.iter().map(|part| part.at_rate(rate)).collect();
        let longest = parts
            .iter()
            .map(|part| part.frames.len())
            .max()
            .unwrap_or(0);
        let frames = (0..longest)
            .map(|at| {
                parts.iter().fold([0.0; 2], |sum, part| {
                    let frame = part.frames.get(at).unwrap_or(&[0.0; 2]);
                    [sum[0] + frame[0], sum[1] + frame[1]]
                })
            })
            .collect();
        Self::new(rate, frames)
    }

    #[must_use]
    pub fn processed(&self, effects: &Sound, tail: f32) -> Self {
        let controls = Controls::default();
        let rate = float(u64::from(self.rate));
        let mut unit = Compiler {
            controls: &controls,
            rate,
            rng: Rng::new(7),
        }
        .compile(effects);
        let mut context = Context {
            rate,
            pitch: 0.0,
            gate: true,
            velocity: 1.0,
            input: [0.0; 2],
        };
        let longest = self.frames.len() + samples(tail * rate);
        let mut frames = Vec::with_capacity(longest);
        for at in 0..longest {
            context.input = self.frames.get(at).copied().unwrap_or([0.0; 2]);
            frames.push(unit.next(&context));
        }
        Self::new(self.rate, frames)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::Sample;
    use crate::graph::{Node, constant};

    fn ramp() -> Sample {
        Sample::new(
            10,
            (0..10)
                .map(|at| [f32::from(u8::try_from(at).unwrap()) / 10.0; 2])
                .collect(),
        )
    }

    #[test]
    fn samples_slice_reverse_and_fade() {
        let ramp = ramp();

        assert!((ramp.seconds() - 1.0).abs() < 1e-6);
        assert_eq!(ramp.slice(0.2, Some(0.5)).frames.len(), 3);
        assert_eq!(ramp.slice(0.8, None).frames[0], [0.8; 2]);
        assert_eq!(ramp.reverse().frames[0], [0.9; 2]);
        assert_eq!(ramp.faded(0.0, 1.0).frames[9][0], 0.9 * 0.1);
        assert_eq!(ramp.normalized(0.45).peak(), 0.45);
        assert!((ramp.at(2.5)[0] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn samples_resample_join_layer_and_process() {
        let ramp = ramp();

        assert_eq!(ramp.resampled(2.0).frames.len(), 5);
        assert_eq!(ramp.at_rate(20).frames.len(), 20);
        assert_eq!(
            Sample::joined(&[ramp.clone(), ramp.clone()]).frames.len(),
            20
        );
        assert_eq!(
            Sample::layered(&[ramp.clone(), ramp.clone()]).frames[5],
            [1.0; 2]
        );
        let halved = ramp.processed(
            &Arc::new(Node::Product(vec![Arc::new(Node::Input), constant(0.5)])),
            0.5,
        );
        assert_eq!(halved.frames.len(), 15);
        assert_eq!(halved.frames[4], [0.2; 2]);
        assert_eq!(
            Sample::from_interleaved(10, 1, &[0.5, 0.25]).frames[1],
            [0.25; 2]
        );
    }
}
