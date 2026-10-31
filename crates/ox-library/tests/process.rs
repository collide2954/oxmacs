use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ox_lang::Isolate;
use ox_lang::sink::{Sink, Started};

#[derive(Debug, Default)]
struct Heard {
    started: Mutex<Vec<(String, Vec<String>, u32)>>,
    ended: Mutex<Vec<(u64, Option<i32>)>>,
    ready: Mutex<usize>,
}

impl Sink for Heard {
    fn process_started(&self, process: Started) -> Option<u64> {
        assert!(process.handle.is::<oxmacs_process::Process>());
        self.started
            .lock()
            .unwrap()
            .push((process.program, process.args, process.pid));
        Some(41)
    }

    fn process_ended(&self, id: u64, code: Option<i32>) {
        self.ended.lock().unwrap().push((id, code));
    }

    fn work_ready(&self) {
        *self.ready.lock().unwrap() += 1;
    }

    fn printed(&self, _: &str) {}
}

fn soon(done: impl Fn() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn evaluated(isolate: &mut Isolate, source: &str) -> String {
    match isolate.eval(source) {
        Ok(value) => isolate.write(value),
        Err(error) => format!("error: {}", error.message),
    }
}

fn echo() -> &'static str {
    if cfg!(windows) {
        r#"(process/start "findstr" ["^"])"#
    } else {
        r#"(process/start "cat" [])"#
    }
}

#[test]
fn run_collects_the_output_and_status() {
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox process) (ox string))) (let [done (await (process/run {:?} [\"--version\"]))] [done.status (string/starts-with? done.stdout \"cargo \")])",
            env!("CARGO")
        ),
    );

    assert_eq!(reply, "[0 true]");
}

#[test]
fn programs_started_in_a_folder_get_the_environment_given_to_it() {
    let mut isolate = ox_library::isolate();
    let folder = tempfile::tempdir().unwrap();
    let inside = folder.path().join("src");
    std::fs::create_dir(&inside).unwrap();
    let place = folder.path().display().to_string();
    let (program, args) = if cfg!(windows) {
        ("cmd", r#"["/C" "echo %OX_FOLDER_TEST%"]"#)
    } else {
        ("sh", r#"["-c" "echo $OX_FOLDER_TEST"]"#)
    };
    let shown = format!(
        r#"(module (import (ox process) (ox string))) (string/trim (get (await (process/run "{program}" {args} :cwd {:?})) :stdout))"#,
        inside.display().to_string()
    );

    evaluated(
        &mut isolate,
        &format!(
            r#"(module (import (ox process))) (process/environment! {place:?} {{OX_FOLDER_TEST: "from the folder"}})"#
        ),
    );
    let given = evaluated(&mut isolate, &shown);
    evaluated(
        &mut isolate,
        &format!("(module (import (ox process))) (process/environment! {place:?} nil)"),
    );
    let taken = evaluated(&mut isolate, &shown);

    assert_eq!(given, r#""from the folder""#);
    assert_ne!(taken, given);
    assert!(
        evaluated(
            &mut isolate,
            "(module (import (ox process))) (process/environment! \"x\" [1])"
        )
        .contains("process/environment! expects a map of variable names to values, or nil")
    );
}

#[test]
fn started_processes_take_input_and_stream_lines() {
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox process)))
(def shell {})
(process/write shell \"b\\na\\n\")
(process/close-input shell)
[(receive shell.lines) (receive shell.lines) (receive shell.lines) (await shell.exit) (process/write shell \"late\")]",
            echo()
        ),
    );

    assert_eq!(reply, r#"["b" "a" nil 0 false]"#);
}

#[test]
fn started_processes_read_their_input_and_then_see_it_end() {
    let mut isolate = ox_library::isolate();
    let program = if cfg!(windows) {
        r#""findstr" ["^"]"#
    } else {
        r#""cat" []"#
    };

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox process)))
(def fed (process/start {program} :input \"b\\na\\n\"))
[(receive fed.lines) (receive fed.lines) (receive fed.lines) (await fed.exit) (process/write fed \"late\")]"
        ),
    );

    assert_eq!(reply, r#"["b" "a" nil 0 false]"#);
}

#[test]
fn output_that_is_not_utf8_keeps_streaming() {
    let mut isolate = ox_library::isolate();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("bytes.txt");
    std::fs::write(&file, b"\xffz\nok\n").unwrap();
    let program = if cfg!(windows) {
        format!("\"cmd\" [\"/C\" \"type\" {:?}]", file.display().to_string())
    } else {
        format!("\"cat\" [{:?}]", file.display().to_string())
    };

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox process)))
(def shown (process/start {program}))
[(receive shown.lines) (receive shown.lines) (receive shown.lines)]"
        ),
    );

    assert_eq!(reply, "[\"\u{fffd}z\" \"ok\" nil]");
}

