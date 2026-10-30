use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use iced::Size;
use oxmacs_core::editor::Visit;
use oxmacs_gui::app::{App, Message};

use super::tank::{CALM, Hand};
use super::{Film, PUMPING, settle, track};

const TYPING: f64 = 2.2;
const BAR: f64 = 4.0 * 60.0 / 66.0;

const BELLS: &str = r#"(module (import (ox synth :as s)))

(def glock
(-> (s/mix (s/sine) (s/gain (s/sine (s/mul (s/pitch) 2.76)) 0.3))
(s/envelope :attack 0.002 :decay 1.6 :sustain 0 :release 1.6)
(s/gain 0.16)))

(def echoes
(-> (s/input) (s/delay 0.68 :feedback 0.4 :mix 0.3 :cross true)
(s/reverb :size 0.92 :mix 0.45)))

(def bells
(s/notes (str "f#5 . a5 . e6 . d6 . | c#6 . e5 . a5 . . . | "
"d6 . f#5 . b5 . a5 . | g5 . b5 . d6 _ _ .")))

(s/loop
(s/song :tempo 66
(s/track :glock glock bells :effects echoes)))"#;

const ORGAN: &str = "(def organ
(-> (s/mix (s/sine) (s/gain (s/sine :octave 1) 0.45)
(s/gain (s/triangle :octave -1) 0.35))
(s/envelope :attack 1.4 :release 2.2)
(s/gain 0.07)))

";

const HALL: &str = "(def hall (-> (s/input) (s/reverb :size 0.95 :damping 0.45 :mix 0.5)))

";

const RISER: &str = "(def riser
(let [bloom (s/reverse (s/render (s/reverb organ :size 0.97 :mix 0.8)
:seconds 2 :note :d4))]
(s/fade (s/slice bloom (- (get (s/info bloom) :seconds) 3.6)) :in 0.5)))

";

const BOW: &str = "(def bow
(-> (s/mix (s/saw (s/mul (s/pitch) (s/lfo 5.2 0.997 1.003)))
(s/saw (s/mul (s/pitch) (s/lfo 4.6 0.996 1.004)) :detune 7))
(s/lowpass (s/control :tone 900 :low 300 :high 3000) :resonance 0.15)
(s/drive 0.2)
(s/envelope :attack 1.8 :sustain 0.9 :release 2.5)
(s/gain (s/control :swell 0.3))
(s/gain 0.12)))

";

const VOICE: &str = "(def (vowel source)
(s/mix (s/bandpass source 750 :resonance 0.8)
(s/gain (s/bandpass source 1150 :resonance 0.8) 0.6)
(s/gain (s/bandpass source 2800 :resonance 0.85) 0.2)))

(def voice
(-> (s/saw (s/mul (s/pitch) (s/lfo 5.6 0.993 1.007)))
vowel
(s/envelope :attack 1.1 :release 1.8)
(s/gain 0.07)))

";

const DRUMS: &str = "(def bass
(-> (s/triangle) (s/lowpass 380) (s/envelope :attack 0.08 :release 1) (s/gain 0.18)))

(def kick
(-> (s/sine (s/mix 44 (s/envelope 120 :attack 0.001 :decay 0.08 :sustain 0)))
(s/envelope :attack 0.001 :decay 0.7 :sustain 0 :release 0.4)
(s/gain 0.34)))

(def snare
(-> (s/noise :pink) (s/bandpass 1700 :resonance 0.3)
(s/envelope :attack 0.001 :decay 0.3 :sustain 0 :release 0.3)
(s/gain 1.35)))

";

struct Session {
    started: f64,
}

impl Session {
    fn bar_after(&self, film: &Film, ahead: f64) -> i64 {
        let now: f64 = film.frames.iter().map(|(_, seconds)| seconds).sum();
        bar_number(((now - self.started) / BAR).ceil() + ahead)
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "bar numbers in a few minutes of music are small"
)]
fn bar_number(bars: f64) -> i64 {
    bars as i64
}

pub(super) fn listen(film: &mut Film, app: &mut App, seconds: f64) {
    film.play(app, seconds);
}

pub(super) fn reload(film: &mut Film, app: &mut App, hand: &mut Hand) {
    hand.key(film, app, "C-c C-b");
    settle(app);
}

fn above(film: &mut Film, app: &mut App, anchor: &str, text: &str) {
    let mut slow = Hand::new(CALM);
    slow.seek(film, app, anchor);
    slow.key(film, app, "C-a");
    Hand::new(TYPING).write(film, app, text);
}

