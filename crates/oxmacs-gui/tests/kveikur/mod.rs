use std::fs;
use std::path::{Path, PathBuf};

use oxmacs_gui::app::App;

use super::sigur::{finish, listen, music_app, music_film, music_home, reload, report};
use super::tank::{CALM, Hand};
use super::{Film, PUMPING, settle};

const TYPING: f64 = 4.8;

const KINDLING: &str = r#"(module (import (ox synth :as s)))

(def (kit name) (s/load (str "samples/" name ".wav")))
(def room (-> (s/input) (s/reverb :size 0.96 :damping 0.4 :mix 0.5)))

(def wind
(s/track :wind (-> (s/sampler (kit "wind")) (s/gain 0.15) (s/gain (s/control :dawn 0)))
(s/beats "x | . | . | .")))
(def hum
(s/track :hum (-> (s/sampler (kit "hum") :root :d3) (s/gain 0.12) (s/gain (s/control :dawn 0)))
(s/notes "d3 | . | d3 | .")))

(s/loop (s/song :tempo 76
wind hum))

(s/automate :dawn [[0 0] [8 1]])"#;

const ANVIL: &str = r#"(def seed "d4 . . f4 | eb4 _ . . | . . . . | . . . .")
(def anvil
(s/track :anvil (s/gain (s/sampler (kit "anvil") :root :eb4) 0.15)
(s/notes seed) :effects room))
(def bowed
(-> (s/mix (s/saw (s/mul (s/pitch) (s/lfo 5.2 0.997 1.003)))
(s/saw (s/mul (s/pitch) (s/lfo 4.4 0.996 1.004)) :detune 9)
(s/saw :octave -1 :detune -6))
(s/drive 0.7) (s/lowpass (s/control :bow 500 :low 200 :high 5000) :resonance 0.2)
(s/chorus :mix 0.4) (s/envelope :attack 3 :release 3)))
(def drone
(s/track :drone (s/gain bowed 0.06)
(s/notes "d3 _ _ _ | _ _ _ _ | a2 _ _ _ | _ _ _ _") :effects room))

"#;

const RIFF: &str = r#"(def (hit name pattern sample gain effects)
(s/track name (s/gain (s/sampler (kit sample)) gain) (s/beats pattern) :effects effects))
(def drums
(-> (s/input) (s/compress :threshold 0.25 :ratio 3) (s/drive 0.1)
(s/reverb :size 0.82 :damping 0.35 :mix 0.28)))
(def kick (hit :kick "X . . . . . x . | X . . x . . . ." "kick" 0.45 drums))
(def snare (hit :snare ". . . . X . . ." "snare" 0.45 drums))
(def crash
(s/track :crash (s/gain (s/sampler (s/layer (kit "crash") (kit "boom"))) 0.6)
(s/beats "x | . | . | .") :effects drums))

(def fuzz
(-> (s/mix (s/saw) (s/square :octave -1 :width 0.3))
(s/fold (s/control :fold 0.2))
(s/lowpass (s/control :growl 700 :low 200 :high 4000) :resonance 0.25)
(s/envelope :attack 0.004 :decay 0.25 :sustain 0.75 :release 0.12)))
(def ducked (-> (s/input) (s/compress :key :kick :threshold 0.03 :ratio 12)))
(def bass
(s/track :bass (s/gain fuzz 0.32)
(s/notes "d2 d2 . d2 . . eb2 . | d2 _ _ . c2 . bb1 .") :effects ducked))

"#;

const VOICE: &str = r#"(def toms (hit :toms ". . . . . . . . | . . . . . x . x" "tom" 0.4 drums))
(def lift
(s/track :lift
(-> (s/noise) (s/bandpass (s/mix 300 (s/gain (s/control :lift 0) 6000)) :resonance 0.5)
(s/gain (s/control :lift 0)) (s/gain 0.35))
(s/beats "x | _ | _ | _") :effects room))
(def stutter
(let [bit (s/fade (s/slice (kit "sung-a") 0.5 0.62) :out 0.03)]
(s/join bit bit (s/reverse bit) bit (s/resample bit 2) (s/resample bit 2))))
(def glitch
(s/track :glitch (s/gain (s/sampler stutter :root :d5) 1)
(s/notes ". . . . | . . . . | d5 . . . | . . f5 eb5") :effects room))
(def chain (hit :chain ". . x . . . x x | . . . . . . . ." "chain" 0.25 room))
(def voice
(-> (s/mix (s/grains (kit "sung-a") :root :d5 :size 0.3 :density 22 :spread 0.12)
(s/gain (s/vowel (s/saw (s/mul (s/pitch) (s/lfo 5.5 0.994 1.006))) (s/lfo 0.15 0 3)) 0.12))
(s/envelope :attack 0.35 :release 1.5)))
(def high
(s/track :high (s/gain voice 0.55)
(s/notes "d5 _ _ _ f5 _ eb5 _ | d5 _ _ _ _ _ . . | f5 _ _ _ a5 _ g5 _ | f5 _ eb5 _ d5 _ _ _")
:effects room))
(def glock
(s/track :glock
(-> (s/mix (s/sine) (s/gain (s/sine (s/mul (s/pitch) 2.76)) 0.3))
(s/envelope :attack 0.002 :decay 1.4 :sustain 0 :release 1.4) (s/gain 0.11))
(s/notes "d6 . a5 . f6 . a5 . | eb6 . a5 . d6 . . .") :effects room))

