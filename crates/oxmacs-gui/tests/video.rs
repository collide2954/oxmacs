mod sample_book;
mod sample_paper;
mod support;

use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::task::{Context, Poll};
use std::thread;
use std::time::{Duration, Instant};

use iced::futures::stream::BoxStream;
use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{self, Key, Location, Modifiers};
use iced::{Size, Task};
use iced_runtime::Action;
use iced_test::Simulator;
use oxmacs_core::editor::Visit;
use oxmacs_core::view::{Node, ViewBuffer};
use oxmacs_gui::app::{App, Message};

use support::{chord, settings, typed};

thread_local! {
    static PUMPING: Cell<bool> = const { Cell::new(false) };
    static STREAMS: RefCell<Vec<BoxStream<'static, Action<Message>>>> =
        const { RefCell::new(Vec::new()) };
}

fn track(task: Task<Message>) {
    if PUMPING.get()
        && let Some(stream) = iced_runtime::task::into_stream(task)
    {
        STREAMS.with_borrow_mut(|streams| streams.push(stream));
    }
}

fn pump(app: &mut App) {
    if !PUMPING.get() {
        return;
    }
    let waker = iced::futures::task::noop_waker();
    let mut context = Context::from_waker(&waker);
    loop {
        let mut arrived = Vec::new();
        let mut kept = Vec::new();
        for mut stream in STREAMS.take() {
            loop {
                match stream.as_mut().poll_next(&mut context) {
                    Poll::Ready(Some(Action::Output(message))) => arrived.push(message),
                    Poll::Ready(Some(_)) => {}
                    Poll::Ready(None) => break,
                    Poll::Pending => {
                        kept.push(stream);
                        break;
                    }
                }
            }
        }
        STREAMS.with_borrow_mut(|streams| streams.extend(kept));
        if arrived.is_empty() {
            return;
        }
        for message in arrived {
            track(app.update(message));
        }
    }
}

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

struct Film {
    folder: PathBuf,
    frames: Vec<(String, f64)>,
    seed: u64,
    size: Size,
    uploads: usize,
    output: Option<(u32, u32)>,
    pace: f64,
    clock: Option<Instant>,
}

impl Film {
    fn new(folder: PathBuf) -> Self {
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        Self {
            folder,
            frames: Vec::new(),
            seed: 7,
            size: SIZE,
            uploads: 0,
            output: None,
            pace: 1.0,
            clock: None,
        }
    }

    fn ticking(self, start: Instant) -> Self {
        Self {
            clock: Some(start),
            ..self
        }
    }

    fn sized(self, size: Size) -> Self {
        Self { size, ..self }
    }

    fn paced(self, pace: f64) -> Self {
        Self { pace, ..self }
    }

    fn warmed(self, uploads: usize) -> Self {
        Self { uploads, ..self }
    }

    fn scaled_to(self, width: u32, height: u32) -> Self {
        Self {
            output: Some((width, height)),
            ..self
        }
    }

    fn shot(&mut self, app: &mut App, seconds: f64) {
        let mut ui = Simulator::with_size(settings(), self.size, app.view());
        for _ in 0..self.uploads {
            let _ = ui.snapshot(&app.theme());
        }
        let snapshot = ui.snapshot(&app.theme()).unwrap();
        let stem = format!("frame-{:05}", self.frames.len());
        snapshot.matches_image(self.folder.join(&stem)).unwrap();
        for message in ui.into_messages() {
            track(app.update(message));
        }
        self.frames.push((stem, seconds));
        if let Some(clock) = &mut self.clock {
            *clock += Duration::from_secs_f64(seconds);
            let now = *clock;
            track(app.update(Message::Tick(now)));
        }
    }

    fn mark(&self, scene: &str) {
        let _ = fs::write(
            self.folder.join(format!("scene-{scene}.txt")),
            self.frames.len().to_string(),
        );
    }

    fn hold(&mut self, seconds: f64) {
        if let Some(last) = self.frames.last_mut() {
            last.1 += seconds * self.pace;
        }
    }

