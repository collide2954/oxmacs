use std::path::Path;
use std::time::Duration;

use rodeo::audio::{Audio, Bus, Capture, PlayOptions, Sound};

const RATE: u32 = 1000;

fn capture() -> (Audio, Capture) {
    Audio::capture(1, RATE)
}

fn tone(level: f32, count: usize) -> Sound {
    Sound::from_samples(1, RATE, vec![level; count]).unwrap()
}

fn wav(level: i16, count: usize) -> Vec<u8> {
    let data = u32::try_from(count * 2).unwrap();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for _ in 0..count {
        bytes.extend_from_slice(&level.to_le_bytes());
    }
    bytes
}

fn write_wav(folder: &Path, name: &str, level: i16, count: usize) -> std::path::PathBuf {
    let path = folder.join(name);
    std::fs::write(&path, wav(level, count)).unwrap();
    path
}

#[test]
fn sounds_decode_into_memory() {
    let folder = tempfile::tempdir().unwrap();
    let path = write_wav(folder.path(), "coin.wav", 16384, 250);
    let sound = Sound::load(&path).unwrap();
    assert_eq!((sound.channels(), sound.rate()), (1, RATE));
    assert_eq!(sound.duration(), Duration::from_millis(250));
    assert!(Sound::from_samples(0, RATE, vec![0.0]).is_err());
    let missing = Sound::load(folder.path().join("missing.wav")).unwrap_err();
    assert!(missing.to_string().starts_with("can't read "), "{missing}");
}

#[test]
fn effects_play_at_their_volume_times_the_buses() {
    let (mut audio, mut output) = capture();
    let sound = tone(0.5, 100);
    audio.play(&sound, PlayOptions::default());
    assert_eq!(output.take(10), [0.5; 10]);
    let (mut audio, mut output) = capture();
    audio.set_master(0.5);
    audio.set_bus(Bus::Effects, 0.5);
    audio.play(
        &sound,
        PlayOptions {
            volume: 0.5,
            ..PlayOptions::default()
        },
    );
    assert_eq!(output.take(10), [0.0625; 10]);
    assert_eq!(audio.bus(Bus::Effects), 0.5);
    assert_eq!(audio.master(), 0.5);
}

#[test]
fn voices_mix_and_finish() {
    let (mut audio, mut output) = capture();
    let first = audio.play(&tone(0.25, 20), PlayOptions::default());
    audio.play(&tone(0.5, 10), PlayOptions::default());
    let mixed = output.take(30);
    assert_eq!(&mixed[..10], [0.75; 10]);
    assert_eq!(&mixed[10..20], [0.25; 10]);
    assert_eq!(&mixed[20..], [0.0; 10]);
    audio.update(Duration::ZERO);
    assert!(!audio.playing(first));
    assert_eq!(audio.voices(), 0);
}

#[test]
fn voices_stop_pause_and_resume() {
    let (mut audio, mut output) = capture();
    let voice = audio.play(&tone(0.5, 1000), PlayOptions::default());
    output.take(10);
    audio.pause(voice);
    output.take(10);
    assert_eq!(output.take(10), [0.0; 10]);
    assert!(audio.paused(voice));
    audio.resume(voice);
    output.take(10);
    assert_eq!(output.take(10), [0.5; 10]);
    audio.stop(voice);
    output.take(10);
    assert_eq!(output.take(10), [0.0; 10]);
    audio.update(Duration::ZERO);
    assert!(!audio.playing(voice));
}

#[test]
fn looping_and_pitch_change_how_long_sounds_last() {
    let (mut audio, mut output) = capture();
    audio.play(
        &tone(0.5, 10),
        PlayOptions {
            looping: true,
            ..PlayOptions::default()
        },
    );
    assert_eq!(output.take(35), [0.5; 35]);
    let (mut audio, mut output) = capture();
    audio.play(
        &tone(0.5, 100),
        PlayOptions {
            pitch: 2.0,
            ..PlayOptions::default()
        },
    );
    let heard = output
        .take(200)
        .iter()
        .filter(|sample| **sample != 0.0)
        .count();
    assert!((45..=55).contains(&heard), "{heard}");
}

#[test]
fn music_streams_and_crossfades() {
    let folder = tempfile::tempdir().unwrap();
    let calm = write_wav(folder.path(), "calm.wav", 16384, 5000);
    let boss = write_wav(folder.path(), "boss.wav", 8192, 5000);
    let (mut audio, mut output) = capture();
    let first = audio
        .play_music(&calm, PlayOptions::default(), Duration::ZERO)
        .unwrap();
    assert_eq!(audio.music(), Some(first));
    assert_eq!(output.take(10), [0.5; 10]);
    let second = audio
        .play_music(&boss, PlayOptions::default(), Duration::from_millis(100))
        .unwrap();
    assert_eq!(audio.music(), Some(second));
    output.take(10);
    assert_eq!(output.take(5), [0.5; 5]);
    audio.update(Duration::from_millis(50));
    output.take(10);
    assert_eq!(output.take(5), [0.25 + 0.125; 5]);
    audio.update(Duration::from_millis(60));
    output.take(10);
    assert_eq!(output.take(5), [0.25; 5]);
    assert!(!audio.playing(first));
    audio.set_bus(Bus::Music, 0.5);
    output.take(10);
    assert_eq!(output.take(5), [0.125; 5]);
    audio.stop_music(Duration::ZERO);
    assert_eq!(audio.music(), None);
}

#[test]
fn silent_audio_plays_nothing_without_failing() {
    let mut audio = Audio::silent();
    let voice = audio.play(&tone(0.5, 10), PlayOptions::default());
    assert!(audio.playing(voice));
    audio.update(Duration::from_secs(1));
    assert!(!audio.playing(voice));
}