"#;

const GROOVE: &str = r#"(def steel (hit :steel ". x . . x . . | . . . x . . x" "steel" 1 drums))
(def clank
(-> (s/mul (s/sine 310) (s/sine 1123)) (s/mix (s/gain (s/noise) 0.3))
(s/fold 0.4) (s/bandpass 2200 :resonance 0.5)
(s/envelope :attack 0.001 :decay 0.18 :sustain 0)))
(def scrap
(s/track :scrap (s/gain clank 0.25)
(s/beats "x . . x . x . | x . x . . x x") :effects drums))
(def brass
(-> (s/mix (s/grains (kit "trombone") :root :d3 :position 0.4 :size 0.3 :density 16)
(s/grains (kit "horn") :root :d4 :position 0.4 :size 0.3 :density 16))
(s/envelope :attack 1 :release 2)))
(def horns
(s/track :horns (s/gain brass (s/control :swell 0.12))
(s/chords ". | . | . | . | Dm _ | . | Bb _ | ." :octave 3) :effects room))

"#;

const HUSH: &str = r#"(def piano
(-> (s/mix (s/triangle) (s/gain (s/sine :octave 1) 0.3)) (s/lowpass 1400)
(s/envelope :attack 0.005 :decay 1.8 :sustain 0 :release 1.5)))
(def felt
(s/track :felt (s/gain piano 0.08)
(s/chords ". . Bmaj9 . | G#m9 . . . | Emaj9 . . . | F#sus4 . F# ." :octave 3)
:effects room))
(def glass
(s/track :glass (s/gain (s/sampler (kit "glass") :root :f#5) 0.05)
(s/notes "f#5 | . | b4 | .") :effects room))
(def chest
(-> (s/grains (kit "low-o") :root :g3 :size 0.3 :density 20 :spread 0.12)
(s/envelope :attack 0.6 :release 1.5)))
(def low
(s/track :low (s/gain chest 0.2)
(s/notes ". | g#2 _ _ _ | e2 _ _ _ | f#2 _ _ _") :effects room))
(def air
(s/track :air (s/gain voice 0.5)
(s/notes ". | d#5 _ e5 _ | b4 _ _ _ | c#5 _ _ _") :effects room))
(def ice (hit :ice ". . x . | . . . . | . x . . | . . . x" "ice" 0.08 room))

"#;

const BROKEN: &str = r"(def (broken sound)
(s/crush sound :bits (s/control :bits 16 :low 1 :high 16)
:rate (s/control :rate 48000 :low 200 :high 48000)))
";

const WALL: &str = r#"(def wall
(s/track :wall
(-> (s/mix (s/sampler (kit "factory") :loop true) (s/gain (s/noise :brown) 0.5))
(s/highpass 150) (s/gain (s/control :wall 0)))
(s/beats "x | _ | _ | _ | _ | _ | _ | _") :effects room))

"#;

const EMBER: &str = r#"(def hall (-> (s/input) (s/reverb :size 0.97 :damping 0.5 :mix 0.6)))
(def music-box
(-> (s/mix (s/sine) (s/gain (s/sine (s/mul (s/pitch) 4)) 0.2)
(s/gain (s/sine (s/mul (s/pitch) 6.3)) 0.05))
(s/envelope :attack 0.001 :decay 2 :sustain 0 :release 2)))
(def ember
(s/track :ember (s/gain music-box 0.11)
(s/notes "d6 _ . . f#6 _ e6 _ | d6 _ _ _ . . . . | f#6 _ . . a6 _ g6 _ | f#6 _ e6 _ d6 _ _ _")
:effects hall))

"#;

struct Session {
    marks: Vec<(f64, f64, f64)>,
}

fn now(film: &Film) -> f64 {
    film.frames.iter().map(|(_, seconds)| seconds).sum()
}

