use std::sync::Arc;

use crate::graph::Sound;
use crate::{len_float, notes};

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub start: f32,
    pub length: f32,
    pub pitches: Vec<f32>,
    pub velocity: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub events: Vec<Event>,
    pub bars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Notes,
    Chords { octave: i32 },
    Beats { note: f32 },
}

impl Kind {
    fn read(self, token: &str, velocity: f32) -> Option<(Vec<f32>, f32)> {
        match self {
            Self::Notes => Some((vec![notes::midi(token)?], velocity)),
            Self::Chords { octave } => Some((notes::chord(token, octave)?, velocity)),
            Self::Beats { note } => match token {
                "x" => Some((vec![note], velocity)),
                "X" => Some((vec![note], 1.0)),
                _ => None,
            },
        }
    }

    fn wanted(self) -> &'static str {
        match self {
            Self::Notes => "notes such as a4, c#5 or bb3",
            Self::Chords { .. } => "chords such as Dm9, Bbmaj7, F or D/F#",
            Self::Beats { .. } => "x for a hit and X for an accent",
        }
    }
}

pub fn pattern(text: &str, kind: Kind, velocity: f32) -> Result<Pattern, String> {
    let mut bars: Vec<&str> = text.split('|').collect();
    if bars.len() > 1 && bars.last().is_some_and(|last| last.trim().is_empty()) {
        bars.pop();
    }
    let mut events: Vec<Event> = Vec::new();
    for (index, bar) in bars.iter().enumerate() {
        let slots: Vec<&str> = bar.split_whitespace().collect();
        if slots.is_empty() {
            continue;
        }
        let width = 1.0 / len_float(slots.len());
        for (at, slot) in slots.iter().enumerate() {
            let start = len_float(index) + len_float(at) * width;
            match *slot {
                "." => {}
                "_" => {
                    if let Some(last) = events.last_mut()
                        && (last.start + last.length - start).abs() < 1e-4
                    {
                        last.length += width;
                    }
                }
                token => {
                    let (pitches, loudness) = kind.read(token, velocity).ok_or_else(|| {
                        format!(
                            "can't read {token:?} in bar {}; write {}, _ to hold and . for silence",
                            index + 1,
                            kind.wanted()
                        )
                    })?;
                    events.push(Event {
                        start,
                        length: width,
                        pitches,
                        velocity: loudness,
                    });
                }
            }
        }
    }
    Ok(Pattern {
        events,
        bars: bars.len().max(1),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub name: Arc<str>,
    pub sound: Sound,
    pub pattern: Arc<Pattern>,
    pub effects: Option<Sound>,
    pub gain: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Automation {
    pub control: Arc<str>,
    pub points: Vec<(f32, f32)>,
}

impl Automation {
    pub fn at(&self, bar: f32) -> Option<f32> {
        let (first, last) = (self.points.first()?, self.points.last()?);
        if bar <= first.0 {
            return Some(first.1);
        }
        if bar >= last.0 {
            return Some(last.1);
        }
        self.points.windows(2).find_map(|pair| {
            let ((from, low), (to, high)) = (pair[0], pair[1]);
            (bar >= from && bar <= to).then(|| {
                let span = to - from;
                if span <= 0.0 {
                    high
                } else {
                    low + (high - low) * (bar - from) / span
                }
            })
        })
    }

    pub fn end(&self) -> f32 {
        self.points.last().map_or(0.0, |(bar, _)| *bar)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Song {
    pub tempo: f32,
    pub meter: f32,
    pub tracks: Vec<Track>,
    pub bars: Option<usize>,
    pub automations: Vec<Automation>,
}

impl Song {
    pub fn seconds_per_bar(&self) -> f32 {
        60.0 / self.tempo * self.meter
    }

    pub fn length(&self) -> f32 {
        if let Some(bars) = self.bars {
            return len_float(bars);
        }
        let tracks = self
            .tracks
            .iter()
            .map(|track| len_float(track.pattern.bars))
            .fold(1.0, f32::max);
        self.automations
            .iter()
            .map(|automation| automation.end().ceil())
            .fold(tracks, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::{Automation, Kind, pattern};

    #[test]
    fn slots_split_each_bar_evenly_and_hold_across_bars() {
        let found = pattern("a4 . c5 e5 | _ _ . d5", Kind::Notes, 0.8).unwrap();

        assert_eq!(found.bars, 2);
        let shape: Vec<(f32, f32, f32)> = found
            .events
            .iter()
            .map(|event| (event.start, event.length, event.pitches[0]))
            .collect();
        assert_eq!(
            shape,
            [
                (0.0, 0.25, 69.0),
                (0.5, 0.25, 72.0),
                (0.75, 0.75, 76.0),
                (1.75, 0.25, 74.0),
            ]
        );
    }

    #[test]
    fn chords_beats_and_mistakes() {
        let chords = pattern("Dm9 _ | F", Kind::Chords { octave: 3 }, 0.7).unwrap();
        assert_eq!(chords.events.len(), 2);
        assert_eq!(chords.events[0].length, 1.0);
        assert_eq!(chords.events[1].pitches, [53.0, 57.0, 60.0]);

        let beats = pattern("X . x . |", Kind::Beats { note: 36.0 }, 0.6).unwrap();
        assert_eq!(beats.bars, 1);
        let loudness: Vec<f32> = beats.events.iter().map(|event| event.velocity).collect();
        assert_eq!(loudness, [1.0, 0.6]);

        let wrong = pattern("a4 | h9", Kind::Notes, 0.8).unwrap_err();
        assert_eq!(
            wrong,
            "can't read \"h9\" in bar 2; write notes such as a4, c#5 or bb3, _ to hold and . for silence"
        );
        assert_eq!(pattern(" | ", Kind::Notes, 0.8).unwrap().bars, 1);
    }

    #[test]
    fn automations_ease_between_their_points_and_hold_at_the_ends() {
        let swell = Automation {
            control: "swell".into(),
            points: vec![(2.0, 0.0), (6.0, 1.0)],
        };

        assert_eq!(swell.at(0.0), Some(0.0));
        assert_eq!(swell.at(4.0), Some(0.5));
        assert_eq!(swell.at(9.0), Some(1.0));
    }
}
