use std::fs;
use std::path::{Path, PathBuf};

use oxmacs_gui::app::App;

use super::sigur::{finish, listen, music_app, music_film, music_home, reload, report};
use super::tank::{CALM, Hand};
use super::{Film, PUMPING, settle};

const TYPING: f64 = 3.3;

const KINDLING: &str = r#"(module (import (ox synth :as s)))

(def (kit name) (s/load (str "samples/" name ".wav")))
(def room (-> (s/input) (s/reverb :size 0.96 :damping 0.4 :mix 0.5)))

(def wind
(s/track :wind (s/gain (s/sampler (kit "wind")) 0.2)
(s/beats "x | . | . | .")))
(def hum
(s/track :hum (s/gain (s/sampler (kit "hum") :root :d3) 0.12)
(s/notes "d3 | . | d3 | .")))

(def seed "d4 . . f4 | eb4 _ . . | . . . . | . . . .")
(def anvil
(s/track :anvil (s/gain (s/sampler (kit "anvil") :root :eb4) 0.2)
(s/notes seed) :effects room))

(s/loop (s/song :tempo 76
wind hum anvil))"#;

const GLITCH: &str = r#"(def stutter
(let [bit (s/fade (s/slice (kit "sung-a") 0.5 0.62) :out 0.03)]
(s/join bit bit (s/reverse bit) bit (s/resample bit 2) (s/resample bit 2))))
(def glitch
(s/track :glitch (s/gain (s/sampler stutter :root :d5) 1.2)
(s/notes ". . . . | . . . . | d5 . . . | . . f5 eb5") :effects room))
(def chain
(s/track :chain (s/gain (s/sampler (kit "chain")) 0.3)
(s/beats ". . x . . . x x | . . . . . . . .") :effects room))

"#;

const RIFF: &str = r#"(def drums
(-> (s/input) (s/crush :bits 7 :rate 22000) (s/drive 0.3)
(s/reverb :size 0.6 :mix 0.2)))
(def kick
(s/track :kick (s/gain (s/sampler (kit "kick")) 0.35)
(s/beats "X . . . . . x . | X . . x . . . .") :effects drums))
(def snare
(s/track :snare (s/gain (s/sampler (kit "snare")) 0.35)
(s/beats ". . . . X . . .") :effects drums))
(def toms
(s/track :toms (s/gain (s/sampler (kit "tom")) 0.2)
(s/beats ". . . . . . . . | . . . . . x . x") :effects drums))

(def fuzz
(-> (s/mix (s/saw) (s/square :octave -1 :width 0.3))
(s/fold (s/control :fold 0.2))
(s/lowpass (s/control :growl 600 :low 200 :high 4000) :resonance 0.25)
(s/envelope :attack 0.004 :decay 0.25 :sustain 0.75 :release 0.12)))
(def ducked (-> (s/input) (s/compress :key :kick :threshold 0.03 :ratio 12)))
(def bass
(s/track :bass (s/gain fuzz 0.3)
(s/notes "d2 d2 . d2 . . eb2 . | d2 _ _ . c2 . bb1 .") :effects ducked))

"#;

const FALSETTO: &str = r#"(def falsetto
(-> (s/mix (s/saw (s/mul (s/pitch) (s/lfo 5.5 0.994 1.006)))
(s/gain (s/noise :pink) 0.2))
(s/vowel (s/lfo 0.15 0 3))
(s/envelope :attack 0.5 :release 1.4)))
(def high
(s/track :high (s/gain falsetto 0.13)
(s/notes "d5 _ _ _ | _ _ f5 _ | eb5 _ _ _ | _ _ _ .") :effects room))

"#;

