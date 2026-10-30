use std::io;
use std::path::Path;

pub fn encode(rate: u32, interleaved: &[f32]) -> Vec<u8> {
    let data = u32::try_from(interleaved.len() * 2).unwrap_or(u32::MAX);
    let mut bytes = Vec::with_capacity(44 + interleaved.len() * 2);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&data.saturating_add(36).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 4).to_le_bytes());
    bytes.extend_from_slice(&4_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for sample in interleaved {
        bytes.extend_from_slice(&crate::pcm(*sample).to_le_bytes());
    }
    bytes
}

pub fn save(path: &Path, rate: u32, interleaved: &[f32]) -> io::Result<()> {
    oxmacs_files::write(
        path,
        &encode(rate, interleaved),
        oxmacs_files::NewFile::Shared,
    )
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn wav_files_hold_sixteen_bit_stereo() {
        let bytes = encode(48_000, &[0.0, 1.0, -1.0, 0.5]);

        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(bytes.len(), 44 + 8);
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2);
        let samples: Vec<i16> = bytes[44..]
            .chunks(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(samples, [0, 32767, -32767, 16384]);
    }
}
