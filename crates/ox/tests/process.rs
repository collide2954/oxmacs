use ox::Isolate;

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
    let mut isolate = Isolate::new();

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
    let mut isolate = Isolate::new();

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
fn errors_arrive_on_their_own_channel() {
    let mut isolate = Isolate::new();

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
fn started_processes_can_be_killed() {
    let mut isolate = Isolate::new();
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
fn programs_that_cannot_start_raise() {
    let mut isolate = Isolate::new();

    let reply = evaluated(
        &mut isolate,
        "(module (import (ox process))) (try (process/start \"oxmacs-no-such-program\" []) (catch error e e.message))",
    );

    assert!(
        reply.starts_with("\"Couldn't run oxmacs-no-such-program: "),
        "{reply}"
    );
}