    fn play(&mut self, app: &mut App, seconds: f64) {
        let mut left = seconds;
        while left > 0.0 {
            self.shot(app, 1.0 / 30.0);
            left -= 1.0 / 30.0;
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
        let filters = match self.output {
            Some((width, height)) => {
                format!("fps=30,scale={width}:{height}:flags=lanczos,format=yuv420p")
            }
            None => "fps=30,format=yuv420p".to_owned(),
        };
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
            .arg("-vf")
            .arg(&filters)
            .args([
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

fn keys(app: &mut App, notation: &str) {
    track(app.update(chord(notation)));
}

fn type_char(app: &mut App, c: char) {
    track(app.update(typed(c)));
}

fn settle(app: &mut App) {
    for _ in 0..3 {
        track(app.update(Message::OxTick));
        pump(app);
    }
}

fn wait(app: &mut App, what: &str, done: impl Fn(&App) -> bool) {
    for _ in 0..3000 {
        track(app.update(Message::OxTick));
        pump(app);
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
        "(settings {theme: :adwaita-dark mode-line-style: :powerline animations: false font-size: 11})\n(use-plugin git)\n(use-plugin markdown)\n(use-plugin calendar)\n(use-plugin screensaver)\n(use-plugin keycast)\n",
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

const STUDIO: &str = r##"(module
  (import (ox image) (ox ui)))

(def (look picture)
  picture)

(defonce shown (atom nil))
(defonce reloads (atom 0))

(def stars
  (map (fn [i] [(mod (+ (* i 137) (* i i 11)) 480)
                (+ 6 (mod (+ (* i 71) (* i i 7)) 120))
                (+ 0.8 (* 0.3 (mod i 3)))])
       (range 70)))

(def (scene)
  (image/edit (image/new 480 300 :color "#0b1633")
    (fn [c]
      (map (fn [i] (image/rect! c 0 (+ 60 (* i 15)) 480 300
                                :fill "#24407a" :opacity 0.08))
           (range 12))
      (map (fn [[x y r]] (image/circle! c x y r :fill :white :opacity 0.8)) stars)
      (image/circle! c 370 80 34 :fill "#ffd166")
      (image/circle! c 384 72 30 :fill "#0b1633")
      (image/polygon! c [[0 300] [0 196] [80 160] [180 212] [300 140]
                         [400 198] [480 172] [480 300]] :fill "#2a9d8f")
      (image/polygon! c [[0 300] [0 244] [130 206] [250 252] [370 218]
                         [480 246] [480 300]] :fill "#264653")
      (image/rect! c 196 228 22 30 :fill "#e9c46a")
      (image/polygon! c [[190 230] [207 212] [224 230]] :fill "#e76f51"))))

(def (render)
  (ui/column
    (ui/text (str "Studio   reload " @reloads) :face :heading)
    (if-let [picture @shown]
      (ui/image picture)
      (ui/text "Rendering..." :face :comment))
    :spacing 6))

(swap! reloads inc)
(open-view "*Studio*" render :beside true)
(spawn (reset! shown (await (look (scene)))))
"##;

const LOOKS: &[&str] = &[
    "(-> picture (image/hue 150)))",
    "(-> picture (image/sepia) (image/vignette 0.9)))",
    "(-> picture (image/pixelate 8)))",
    "(-> picture (image/blur 3) (image/paste! (image/edges picture) 0 0 :blend :screen)))",
    "(-> picture (image/grayscale) (image/tint \"#ff8fab\")))",
];

fn studio_home() -> PathBuf {
    let user = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap();
    let home = user.join("oxmacs-studio");
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
        "(settings {theme: :adwaita-dark mode-line-style: :powerline animations: false font-size: 12})\n",
    )
    .unwrap();
    fs::write(home.join("studio.ox"), STUDIO).unwrap();
    home
}

fn picture_key(app: &App) -> Option<usize> {
    fn found(node: &Node<usize>) -> Option<usize> {
        match node {
            Node::Column { children, .. } | Node::Row { children, .. } => {
                children.iter().find_map(found)
            }
            Node::Scrollable(child) => found(child),
            Node::Image { picture, .. } => Some(picture.key()),
            _ => None,
        }
    }
    let editor = app.editor();
    let id = editor.buffer_named("*Studio*")?;
    let (_, view) = editor.special_buffer::<ViewBuffer>(id)?;
    found(&view.node)
}

fn reload(film: &mut Film, app: &mut App) {
    let before = picture_key(app);
    film.press(app, "C-c C-k");
    wait(app, "the new picture", |app| {
        picture_key(app).is_some_and(|key| Some(key) != before)
    });
    settle(app);
    film.shot(app, 2.4);
}

#[test]
#[ignore = "renders a square video of ox/image into $OXMACS_IMAGE_VIDEO"]
fn image_video() {
    let Ok(out) = std::env::var("OXMACS_IMAGE_VIDEO") else {
        return;
    };
    let out = PathBuf::from(out);
    let home = studio_home();
    let mut film = Film::new(out.with_extension("frames")).sized(Size::new(720.0, 720.0));
    let (mut app, _) = App::boot(&[], Some(&home));
    settle(&mut app);
    film.shot(&mut app, 1.5);
    film.press(&mut app, "C-x C-f");
    film.type_slowly(&mut app, "~/oxmacs-studio/studio.ox");
    film.press(&mut app, "RET");
    film.hold(0.8);
    film.press(&mut app, "C-x 2");
    film.mark("first");
    reload(&mut film, &mut app);
    for (number, look) in LOOKS.iter().enumerate() {
        film.mark(&format!("look-{number}"));
        film.press(&mut app, "M-< C-n C-n C-n C-n C-a C-k");
        film.hold(0.3);
        film.type_slowly(&mut app, &format!("  {look}"));
        reload(&mut film, &mut app);
    }
    film.press(&mut app, "M-< C-n C-n C-n C-n C-a C-k");
    film.type_slowly(&mut app, "  picture)");
    reload(&mut film, &mut app);
    film.hold(1.0);
    film.finish(&out);
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

const KEYCAST_NOTES: &str = "# Keys worth showing\n\nC-n moves down a line.\nC-x C-s saves.\nM-x runs any command by name.\n";

#[test]
#[ignore = "renders snapshots of the keycast plugin into $OXMACS_KEYCAST_SHOTS"]
fn keycast_shots() {
    let Ok(out) = std::env::var("OXMACS_KEYCAST_SHOTS") else {
        return;
    };
    let shots = [
        ("bottom-right", "adwaita-dark", "bottom-right"),
        ("bottom-center", "adwaita-dark", "bottom-center"),
        ("mode-line", "adwaita-dark", "mode-line"),
        ("light", "adwaita", "bottom-right"),
    ];
    for (name, theme, placement) in shots {
        let home = tempfile::tempdir().unwrap();
        fs::write(
            home.path().join("init.ox"),
            format!(
                "(settings {{theme: :{theme} mode-line-style: :powerline animations: false font-size: 11}})\n(use-plugin keycast :config {{placement: :{placement}}})\n"
            ),
        )
        .unwrap();
        let notes = home.path().join("notes.md");
        fs::write(&notes, KEYCAST_NOTES).unwrap();
        let visit = Visit {
            path: notes,
            line: None,
            column: None,
        };
        let mut film = Film::new(PathBuf::from(&out).join(name)).sized(Size::new(960.0, 540.0));
        let (mut app, _) = App::boot(&[visit], Some(home.path()));
        settle(&mut app);
        for chord in ["C-n", "C-n", "C-n", "C-x", "C-s", "M-x"] {
            keys(&mut app, chord);
            settle(&mut app);
        }
        for c in "end-of-buffer".chars() {
            type_char(&mut app, c);
        }
        keys(&mut app, "RET");
        settle(&mut app);
        film.shot(&mut app, 1.0);
    }
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
    let start = Instant::now();
    for frame in 0..150u32 {
        let _ = app.update(Message::Tick(
            start + Duration::from_millis(u64::from(frame) * 66),
        ));
        film.shot(app, 0.066);
    }
    film.press(app, "q");
    film.hold(1.5);
}

const CARS: &str = "# Fuel economy in the 1974 Motor Trend road tests
cars <- mtcars
cars$cyl <- factor(cars$cyl)

fit <- lm(mpg ~ wt, data = cars)
summary(fit)

plot(mpg ~ wt, data = cars, pch = 19, col = \"steelblue\")
abline(fit, col = \"tomato\", lwd = 2)
hist(cars$mpg, col = \"grey80\", main = \"Miles per gallon\")
boxplot(mpg ~ cyl, data = cars, col = \"wheat\")
";

const DESCRIPTION: &str = "Package: tinystats
Title: Small Summaries
Version: 0.1.0
Authors@R: person(\"Ada\", \"Lovelace\", email = \"ada@example.com\", role = c(\"aut\", \"cre\"))
Description: A few small summary statistics.
License: MIT
Encoding: UTF-8
Suggests: testthat (>= 3.0.0)
Config/testthat/edition: 3
";

const STATS: &str = "#' Coefficient of variation
cv <- function(x) sd(x) / mean(x)

#' Range as a single number
spread <- function(x) max(x) - min(x)
";

const TESTS: &str = "test_that(\"cv divides the deviation by the mean\", {
  expect_equal(cv(c(2, 4, 6)), 0.5)
})

test_that(\"spread is max minus min\", {
  expect_equal(spread(c(3, 9, 4)), 6)
})

test_that(\"spread ignores missing values\", {
  expect_equal(spread(c(3, NA, 9)), 6)
})
";

fn r_home() -> PathBuf {
    let user = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap();
    let home = user.join("oxmacs-r-demo");
    if home.join(".oxmacs-demo").exists() {
        fs::remove_dir_all(&home).unwrap();
    }
    assert!(
        !home.exists(),
        "{} exists and wasn't made by this demo",
        home.display()
    );
    let package = home.join("tinystats");
    fs::create_dir_all(home.join("analysis")).unwrap();
    fs::create_dir_all(package.join("R")).unwrap();
    fs::create_dir_all(package.join("tests").join("testthat")).unwrap();
    fs::write(home.join(".oxmacs-demo"), "").unwrap();
    fs::write(
        home.join("init.ox"),
        "(settings {theme: :adwaita-dark animations: false font-size: 12})\n(use-plugin r :config {language-server: false})\n(use-plugin keycast)\n",
    )
    .unwrap();
    fs::write(home.join("analysis").join("cars.R"), CARS).unwrap();
    fs::write(package.join("DESCRIPTION"), DESCRIPTION).unwrap();
    fs::write(package.join("R").join("stats.R"), STATS).unwrap();
    fs::write(
        package.join("tests").join("testthat").join("test-stats.R"),
        TESTS,
    )
    .unwrap();
    home
}

fn text_of(app: &App, name: &str) -> String {
    let editor = app.editor();
    editor
        .buffer_named(name)
        .and_then(|id| editor.buffer_by_id(id))
        .map(oxmacs_core::text::Buffer::contents)
        .unwrap_or_default()
}

fn prompted(app: &App, console: &str) -> bool {
    text_of(app, console).trim_end_matches(' ').ends_with('>')
}

fn until(app: &mut App, what: &str, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        track(app.update(Message::OxTick));
        pump(app);
        if done(app) {
            settle(app);
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}: {:?} in {}",
            app.editor().message(),
            app.editor().buffer().name()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn rest(app: &mut App, seconds: f64) {
    thread::sleep(Duration::from_secs_f64(seconds));
    track(app.update(Message::Idle));
    settle(app);
}

fn select(film: &mut Film, app: &mut App, wanted: impl Fn(&str) -> bool) {
    for _ in 0..8 {
        if wanted(app.editor().buffer().name()) {
            return;
        }
        film.press(app, "C-x o");
    }
    panic!("no window shows the buffer wanted");
}

fn command(film: &mut Film, app: &mut App, name: &str) {
    film.press(app, "M-x");
    film.type_slowly(app, name);
    film.press(app, "RET");
}

fn step(film: &mut Film, app: &mut App, console: &str, hold: f64) {
    film.press(app, "C-RET");
    until(app, "R's prompt", |app| prompted(app, console));
    film.shot(app, hold);
}

fn open_cars(film: &mut Film, app: &mut App) {
    film.mark("open");
    film.press(app, "C-x C-f");
    film.type_slowly(app, "~/oxmacs-r-demo/analysis/cars.R");
    film.press(app, "RET");
    film.hold(1.2);
    command(film, app, "r-layout");
    until(app, "the R console", |app| prompted(app, "*R:analysis*"));
    film.shot(app, 2.6);
    film.press(app, "C-n");
}

fn images(node: &Node<usize>) -> usize {
    match node {
        Node::Image { .. } => 1,
        Node::Column { children, .. } | Node::Row { children, .. } => {
            children.iter().map(images).sum()
        }
        Node::Scrollable(child) => images(child),
        _ => 0,
    }
}

fn plot_images(app: &App) -> usize {
    let editor = app.editor();
    editor
        .buffer_names()
        .iter()
        .filter(|name| name.contains("plots"))
        .filter_map(|name| editor.buffer_named(name))
        .filter_map(|id| editor.special_buffer::<ViewBuffer>(id))
        .map(|(_, view)| images(&view.node))
        .sum()
}

fn plotted(app: &App) -> bool {
    plot_images(app) > 0
}

fn run_cars(film: &mut Film, app: &mut App) {
    film.mark("run");
    let console = "*R:analysis*";
    step(film, app, console, 0.6);
    step(film, app, console, 0.6);
    step(film, app, console, 0.6);
    step(film, app, console, 3.6);
    film.mark("plots");
    for hold in [2.2, 2.2, 2.2, 2.8] {
        step(film, app, console, 0.2);
        until(app, "the plot", plotted);
        rest(app, 0.3);
        film.shot(app, hold);
    }
}

fn browse_plots(film: &mut Film, app: &mut App) {
    film.mark("gallery");
    select(film, app, |name| name.contains("plots"));
    film.hold(0.6);
    for key in ["p", "p", "p"] {
        film.press(app, key);
        rest(app, 0.4);
        film.shot(app, 1.0);
    }
    film.press(app, "v");
    until(app, "the gallery", |app| plot_images(app) >= 3);
    rest(app, 0.3);
    film.shot(app, 3.2);
    film.press(app, "v");
    rest(app, 0.5);
    film.shot(app, 0.6);
}

fn to_column(app: &mut App, name: &str) {
    let wanted = format!(" {name}");
    let column = app
        .editor()
        .pane()
        .header_lines()
        .iter()
        .find(|(text, _)| text.split_whitespace().next() == Some(name))
        .and_then(|(text, _)| text.find(&wanted).map(|at| at + 1))
        .expect("the column");
    keys(app, "M-<");
    for _ in 0..=column {
        keys(app, "C-f");
    }
    settle(app);
}

fn explore_cars(film: &mut Film, app: &mut App) {
    film.mark("data");
    select(film, app, |name| name.contains("environment"));
    film.hold(1.0);
    film.press(app, "C-s");
    film.type_slowly(app, "cars");
    film.press(app, "RET");
    film.press(app, "RET");
    until(app, "the table of cars", |app| {
        app.editor().buffer().name().contains("cars") && app.editor().buffer().len_lines() > 5
    });
    film.shot(app, 2.2);
    to_column(app, "mpg");
    film.shot(app, 0.6);
    film.press(app, "s");
    rest(app, 0.6);
    film.shot(app, 1.4);
    to_column(app, "mpg");
    film.press(app, "s");
    rest(app, 0.6);
    film.shot(app, 2.0);
    to_column(app, "mpg");
    film.press(app, "i");
    rest(app, 0.8);
    film.shot(app, 2.2);
    film.press(app, "/");
    film.type_slowly(app, "cyl == 4");
    film.press(app, "RET");
    rest(app, 0.8);
    film.shot(app, 2.4);
}

fn complete_and_ask(film: &mut Film, app: &mut App) {
    film.mark("smarts");
    select(film, app, |name| name == "cars.R");
    film.press(app, "M->");
    film.type_slowly(app, "med");
    rest(app, 0.4);
    until(app, "completions", |app| {
        app.editor().pane().popup().is_some()
    });
    film.shot(app, 1.6);
    film.press(app, "RET");
    film.type_slowly(app, "(");
    rest(app, 0.6);
    until(app, "the arguments of median", |app| {
        app.editor()
            .message()
            .is_some_and(|shown| shown.starts_with("median("))
    });
    film.shot(app, 1.8);
    film.type_slowly(app, "cars$mpg)");
    if app.editor().pane().popup().is_some() {
        film.press(app, "C-g");
    }
    film.press(app, "C-RET");
    until(app, "R's prompt", |app| prompted(app, "*R:analysis*"));
    film.shot(app, 1.6);
    film.mark("help");
    film.press(app, "C-c C-v");
    film.type_slowly(app, "median");
    film.press(app, "RET");
    until(app, "R's help", |app| {
        app.editor().buffer().name() == "*R help*"
    });
    film.shot(app, 3.2);
    film.press(app, "TAB");
    film.hold(0.8);
    film.press(app, "RET");
    until(app, "the linked help", |app| {
        !text_of(app, "*R help*").starts_with("median")
    });
    film.shot(app, 2.0);
    film.press(app, "l");
    rest(app, 0.4);
    film.shot(app, 1.2);
}

fn test_package(film: &mut Film, app: &mut App) {
    film.mark("package");
    film.press(app, "C-x 1");
    film.press(app, "C-x C-f");
    film.type_slowly(app, "~/oxmacs-r-demo/tinystats/tests/testthat/test-stats.R");
    film.press(app, "RET");
    film.hold(1.0);
    film.press(app, "C-x 3");
    film.press(app, "C-x C-f");
    film.type_slowly(app, "~/oxmacs-r-demo/tinystats/R/stats.R");
    film.press(app, "RET");
    film.hold(1.2);
    command(film, app, "r-test");
    until(app, "the test results", |app| {
        app.editor().buffer().name() == "*R tests*" && text_of(app, "*R tests*").contains("missing")
    });
    film.shot(app, 4.0);
    film.mark("fix");
    select(film, app, |name| name == "stats.R");
    for call in ["max(x", "min(x"] {
        film.press(app, "M-< C-s");
        film.type_slowly(app, call);
        film.press(app, "RET");
        film.type_slowly(app, ", na.rm = TRUE");
    }
    film.press(app, "C-x C-s");
    film.hold(0.8);
    command(film, app, "r-test");
    until(app, "the tests passing", |app| {
        !text_of(app, "*R tests*").contains('\u{2717}')
            && text_of(app, "*R tests*").contains("missing")
    });
    film.shot(app, 4.0);
}

#[test]
#[ignore = "renders a 16:9 video of the r plugin into $OXMACS_R_VIDEO"]
fn r_video() {
    let Ok(out) = std::env::var("OXMACS_R_VIDEO") else {
        return;
    };
    let out = PathBuf::from(out);
    let home = r_home();
    PUMPING.set(true);
    let mut film = Film::new(out.with_extension("frames"))
        .sized(Size::new(1280.0, 720.0))
        .warmed(5)
        .scaled_to(1280, 720);
    let (mut app, booted) = App::boot(&[], Some(&home));
    track(booted);
    track(app.update(Message::Focus(true)));
    settle(&mut app);
    film.shot(&mut app, 1.5);
    open_cars(&mut film, &mut app);
    run_cars(&mut film, &mut app);
    browse_plots(&mut film, &mut app);
    explore_cars(&mut film, &mut app);
    complete_and_ask(&mut film, &mut app);
    test_package(&mut film, &mut app);
    film.hold(2.5);
    film.finish(&out);
}

fn news_home() -> PathBuf {
    let user = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap();
    let home = user.join("oxmacs-news-demo");
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
        "(settings {theme: :adwaita-dark animations: false font-size: 12})\n(use-plugin zap :config {duration: 4000})\n(use-plugin keycast)\n",
    )
    .unwrap();
    home
}

const NEWS_HEADER: &str = "(module (import (ox http) (ox xml) (ox string) (ox regex) (ox ui)))";

const NEWS_FEED: &str = "(def feed \"https://blog.rust-lang.org/feed.xml\")";

const NEWS_STATE: &str = "(defonce news (atom {entries: [] at: 0 open: false}))";

const NEWS_PLAIN: &str = "(def (plain html)
(string/trim (regex/replace \"[[:space:]]+\" (regex/replace \"<[^>]*>\" html \"\") \" \")))";

const NEWS_ENTRY: &str = "(def (entry el)
{title: (xml/text (xml/find el \"title\"))
date: (string/slice (xml/text (xml/find el \"updated\")) 0 10)
summary: (plain (or (xml/text (xml/find el \"content\")) \"\"))})";

const NEWS_TRY: &str =
    "(count (xml/find-all (xml/parse (get (await (http/get feed)) :body)) \"entry\"))";

const NEWS_FETCH: &str = "(def (fetch)
(let [reply (await (http/get feed))
root (xml/parse reply.body)]
(swap! news assoc :entries (map entry (xml/find-al root \"entry\")) :at 0 :open false)
(count (get @news :entries))))";

const NEWS_VIEW: &str = "(def (headline i)
(let [now @news
item (get now.entries i)]
(ui/text \"{item.date}  {item.title}\" :face (if (= i now.at) :heading nil))))

(def (page)
(let [now @news
chosen (get now.entries now.at)]
(ui/column
(ui/text \"The Rust blog\" :face :heading :size 20)
(apply ui/column (map headline (range (count now.entries))))
(ui/text (if now.open (string/truncate chosen.summary 600) \"\") :face :comment)
:spacing 10)))

(def (move by)
(swap! news (fn [now]
(assoc now :open false
:at (max 0 (min (+ now.at by) (dec (count now.entries))))))))";

const NEWS_COMMANDS: &str = "(defcommand news-next \"Next headline\" (move 1))
(defcommand news-previous \"Previous headline\" (move -1))
(defcommand news-read \"Read the headline\" (swap! news update :open not))
(defcommand news \"Read the Rust blog\"
(open-view \"*News*\" page :beside true :select true
:keys {\"n\" 'news-next \"p\" 'news-previous \"RET\" 'news-read \"g\" 'news \"q\" 'quit-window})
(fetch))";

const SLOWED: f64 = 8.0;

fn fading(film: &mut Film, app: &mut App) {
    let mut shown = Instant::now();
    film.shot(app, 0.0);
    while flashing(app) {
        settle(app);
        let now = Instant::now();
        film.hold(now.duration_since(shown).as_secs_f64() / SLOWED);
        shown = now;
        film.shot(app, 0.0);
    }
    film.hold(Instant::now().duration_since(shown).as_secs_f64() / SLOWED + 0.2);
}

fn flashing(app: &App) -> bool {
    app.editor().flashing(Instant::now())
}

fn evaluate_above(film: &mut Film, app: &mut App) {
    film.press(app, "C-p C-e");
    film.press(app, "C-x C-e");
    until(app, "the form's flash", flashing);
    fading(film, app);
    film.press(app, "C-n");
}

fn view_texts(app: &App, name: &str) -> Vec<String> {
    fn gathered(node: &Node<usize>, out: &mut Vec<String>) {
        match node {
            Node::Column { children, .. } | Node::Row { children, .. } => {
                for child in children {
                    gathered(child, out);
                }
            }
            Node::Scrollable(child) => gathered(child, out),
            Node::Text { text, .. } => out.push(text.clone()),
            _ => {}
        }
    }
    let editor = app.editor();
    let mut out = Vec::new();
    if let Some((_, view)) = editor
        .buffer_named(name)
        .and_then(|id| editor.special_buffer::<ViewBuffer>(id))
    {
        gathered(&view.node, &mut out);
    }
    out
}

fn start_news(film: &mut Film, app: &mut App, home: &Path) {
    film.mark("start");
    app.start(
        &[Visit {
            path: home.join("news.ox"),
            line: None,
            column: None,
        }],
        false,
    );
    settle(app);
    film.shot(app, 0.8);
    film.press(app, "C-x 3");
    for form in [NEWS_HEADER, NEWS_FEED, NEWS_STATE, NEWS_PLAIN, NEWS_ENTRY] {
        film.type_code(app, form);
        evaluate_above(film, app);
        film.press(app, "RET");
    }
}

fn try_the_feed(film: &mut Film, app: &mut App) {
    film.mark("repl");
    film.press(app, "C-x o");
    film.press(app, "C-c C-z");
    film.type_slowly(app, NEWS_TRY);
    film.press(app, "RET");
    until(app, "the feed in the REPL", |app| {
        text_of(app, "*ox repl*")
            .rsplit_once(NEWS_TRY)
            .is_some_and(|(_, after)| after.contains("ox>"))
    });
    fading(film, app);
    film.shot(app, 1.5);
    film.press(app, "C-x o");
}

fn fetch_with_a_typo(film: &mut Film, app: &mut App) {
    film.mark("typo");
    film.type_code(app, NEWS_FETCH);
    evaluate_above(film, app);
    film.press(app, "RET");
    film.type_code(app, "(fetch)");
    rest(app, 0.7);
    until(app, "the checker", |app| {
        app.editor().pane().status().lint.is_some()
    });
    film.shot(app, 1.5);
    film.press(app, "C-p C-e");
    film.press(app, "C-x C-e");
    until(app, "the failing fetch", flashing);
    fading(film, app);
    film.shot(app, 2.0);
}

fn fix_the_typo(film: &mut Film, app: &mut App) {
    film.mark("fix");
    select(film, app, |name| name == "news.ox");
    film.press(app, "M-<");
    film.press(app, "C-s");
    film.type_slowly(app, "find-al");
    film.press(app, "RET");
    film.type_slowly(app, "l");
    film.press(app, "C-M-x");
    until(app, "the fixed fetch's flash", flashing);
    fading(film, app);
    film.press(app, "M-> C-p C-e");
    film.press(app, "C-x C-e");
    until(app, "the fetch", flashing);
    fading(film, app);
    film.shot(app, 1.5);
    rest(app, 0.7);
    until(app, "the checker to clear the error", |app| {
        app.editor()
            .pane()
            .status()
            .lint
            .is_none_or(|lint| !lint.contains('E'))
    });
    film.shot(app, 1.0);
    film.press(app, "C-x 1");
}

fn build_the_view(film: &mut Film, app: &mut App) {
    film.mark("view");
    film.press(app, "M-> RET");
    film.type_code(app, NEWS_VIEW);
    film.press(app, "RET");
    film.type_code(app, NEWS_COMMANDS);
    film.press(app, "C-c C-b");
    until(app, "the buffer's flashes", flashing);
    fading(film, app);
    rest(app, 0.7);
    until(app, "the checker to find nothing", |app| {
        app.editor()
            .pane()
            .status()
            .lint
            .is_none_or(|lint| lint.is_empty())
    });
    film.shot(app, 1.0);
}

fn read_the_news(film: &mut Film, app: &mut App) {
    film.mark("read");
    command(film, app, "news");
    until(app, "the headlines", |app| {
        view_texts(app, "*News*").len() > 4
    });
    film.shot(app, 2.5);
    for key in ["n", "n", "n", "p"] {
        film.press(app, key);
        film.shot(app, 0.6);
    }
    film.press(app, "RET");
    film.shot(app, 4.0);
    film.press(app, "n RET");
    film.shot(app, 3.5);
    film.press(app, "q");
    film.shot(app, 1.0);
}

#[test]
#[ignore = "renders a 16:9 video of zap and a newsfeed reader built in Ox into $OXMACS_NEWS_VIDEO"]
fn news_video() {
    let Ok(out) = std::env::var("OXMACS_NEWS_VIDEO") else {
        return;
    };
    let out = PathBuf::from(out);
    let home = news_home();
    PUMPING.set(true);
    let mut film = Film::new(out.with_extension("frames"))
        .sized(Size::new(1280.0, 720.0))
        .scaled_to(1280, 720);
    let (mut app, booted) = App::boot(&[], Some(&home));
    track(booted);
    track(app.update(Message::Focus(true)));
    settle(&mut app);
    film.shot(&mut app, 1.5);
    start_news(&mut film, &mut app, &home);
    try_the_feed(&mut film, &mut app);
    fetch_with_a_typo(&mut film, &mut app);
    fix_the_typo(&mut film, &mut app);
    build_the_view(&mut film, &mut app);
    read_the_news(&mut film, &mut app);
    film.hold(2.5);
    film.finish(&out);
}

const GARDEN_CARGO: &str =
    "[package]\nname = \"garden\"\nversion = \"0.1.0\"\nedition = \"2024\"\n";

const GARDEN_README: &str = "# Garden\n\nA small planner for a kitchen garden: what to sow, when, and in which bed.\n\nRun it with `cargo run -- spring`.\n";

const GARDEN_MAIN: &str = "mod soil;\nmod weather;\n\nuse soil::Bed;\n\nfn main() {\n    let season = std::env::args().nth(1).unwrap_or_else(|| \"spring\".to_owned());\n    let beds = [Bed::new(\"north\", 6.5), Bed::new(\"south\", 7.0)];\n    for bed in &beds {\n        println!(\"{season}: the {} bed, pH {:.1}, sow {}\", bed.name, bed.ph, bed.crop());\n    }\n}\n";

const GARDEN_SOIL: &str = "pub struct Bed {\n    pub name: &'static str,\n    pub ph: f32,\n}\n\nimpl Bed {\n    pub fn new(name: &'static str, ph: f32) -> Self {\n        Self { name, ph }\n    }\n\n    pub fn crop(&self) -> &'static str {\n        if self.ph < 6.8 { \"potatoes\" } else { \"beans\" }\n    }\n}\n";

const GARDEN_WEATHER: &str =
    "pub fn frost_likely(month: u32) -> bool {\n    matches!(month, 11 | 12 | 1 | 2 | 3)\n}\n";

const GARDEN_PLAN: &str = "# Plan for the season\n\nThe north bed is sour and wet, so potatoes go there first, with a row of broad beans along the wall where the wind cannot reach them.\n\nThe south bed drains well. Beans in May, then lettuce under the beans once they are tall enough to give some shade.\n";

fn showcase_home() -> PathBuf {
    let user = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap();
    let home = user.join("oxmacs-showcase");
    if home.join(".oxmacs-demo").exists() {
        fs::remove_dir_all(&home).unwrap();
    }
    assert!(
        !home.exists(),
        "{} exists and wasn't made by this demo",
        home.display()
    );
    let garden = home.join("garden");
    fs::create_dir_all(garden.join("src")).unwrap();
    fs::create_dir_all(garden.join("notes")).unwrap();
    fs::create_dir_all(home.join("books")).unwrap();
    fs::create_dir_all(home.join("papers")).unwrap();
    fs::write(home.join(".oxmacs-demo"), "").unwrap();
    let books = home.join("books");
    fs::write(
        home.join("init.ox"),
        format!(
            "(settings {{theme: :adwaita-dark animations: false font-size: 12 language-servers: false rust-lint: :off}})\n(use-plugin keycast :config {{placement: :bottom-center}})\n(use-plugin tabs)\n(use-plugin zen :config {{fullscreen: false width: 72}})\n(use-plugin epub :config {{library: {:?} size: 16}})\n(use-plugin pdf)\n",
            books.display().to_string()
        ),
    )
    .unwrap();
    fs::write(garden.join("Cargo.toml"), GARDEN_CARGO).unwrap();
    fs::write(garden.join("README.md"), GARDEN_README).unwrap();
    fs::write(garden.join("src").join("main.rs"), GARDEN_MAIN).unwrap();
    fs::write(garden.join("src").join("soil.rs"), GARDEN_SOIL).unwrap();
    fs::write(garden.join("notes").join("plan.md"), GARDEN_PLAN).unwrap();
    git(&garden, &["init", "-q", "-b", "main"]);
    git(&garden, &["config", "user.name", "Ada Lovelace"]);
    git(&garden, &["config", "user.email", "ada@example.com"]);
    git(&garden, &["config", "commit.gpgsign", "false"]);
    git(&garden, &["add", "."]);
    git(&garden, &["commit", "-q", "-m", "Plan the beds"]);
    fs::write(
        garden.join("src").join("soil.rs"),
        GARDEN_SOIL.replace("6.8", "6.6"),
    )
    .unwrap();
    fs::write(garden.join("src").join("weather.rs"), GARDEN_WEATHER).unwrap();
    sample_book::write(&books, "patient-ox.epub");
    sample_book::write_book(
        &books,
        "seventeen-furrows.epub",
        &sample_book::Book {
            title: "Seventeen Furrows",
            author: "Mira Hollis",
            sky: [96.0, 40.0, 52.0],
        },
    );
    sample_book::write_book(
        &books,
        "stones-at-the-hedge.epub",
        &sample_book::Book {
            title: "Stones at the Hedge",
            author: "Ruth Hollis",
            sky: [24.0, 84.0, 64.0],
        },
    );
    sample_paper::write(&home.join("papers"), "patient-furrows.pdf");
    home
}

fn shown_node(app: &App, name: &str, wanted: fn(&Node<usize>) -> bool) -> bool {
    let editor = app.editor();
    editor
        .buffer_named(name)
        .and_then(|id| editor.special_buffer::<ViewBuffer>(id))
        .is_some_and(|(_, view)| wanted(&view.node))
}

fn pages_drawn(app: &mut App, size: Size) {
    for _ in 0..200 {
        let mut ui = Simulator::with_size(settings(), size, app.view());
        let _ = ui.snapshot(&app.theme());
        for message in ui.into_messages() {
            track(app.update(message));
        }
        settle(app);
        if !oxmacs_gui::widgets::pdf::drawing() {
            return;
        }
        thread::sleep(Duration::from_millis(30));
    }
}

fn read_paper(film: &mut Film, app: &mut App, notation: &str, hold: f64) {
    for chord in notation.split(' ') {
        keys(app, chord);
        settle(app);
        pages_drawn(app, film.size);
        film.shot(app, 0.35);
    }
    film.hold(hold);
}

fn prompt_answer(film: &mut Film, app: &mut App, notation: &str, answer: &str) {
    film.press(app, notation);
    film.type_slowly(app, answer);
    film.press(app, "RET");
}

fn rename_tab(film: &mut Film, app: &mut App, name: &str) {
    film.press(app, "C-x t r C-a C-k");
    film.type_slowly(app, name);
    film.press(app, "RET");
}

fn explore(film: &mut Film, app: &mut App) {
    film.mark("explorer");
    film.press(app, "C-x C-d");
    until(app, "the file tree", |app| {
        text_of(app, "*explorer*").contains("soil.rs")
    });
    film.shot(app, 3.5);
    film.press(app, "M-<");
    film.hold(1.0);
    film.press(app, "TAB");
    film.hold(2.5);
    film.press(app, "TAB");
    film.hold(1.0);
    film.type_slowly(app, "soil");
    film.hold(2.5);
    film.press(app, "RET");
    film.hold(3.0);
    film.press(app, "C-x C-d");
    film.hold(1.0);
    film.type_slowly(app, "wea");
    film.hold(1.5);
    film.press(app, "SPC");
    film.hold(2.5);
    film.press(app, "RET");
    film.hold(2.0);
    film.press(app, "C-x C-d C-x C-d");
    film.hold(2.0);
}

fn tabs(film: &mut Film, app: &mut App) {
    film.mark("tabs");
    rename_tab(film, app, "Garden");
    film.hold(1.5);
    film.press(app, "C-x t 2");
    film.hold(2.0);
    rename_tab(film, app, "Reading");
    film.hold(2.0);
}

fn read_book(film: &mut Film, app: &mut App) {
    film.mark("epub");
    prompt_answer(film, app, "C-x C-f", "../../books/patient-ox.epub");
    until(app, "the book", |app| {
        shown_node(app, "patient-ox.epub", |node| {
            matches!(node, Node::Pages(_))
        })
    });
    film.shot(app, 0.3);
    film.shot(app, 4.0);
    for (notation, hold) in [("SPC", 5.0), ("SPC", 4.0), ("SPC", 3.0), ("]", 3.5)] {
        film.press(app, notation);
        film.hold(hold);
    }
    prompt_answer(film, app, "o", "The Harvest");
    film.hold(5.0);
    prompt_answer(film, app, "s", "stone");
    film.hold(4.0);
    for (notation, hold) in [
        ("C-s", 2.5),
        ("C-s", 2.5),
        ("+ +", 3.0),
        ("- -", 1.5),
        ("i", 3.0),
    ] {
        film.press(app, notation);
        film.hold(hold);
    }
    command(film, app, "epub-library");
    until(app, "the library", |app| {
        view_texts(app, "*epub library*")
            .iter()
            .any(|text| text.starts_with("Stones at"))
    });
    film.shot(app, 0.5);
    film.shot(app, 6.0);
    film.press(app, "q");
    film.hold(1.5);
}

fn read_pdf(film: &mut Film, app: &mut App) {
    film.mark("pdf");
    film.press(app, "C-x t 2");
    rename_tab(film, app, "Papers");
    prompt_answer(film, app, "C-x b", "main.rs");
    prompt_answer(film, app, "C-x C-f", "../../papers/patient-furrows.pdf");
    until(app, "the paper", |app| {
        shown_node(app, "patient-furrows.pdf", |node| {
            matches!(node, Node::Pdf(_))
        })
    });
    for (notation, hold) in [("C-n", 4.0), ("SPC", 3.0), ("SPC", 3.0), ("n", 3.0)] {
        read_paper(film, app, notation, hold);
    }
    film.press(app, "o");
    film.type_slowly(app, "Results");
    film.press(app, "TAB");
    read_paper(film, app, "RET", 3.5);
    for (notation, hold) in [
        ("H", 3.5),
        ("c", 2.0),
        ("n", 2.0),
        ("p", 2.0),
        ("m", 4.0),
        ("m", 1.0),
        ("c", 1.5),
        ("W", 2.0),
    ] {
        read_paper(film, app, notation, hold);
    }
    film.press(app, "s");
    film.type_slowly(app, "furrow");
    read_paper(film, app, "RET", 4.0);
    for (notation, hold) in [
        ("C-s", 2.5),
        ("C-s", 2.5),
        ("+", 2.5),
        ("+", 2.0),
        ("W", 1.5),
    ] {
        read_paper(film, app, notation, hold);
    }
    film.press(app, "M-g g");
    film.type_slowly(app, "4");
    read_paper(film, app, "RET", 3.5);
    read_paper(film, app, "i", 3.5);
}

fn write_in_zen(film: &mut Film, app: &mut App) {
    film.mark("zen");
    film.press(app, "C-x t 2");
    rename_tab(film, app, "Writing");
    prompt_answer(film, app, "C-x b", "main.rs");
    prompt_answer(film, app, "C-x C-f", "../notes/plan.md");
    film.hold(2.0);
    command(film, app, "zen");
    film.hold(3.5);
    film.press(app, "M-> RET");
    film.type_slowly(
        app,
        "Stones go to the wall by the hedge. Every one of them is a lesson in patience, and the wall is longer every spring.",
    );
    film.hold(3.0);
    film.press(app, "M-<");
    film.hold(2.5);
    film.press(app, "C-n C-n C-n C-n");
    film.hold(2.5);
    film.press(app, "C-x C-s");
    film.hold(1.5);
    command(film, app, "zen");
    film.hold(3.0);
}

fn round_the_tabs(film: &mut Film, app: &mut App) {
    film.mark("outro");
    for _ in 0..4 {
        film.press(app, "C-TAB");
        pages_drawn(app, film.size);
        film.shot(app, 2.5);
    }
}
#[test]
#[ignore = "renders a 16:9 tour of the tabs, explorer, epub, pdf and zen plugins into $OXMACS_SHOWCASE_VIDEO"]
fn showcase_video() {
    let Ok(out) = std::env::var("OXMACS_SHOWCASE_VIDEO") else {
        return;
    };
    let out = PathBuf::from(out);
    let home = showcase_home();
    PUMPING.set(true);
    let mut film = Film::new(out.with_extension("frames"))
        .sized(Size::new(1280.0, 720.0))
        .warmed(5)
        .paced(1.4)
        .scaled_to(1280, 720);
    let (mut app, booted) = App::boot(
        &[Visit {
            path: home.join("garden").join("src").join("main.rs"),
            line: None,
            column: None,
        }],
        Some(&home),
    );
    track(booted);
    track(app.update(Message::Focus(true)));
    settle(&mut app);
    film.shot(&mut app, 2.0);
    explore(&mut film, &mut app);
    tabs(&mut film, &mut app);
    read_book(&mut film, &mut app);
    read_pdf(&mut film, &mut app);
    write_in_zen(&mut film, &mut app);
    round_the_tabs(&mut film, &mut app);
    film.hold(2.5);
    film.finish(&out);
}

const JUMP: &str = r##"(module (import (ox rodeo)))

(rodeo/screen 256 144)
(rodeo/actions {left: [:left] right: [:right] jump: [:space :up]})

(def ground 108)
(def speed 90)
(def gravity 600)
(def jump -240)

(defonce player (atom {x: 30 y: ground vy: 0}))

(def (tick)
  (let [p @player
        dt (rodeo/dt)
        across (cond ((rodeo/held? :left) (- speed)) ((rodeo/held? :right) speed) (else 0))
        standing (>= p.y ground)
        vy (if (and standing (rodeo/pressed? :jump)) jump (+ p.vy (* gravity dt)))
        y (min ground (+ p.y (* vy dt)))]
    (reset! player {x: (mod (+ p.x (* across dt)) 256) y: y vy: (if (>= y ground) 0 vy)})))

(def (draw)
  (let [p @player]
    (rodeo/clear "#1d2b53")
    (rodeo/rect [0 124 256 20] "#008751")
    (rodeo/rect [p.x p.y 12 16] "#ff004d")))
"##;

fn rodeo_home() -> PathBuf {
    let user = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap();
    let home = user.join("oxmacs-rodeo-demo");
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
        "(settings {theme: :adwaita-dark animations: false font-size: 12})\n(use-plugin rodeo)\n",
    )
    .unwrap();
    fs::write(home.join("jump.ox"), JUMP).unwrap();
    home
}

fn game_key(app: &mut App, named: Named, down: bool) {
    let key = Key::Named(named);
    let event = if down {
        keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: Physical::Code(Code::ArrowRight),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
            text: None,
            repeat: false,
        }
    } else {
        keyboard::Event::KeyReleased {
            key: key.clone(),
            modified_key: key,
            physical_key: Physical::Code(Code::ArrowRight),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
        }
    };
    track(app.update(Message::Keyboard(event)));
}

fn tap(film: &mut Film, app: &mut App, named: Named) {
    game_key(app, named, true);
    film.play(app, 0.1);
    game_key(app, named, false);
}

fn rodeo_open(film: &mut Film, app: &mut App) {
    film.mark("open");
    film.shot(app, 2.0);
    command(film, app, "rodeo-play");
    settle(app);
    film.play(app, 1.2);
}

fn rodeo_run(film: &mut Film, app: &mut App) {
    film.mark("run");
    game_key(app, Named::ArrowRight, true);
    film.play(app, 1.0);
    tap(film, app, Named::Space);
    film.play(app, 1.2);
    game_key(app, Named::ArrowRight, false);
    film.play(app, 0.6);
}

fn find(film: &mut Film, app: &mut App, text: &str) {
    film.press(app, "M-< C-s");
    film.type_slowly(app, text);
    film.press(app, "RET");
}

fn rodeo_restyle(film: &mut Film, app: &mut App) {
    film.mark("restyle");
    film.press(app, "C-x o");
    find(film, app, "ff004d");
    film.press(app, "DEL DEL DEL DEL DEL DEL");
    film.type_slowly(app, "ffa300");
    film.press(app, "C-M-x");
    film.play(app, 1.5);
    find(film, app, "#1d2b53\")");
    film.press(app, "RET");
    film.type_slowly(app, "(rodeo/circle [210 34] 14 \"#ffec27\"");
    film.press(app, "C-M-x");
    film.play(app, 1.5);
}

fn rodeo_jump(film: &mut Film, app: &mut App) {
    film.mark("jump");
    find(film, app, "-240");
    film.press(app, "DEL DEL DEL");
    film.type_slowly(app, "340");
    film.press(app, "C-M-x");
    film.play(app, 0.4);
    film.press(app, "C-x o");
    game_key(app, Named::ArrowLeft, true);
    film.play(app, 0.5);
    tap(film, app, Named::Space);
    film.play(app, 1.4);
    tap(film, app, Named::Space);
    film.play(app, 1.4);
    game_key(app, Named::ArrowLeft, false);
    film.play(app, 0.5);
}

fn rodeo_break(film: &mut Film, app: &mut App) {
    film.mark("break");
    film.press(app, "C-x o");
    find(film, app, "12 16");
    film.press(app, "DEL DEL DEL");
    film.press(app, "C-M-x");
    film.play(app, 2.5);
    film.type_slowly(app, " 16");
    film.press(app, "C-M-x");
    film.play(app, 0.5);
    command(film, app, "rodeo-pause");
    film.play(app, 1.5);
}

fn rodeo_save(film: &mut Film, app: &mut App) {
    film.mark("save");
    film.press(app, "C-x C-s");
    film.play(app, 2.0);
}

#[test]
#[ignore = "renders a 16:9 video of a rodeo game changing while it runs into $OXMACS_RODEO_VIDEO"]
fn rodeo_video() {
    let Ok(out) = std::env::var("OXMACS_RODEO_VIDEO") else {
        return;
    };
    let out = PathBuf::from(out);
    let home = rodeo_home();
    PUMPING.set(true);
    let mut film = Film::new(out.with_extension("frames"))
        .sized(Size::new(1280.0, 720.0))
        .scaled_to(1280, 720)
        .ticking(Instant::now());
    let visit = Visit {
        path: home.join("jump.ox"),
        line: None,
        column: None,
    };
    let (mut app, booted) = App::boot(&[visit], Some(&home));
    track(booted);
    track(app.update(Message::Focus(true)));
    settle(&mut app);
    rodeo_open(&mut film, &mut app);
    if std::env::var_os("OXMACS_RODEO_SHORT").is_none() {
        rodeo_run(&mut film, &mut app);
        rodeo_restyle(&mut film, &mut app);
        rodeo_jump(&mut film, &mut app);
        rodeo_break(&mut film, &mut app);
        rodeo_save(&mut film, &mut app);
    }
    film.hold(1.5);
    film.finish(&out);
}

const ACID_BASE: &str = include_str!("demos/acid-base.ox");

fn acid_home() -> PathBuf {
    let user = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap();
    let home = user.join("oxmacs-acid-demo");
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
        "(settings {theme: :adwaita-dark animations: false font-size: 12})\n(use-plugin rodeo)\n(use-plugin keycast :config {placement: :bottom-center})\n",
    )
    .unwrap();
    fs::write(home.join("acid-base.ox"), ACID_BASE).unwrap();
    home
}

fn game_letter(app: &mut App, letter: &str, down: bool) {
    let key = Key::Character(letter.into());
    let event = if down {
        keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: Physical::Code(Code::KeyA),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
            text: Some(letter.into()),
            repeat: false,
        }
    } else {
        keyboard::Event::KeyReleased {
            key: key.clone(),
            modified_key: key,
            physical_key: Physical::Code(Code::KeyA),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
        }
    };
    track(app.update(Message::Keyboard(event)));
}

fn tap_letter(film: &mut Film, app: &mut App, letter: &str) {
    game_letter(app, letter, true);
    film.play(app, 0.1);
    game_letter(app, letter, false);
}

fn drops(film: &mut Film, app: &mut App, count: usize, between: f64) {
    for _ in 0..count {
        tap(film, app, Named::Space);
        film.play(app, between);
    }
}

fn pour(film: &mut Film, app: &mut App, count: usize) {
    drops(film, app, count, 0.15);
}

fn acid_open(film: &mut Film, app: &mut App) {
    film.mark("open");
    film.shot(app, 2.5);
    command(film, app, "rodeo-play");
    settle(app);
    film.play(app, 5.0);
}

fn acid_strong(film: &mut Film, app: &mut App) {
    film.mark("strong");
    pour(film, app, 13);
    film.play(app, 1.5);
    drops(film, app, 6, 1.1);
    drops(film, app, 1, 2.8);
    drops(film, app, 1, 2.0);
    pour(film, app, 11);
    film.play(app, 2.0);
}

fn acid_weak(film: &mut Film, app: &mut App) {
    film.mark("weak");
    tap_letter(film, app, "w");
    film.play(app, 6.0);
    pour(film, app, 10);
    film.play(app, 3.0);
    pour(film, app, 8);
    film.play(app, 1.0);
    drops(film, app, 2, 1.4);
    film.play(app, 1.5);
    pour(film, app, 12);
    film.play(app, 2.0);
}

fn edit_value(film: &mut Film, app: &mut App, find_text: &str, remove: usize, value: &str) {
    film.press(app, "C-x o");
    find(film, app, find_text);
    film.press(app, &vec!["DEL"; remove].join(" "));
    film.type_slowly(app, value);
    film.press(app, "C-M-x");
    film.play(app, 0.6);
    film.press(app, "C-x o");
}

fn acid_stronger(film: &mut Film, app: &mut App) {
    film.mark("ka");
    edit_value(film, app, "1.8e-5", 6, "6.8e-4");
    tap_letter(film, app, "w");
    film.play(app, 6.0);
    pour(film, app, 16);
    film.play(app, 1.0);
    pour(film, app, 16);
    film.play(app, 2.0);
}

fn acid_indicator(film: &mut Film, app: &mut App) {
    film.mark("indicator");
    edit_value(film, app, ":universal", 9, "phenolphthalein");
    film.play(app, 1.5);
    tap_letter(film, app, "w");
    film.play(app, 2.5);
    pour(film, app, 18);
    drops(film, app, 2, 1.4);
    drops(film, app, 1, 3.0);
}

fn acid_warmer(film: &mut Film, app: &mut App) {
    film.mark("warmer");
    edit_value(film, app, "temperature 300", 3, "900");
    film.play(app, 5.0);
}
#[test]
#[ignore = "renders a 16:9 video of an acid-base titration simulated in rodeo into $OXMACS_ACID_VIDEO"]
fn acid_base_video() {
    let Ok(out) = std::env::var("OXMACS_ACID_VIDEO") else {
        return;
    };
    let out = PathBuf::from(out);
    let home = acid_home();
    PUMPING.set(true);
    let mut film = Film::new(out.with_extension("frames"))
        .sized(Size::new(1280.0, 720.0))
        .scaled_to(1280, 720)
        .ticking(Instant::now());
    let visit = Visit {
        path: home.join("acid-base.ox"),
        line: None,
        column: None,
    };
    let (mut app, booted) = App::boot(&[visit], Some(&home));
    track(booted);
    track(app.update(Message::Focus(true)));
    settle(&mut app);
    acid_open(&mut film, &mut app);
    if std::env::var_os("OXMACS_ACID_SHORT").is_none() {
        acid_strong(&mut film, &mut app);
        acid_weak(&mut film, &mut app);
        acid_stronger(&mut film, &mut app);
        acid_indicator(&mut film, &mut app);
        acid_warmer(&mut film, &mut app);
    }
    film.hold(2.0);
    film.finish(&out);
}