fn bar_length(tempo: f64, meter: f64) -> f64 {
    60.0 / tempo * meter
}

impl Session {
    fn bar(&self, at: f64) -> f64 {
        let (time, bar, length) = self
            .marks
            .iter()
            .rev()
            .find(|(time, ..)| *time <= at)
            .copied()
            .unwrap_or(self.marks[0]);
        bar + (at - time) / length
    }

    fn time_of(&self, wanted: f64) -> f64 {
        let (time, bar, length) = self
            .marks
            .iter()
            .rev()
            .find(|(_, bar, _)| *bar <= wanted)
            .copied()
            .unwrap_or(self.marks[0]);
        time + (wanted - bar) * length
    }

    fn ahead(&self, film: &Film, every: f64, bars: f64) -> f64 {
        ((self.bar(now(film)) + bars) / every).ceil() * every
    }

    fn swap_at(&mut self, film: &mut Film, app: &mut App, target: f64, meter: Option<(f64, f64)>) {
        let ready = self.time_of(target - 0.25) - now(film);
        if ready > 0.0 {
            listen(film, app, ready);
        }
        reload(film, app, &mut Hand::new(CALM));
        if let Some((tempo, beats)) = meter {
            let at = self.time_of(target);
            self.marks.push((at, target, bar_length(tempo, beats)));
        }
    }

    fn swap(&mut self, film: &mut Film, app: &mut App, every: f64, meter: Option<(f64, f64)>) {
        let target = self.ahead(film, every, 0.3);
        self.swap_at(film, app, target, meter);
    }
}

fn automate_points(film: &mut Film, app: &mut App, control: &str, points: &[(f64, f64)]) {
    let shown: Vec<String> = points
        .iter()
        .map(|(bar, value)| format!("[{bar} {value}]"))
        .collect();
    let mut hand = Hand::new(TYPING);
    hand.key(film, app, "M->");
    hand.write(
        film,
        app,
        &format!("\n(s/automate :{control} [{}])", shown.join(" ")),
    );
    hand.key(film, app, "C-x C-e");
    settle(app);
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
    let start = session.bar(now(film)).ceil() + 1.0;
    automate_points(film, app, control, &[(start, from), (start + bars, to)]);
}

fn rise_into(film: &mut Film, app: &mut App, session: &mut Session, meter: Option<(f64, f64)>) {
    let target = session.ahead(film, 4.0, 2.9);
    automate_points(
        film,
        app,
        "lift",
        &[(target - 1.5, 0.0), (target, 1.0), (target + 0.1, 0.0)],
    );
    session.swap_at(film, app, target, meter);
}

fn seek(film: &mut Film, app: &mut App, anchor: &str, then: Option<&str>) {
    let mut hand = Hand::new(TYPING);
    match then {
        Some(then) => hand.seek_in(film, app, anchor, then),
        None => hand.seek(film, app, anchor),
    }
}

fn replace_to_quote(film: &mut Film, app: &mut App, text: &str) {
    let mut hand = Hand::new(TYPING);
    hand.key(film, app, "C-SPC C-s");
    hand.write(film, app, "\"");
    hand.key(film, app, "RET C-b C-w");
    hand.write(film, app, text);
}

fn pattern(film: &mut Film, app: &mut App, track: &str, kind: &str, text: &str) {
    seek(
        film,
        app,
        &format!("(s/track :{track}"),
        Some(&format!("(s/{kind} \"")),
    );
    replace_to_quote(film, app, text);
}

fn beat(film: &mut Film, app: &mut App, track: &str, text: &str) {
    seek(film, app, &format!("(hit :{track} \""), None);
    replace_to_quote(film, app, text);
}

fn reseed(film: &mut Film, app: &mut App, text: &str) {
    seek(film, app, "(def seed \"", None);
    replace_to_quote(film, app, text);
}

fn arrange(film: &mut Film, app: &mut App, tempo: &str, tracks: &str) {
    seek(film, app, ":tempo ", None);
    let mut hand = Hand::new(TYPING);
    hand.key(film, app, "C-SPC C-s");
    hand.write(film, app, "))");
    hand.key(film, app, "RET C-w");
    hand.write(film, app, &format!("{tempo}\n{tracks}))"));
}

fn above(film: &mut Film, app: &mut App, anchor: &str, text: &str) {
    seek(film, app, anchor, None);
    let mut hand = Hand::new(TYPING);
    hand.key(film, app, "C-a");
    hand.write(film, app, text);
}

fn add(film: &mut Film, app: &mut App, text: &str) {
    above(film, app, "(s/loop (s/song", text);
}