const DRIVE: &str = r#"(def pad
(s/track :pad
(-> (s/grains (kit "sung-o") :root :d5 :size 0.3 :density 18 :spread 0.15)
(s/envelope :attack 0.2 :release 0.6) (s/gain 0.25))
(s/chords "Fm | Dbmaj7 | Ab | Eb/G" :octave 4) :effects ducked))
(def hat
(s/track :hat (s/gain (s/sampler (kit "metal-hat")) 0.2)
(s/beats ". x . x . x . x") :effects drums))
(def clank
(-> (s/mul (s/sine 310) (s/sine 1123)) (s/mix (s/gain (s/noise) 0.3))
(s/fold 0.4) (s/bandpass 2200 :resonance 0.5)
(s/envelope :attack 0.001 :decay 0.18 :sustain 0)))
(def scrap
(s/track :scrap (s/gain clank 0.25)
(s/beats ". . . x . . x . | . x . . . . . x") :effects drums))

"#;

const HUSH: &str = r#"(def piano
(-> (s/mix (s/triangle) (s/gain (s/sine :octave 1) 0.3)) (s/lowpass 1400)
(s/envelope :attack 0.005 :decay 1.8 :sustain 0 :release 1.5)))
(def felt
(s/track :felt (s/gain piano 0.1)
(s/chords "Bmaj9 . . . | G#m9 . . . | Emaj9 . . . | F#sus4 . F# ." :octave 3)
:effects room))
(def glass
(s/track :glass (s/gain (s/sampler (kit "glass") :root :f#5) 0.06)
(s/notes "f#5 | . | b4 | .") :effects room))
(def chest
(-> (s/saw (s/mul (s/pitch) (s/lfo 4.8 0.996 1.004))) (s/vowel :u)
(s/envelope :attack 0.8 :release 1.5)))
(def low
(s/track :low (s/gain chest 0.08)
(s/notes "b2 _ _ _ | g#2 _ _ _ | e2 _ _ _ | f#2 _ _ _") :effects room))
(def air
(s/track :air (s/gain falsetto 0.07)
(s/notes ". | d#5 _ e5 _ | b4 _ _ _ | c#5 _ _ _") :effects room))
(def ice
(s/track :ice (s/gain (s/sampler (kit "ice")) 0.12)
(s/beats ". . x . | . . . . | . x . . | . . . x") :effects room))

"#;

const BRASS: &str = r#"(def brass
(-> (s/mix (s/grains (kit "trombone") :root :d3 :position 0.4 :size 0.3 :density 16)
(s/grains (kit "horn") :root :d4 :position 0.4 :size 0.3 :density 16))
(s/envelope :attack 2 :release 2)))
(def horns
(s/track :horns (s/gain brass (s/control :swell 0.1))
(s/chords "Dm _ | Bbmaj7 _ | F _ | Eb _" :octave 3) :effects room))

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
(s/track :ember (s/gain music-box 0.24)
(s/notes "d6 . . f#6 | e6 _ . . | a5 . . d6 | c#6 _ _ .") :effects hall))

"#;

struct Session {
    marks: Vec<(f64, f64, f64)>,
}

fn now(film: &Film) -> f64 {
    film.frames.iter().map(|(_, seconds)| seconds).sum()
}

fn bar_length(tempo: f64) -> f64 {
    4.0 * 60.0 / tempo
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

    fn swap_at(&mut self, film: &mut Film, app: &mut App, target: f64, tempo: Option<f64>) {
        let ready = self.time_of(target - 0.25) - now(film);
        if ready > 0.0 {
            listen(film, app, ready);
        }
        reload(film, app, &mut Hand::new(CALM));
        if let Some(tempo) = tempo {
            let at = self.time_of(target);
            self.marks.push((at, target, bar_length(tempo)));
        }
    }

    fn swap(&mut self, film: &mut Film, app: &mut App, every: f64, tempo: Option<f64>) {
        let current = self.bar(now(film));
        let mut target = (current / every).ceil() * every;
        if target - current < 0.3 {
            target += every;
        }
        self.swap_at(film, app, target, tempo);
    }
}

fn whole(bar: f64) -> String {
    format!("{bar:.0}")
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
    automate_from(film, app, control, start, (from, to), bars);
}

