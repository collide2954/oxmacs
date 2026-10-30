use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use iced::Size;
use oxmacs_gui::app::App;

use super::{FRAME, Film, PUMPING, keys, run, settle, tank_app, tank_home, tap_letter, type_char};

#[test]
#[ignore = "writes frames of the finished fish tank from $OXMACS_TANK_SOURCE into $OXMACS_TANK_SHOTS"]
fn tank_shots() {
    let (Ok(source), Ok(out)) = (
        std::env::var("OXMACS_TANK_SOURCE"),
        std::env::var("OXMACS_TANK_SHOTS"),
    ) else {
        return;
    };
    let home = tank_home(&fs::read_to_string(source).unwrap());
    PUMPING.set(true);
    let mut film = Film::new(PathBuf::from(out))
        .sized(Size::new(1280.0, 720.0))
        .ticking(Instant::now());
    let mut app = tank_app(&home);
    run(&mut film, &mut app, "rodeo-play");
    settle(&mut app);
    film.play(&mut app, 3.0);
    tap_letter(&mut film, &mut app, "f");
    film.play(&mut app, 6.0);
    tap_letter(&mut film, &mut app, "n");
    film.play(&mut app, 4.0);
}

const TANK: &str = include_str!("../demos/tank.ox");

fn form(head: &str) -> String {
    let lines: Vec<&str> = TANK.lines().collect();
    let start = lines
        .iter()
        .position(|line| line.starts_with(head))
        .unwrap_or_else(|| panic!("tank.ox has no form starting {head}"));
    lines[start..]
        .iter()
        .take_while(|line| !line.trim().is_empty())
        .map(|line| line.trim_start())
        .collect::<Vec<_>>()
        .join("\n")
}

fn forms(heads: &[&str]) -> String {
    heads.iter().map(|head| form(head) + "\n\n").collect()
}

type Stage = fn(&mut Film, &mut App);

pub(super) const RUSH: f64 = 6.0;
pub(super) const CALM: f64 = 1.6;

const KEYBOARD: [&str; 3] = ["qwertyuiop", "asdfghjkl", "zxcvbnm"];

fn beside(c: char, pick: f64) -> char {
    let lower = c.to_ascii_lowercase();
    for row in KEYBOARD {
        if let Some(at) = row.find(lower) {
            let keys: Vec<char> = row.chars().collect();
            let left = at.checked_sub(1).map(|left| keys[left]);
            let right = keys.get(at + 1).copied();
            let chosen = match (left, right) {
                (Some(left), Some(right)) => {
                    if pick < 0.5 {
                        left
                    } else {
                        right
                    }
                }
                (Some(only), None) | (None, Some(only)) => only,
                (None, None) => lower,
            };
            return if c.is_ascii_uppercase() {
                chosen.to_ascii_uppercase()
            } else {
                chosen
            };
        }
    }
    c
}

pub(super) struct Hand {
    fast: f64,
    pending: f64,
    since_typo: usize,
}

impl Hand {
    pub(super) fn new(fast: f64) -> Self {
        Self {
            fast,
            pending: 0.0,
            since_typo: 0,
        }
    }

    pub(super) fn wait(&mut self, film: &mut Film, app: &mut App, seconds: f64) {
        self.pending += seconds / self.fast;
        if self.pending >= FRAME {
            settle(app);
            film.shot(app, self.pending);
            self.pending = 0.0;
        }
    }

    pub(super) fn flush(&mut self, film: &mut Film, app: &mut App) {
        settle(app);
        film.shot(app, self.pending.max(FRAME));
        self.pending = 0.0;
    }

    pub(super) fn key(&mut self, film: &mut Film, app: &mut App, notation: &str) {
        for chord in notation.split(' ') {
            keys(app, chord);
            let pause = 0.15 + 0.25 * film.jitter();
            self.wait(film, app, pause);
        }
    }

    fn slip(&mut self, film: &mut Film, app: &mut App, word: &[char]) {
        let wrong = beside(word[0], film.jitter());
        type_char(app, wrong);
        self.wait(film, app, 0.09);
        let carried = if film.jitter() < 0.45 {
            word.len().min(3) - 1
        } else {
            0
        };
        for c in &word[1..=carried] {
            type_char(app, *c);
            let pause = 0.07 + 0.05 * film.jitter();
            self.wait(film, app, pause);
        }
        let noticed = 0.3 + 0.4 * film.jitter();
        self.wait(film, app, noticed);
        for _ in 0..=carried {
            keys(app, "DEL");
            self.wait(film, app, 0.12);
        }
        self.since_typo = 0;
    }

