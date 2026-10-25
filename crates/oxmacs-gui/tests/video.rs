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
use oxmacs_gui::app::{App, Message};
use oxmacs_gui::fonts;
use oxmacs_ox::views::{Node, ViewBuffer};

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
    size: Size,
    uploads: usize,
    output: Option<(u32, u32)>,
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
        }
    }

    fn sized(self, size: Size) -> Self {
        Self { size, ..self }
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

fn key(app: &mut App, key: Key, text: Option<&str>, modifiers: Modifiers) {
    track(app.update(Message::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: Physical::Code(Code::KeyA),
        location: Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    })));
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
  (ui/column :spacing 6
    (ui/text (str "Studio   reload " @reloads) :face :heading)
    (if-let [picture @shown]
      (ui/image picture)
      (ui/text "Rendering..." :face :comment))))

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
    fn found(node: &Node) -> Option<usize> {
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
    let (_, view) = editor.plugin_buffer::<ViewBuffer>(id)?;
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
        let visit = oxmacs_core::cli::Visit {
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

fn images(node: &Node) -> usize {
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
        .filter_map(|id| editor.plugin_buffer::<ViewBuffer>(id))
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
