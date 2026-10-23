use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use iced::Size;
use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{self, Key, Location, Modifiers};
use iced_test::Simulator;
use oxmacs_gui::app::{App, Message};
use oxmacs_gui::fonts;

const SIZE: Size = Size::new(640.0, 640.0);

const MANIFEST: &str = "(plugin conway
:version \"0.1.0\"
:summary \"Conway's Game of Life on a canvas\"
:commands {conway: \"Watch Conway's Game of Life\"}
:entry \"conway.ox\")";

const LIFE_START: &str = "(module
(import (ox ui)))

(def acorn [[1 0] [3 1] [0 2] [1 2] [4 2] [5 2] [6 2]])
(def offsets [[-1 -1] [0 -1] [1 -1] [-1 0] [1 0] [-1 1] [0 1] [1 1]])
(defonce world (atom {cells: (set []) generation: 0}))

(def (neighbors [x y])
(map (fn [[dx dy]] [(+ x dx) (+ y dy)]) offsets))

(def (step cells)
\"The next generation of a set of live cells.\"
(let [counts (frequencies (mapcat neighbors cells))]
(set (keep (fn [[cell n]]
(when (or (= n 3) (and (= n 2) (contains? cells cell)))
cell))
(entries counts)))))
";

const BLINKER: &str = "(step (set [[0 1] [1 1] [2 1]]))";

const LIFE_END: &str = "(def (draw)
(let [now @world]
(ui/column
(ui/text (str \"Generation \" now.generation \"   \" (count now.cells) \" alive\") :face :heading)
(ui/canvas (vec (map (fn [[x y]] [:rect (* x 0.0125) (* y 9) 0.011 8 :success]) now.cells))
:height 540 :relative true))))

(defcommand conway \"Watch Conway's Game of Life\"
(reset! world {cells: (set (map (fn [[x y]] [(+ x 36) (+ y 26)]) acorn)) generation: 0})
(open-view \"*Conway*\" draw :keys {\"q\" 'quit-window})
(every 100 (fn []
(if (buffer-exists? \"*Conway*\")
(do (swap! world (fn [now] {cells: (step now.cells) generation: (inc now.generation)}))
nil)
:stop))))";

fn settings() -> iced::Settings {
    iced::Settings {
        fonts: vec![fonts::JETBRAINS_MONO_REGULAR.into()],
        default_font: fonts::EDITOR,
        antialiasing: true,
        ..iced::Settings::default()
    }
}

struct Film {
    folder: PathBuf,
    frames: Vec<(String, f64)>,
    seed: u64,
}

impl Film {
    fn new(folder: PathBuf) -> Self {
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        Self {
            folder,
            frames: Vec::new(),
            seed: 7,
        }
    }

    fn shot(&mut self, app: &mut App, seconds: f64) {
        let mut ui = Simulator::with_size(settings(), SIZE, app.view());
        let snapshot = ui.snapshot(&app.theme()).unwrap();
        let stem = format!("frame-{:05}", self.frames.len());
        snapshot.matches_image(self.folder.join(&stem)).unwrap();
        for message in ui.into_messages() {
            let _ = app.update(message);
        }
        self.frames.push((stem, seconds));
    }

    fn mark(&self, scene: &str) {
        let _ = fs::write(
            self.folder.join(format!("scene-{scene}.txt")),
            self.frames.len().to_string(),
        );
    }

    fn hold(&mut self, seconds: f64) {
        if let Some(last) = self.frames.last_mut() {
            last.1 += seconds;
        }
    }

    fn jitter(&mut self) -> f64 {
        self.seed = self
            .seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        f64::from(u32::try_from(self.seed >> 40).unwrap_or(0) % 1000) / 1000.0
    }

    fn press(&mut self, app: &mut App, notation: &str) {
        for chord in notation.split(' ') {
            keys(app, chord);
            settle(app);
            let pause = 0.12 + 0.18 * self.jitter();
            self.shot(app, pause);
        }
    }

    fn type_slowly(&mut self, app: &mut App, text: &str) {
        for c in text.chars() {
            type_char(app, c);
            settle(app);
            let pause = 0.07 + 0.09 * self.jitter();
            self.shot(app, pause);
        }
    }

    fn type_code(&mut self, app: &mut App, code: &str) {
        let mut pending = 0;
        for line in code.split('\n') {
            for c in line.chars() {
                type_char(app, c);
                pending += 1;
                if pending == 3 {
                    settle(app);
                    let pause = 0.07 + 0.03 * self.jitter();
                    self.shot(app, pause);
                    pending = 0;
                }
            }
            settle(app);
            let pause = 0.08 + 0.1 * self.jitter();
            self.shot(app, pause);
            pending = 0;
            if app.editor().pane().popup().is_some() {
                keys(app, "C-g");
            }
            let lines = app.editor().buffer().len_lines();
            keys(app, "RET");
            assert_eq!(
                app.editor().buffer().len_lines(),
                lines + 1,
                "RET after {line:?} added no line"
            );
        }
        settle(app);
        self.shot(app, 0.1);
    }

    fn finish(&self, out: &Path) {
        let names: Vec<String> = fs::read_dir(&self.folder)
            .unwrap()
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .collect();
        let mut list = String::from("ffconcat version 1.0\n");
        let mut closing = String::new();
        for (stem, seconds) in &self.frames {
            let file = names
                .iter()
                .find(|name| name.starts_with(&format!("{stem}-")))
                .unwrap();
            let _ = writeln!(list, "file '{file}'\nduration {seconds:.3}");
            closing.clone_from(file);
        }
        let _ = writeln!(list, "file '{closing}'");
        let listed = self.folder.join("frames.txt");
        fs::write(&listed, list).unwrap();
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-loglevel",
                "error",
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
            ])
            .arg(&listed)
            .args([
                "-vf",
                "fps=30,format=yuv420p",
                "-c:v",
                "libx264",
                "-preset",
                "slow",
                "-crf",
                "18",
                "-movflags",
                "+faststart",
            ])
            .arg(out)
            .status()
            .unwrap();
        assert!(status.success());
    }
}

fn key(app: &mut App, key: Key, text: Option<&str>, modifiers: Modifiers) {
    let _ = app.update(Message::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: Physical::Code(Code::KeyA),
        location: Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    }));
}