fn kindling(film: &mut Film, app: &mut App) -> Session {
    film.mark("kindling");
    film.shot(app, 1.5);
    Hand::new(8.0).write(film, app, KINDLING);
    let mut slow = Hand::new(CALM);
    slow.save(film, app);
    slow.command(film, app, "bellow-play");
    settle(app);
    let mut session = Session {
        marks: vec![(now(film), 0.0, bar_length(76.0, 4.0))],
    };
    listen(film, app, 4.0);
    add(film, app, ANVIL);
    arrange(film, app, "76", "wind hum anvil drone");
    let entry = session.ahead(film, 4.0, 0.3).max(8.0);
    session.swap_at(film, app, entry, None);
    listen(film, app, 8.0);
    session
}

fn riff(film: &mut Film, app: &mut App, session: &mut Session) {
    film.mark("riff");
    add(film, app, RIFF);
    arrange(film, app, "76", "hum anvil drone\nkick snare crash bass");
    let target = session.ahead(film, 4.0, 2.9);
    automate_points(
        film,
        app,
        "bow",
        &[
            (target - 1.5, 500.0),
            (target, 3000.0),
            (target + 4.0, 1400.0),
        ],
    );
    session.swap_at(film, app, target, None);
    listen(film, app, 7.0);
    add(film, app, VOICE);
    arrange(
        film,
        app,
        "76",
        "hum anvil drone glitch chain\nkick snare toms crash bass lift high glock",
    );
    session.swap(film, app, 2.0, None);
    listen(film, app, 8.0);
    pattern(
        film,
        app,
        "bass",
        "notes",
        "g1 g1 . g1 . . ab1 . | g1 _ _ . f1 . eb1 .",
    );
    pattern(
        film,
        app,
        "glock",
        "notes",
        "g6 . d6 . bb5 . d6 . | f6 . d6 . bb5 . . .",
    );
    session.swap(film, app, 2.0, None);
    automate(film, app, session, "growl", 700.0, 1400.0, 8.0);
    listen(film, app, 5.0);
}

fn groove(film: &mut Film, app: &mut App, session: &mut Session) {
    film.mark("groove");
    automate(film, app, session, "fold", 0.2, 0.45, 12.0);
    add(film, app, GROOVE);
    reseed(film, app, "d4 . . eb4 . d4 . | . . f4 . eb4 . .");
    beat(film, app, "kick", "X . . X . . . | X . . X . x .");
    beat(film, app, "snare", ". . . . . X .");
    pattern(
        film,
        app,
        "bass",
        "notes",
        "d2 . . d2 . eb2 . | d2 . . d2 . c2 a1",
    );
    pattern(
        film,
        app,
        "high",
        "notes",
        "a5 _ _ bb5 _ a5 _ | f5 _ _ _ _ _ _ | eb5 _ f5 _ eb5 d5 _ | d5 _ _ _ _ _ _",
    );
    arrange(
        film,
        app,
        "88 :meter 3.5",
        "anvil drone kick snare crash\nbass lift high steel scrap horns",
    );
    rise_into(film, app, session, Some((88.0, 3.5)));
    listen(film, app, 8.0);
    pattern(
        film,
        app,
        "bass",
        "notes",
        "d2 d2 . eb2 . d2 . | f2 . eb2 . d2 . c2",
    );
    pattern(
        film,
        app,
        "high",
        "notes",
        "f5 _ _ eb5 _ d5 _ | c5 _ _ _ _ _ _ | d5 _ eb5 _ f5 _ a5 | g5 _ _ _ _ _ _",
    );
    session.swap(film, app, 2.0, None);
    automate(film, app, session, "bow", 1400.0, 2600.0, 6.0);
    listen(film, app, 6.0);
}

fn hush(film: &mut Film, app: &mut App, session: &mut Session) {
    film.mark("hush");
    add(film, app, HUSH);
    arrange(film, app, "60", "felt glass low air ice");
    session.swap(film, app, 4.0, Some((60.0, 4.0)));
    listen(film, app, 15.0);
}