fn after(film: &mut Film, app: &mut App, anchor: &str, text: &str) {
    let mut slow = Hand::new(CALM);
    slow.seek(film, app, anchor);
    Hand::new(TYPING).write(film, app, text);
}

fn automate(
    film: &mut Film,
    app: &mut App,
    session: &Session,
    control: &str,
    from: f64,
    to: f64,
    bars: f64,
) {
    let start = session.bar_after(film, 0.0);
    let end = start + bar_number(bars);
    let mut slow = Hand::new(CALM);
    slow.key(film, app, "M->");
    Hand::new(TYPING).write(
        film,
        app,
        &format!("\n(s/automate :{control} [[{start} {from}] [{end} {to}]])"),
    );
    slow.key(film, app, "C-x C-e");
    settle(app);
}

fn bells(film: &mut Film, app: &mut App) -> Session {
    film.mark("bells");
    film.shot(app, 1.5);
    Hand::new(4.0).write(film, app, BELLS);
    let mut slow = Hand::new(CALM);
    slow.save(film, app);
    slow.command(film, app, "bellow-play");
    settle(app);
    let started = film.frames.iter().map(|(_, seconds)| seconds).sum();
    listen(film, app, 10.0);
    Session { started }
}

fn harmonium(film: &mut Film, app: &mut App) {
    film.mark("harmonium");
    above(film, app, "(def echoes", ORGAN);
    above(film, app, "(def bells", HALL);
    after(
        film,
        app,
        ":effects echoes)",
        "\n(s/track :organ organ (s/chords \"Dadd9 | A/C# | Bm7 | Gmaj7\") :effects hall)",
    );
    reload(film, app, &mut Hand::new(CALM));
    listen(film, app, 8.0);
}

fn swell(film: &mut Film, app: &mut App) {
    film.mark("swell");
    above(film, app, "(def echoes", RISER);
    after(
        film,
        app,
        "Gmaj7\") :effects hall)",
        "\n(s/track :rise (s/sampler riser) (s/beats \". | . | . | x\"))",
    );
    reload(film, app, &mut Hand::new(CALM));
    listen(film, app, 12.0);
}

fn bowed(film: &mut Film, app: &mut App, session: &Session) {
    film.mark("bow");
    above(film, app, "(def echoes", BOW);
    after(
        film,
        app,
        "\". | . | . | x\"))",
        "\n(s/track :bow bow (s/notes \"a4 _ _ _ | c#5 _ _ _ | d5 _ _ _ | b4 _ _ _\") :effects echoes)",
    );
    reload(film, app, &mut Hand::new(CALM));
    listen(film, app, 6.0);
    automate(film, app, session, "swell", 0.3, 1.0, 8.0);
    listen(film, app, 10.0);
}

fn sung(film: &mut Film, app: &mut App) {
    film.mark("voice");
    above(film, app, "(def echoes", VOICE);
    after(
        film,
        app,
        "b4 _ _ _\") :effects echoes)",
        "\n(s/track :voice voice (s/notes \"d5 _ e5 _ | c#5 _ _ _ | d5 _ f#5 _ | e5 _ _ _\") :effects hall)",
    );
    reload(film, app, &mut Hand::new(CALM));
    listen(film, app, 12.0);
}

fn crescendo(film: &mut Film, app: &mut App, session: &Session) {
    film.mark("crescendo");
    above(film, app, "(def echoes", DRUMS);
    after(
        film,
        app,
        "e5 _ _ _\") :effects hall)",
        "\n(s/track :bass bass (s/notes \"d2 _ _ _ | c#2 _ _ _ | b1 _ _ _ | g1 _ _ _\"))\n(s/track :kick kick (s/beats \"X . . . . . . . | X . . . . . x .\") :effects hall)\n(s/track :snare snare (s/beats \". . . . X . . .\") :effects hall)",
    );
    reload(film, app, &mut Hand::new(CALM));
    listen(film, app, 3.0);
    automate(film, app, session, "tone", 900.0, 3000.0, 8.0);
    listen(film, app, 28.0);
}