    fn line_break(&mut self, film: &mut Film, app: &mut App, rest: &str) {
        settle(app);
        if app.editor().pane().popup().is_some() {
            keys(app, "C-g");
        }
        keys(app, "RET");
        let mut pause = 0.2 + 0.3 * film.jitter();
        if rest.starts_with('\n') || rest.starts_with("(def") {
            pause += 0.5 + 1.3 * film.jitter();
        }
        self.wait(film, app, pause);
    }

    pub(super) fn write(&mut self, film: &mut Film, app: &mut App, text: &str) {
        let chars: Vec<char> = text.chars().collect();
        let mut at = 0;
        while at < chars.len() {
            let c = chars[at];
            if c == '\n' {
                let rest: String = chars[at + 1..].iter().collect();
                self.line_break(film, app, &rest);
                at += 1;
                continue;
            }
            let word: Vec<char> = chars[at..]
                .iter()
                .copied()
                .take_while(char::is_ascii_alphabetic)
                .collect();
            let starts_word = at == 0 || !chars[at - 1].is_ascii_alphabetic();
            if starts_word && word.len() >= 3 && self.since_typo > 40 && film.jitter() < 0.06 {
                self.slip(film, app, &word);
            }
            type_char(app, c);
            self.since_typo += 1;
            let mut pause = 0.06 + 0.08 * film.jitter();
            if c == ' ' {
                pause += 0.04 * film.jitter();
            }
            self.wait(film, app, pause);
            at += 1;
        }
        self.flush(film, app);
    }

    pub(super) fn seek(&mut self, film: &mut Film, app: &mut App, text: &str) {
        self.key(film, app, "M-<");
        self.key(film, app, "C-s");
        self.write(film, app, text);
        self.key(film, app, "RET");
    }

    pub(super) fn seek_in(&mut self, film: &mut Film, app: &mut App, anchor: &str, text: &str) {
        self.seek(film, app, anchor);
        self.key(film, app, "C-s");
        self.write(film, app, text);
        self.key(film, app, "RET");
    }

    pub(super) fn command(&mut self, film: &mut Film, app: &mut App, name: &str) {
        self.key(film, app, "M-x");
        self.write(film, app, name);
        self.key(film, app, "RET");
    }

    pub(super) fn save(&mut self, film: &mut Film, app: &mut App) {
        self.key(film, app, "C-x C-s");
        settle(app);
    }
}

fn above(film: &mut Film, app: &mut App, anchor: &str, text: &str) {
    let mut slow = Hand::new(CALM);
    slow.seek(film, app, anchor);
    slow.key(film, app, "C-a");
    Hand::new(RUSH).write(film, app, text);
}

fn to_game(film: &mut Film, app: &mut App, hand: &mut Hand, wanted: bool) {
    let there = app.editor().buffer().name() == "*game*";
    if there != wanted {
        hand.key(film, app, "C-x o");
    }
}

fn game_paused(app: &App) -> bool {
    app.editor()
        .buffer_named("*game*")
        .and_then(|id| oxmacs_core::view::game::screen(app.editor(), id))
        .is_some_and(|screen| screen.status.is_some())
}

fn water(film: &mut Film, app: &mut App) {
    film.mark("water");
    film.shot(app, 1.5);
    let start = format!(
        "(module (import (ox rodeo) (ox math)))\n\n(rodeo/screen 640 480 :smooth true)\n\n(def W 640)\n(def H 480)\n(def sand 410)\n\n(defonce light (atom 1))\n\n{}{}\n\n(def (draw)\n(let [t (rodeo/time)]\n(water)))",
        forms(&["(def (mix", "(def (lit"]),
        form("(def (water)").replace("rodeo/rect", "rodeo/rcet"),
    );
    Hand::new(RUSH).write(film, app, &start);
    let mut slow = Hand::new(CALM);
    slow.save(film, app);
    slow.wait(film, app, 0.8);
    slow.command(film, app, "rodeo-play");
    film.play(app, 2.5);
    to_game(film, app, &mut slow, false);
    slow.seek(film, app, "rcet");
    for _ in 0..4 {
        slow.key(film, app, "DEL");
    }
    slow.write(film, app, "rect");
    slow.save(film, app);
    film.play(app, 0.8);
    if game_paused(app) {
        slow.command(film, app, "rodeo-pause");
    }
    film.play(app, 2.5);
}