fn automate_from(
    film: &mut Film,
    app: &mut App,
    control: &str,
    start: f64,
    (from, to): (f64, f64),
    bars: f64,
) {
    let mut hand = Hand::new(TYPING);
    hand.key(film, app, "M->");
    hand.write(
        film,
        app,
        &format!(
            "\n(s/automate :{control} [[{} {from}] [{} {to}]])",
            whole(start),
            whole(start + bars)
        ),
    );
    hand.key(film, app, "C-x C-e");
    settle(app);
}

fn seek(film: &mut Film, app: &mut App, anchor: &str, then: Option<&str>) {
    let mut hand = Hand::new(TYPING);
    match then {
        Some(then) => hand.seek_in(film, app, anchor, then),
        None => hand.seek(film, app, anchor),
    }
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

fn replace_to_quote(film: &mut Film, app: &mut App, text: &str) {
    let mut hand = Hand::new(TYPING);
    hand.key(film, app, "C-SPC C-s");
    hand.write(film, app, "\"");
    hand.key(film, app, "RET C-b C-w");
    hand.write(film, app, text);
}

fn reseed(film: &mut Film, app: &mut App, text: &str) {
    seek(film, app, "(def seed \"", None);
    replace_to_quote(film, app, text);
}

fn arrange(film: &mut Film, app: &mut App, tempo: u32, tracks: &str) {
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
    Hand::new(6.0).write(film, app, KINDLING);
    let mut slow = Hand::new(CALM);
    slow.save(film, app);
    slow.command(film, app, "bellow-play");
    settle(app);
    let mut session = Session {
        marks: vec![(now(film), 0.0, bar_length(76.0))],
    };
    listen(film, app, 8.0);
    add(film, app, GLITCH);
    reseed(film, app, "d4 . . f4 | eb4 _ . d4 | . . f4 . | eb4 . c4 .");
    arrange(film, app, 76, "wind hum anvil glitch chain");
    session.swap(film, app, 2.0, None);
    listen(film, app, 5.0);
    session
}

fn riff(film: &mut Film, app: &mut App, session: &mut Session) {
    film.mark("riff");
    add(film, app, RIFF);
    arrange(
        film,
        app,
        76,
        "hum anvil glitch chain\nkick snare toms bass",
    );
    session.swap(film, app, 4.0, None);
    listen(film, app, 7.0);
    automate(film, app, session, "growl", 600.0, 2600.0, 8.0);
    automate(film, app, session, "fold", 0.2, 0.45, 8.0);
    pattern(
        film,
        app,
        "bass",
        "notes",
        "g1 g1 . g1 . . ab1 . | g1 _ _ . f1 . eb1 .",
    );
    session.swap(film, app, 2.0, None);
    listen(film, app, 4.0);
    add(film, app, FALSETTO);
    arrange(
        film,
        app,
        76,
        "hum anvil glitch chain\nkick snare toms bass high",
    );
    session.swap(film, app, 2.0, None);
    listen(film, app, 6.0);
    pattern(
        film,
        app,
        "bass",
        "notes",
        "d2 . d2 eb2 . d2 f2 . | eb2 _ d2 . c2 . a1 .",
    );
    pattern(
        film,
        app,
        "toms",
        "beats",
        ". . x . . x . x | . x . x x . x x",
    );
    pattern(
        film,
        app,
        "high",
        "notes",
        "a5 _ _ _ | _ _ bb5 _ | a5 _ f5 _ | eb5 _ d5 _",
    );
    session.swap(film, app, 2.0, None);
    listen(film, app, 4.0);
}

fn drive(film: &mut Film, app: &mut App, session: &mut Session) {
    film.mark("drive");
    automate(film, app, session, "fold", 0.45, 0.8, 12.0);
    add(film, app, DRIVE);
    pattern(film, app, "kick", "beats", "X . x . X . x .");
    pattern(film, app, "snare", "beats", ". . X . . . X .");
    pattern(
        film,
        app,
        "bass",
        "notes",
        "f1 f1 f1 f1 f1 f1 gb1 f1 | f1 f1 f1 f1 eb1 eb1 db1 eb1",
    );
    pattern(
        film,
        app,
        "glitch",
        "notes",
        "f5 . d5 . e5 . . . | . . . . . . . .",
    );
    arrange(film, app, 116, "glitch kick snare hat scrap\nbass pad");
    session.swap(film, app, 4.0, Some(116.0));
    listen(film, app, 7.0);
    pattern(film, app, "pad", "chords", "Fm | Gbmaj7 | Ebm | Db");
    pattern(
        film,
        app,
        "glitch",
        "notes",
        "e5 . d5 . f5 . . . | . . . . . . . .",
    );
    session.swap(film, app, 2.0, None);
    automate(film, app, session, "growl", 2600.0, 4000.0, 8.0);
    listen(film, app, 4.0);
}

fn hush(film: &mut Film, app: &mut App, session: &mut Session) {
    film.mark("hush");
    add(film, app, HUSH);
    arrange(film, app, 60, "wind felt glass low air ice");
    session.swap(film, app, 4.0, Some(60.0));
    listen(film, app, 10.0);
}

fn rise(film: &mut Film, app: &mut App, session: &mut Session) {
    film.mark("return");
    add(film, app, BRASS);
    reseed(film, app, "d4 _ _ _ | f4 _ _ _ | eb4 _ _ _ | d4 _ c4 _");
    pattern(
        film,
        app,
        "high",
        "notes",
        "d5 _ _ _ | f5 _ _ _ | eb5 _ _ _ | d5 _ c5 _",
    );
    pattern(
        film,
        app,
        "low",
        "notes",
        "d3 _ _ _ | f3 _ _ _ | eb3 _ _ _ | d3 _ c3 _",
    );
    arrange(film, app, 76, "hum anvil horns high low");
    session.swap(film, app, 4.0, Some(76.0));
    automate(film, app, session, "swell", 0.1, 0.35, 8.0);
    listen(film, app, 5.0);
    pattern(
        film,
        app,
        "kick",
        "beats",
        "X . . . . . x . | X . . x . . . .",
    );
    pattern(film, app, "snare", "beats", ". . . . X . . .");
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
        76,
        "hum anvil horns high low\nkick snare toms bass chain",
    );
    session.swap(film, app, 2.0, None);
    listen(film, app, 6.0);
}