fn rise(film: &mut Film, app: &mut App, session: &mut Session) {
    film.mark("return");
    reseed(film, app, "d4 _ _ _ | f4 _ _ _ | eb4 _ _ _ | d4 _ c4 _");
    pattern(
        film,
        app,
        "high",
        "notes",
        "d5 _ _ _ _ _ _ _ | f5 _ _ _ eb5 _ _ _ | d5 _ _ _ c5 _ _ _ | bb4 _ _ _ _ _ _ _",
    );
    pattern(
        film,
        app,
        "low",
        "notes",
        "d3 _ _ _ _ _ _ _ | f3 _ _ _ eb3 _ _ _ | d3 _ _ _ c3 _ _ _ | bb2 _ _ _ _ _ _ _",
    );
    pattern(film, app, "horns", "chords", "Dm _ | Bbmaj7 _ | F _ | Eb _");
    pattern(
        film,
        app,
        "glock",
        "notes",
        "d6 . f6 . eb6 . . . | . . . . . . . . | f6 . a6 . g6 . . . | . . . . . . . .",
    );
    arrange(film, app, "76", "hum anvil drone horns high low glock lift");
    session.swap(film, app, 4.0, Some((76.0, 4.0)));
    automate(film, app, session, "swell", 0.12, 0.22, 8.0);
    listen(film, app, 5.0);
    beat(film, app, "kick", "X . . . . . x . | X . . x . . . .");
    beat(film, app, "snare", ". . . . X . . .");
    pattern(
        film,
        app,
        "bass",
        "notes",
        "d2 _ d2 d2 . d2 eb2 . | d2 _ _ . c2 . bb1 .",
    );
    arrange(
        film,
        app,
        "76",
        "hum anvil drone horns high low glock lift\nkick snare toms crash bass chain",
    );
    rise_into(film, app, session, None);
    listen(film, app, 6.0);
}

fn wall(film: &mut Film, app: &mut App, session: &mut Session) {
    film.mark("wall");
    above(film, app, "(def room", BROKEN);
    add(film, app, WALL);
    for anchor in ["(def room", "(def drums", "(def ducked"] {
        seek(film, app, anchor, Some("(s/input)"));
        Hand::new(TYPING).write(film, app, " broken");
    }
    arrange(
        film,
        app,
        "76",
        "hum anvil drone horns high low glock lift\nkick snare toms crash bass chain wall",
    );
    rise_into(film, app, session, None);
    automate(film, app, session, "wall", 0.0, 1.0, 8.0);
    automate(film, app, session, "bow", 1400.0, 4500.0, 8.0);
    listen(film, app, 10.0);
}

fn collapse(film: &mut Film, app: &mut App, session: &mut Session) {
    film.mark("collapse");
    let end = session.ahead(film, 4.0, 6.0);
    automate_points(film, app, "bits", &[(end - 4.0, 16.0), (end, 1.5)]);
    automate_points(film, app, "rate", &[(end - 4.0, 48000.0), (end, 300.0)]);
    add(film, app, EMBER);
    arrange(film, app, "60", "hum ember");
    session.swap_at(film, app, end, Some((60.0, 4.0)));
    listen(film, app, 17.0);
    let mut slow = Hand::new(CALM);
    slow.command(film, app, "bellow-stop");
    listen(film, app, 5.0);
}

fn kveikur_home(samples: &Path) -> PathBuf {
    let home = music_home("oxmacs-kveikur-demo");
    let folder = home.join("samples");
    fs::create_dir_all(&folder).unwrap();
    for entry in fs::read_dir(samples).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "wav") {
            fs::copy(&path, folder.join(path.file_name().unwrap())).unwrap();
        }
    }
    home
}

#[test]
#[ignore = "renders a live-coding video of an original, heavier post-rock piece made with bellow, (ox synth) and free samples, with its sound, into $OXMACS_KVEIKUR_VIDEO"]
fn kveikur_video() {
    let (Ok(out), Ok(samples)) = (
        std::env::var("OXMACS_KVEIKUR_VIDEO"),
        std::env::var("OXMACS_KVEIKUR_SAMPLES"),
    ) else {
        return;
    };
    let out = PathBuf::from(out);
    let dry = std::env::var("OXMACS_KVEIKUR_DRY").is_ok();
    let home = kveikur_home(Path::new(&samples));
    std::env::set_current_dir(&home).unwrap();
    PUMPING.set(true);
    let mut film = music_film(&out, dry);
    let mut app = music_app(&home);
    let mut session = kindling(&mut film, &mut app);
    report("kindling", &film, &app);
    riff(&mut film, &mut app, &mut session);
    report("riff", &film, &app);
    groove(&mut film, &mut app, &mut session);
    report("groove", &film, &app);
    hush(&mut film, &mut app, &mut session);
    report("hush", &film, &app);
    rise(&mut film, &mut app, &mut session);
    report("return", &film, &app);
    wall(&mut film, &mut app, &mut session);
    report("wall", &film, &app);
    collapse(&mut film, &mut app, &mut session);
    film.hold(1.0);
    report("end", &film, &app);
    finish(&mut film, &app, &out, dry);
}