fn keys(app: &mut App, chord: &str) {
    let mut modifiers = Modifiers::empty();
    let mut rest = chord;
    loop {
        if let Some(after) = rest.strip_prefix("C-") {
            modifiers |= Modifiers::CTRL;
            rest = after;
        } else if let Some(after) = rest.strip_prefix("M-") {
            modifiers |= Modifiers::ALT;
            rest = after;
        } else {
            break;
        }
    }
    let named = match rest {
        "RET" => Some(Named::Enter),
        "TAB" => Some(Named::Tab),
        "SPC" => Some(Named::Space),
        "DEL" => Some(Named::Backspace),
        "ESC" => Some(Named::Escape),
        _ => None,
    };
    match named {
        Some(named) => key(app, Key::Named(named), None, modifiers),
        None => key(app, Key::Character(rest.into()), Some(rest), modifiers),
    }
}

fn type_char(app: &mut App, c: char) {
    if c == '\n' {
        keys(app, "RET");
        return;
    }
    let text = c.to_string();
    key(
        app,
        Key::Character(text.as_str().into()),
        Some(&text),
        Modifiers::empty(),
    );
}

fn settle(app: &mut App) {
    for _ in 0..3 {
        let _ = app.update(Message::OxTick);
    }
}

fn wait(app: &mut App, what: &str, done: impl Fn(&App) -> bool) {
    for _ in 0..3000 {
        let _ = app.update(Message::OxTick);
        if done(app) {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!(
        "timed out waiting for {what}: {:?} in {}",
        app.editor().message(),
        app.editor().buffer().name()
    );
}

fn git(folder: &Path, arguments: &[&str]) {
    let done = Command::new("git")
        .args(arguments)
        .current_dir(folder)
        .output()
        .unwrap();
    assert!(
        done.status.success(),
        "{}",
        String::from_utf8_lossy(&done.stderr)
    );
}

fn demo_home() -> PathBuf {
    let user = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap();
    let home = user.join("oxmacs-demo");
    if home.join(".oxmacs-demo").exists() {
        fs::remove_dir_all(&home).unwrap();
    }
    assert!(
        !home.exists(),
        "{} exists and wasn't made by this demo",
        home.display()
    );
    fs::create_dir_all(home.join("plugins").join("conway")).unwrap();
    fs::write(home.join(".oxmacs-demo"), "").unwrap();
    fs::write(
        home.join(".gitignore"),
        ".oxmacs-demo\ntutorial.txt\ntutorial.place\n",
    )
    .unwrap();
    fs::write(
        home.join("init.ox"),
        "(settings {theme: :adwaita-dark mode-line-style: :powerline animations: false font-size: 11})\n",
    )
    .unwrap();
    git(&home, &["init", "-q", "-b", "main"]);
    git(&home, &["config", "user.name", "Ada Lovelace"]);
    git(&home, &["config", "user.email", "ada@example.com"]);
    git(&home, &["config", "commit.gpgsign", "false"]);
    git(&home, &["add", "."]);
    git(&home, &["commit", "-q", "-m", "Start my oxmacs config"]);
    fs::write(home.join("README.md"), README).unwrap();
    calendar(&home);
    home
}

const README: &str = "# My oxmacs setup

Notes on how I use **oxmacs**, kept next to my config.

## Plugins

- `conway` plays the Game of Life
- the calendar keeps my agenda in `.ics` files
- the Ox manual is one key away, on `C-h i`

## A key I like

```
(bind-key \"C-c c\" 'calendar)
```
";

fn calendar(home: &Path) {
    let folder = home.join("calendar").join("Personal");
    fs::create_dir_all(&folder).unwrap();
    let today = jiff::Zoned::now().date();
    let at = |days: i64, hour: i8, minute: i8| {
        today
            .checked_add(jiff::Span::new().days(days))
            .unwrap()
            .at(hour, minute, 0, 0)
            .to_zoned(jiff::tz::TimeZone::system())
            .unwrap()
            .timestamp()
            .strftime("%Y%m%dT%H%M%SZ")
            .to_string()
    };
    let day = |days: i64| {
        today
            .checked_add(jiff::Span::new().days(days))
            .unwrap()
            .strftime("%Y%m%d")
            .to_string()
    };
    let events = [
        (
            "standup",
            format!(
                "DTSTART:{}\r\nDTEND:{}\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR\r\n",
                at(0, 9, 30),
                at(0, 9, 45)
            ),
            "Team standup",
        ),
        (
            "lunch",
            format!("DTSTART:{}\r\nDTEND:{}\r\n", at(1, 12, 30), at(1, 13, 30)),
            "Lunch with Bo",
        ),
        (
            "review",
            format!("DTSTART:{}\r\nDTEND:{}\r\n", at(2, 15, 0), at(2, 16, 0)),
            "Plugin review",
        ),
        (
            "release",
            format!(
                "DTSTART;VALUE=DATE:{}\r\nDTEND;VALUE=DATE:{}\r\n",
                day(4),
                day(5)
            ),
            "Release day",
        ),
    ];
    for (uid, times, summary) in events {
        fs::write(
            folder.join(format!("{uid}.ics")),
            format!(
                "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:{uid}@example.com\r\n{times}SUMMARY:{summary}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
            ),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "renders a demo video with ffmpeg into $OXMACS_VIDEO"]
fn demo_video() {
    let Ok(out) = std::env::var("OXMACS_VIDEO") else {
        return;
    };
    let out = PathBuf::from(out);
    let home = demo_home();
    let mut film = Film::new(out.with_extension("frames"));
    let (mut app, _) = App::boot(&[], Some(&home));
    settle(&mut app);
    film.shot(&mut app, 2.5);
    show_tutorial(&mut film, &mut app);
    write_plugin(&mut film, &mut app);
    try_plugin(&mut film, &mut app);
    read_manual(&mut film, &mut app);
    preview_markdown(&mut film, &mut app);
    browse_files(&mut film, &mut app);
    commit_plugin(&mut film, &mut app);
    show_calendar(&mut film, &mut app);
    show_matrix(&mut film, &mut app);
    film.finish(&out);
}

fn show_tutorial(film: &mut Film, app: &mut App) {
    film.mark("tutorial");
    film.press(app, "C-h t");
    wait(app, "the tutorial", |app| {
        app.editor().buffer().name() == "tutorial.txt"
    });
    film.shot(app, 2.5);
    film.press(app, "C-v");
    film.hold(1.5);
    film.press(app, "C-x k RET");
    film.hold(0.5);
}

fn read_manual(film: &mut Film, app: &mut App) {
    film.mark("manual");
    film.press(app, "C-h i");
    wait(app, "the manual", |app| {
        app.editor().buffer().name() == "*Ox manual*"
    });
    film.shot(app, 2.5);
    film.press(app, "TAB TAB");
    film.hold(0.6);
    film.press(app, "RET");
    wait(app, "the language page", |app| {
        app.editor().buffer().contents().starts_with("The language")
    });
    film.shot(app, 2.0);
    film.press(app, "C-s");
    film.type_slowly(app, "(let [name");
    film.press(app, "RET");
    film.hold(0.8);
    film.mark("example");
    film.press(app, "x");
    wait(app, "the example to run", |app| {
        app.editor()
            .buffer_named("*ox repl*")
            .and_then(|id| app.editor().buffer_by_id(id))
            .is_some_and(|buffer| buffer.contents().contains("new messages"))
    });
    film.shot(app, 2.5);
    film.mark("lookup");
    film.press(app, "i");
    film.type_slowly(app, "frequencies");
    film.press(app, "RET");
    wait(app, "the reference", |app| {
        app.editor().buffer().contents().starts_with("Core library")
    });
    film.shot(app, 3.0);
    film.press(app, "C-x 1");
    film.press(app, "q");
}

fn preview_markdown(film: &mut Film, app: &mut App) {
    film.mark("markdown");
    film.press(app, "C-x C-f");
    film.type_slowly(app, "~/oxmacs-demo/README.md");
    film.press(app, "RET");
    film.hold(0.8);
    film.press(app, "M-x");
    film.type_slowly(app, "markdown-preview");
    film.press(app, "RET");
    wait(app, "the preview", |app| {
        app.editor()
            .buffer_named("*Markdown: README.md*")
            .is_some_and(|id| app.editor().buffer_shown(id))
    });
    film.shot(app, 3.0);
    film.press(app, "C-x 1");
}

fn browse_files(film: &mut Film, app: &mut App) {
    film.mark("dired");
    film.press(app, "C-x d");
    film.hold(0.5);
    film.press(app, "RET");
    wait(app, "the listing", |app| {
        app.editor().buffer().contents().contains("README.md")
    });
    film.shot(app, 2.0);
    film.press(app, "n n");
    film.hold(0.8);
    film.press(app, "q");
}

fn show_calendar(film: &mut Film, app: &mut App) {
    film.mark("calendar");
    film.press(app, "M-x");
    film.type_slowly(app, "calendar");
    film.press(app, "RET");
    wait(app, "the agenda", |app| {
        app.editor().buffer().name() == "*Calendar*"
            && app.editor().buffer().contents().contains("Lunch with Bo")
    });
    film.shot(app, 3.0);
    film.press(app, "m");
    wait(app, "the month", |app| {
        app.editor().buffer().name() == "*Calendar month*"
    });
    for _ in 0..40 {
        thread::sleep(Duration::from_millis(10));
        settle(app);
    }
    film.shot(app, 2.5);
    film.press(app, "q");
    film.press(app, "q");
}

fn write_plugin(film: &mut Film, app: &mut App) {
    film.press(app, "C-x C-f");
    film.hold(0.6);
    film.type_slowly(app, "~/oxmacs-demo/plugins/conway/plugin.ox");
    film.hold(0.4);
    film.press(app, "RET");
    film.type_code(app, MANIFEST);
    film.press(app, "C-x C-s");
    film.hold(1.2);

    film.press(app, "C-x C-f");
    film.type_slowly(app, "conway.ox");
    film.press(app, "RET");
    film.type_code(app, LIFE_START);
    film.type_code(app, LIFE_END);
    film.mark("load");
    film.press(app, "C-c C-k");
    wait(app, "the plugin to start", |app| {
        app.editor()
            .message()
            .is_some_and(|text| text.starts_with("Started the conway plugin"))
    });
    film.shot(app, 2.5);
    film.type_slowly(app, BLINKER);
    film.press(app, "C-x C-e");
    for _ in 0..40 {
        thread::sleep(Duration::from_millis(10));
        settle(app);
    }
    film.mark("blinker");
    film.shot(app, 2.5);
    film.press(app, "C-a C-k");
    film.press(app, "C-r");
    film.type_slowly(app, "frequencies");
    film.press(app, "RET");
    film.mark("docs");
    film.press(app, "C-c C-d");
    wait(app, "the docs", |app| {
        app.editor().buffer_named("*ox doc*").is_some()
    });
    film.shot(app, 3.0);
    film.press(app, "q");
    assert_eq!(app.editor().buffer().name(), "conway.ox");
}

fn try_plugin(film: &mut Film, app: &mut App) {
    film.press(app, "M-x");
    film.type_slowly(app, "conway");
    film.hold(0.3);
    film.press(app, "RET");
    wait(app, "the Life view", |app| {
        app.editor().buffer().name() == "*Conway*"
    });
    film.mark("life");
    for _ in 0..80 {
        thread::sleep(Duration::from_millis(100));
        settle(app);
        film.shot(app, 0.1);
    }
    film.press(app, "q");
}

fn commit_plugin(film: &mut Film, app: &mut App) {
    film.mark("git");
    film.press(app, "C-x g");
    wait(app, "git status", |app| {
        app.editor().buffer().contents().contains("Untracked")
    });
    film.shot(app, 2.0);
    film.press(app, "C-s");
    film.type_slowly(app, "plugins/");
    film.press(app, "RET");
    film.hold(0.4);
    film.press(app, "s");
    wait(app, "staging", |app| {
        app.editor().buffer().contents().contains("Staged")
    });
    film.shot(app, 1.5);
    film.press(app, "c");
    film.hold(0.8);
    film.press(app, "c");
    wait(app, "the commit message", |app| {
        app.editor().buffer().name().contains("COMMIT_EDITMSG")
    });
    film.shot(app, 1.2);
    film.type_slowly(app, "Add a Game of Life plugin");
    film.hold(0.6);
    film.mark("commit");
    film.press(app, "C-c C-c");
    wait(app, "the commit", |app| {
        app.editor().buffer().name().starts_with("git:")
            && app
                .editor()
                .buffer()
                .contents()
                .contains("Add a Game of Life plugin")
    });
    film.shot(app, 3.0);
    film.press(app, "C-x 1");
}

fn show_matrix(film: &mut Film, app: &mut App) {
    film.mark("matrix");
    film.press(app, "M-x");
    film.type_slowly(app, "screensaver-preview");
    film.press(app, "RET");
    wait(app, "the screensaver choice", |app| {
        app.editor().minibuffer().is_some()
    });
    film.type_slowly(app, "matrix");
    film.press(app, "RET");
    settle(app);
    let start = iced::time::Instant::now();
    for frame in 0..150u32 {
        let _ = app.update(Message::Tick(
            start + Duration::from_millis(u64::from(frame) * 66),
        ));
        film.shot(app, 0.066);
    }
    film.press(app, "q");
    film.hold(1.5);
}