fn wall(film: &mut Film, app: &mut App, session: &mut Session) {
    film.mark("wall");
    above(film, app, "(def room", BROKEN);
    add(film, app, WALL);
    for (anchor, then) in [
        ("(def room", "(s/input)"),
        ("(def drums", "(s/input)"),
        ("(def ducked", "(s/input)"),
    ] {
        seek(film, app, anchor, Some(then));
        Hand::new(TYPING).write(film, app, " broken");
    }
    arrange(
        film,
        app,
        76,
        "hum anvil horns high low\nkick snare toms bass chain wall",
    );
    session.swap(film, app, 2.0, None);
    automate(film, app, session, "wall", 0.0, 0.45, 8.0);
    automate(film, app, session, "fold", 0.8, 1.0, 8.0);
    listen(film, app, 10.0);
}

fn collapse(film: &mut Film, app: &mut App, session: &mut Session) {
    film.mark("collapse");
    let end = ((session.bar(now(film)) + 9.0) / 4.0).ceil() * 4.0;
    automate_from(film, app, "bits", end - 6.0, (16.0, 1.5), 6.0);
    automate_from(film, app, "rate", end - 6.0, (48000.0, 300.0), 6.0);
    add(film, app, EMBER);
    arrange(film, app, 60, "wind ember");
    session.swap_at(film, app, end, Some(60.0));
    listen(film, app, 18.0);
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
    drive(&mut film, &mut app, &mut session);
    report("drive", &film, &app);
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