fn sand(film: &mut Film, app: &mut App) {
    film.mark("sand");
    let mut slow = Hand::new(CALM);
    slow.seek(film, app, "(ox math)");
    slow.write(film, app, " (ox random) (ox geometry :as v)");
    slow.seek(film, app, "(def sand 410)");
    slow.write(film, app, "\n(def tau 6.28318)\n(def seed (random/new 42))");
    let bed = "(def (bed t)\n(rodeo/polygon (into dunes [[W H] [0 H]]) (lit [198 176 132])))\n\n";
    let text = forms(&["(def (wave", "(def (surface", "(def dunes", "(def pebbles"])
        + bed
        + &forms(&["(def (front-sand)"]);
    above(film, app, "(def (draw)", &text);
    slow.seek_in(film, app, "(def (draw)", "(water)");
    slow.write(film, app, "\n(bed t)\n(front-sand)\n(surface t)");
    slow.save(film, app);
    film.play(app, 3.0);
}

fn rocks(film: &mut Film, app: &mut App) {
    film.mark("rocks");
    let text = forms(&["(def (rock", "(def rocks", "(def (toward", "(def (stones)"]);
    above(film, app, "(def (front-sand)", &text);
    let mut slow = Hand::new(CALM);
    slow.seek_in(film, app, "(def (draw)", "(bed t)");
    slow.write(film, app, "\n(stones)");
    slow.save(film, app);
    film.play(app, 2.0);
}

fn plants(film: &mut Film, app: &mut App) {
    film.mark("plants");
    above(
        film,
        app,
        "(def (draw)",
        &forms(&["(def weeds", "(def (blade"]),
    );
    let mut slow = Hand::new(CALM);
    slow.seek_in(film, app, "(def (draw)", "(water)");
    slow.write(film, app, "\n(each [p (take 12 weeds)] (blade p t 0.45))");
    slow.seek_in(film, app, "(def (draw)", "(front-sand)");
    slow.write(film, app, "\n(each [p (drop 12 weeds)] (blade p t 0))");
    slow.save(film, app);
    film.play(app, 4.0);
}

fn light(film: &mut Film, app: &mut App) {
    film.mark("light");
    let rays = forms(&["(def (rays"]).replace("(+ 12 (* 6", "(+ 40 (* 6");
    above(film, app, "(def dunes", &rays);
    let mut slow = Hand::new(CALM);
    slow.seek_in(film, app, "(def (draw)", "(water)");
    slow.write(film, app, "\n(rays t)");
    slow.seek(film, app, "(lit [198 176 132]))");
    let caustics: Vec<&str> = TANK
        .lines()
        .skip_while(|line| !line.starts_with("(def (bed t)"))
        .skip(2)
        .take(4)
        .map(str::trim_start)
        .collect();
    let caustics = caustics.join("\n");
    let opened = caustics.trim_end_matches(')');
    let closing = caustics.len() - opened.len() - 1;
    Hand::new(RUSH).write(film, app, &format!("\n{opened}{}", ")".repeat(closing)));
    slow.save(film, app);
    film.play(app, 4.0);
    slow.seek(film, app, "(+ 40");
    slow.key(film, app, "DEL");
    slow.key(film, app, "DEL");
    slow.write(film, app, "12");
    slow.save(film, app);
    film.play(app, 3.5);
}

fn fish(film: &mut Film, app: &mut App) {
    film.mark("fish");
    let text = forms(&["(def kinds", "(def (half", "(def (place", "(def (fish f t)"]);
    above(film, app, "(def (draw)", &text);
    let mut slow = Hand::new(CALM);
    slow.seek_in(film, app, "(def (draw)", "(blade p t 0))");
    slow.write(
        film,
        app,
        "\n(fish {kind: :angel at: [320 220] face: 1 tilt: 0 phase: 0} t)",
    );
    slow.save(film, app);
    film.play(app, 3.0);
    slow.seek(film, app, ":angel at");
    slow.key(film, app, "M-DEL");
    slow.key(film, app, "M-DEL");
    slow.write(film, app, "gourami at");
    slow.save(film, app);
    film.play(app, 2.5);
}

