use ox_lang::Isolate;

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
    let _ = isolate.run(std::time::Duration::from_millis(10));
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