#[test]
fn errors_arrive_on_their_own_channel() {
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox process) (ox string)))
(def broken (process/start {:?} [\"--oxmacs-no-such-flag\"]))
(def (drain channel found) (if-let [line (receive channel)] (drain channel (conj found line)) found))
(def said (drain broken.errors []))
[(some (fn [line] (string/includes? line \"oxmacs-no-such-flag\")) said) (not= (await broken.exit) 0)]",
            env!("CARGO")
        ),
    );

    assert_eq!(reply, "[true true]");
}

#[test]
fn processes_nobody_waits_for_let_the_isolate_rest() {
    let mut isolate = ox_library::isolate();
    let long = if cfg!(windows) {
        r#"(process/start "ping" ["-n" "60" "127.0.0.1"])"#
    } else {
        r#"(process/start "sleep" ["60"])"#
    };
    isolate
        .eval(&format!(
            "(module (import (ox process))) (def running {long}) nil"
        ))
        .unwrap();
    assert_eq!(isolate.next_wake(), ox_lang::task::Wake::Never);

    isolate.eval("(spawn (await running.exit))").unwrap();
    let _ = isolate.run(Duration::from_millis(10));
    assert_eq!(isolate.next_wake(), ox_lang::task::Wake::Soon);

    isolate
        .eval("(module (import (ox process))) (process/kill running)")
        .unwrap();
}

#[test]
fn started_processes_can_be_killed() {
    let mut isolate = ox_library::isolate();
    let long = if cfg!(windows) {
        r#"(process/start "ping" ["-n" "60" "127.0.0.1"])"#
    } else {
        r#"(process/start "sleep" ["60"])"#
    };

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox process)))
(def waiting {long})
(process/kill waiting)
(await waiting.exit)
:stopped"
        ),
    );

    assert_eq!(reply, ":stopped");
}

#[test]
fn started_processes_can_be_asked_to_stop() {
    let mut isolate = ox_library::isolate();
    let long = if cfg!(windows) {
        r#"(process/start "ping" ["-n" "60" "127.0.0.1"])"#
    } else {
        r#"(process/start "sleep" ["60"])"#
    };

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox process)))
(def waiting {long})
(process/stop waiting)
(await waiting.exit)
:stopped"
        ),
    );

    assert_eq!(reply, ":stopped");
}

#[test]
fn programs_are_found_on_the_path_and_the_system_is_named() {
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        "(module (import (ox process) (ox path) (ox string))) [(string/starts-with? (path/name (process/which \"cargo\")) \"cargo\") (process/which \"oxmacs-no-such-program\") (keyword? (process/os))]",
    );

    assert_eq!(reply, "[true nil true]");
    let os = evaluated(&mut isolate, "(module (import (ox process))) (process/os)");
    assert_eq!(os, format!(":{}", std::env::consts::OS));
}

#[test]
fn started_processes_are_reported_to_the_sink_with_the_id_it_gives() {
    let heard = Arc::new(Heard::default());
    let mut isolate = ox_library::isolate();
    isolate.set_sink(heard.clone());

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox process))) (def shown (process/start {:?} [\"--version\"])) [shown.id (await shown.exit)]",
            env!("CARGO")
        ),
    );

    assert_eq!(reply, "[41 0]");
    let started = heard.started.lock().unwrap().clone();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].0, env!("CARGO"));
    assert_eq!(started[0].1, ["--version"]);
    assert!(soon(|| *heard.ended.lock().unwrap() == [(41, Some(0))]));
}

#[test]
fn finished_background_work_wakes_the_sink() {
    let heard = Arc::new(Heard::default());
    let mut isolate = ox_library::isolate();
    isolate.set_sink(heard.clone());

    isolate
        .eval(&format!(
            "(module (import (ox process))) (def running (process/run {:?} [\"--version\"])) nil",
            env!("CARGO")
        ))
        .unwrap();

    assert!(soon(|| *heard.ready.lock().unwrap() > 0));
}

#[test]
fn without_a_sink_started_processes_have_no_id() {
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        &format!(
            "(module (import (ox process))) (def shown (process/start {:?} [\"--version\"])) [shown.id (await shown.exit)]",
            env!("CARGO")
        ),
    );

    assert_eq!(reply, "[nil 0]");
}

#[test]
fn programs_that_cannot_start_raise() {
    let mut isolate = ox_library::isolate();

    let reply = evaluated(
        &mut isolate,
        "(module (import (ox process))) (try (process/start \"oxmacs-no-such-program\" []) (catch error e e.message))",
    );

    assert!(
        reply.starts_with("\"Couldn't run oxmacs-no-such-program: "),
        "{reply}"
    );
}