fn school(film: &mut Film, app: &mut App) {
    film.mark("school");
    let pick = "(def (pick-goal f)\n(let [bottom (= f.kind :cory)]\n(cond\n[(or (< (v/distance f.at f.goal) 16) (random/chance? 0.005 :from seed))\n(assoc f :goal [(random/range 40 600 :from seed)\n(if bottom (random/range 392 402 :from seed) (random/range 80 360 :from seed))])]\n[else f])))\n\n";
    let tick =
        "(def (tick)\n(let [all @school]\n(reset! school (vec (for [f all] (swim f all))))))\n\n";
    let text = forms(&["(def (fish-of", "(defonce school"])
        + pick
        + &forms(&["(def (flock", "(def (swim"])
        + tick;
    above(film, app, "(def (draw)", &text);
    let mut slow = Hand::new(CALM);
    slow.seek_in(film, app, "(def (draw)", "(fish {kind:");
    slow.key(film, app, "C-a");
    slow.key(film, app, "C-k");
    slow.key(film, app, "TAB");
    slow.write(film, app, "(each [f @school] (fish f t))");
    slow.save(film, app);
    film.play(app, 6.0);
}

fn bubbles(film: &mut Film, app: &mut App) {
    film.mark("bubbles");
    above(
        film,
        app,
        "(def (pick-goal",
        "(defonce bubbles (atom []))\n\n",
    );
    above(
        film,
        app,
        "(def (tick)",
        &forms(&["(def stone", "(def (bubble)", "(def (lift"]),
    );
    let mut slow = Hand::new(CALM);
    slow.seek(film, app, "(swim f all)))))");
    Hand::new(RUSH).write(
        film,
        app,
        "\n(when (random/chance? 0.3 :from seed) (swap! bubbles conj (bubble)))\n(swap! bubbles (fn [all] (vec (for [b all :when (> (second b.at) 34)] (lift b)))))",
    );
    above(film, app, "(def (draw)", &(form("(def (air)") + ")\n\n"));
    slow.seek_in(film, app, "(def (draw)", "(fish f t))");
    slow.write(film, app, "\n(air)");
    slow.save(film, app);
    slow.wait(film, app, 2.2);
    slow.seek(film, app, "[255 255 255 200]))))");
    slow.key(film, app, "DEL");
    slow.save(film, app);
    film.play(app, 4.0);
}

fn food(film: &mut Film, app: &mut App) {
    film.mark("food");
    let mut slow = Hand::new(CALM);
    let mut fast = Hand::new(RUSH);
    slow.seek(film, app, "(rodeo/screen 640 480 :smooth true)");
    slow.write(film, app, "\n(rodeo/actions {feed: [:f]})");
    above(film, app, "(defonce bubbles", "(defonce food (atom []))\n");
    slow.seek(film, app, "(= f.kind :cory)");
    fast.write(
        film,
        app,
        "\nflake (first (sort-by (fn [p] (v/distance f.at p.at)) @food))",
    );
    slow.seek(film, app, "(cond");
    fast.write(
        film,
        app,
        "\n[(and flake (< (v/distance f.at flake.at) 160)) (assoc f :goal flake.at)]",
    );
    above(film, app, "(def stone", &forms(&["(def (eaten?"]));
    above(
        film,
        app,
        "(def (bubble)",
        &forms(&["(def (flake)", "(def (drift"]),
    );
    slow.seek(film, app, "(def (tick)");
    fast.write(
        film,
        app,
        "\n(when (rodeo/pressed? :feed) (swap! food into (for [_ (range 20)] (flake))))",
    );
    slow.seek(film, app, "(swim f all))))");
    fast.write(
        film,
        app,
        "\n(swap! food (fn [flakes] (vec (for [p flakes :when (not (eaten? p all))] (drift p)))))",
    );
    above(film, app, "(def (draw)", &forms(&["(def (flakes)"]));
    slow.seek_in(film, app, "(def (draw)", "(fish f t))");
    slow.write(film, app, "\n(flakes)");
    slow.save(film, app);
    film.play(app, 1.0);
    to_game(film, app, &mut slow, true);
    film.play(app, 0.6);
    tap_letter(film, app, "f");
    film.play(app, 9.0);
    to_game(film, app, &mut slow, false);
}

