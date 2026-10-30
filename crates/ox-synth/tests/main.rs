use std::sync::Arc;

use ox_synth::{Command, Controls, Engine, RATE};

fn evaluated(source: &str) -> String {
    let mut isolate = ox_lang::Isolate::new();
    ox_synth::install(&mut isolate);
    match isolate.eval(&format!("(module (import (ox synth :as s))) {source}")) {
        Ok(value) => isolate.write(value),
        Err(error) => format!("error: {}", error.message),
    }
}

fn number(source: &str) -> f32 {
    let shown = evaluated(source);
    shown
        .parse()
        .unwrap_or_else(|_| panic!("{source} gave {shown}"))
}

#[test]
fn a_held_sine_lasts_as_long_as_it_is_held_at_its_velocity() {
    let seconds = number("(get (s/info (s/render (s/sine) :seconds 0.5)) :seconds)");
    let peak = number("(get (s/info (s/render (s/sine) :note :a4 :velocity 0.5)) :peak)");

    assert!((0.5..0.53).contains(&seconds), "{seconds}");
    assert!((peak - 0.5_f32.tanh()).abs() < 0.01, "{peak}");
}

#[test]
fn envelopes_ring_on_after_the_note_is_let_go() {
    let seconds = number(
        "(get (s/info (s/render (s/envelope (s/sine) :release 0.5) :seconds 0.5)) :seconds)",
    );

    assert!((0.8..1.6).contains(&seconds), "{seconds}");
}

#[test]
fn a_lowpass_darkens_a_saw() {
    let open = number("(get (s/info (s/render (s/saw 110) :seconds 0.3)) :rms)");
    let closed = number("(get (s/info (s/render (s/lowpass (s/saw 110) 150) :seconds 0.3)) :rms)");

    assert!(closed < open * 0.8, "{closed} {open}");
}

#[test]
fn songs_play_their_patterns_for_their_bars_and_let_tails_ring() {
    let seconds = number(
        r#"(get (s/info (s/render (s/song :tempo 120
              (s/track :bell (s/envelope (s/sine) :release 0.05) (s/notes "a4 . . . | c5 . . .")))))
          :seconds)"#,
    );

    assert!((4.0..4.5).contains(&seconds), "{seconds}");
}

#[test]
fn patterns_say_what_they_cannot_read() {
    assert_eq!(
        evaluated(r#"(s/notes "a4 h9")"#),
        "error: synth/notes can't read \"h9\" in bar 1; write notes such as a4, c#5 or bb3, _ to hold and . for silence"
    );
    assert_eq!(
        evaluated(r#"(s/track :x (s/sine) "a4")"#),
        "error: synth/track expects a pattern from synth/notes, synth/chords or synth/beats, got \"a4\""
    );
}

#[test]
fn samples_are_cut_turned_joined_and_resaved() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("tone.wav");
    let shown = path.display().to_string().replace('\\', "/");
    let seconds = number(&format!(
        r#"(def tone (s/render (s/sine) :seconds 1))
           (s/save (s/join (s/slice tone 0 0.25) (s/reverse (s/slice tone 0 0.5))) "{shown}")
           (get (s/info (s/load "{shown}")) :seconds)"#
    ));

    assert!((seconds - 0.75).abs() < 0.01, "{seconds}");
}

#[test]
fn a_sampler_plays_its_sample_at_the_note_s_pitch() {
    let seconds = number(
        "(def hit (s/render (s/sine) :seconds 1))
         (get (s/info (s/render (s/sampler hit :root :c4) :note :c5 :seconds 0.1)) :seconds)",
    );

    assert!((0.5..0.6).contains(&seconds), "{seconds}");
}

#[test]
fn grains_and_effects_make_sound_from_a_sample() {
    let rms = number(
        "(def pad (s/render (s/saw) :seconds 1))
         (get (s/info (s/render (s/reverb (s/grains pad :density 30) :mix 0.5) :seconds 1)) :rms)",
    );
    let longer = number(
        "(def click (s/render (s/sine) :seconds 0.05))
         (- (get (s/info (s/process click (s/delay (s/input) 0.2 :mix 1) :tail 1)) :seconds)
            (get (s/info click) :seconds))",
    );

    assert!(rms > 0.01, "{rms}");
    assert!((longer - 1.0).abs() < 0.001, "{longer}");
}

#[test]
fn automations_move_controls_bar_by_bar() {
    let mut isolate = ox_lang::Isolate::new();
    ox_synth::install(&mut isolate);
    let song = isolate
        .eval(
            r#"(module (import (ox synth :as s)))
               (s/song :tempo 240 :automate {swell: [[0 0] [2 1]]}
                 (s/track :pad (s/gain (s/sine) (s/control :swell 0)) (s/notes "a4")))"#,
        )
        .unwrap();
    let song = ox_synth::song_of(isolate.heap(), song).unwrap();
    let controls = Arc::new(Controls::default());
    let mut engine = Engine::offline(RATE, Arc::clone(&controls));
    engine.apply(Command::Loop(song));

    engine.render(48_000);

    let swell = controls.get("swell").unwrap();
    assert!((swell - 0.5).abs() < 0.01, "{swell}");
}

#[test]
fn a_new_song_takes_over_at_the_next_bar() {
    let mut isolate = ox_lang::Isolate::new();
    ox_synth::install(&mut isolate);
    let mut song = |text: &str| {
        let value = isolate
            .eval(&format!(
                r#"(module (import (ox synth :as s)))
                   (s/song :tempo 240 (s/track :lead (s/sine) (s/notes "{text}")))"#
            ))
            .unwrap();
        ox_synth::song_of(isolate.heap(), value).unwrap()
    };
    let (first, second) = (song("a4"), song(". . . a4"));
    let mut engine = Engine::offline(RATE, Arc::new(Controls::default()));
    engine.apply(Command::Loop(first));
    let opening = engine.render(12_000);
    engine.apply(Command::Loop(second));

    let rest = engine.render(96_000);

    let loud = |frames: &[f32]| {
        frames
            .iter()
            .fold(0.0_f32, |peak, value| peak.max(value.abs()))
    };
    assert!(loud(&opening) > 0.3);
    assert!(
        loud(&rest[2 * 30_000..2 * 36_000]) > 0.3,
        "the first song plays to its bar's end"
    );
    assert!(
        loud(&rest[2 * 37_000..2 * 71_000]) < 0.01,
        "the second song rests"
    );
    assert!(
        loud(&rest[2 * 73_000..2 * 80_000]) > 0.3,
        "and plays its last beat"
    );
}
