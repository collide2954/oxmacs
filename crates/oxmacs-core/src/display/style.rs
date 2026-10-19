use std::ops::Range;

use crate::theme::Highlight;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineStyle {
    pub background: Option<Highlight>,
    pub spans: Vec<(Range<usize>, Highlight)>,
}

pub fn runs(faces: &[Option<Highlight>]) -> Vec<(Range<usize>, Highlight)> {
    let mut runs: Vec<(Range<usize>, Highlight)> = Vec::new();
    for (index, face) in faces.iter().enumerate() {
        let Some(face) = face else {
            continue;
        };
        match runs.last_mut() {
            Some((range, last)) if range.end == index && last == face => range.end = index + 1,
            _ => runs.push((index..index + 1, *face)),
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbouring_columns_with_the_same_face_become_one_span() {
        let faces = [
            Some(Highlight::Keyword),
            Some(Highlight::Keyword),
            None,
            Some(Highlight::String),
            Some(Highlight::Comment),
            Some(Highlight::Comment),
        ];

        assert_eq!(
            runs(&faces),
            [
                (0..2, Highlight::Keyword),
                (3..4, Highlight::String),
                (4..6, Highlight::Comment)
            ]
        );
    }
}
