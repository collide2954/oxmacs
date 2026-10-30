const LETTERS: [(char, i32); 7] = [
    ('c', 0),
    ('d', 2),
    ('e', 4),
    ('f', 5),
    ('g', 7),
    ('a', 9),
    ('b', 11),
];

const QUALITIES: [(&str, &[i32]); 22] = [
    ("", &[0, 4, 7]),
    ("m", &[0, 3, 7]),
    ("5", &[0, 7]),
    ("6", &[0, 4, 7, 9]),
    ("m6", &[0, 3, 7, 9]),
    ("7", &[0, 4, 7, 10]),
    ("maj7", &[0, 4, 7, 11]),
    ("m7", &[0, 3, 7, 10]),
    ("mmaj7", &[0, 3, 7, 11]),
    ("9", &[0, 4, 7, 10, 14]),
    ("maj9", &[0, 4, 7, 11, 14]),
    ("m9", &[0, 3, 7, 10, 14]),
    ("add9", &[0, 4, 7, 14]),
    ("madd9", &[0, 3, 7, 14]),
    ("11", &[0, 4, 7, 10, 14, 17]),
    ("m11", &[0, 3, 7, 10, 14, 17]),
    ("13", &[0, 4, 7, 10, 14, 21]),
    ("maj13", &[0, 4, 7, 11, 14, 21]),
    ("sus2", &[0, 2, 7]),
    ("sus4", &[0, 5, 7]),
    ("dim", &[0, 3, 6]),
    ("aug", &[0, 4, 8]),
];

fn root(text: &str) -> Option<(i32, &str)> {
    let mut characters = text.chars();
    let letter = characters.next()?.to_ascii_lowercase();
    let (_, step) = LETTERS.iter().find(|(known, _)| *known == letter)?;
    let rest = characters.as_str();
    let (shift, rest) = match rest.chars().next() {
        Some('#') => (1, &rest[1..]),
        Some('b') => (-1, &rest[1..]),
        _ => (0, rest),
    };
    Some((step + shift, rest))
}

pub fn midi(name: &str) -> Option<f32> {
    let (step, octave) = root(name)?;
    let octave: i32 = octave.parse().ok()?;
    Some(f32::from(i16::try_from(12 * (octave + 1) + step).ok()?))
}

pub fn frequency(midi: f32) -> f32 {
    440.0 * ((midi - 69.0) / 12.0).exp2()
}

pub fn chord(name: &str, octave: i32) -> Option<Vec<f32>> {
    let (chord, bass) = match name.split_once('/') {
        Some((chord, bass)) => (chord, Some(bass)),
        None => (name, None),
    };
    let (step, quality) = root(chord)?;
    let (_, intervals) = QUALITIES.iter().find(|(known, _)| *known == quality)?;
    let lowest = 12 * (octave + 1) + step;
    let mut notes: Vec<f32> = intervals
        .iter()
        .map(|interval| f32::from(i16::try_from(lowest + interval).unwrap_or(0)))
        .collect();
    if let Some(bass) = bass {
        let (low, rest) = root(bass)?;
        if !rest.is_empty() {
            return None;
        }
        let mut under = 12 * octave + low;
        while under >= lowest {
            under -= 12;
        }
        notes.insert(0, f32::from(i16::try_from(under).ok()?));
    }
    Some(notes)
}

#[cfg(test)]
mod tests {
    use super::{chord, frequency, midi};

    #[test]
    fn note_names_become_midi_numbers_and_frequencies() {
        assert_eq!(midi("a4"), Some(69.0));
        assert_eq!(midi("C4"), Some(60.0));
        assert_eq!(midi("c#5"), Some(73.0));
        assert_eq!(midi("bb3"), Some(58.0));
        assert_eq!(midi("h4"), None);
        assert_eq!(midi("a"), None);
        assert!((frequency(69.0) - 440.0).abs() < 1e-3);
        assert!((frequency(57.0) - 220.0).abs() < 1e-3);
    }

    #[test]
    fn chord_names_become_their_notes() {
        assert_eq!(chord("Dm9", 3), Some(vec![50.0, 53.0, 57.0, 60.0, 64.0]));
        assert_eq!(chord("Bbmaj7", 3), Some(vec![58.0, 62.0, 65.0, 69.0]));
        assert_eq!(chord("F", 3), Some(vec![53.0, 57.0, 60.0]));
        assert_eq!(chord("D/F#", 3), Some(vec![42.0, 50.0, 54.0, 57.0]));
        assert_eq!(chord("Xm", 3), None);
        assert_eq!(chord("Cweird", 3), None);
    }
}