fn fade(film: &mut Film, app: &mut App, session: &Session) {
    film.mark("fade");
    let mut slow = Hand::new(CALM);
    slow.seek(film, app, "e5 _ _ _\") :effects hall)");
    slow.key(film, app, "C-SPC C-s");
    slow.write(film, app, ". . . . X . . .\") :effects hall)");
    slow.key(film, app, "RET C-w");
    reload(film, app, &mut slow);
    listen(film, app, 6.0);
    automate(film, app, session, "swell", 1.0, 0.0, 4.0);
    automate(film, app, session, "tone", 3000.0, 600.0, 4.0);
    listen(film, app, 16.0);
    slow.seek(film, app, "Gmaj7\") :effects hall)");
    slow.key(film, app, "C-SPC C-s");
    slow.write(film, app, "f#5 _ | e5 _ _ _\") :effects hall)");
    slow.key(film, app, "RET C-w");
    reload(film, app, &mut slow);
    listen(film, app, 18.0);
}

pub(super) fn music_home(folder: &str) -> PathBuf {
    let user = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap();
    let home = user.join(folder);
    if home.join(".oxmacs-demo").exists() {
        fs::remove_dir_all(&home).unwrap();
    }
    assert!(
        !home.exists(),
        "{} exists and wasn't made by this demo",
        home.display()
    );
    fs::create_dir_all(&home).unwrap();
    fs::write(home.join(".oxmacs-demo"), "").unwrap();
    fs::write(
        home.join("init.ox"),
        "(settings {theme: :adwaita-dark animations: false font-size: 11})\n(use-plugin bellow)\n(use-plugin keycast :config {placement: :bottom-center})\n",
    )
    .unwrap();
    fs::write(home.join("song.ox"), "").unwrap();
    home
}

pub(super) fn music_app(home: &Path) -> App {
    let visit = Visit {
        path: home.join("song.ox"),
        line: None,
        column: None,
    };
    let (mut app, booted) = App::boot(&[visit], Some(home));
    track(booted);
    track(app.update(Message::Focus(true)));
    settle(&mut app);
    app
}

pub(super) fn report(stage: &str, film: &Film, app: &App) {
    let seconds: f64 = film.frames.iter().map(|(_, seconds)| seconds).sum();
    eprintln!(
        "stage {stage}: {} frames, {seconds:.1}s, in {}, message {:?}",
        film.frames.len(),
        app.editor().buffer().name(),
        app.editor().message()
    );
}

#[test]
#[ignore = "renders a live-coding video of an original post-rock piece made with bellow and (ox synth), with its sound, into $OXMACS_SIGUR_VIDEO"]
fn sigur_video() {
    let Ok(out) = std::env::var("OXMACS_SIGUR_VIDEO") else {
        return;
    };
    let out = PathBuf::from(out);
    let dry = std::env::var("OXMACS_SIGUR_DRY").is_ok();
    let home = music_home("oxmacs-sigur-demo");
    PUMPING.set(true);
    let mut film = music_film(&out, dry);
    let mut app = music_app(&home);
    let session = bells(&mut film, &mut app);
    report("bells", &film, &app);
    harmonium(&mut film, &mut app);
    report("harmonium", &film, &app);
    swell(&mut film, &mut app);
    report("swell", &film, &app);
    bowed(&mut film, &mut app, &session);
    report("bow", &film, &app);
    sung(&mut film, &mut app);
    report("voice", &film, &app);
    crescendo(&mut film, &mut app, &session);
    report("crescendo", &film, &app);
    fade(&mut film, &mut app, &session);
    film.hold(1.0);
    report("end", &film, &app);
    finish(&mut film, &app, &out, dry);
}

pub(super) fn music_film(out: &Path, dry: bool) -> Film {
    Film::new(out.with_extension("frames"))
        .sized(Size::new(1280.0, 720.0))
        .scaled_to(1280, 720)
        .ticking(Instant::now())
        .listening()
        .dry(dry)
}

pub(super) fn finish(film: &mut Film, app: &App, out: &Path, dry: bool) {
    let editor = app.editor();
    let written = editor
        .buffer_named("song.ox")
        .and_then(|id| editor.buffer_by_id(id))
        .map(oxmacs_core::text::Buffer::contents)
        .unwrap_or_default();
    let _ = fs::write(out.with_extension("ox"), written);
    let logged = editor
        .buffer_named("*ox log*")
        .and_then(|id| editor.buffer_by_id(id))
        .map(oxmacs_core::text::Buffer::contents)
        .unwrap_or_default();
    let _ = fs::write(out.with_extension("log"), logged);
    match &film.listening {
        Some(listening) if dry => {
            ox_synth::wav::save(
                &out.with_extension("wav"),
                ox_synth::RATE,
                &listening.samples,
            )
            .unwrap();
        }
        _ => film.finish_heard(out),
    }
}