fn night(film: &mut Film, app: &mut App) {
    film.mark("night");
    let mut slow = Hand::new(CALM);
    slow.seek(film, app, "(defonce light (atom 1))");
    slow.write(film, app, "\n(defonce dark (atom false))");
    slow.seek(film, app, "[:f]");
    slow.write(film, app, " night: [:n]");
    slow.seek(film, app, "(def (tick)");
    slow.write(
        film,
        app,
        "\n(when (rodeo/pressed? :night) (swap! dark not))",
    );
    slow.seek(film, app, "(flake))))");
    slow.write(
        film,
        app,
        "\n(swap! light (fn [l] (+ l (* 0.015 (- (if @dark 0 1) l)))))",
    );
    slow.save(film, app);
    film.play(app, 0.8);
    to_game(film, app, &mut slow, true);
    film.play(app, 0.5);
    tap_letter(film, app, "n");
    film.play(app, 6.0);
    tap_letter(film, app, "n");
    film.play(app, 3.0);
    to_game(film, app, &mut slow, false);
}

fn glass(film: &mut Film, app: &mut App) {
    film.mark("glass");
    above(film, app, "(def (draw)", &forms(&["(def (glass)"]));
    let mut slow = Hand::new(CALM);
    slow.seek_in(film, app, "(def (draw)", "(surface t)");
    slow.write(film, app, "\n(glass)");
    slow.save(film, app);
    film.play(app, 2.0);
}

fn finale(film: &mut Film, app: &mut App) {
    film.mark("finale");
    let mut slow = Hand::new(1.0);
    to_game(film, app, &mut slow, true);
    slow.key(film, app, "C-x 1");
    film.play(app, 5.0);
    tap_letter(film, app, "f");
    film.play(app, 10.0);
    tap_letter(film, app, "n");
    film.play(app, 8.0);
    tap_letter(film, app, "f");
    film.play(app, 6.0);
    tap_letter(film, app, "n");
    film.play(app, 6.0);
}

fn compare(written: &str) {
    let wanted: Vec<&str> = TANK.lines().map(str::trim_end).collect();
    let got: Vec<&str> = written.lines().map(str::trim_end).collect();
    if wanted != got {
        let first = wanted
            .iter()
            .zip(&got)
            .position(|(a, b)| a != b)
            .unwrap_or(wanted.len().min(got.len()));
        eprintln!(
            "tank.ox differs at line {}: wanted {:?}, got {:?}",
            first + 1,
            wanted.get(first),
            got.get(first)
        );
    }
}

#[test]
#[ignore = "renders a 16:9 video of building a fish tank in rodeo from an empty file, with music, into $OXMACS_TANK_VIDEO"]
fn tank_video() {
    let Ok(out) = std::env::var("OXMACS_TANK_VIDEO") else {
        return;
    };
    let out = PathBuf::from(out);
    let dry = std::env::var("OXMACS_TANK_DRY").is_ok();
    let home = tank_home("");
    PUMPING.set(true);
    let mut film = Film::new(out.with_extension("frames"))
        .sized(Size::new(1280.0, 720.0))
        .scaled_to(1280, 720)
        .ticking(Instant::now())
        .dry(dry);
    let mut app = tank_app(&home);
    let stages: Vec<(&str, Stage)> = vec![
        ("water", water),
        ("sand", sand),
        ("rocks", rocks),
        ("plants", plants),
        ("light", light),
        ("fish", fish),
        ("school", school),
        ("bubbles", bubbles),
        ("food", food),
        ("night", night),
        ("glass", glass),
    ];
    let until = std::env::var("OXMACS_TANK_STAGES")
        .ok()
        .and_then(|count| count.parse().ok())
        .unwrap_or(stages.len());
    for (name, stage) in stages.iter().take(until) {
        stage(&mut film, &mut app);
        report(name, &film, &app);
    }
    if until == stages.len() {
        compare(&fs::read_to_string(home.join("tank.ox")).unwrap());
        finale(&mut film, &mut app);
    }
    film.hold(2.0);
    report("end", &film, &app);
    if !dry {
        film.finish_scored(&out);
    }
}

fn report(stage: &str, film: &Film, app: &App) {
    let seconds: f64 = film.frames.iter().map(|(_, seconds)| seconds).sum();
    eprintln!(
        "stage {stage}: {} frames, {seconds:.1}s, in {}, paused {}, message {:?}",
        film.frames.len(),
        app.editor().buffer().name(),
        game_paused(app),
        app.editor().message()
    );
}
