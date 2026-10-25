use std::path::PathBuf;

use ox::heap::{Heap, Hold};
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::command::ScopeId;
use oxmacs_core::editor::{ConsoleSpec, Editor, JobSpec};
use oxmacs_core::text::BufferId;
use regex::Regex;

use crate::log;

#[derive(Debug)]
struct Known {
    buffer: BufferId,
    output: Option<Hold>,
    idle: Option<Hold>,
    resized: Option<Hold>,
    width: Option<usize>,
}

#[derive(Debug)]
enum Heard {
    Output(BufferId, String, String),
    Idle(BufferId),
}

#[derive(Debug, Default)]
pub struct Consoles {
    known: Vec<Known>,
    heard: Vec<Heard>,
    labels: Vec<&'static str>,
    scopes: Vec<(&'static str, ScopeId)>,
}

fn consoles(editor: &mut Editor) -> Option<&mut Consoles> {
    if editor.plugin_state::<Consoles>().is_none() {
        editor.set_plugin_state(Consoles::default());
    }
    editor.plugin_state_mut::<Consoles>()
}

const ERRORS: &str = "\u{1}errors";

fn heard(editor: &mut Editor, id: BufferId, kind: &str, payload: &str) {
    if kind == ERRORS {
        crate::flashes::send_failed(editor, id);
        return;
    }
    if let Some(consoles) = consoles(editor) {
        consoles
            .heard
            .push(Heard::Output(id, kind.to_owned(), payload.to_owned()));
    }
}

fn idle(editor: &mut Editor, id: BufferId) {
    crate::flashes::send_settled(editor, id);
    if let Some(consoles) = consoles(editor) {
        consoles.heard.push(Heard::Idle(id));
    }
}

fn made(made: Result<Value, ox::heap::HeapFull>) -> Result<Value, Error> {
    made.map_err(|full| Error::new(full.to_string()))
}

fn text(heap: &Heap, native: &str, what: &str, value: Value) -> Result<String, Error> {
    heap.text(value).map(str::to_owned).ok_or_else(|| {
        Error::new(format!(
            "{native} expects {what}, got {}",
            print::brief(heap, value)
        ))
    })
}

fn texts(heap: &Heap, value: Value, wanted: &str) -> Result<Vec<String>, Error> {
    heap.elements(value)
        .filter(|_| heap.text(value).is_none())
        .map(|items| {
            items
                .into_iter()
                .map(|item| print::display(heap, item))
                .collect()
        })
        .ok_or_else(|| {
            Error::new(format!(
                "console-start expects {wanted}, got {}",
                print::brief(heap, value)
            ))
        })
}

fn name_of(heap: &Heap, value: Value) -> String {
    match value {
        Value::Keyword(name) | Value::Symbol(name) => name.name().to_owned(),
        _ => print::display(heap, value),
    }
}

fn pairs(heap: &Heap, value: Value, option: &str) -> Result<Vec<(String, String)>, Error> {
    heap.entries(value)
        .map(|entries| {
            entries
                .into_iter()
                .map(|(key, item)| (name_of(heap, key), name_of(heap, item)))
                .collect()
        })
        .ok_or_else(|| {
            Error::new(format!(
                "console-start expects a map for :{option}, got {}",
                print::brief(heap, value)
            ))
        })
}

#[derive(Default)]
struct Options {
    program: Option<String>,
    args: Vec<String>,
    directory: Option<PathBuf>,
    env: Vec<(String, String)>,
    label: Option<String>,
    keys: Vec<(String, String)>,
    output: Option<Value>,
    idle: Option<Value>,
    resized: Option<Value>,
    prompts: Vec<String>,
    errors: Vec<String>,
    watches: Vec<(String, String)>,
}

fn options(heap: &Heap, given: &[Value]) -> Result<Options, Error> {
    let mut found = Options::default();
    for pair in given.chunks(2) {
        let [Value::Keyword(option), value] = pair else {
            return Err(unknown(heap, pair[0]));
        };
        match option.name() {
            "program" => found.program = Some(text(heap, "console-start", "a program", *value)?),
            "args" => found.args = texts(heap, *value, "a vector of arguments for :args")?,
            "directory" => {
                found.directory = Some(PathBuf::from(text(
                    heap,
                    "console-start",
                    "a folder",
                    *value,
                )?));
            }
            "env" => found.env = pairs(heap, *value, "env")?,
            "label" => found.label = Some(print::display(heap, *value)),
            "keys" => found.keys = pairs(heap, *value, "keys")?,
            "on-output" => found.output = Some(*value),
            "on-idle" => found.idle = Some(*value),
            "on-resize" => found.resized = Some(*value),
            "prompts" => found.prompts = texts(heap, *value, "a vector of patterns for :prompts")?,
            "errors" => found.errors = texts(heap, *value, "a vector of patterns for :errors")?,
            "watch" => found.watches = pairs(heap, *value, "watch")?,
            _ => return Err(unknown(heap, pair[0])),
        }
    }
    Ok(found)
}

fn unknown(heap: &Heap, option: Value) -> Error {
    Error::new(format!(
        "console-start takes :program, :args, :directory, :env, :label, :keys, :on-output, :on-idle, :on-resize, :prompts, :errors and :watch, not {}",
        print::brief(heap, option)
    ))
}

fn leaked(editor: &mut Editor, label: &str) -> &'static str {
    let Some(consoles) = consoles(editor) else {
        return "console";
    };
    if let Some(known) = consoles.labels.iter().find(|known| **known == label) {
        return known;
    }
    let label: &'static str = Box::leak(label.to_owned().into_boxed_str());
    consoles.labels.push(label);
    label
}

fn scope(editor: &mut Editor, label: &'static str) -> ScopeId {
    if let Some((_, scope)) = editor
        .plugin_state::<Consoles>()
        .and_then(|consoles| consoles.scopes.iter().find(|(known, _)| *known == label))
    {
        return *scope;
    }
    let title = leaked(editor, &format!("{label} console"));
    let scope = editor.add_scope(title);
    if let Some(consoles) = consoles(editor) {
        consoles.scopes.push((label, scope));
    }
    scope
}

fn pattern_of(pattern: &str) -> Result<Regex, Error> {
    Regex::new(pattern).map_err(|error| {
        Error::new(format!(
            "console-start can't read the pattern {pattern}: {error}"
        ))
    })
}

fn patterns(given: &[String]) -> Result<Vec<Regex>, Error> {
    given.iter().map(|pattern| pattern_of(pattern)).collect()
}

fn release(heap: &mut Heap, hold: Option<Hold>) {
    if let Some(hold) = hold {
        heap.release(hold);
    }
}

fn start(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = text(heap, "console-start", "a buffer name", arguments[0])?;
    let given = options(heap, &arguments[1..])?;
    let program = given
        .program
        .ok_or_else(|| Error::new("console-start needs a :program"))?;
    let label = leaked(editor, given.label.as_deref().unwrap_or(&name));
    let scope = if given.keys.is_empty() {
        None
    } else {
        let scope = scope(editor, label);
        for (keys, command) in &given.keys {
            let id = editor
                .command_named(command)
                .ok_or_else(|| Error::new(format!("There is no command named {command}")))?;
            editor
                .bind(scope, keys, id)
                .map_err(|error| Error::new(format!("{keys}: {error}")))?;
        }
        Some(scope)
    };
    let existing = editor
        .buffer_named(&name)
        .filter(|id| editor.consoles(label).contains(id));
    let spec = ConsoleSpec {
        prompts: patterns(&given.prompts)?,
        watches: given
            .watches
            .into_iter()
            .chain(
                given
                    .errors
                    .into_iter()
                    .map(|pattern| (ERRORS.to_owned(), pattern)),
            )
            .map(|(kind, pattern)| Ok((kind, pattern_of(&pattern)?)))
            .collect::<Result<_, Error>>()?,
        name: name.clone(),
        label,
        job: JobSpec {
            program,
            args: given.args,
            directory: given
                .directory
                .unwrap_or_else(|| editor.default_directory()),
            env: given.env,
            label,
            name: name.clone(),
            buffer: None,
            interactive: true,
        },
        on_marker: heard,
        on_idle: idle,
        scope,
    };
    let buffer = editor.start_console(spec, existing);
    let output = given.output.map(|function| heap.hold(function));
    let waiting = given.idle.map(|function| heap.hold(function));
    let resized = given.resized.map(|function| heap.hold(function));
    if let Some(consoles) = consoles(editor) {
        if let Some(at) = consoles
            .known
            .iter()
            .position(|known| known.buffer == buffer)
        {
            let old = consoles.known.remove(at);
            release(heap, old.output);
            release(heap, old.idle);
            release(heap, old.resized);
        }
        consoles.known.push(Known {
            buffer,
            output,
            idle: waiting,
            resized,
            width: None,
        });
    }
    made(heap.string(&name))
}

fn console(heap: &Heap, editor: &Editor, native: &str, value: Value) -> Result<BufferId, Error> {
    let name = text(heap, native, "a console's buffer name", value)?;
    editor
        .buffer_named(&name)
        .ok_or_else(|| Error::new(format!("There is no buffer named {name}")))
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("console-start", 1, None, start);
    isolate.define_host_native::<Editor>("console-send", 2, Some(2), |heap, editor, arguments| {
        let id = console(heap, editor, "console-send", arguments[0])?;
        let code = print::display(heap, arguments[1]);
        if let Ok(source) = crate::handles::current(heap, editor)
            && source != id
        {
            crate::flashes::sent(editor, id, source, &code);
        }
        editor.console_evaluate(id, &code);
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>(
        "console-request",
        2,
        Some(2),
        |heap, editor, arguments| {
            let id = console(heap, editor, "console-request", arguments[0])?;
            let call = print::display(heap, arguments[1]);
            Ok(Value::Bool(editor.console_request(id, &call)))
        },
    );
    install_states(isolate);
    install_history(isolate);
}

fn install_states(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>(
        "console-running?",
        1,
        Some(1),
        |heap, editor, arguments| {
            let name = print::display(heap, arguments[0]);
            Ok(Value::Bool(
                editor
                    .buffer_named(&name)
                    .is_some_and(|id| editor.console_running(id)),
            ))
        },
    );
    isolate.define_host_native::<Editor>(
        "console-waiting?",
        1,
        Some(1),
        |heap, editor, arguments| {
            let name = print::display(heap, arguments[0]);
            Ok(Value::Bool(
                editor
                    .buffer_named(&name)
                    .is_some_and(|id| editor.console_waiting(id)),
            ))
        },
    );
    isolate.define_host_native::<Editor>(
        "console-prompt",
        1,
        Some(1),
        |heap, editor, arguments| {
            let name = print::display(heap, arguments[0]);
            match editor
                .buffer_named(&name)
                .and_then(|id| editor.console_prompt(id))
            {
                Some(prompt) => made(heap.string(&prompt)),
                None => Ok(Value::Nil),
            }
        },
    );
    isolate.define_host_native::<Editor>("consoles", 1, Some(1), |heap, editor, arguments| {
        let label = print::display(heap, arguments[0]);
        let names = editor
            .consoles(&label)
            .into_iter()
            .filter_map(|id| {
                editor
                    .buffer_by_id(id)
                    .map(|buffer| buffer.name().to_owned())
            })
            .collect::<Vec<String>>();
        let mut values = Vec::with_capacity(names.len());
        for name in names {
            values.push(made(heap.string(&name))?);
        }
        made(heap.vector_from(&values))
    });
}

fn install_history(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>(
        "console-history",
        1,
        Some(1),
        |heap, editor, arguments| {
            let id = console(heap, editor, "console-history", arguments[0])?;
            let mut items = Vec::new();
            for input in editor.console_history(id) {
                items.push(made(heap.string(&input))?);
            }
            made(heap.vector_from(&items))
        },
    );
    isolate.define_host_native::<Editor>(
        "set-console-history",
        2,
        Some(2),
        |heap, editor, arguments| {
            let id = console(heap, editor, "set-console-history", arguments[0])?;
            let items = heap
                .elements(arguments[1])
                .filter(|_| heap.text(arguments[1]).is_none())
                .ok_or_else(|| {
                    Error::new(format!(
                        "set-console-history expects a vector of inputs, got {}",
                        print::brief(heap, arguments[1])
                    ))
                })?
                .into_iter()
                .map(|item| print::display(heap, item))
                .collect();
            editor.set_console_history(id, items);
            Ok(Value::Nil)
        },
    );
    isolate.define_host_native::<Editor>("console-input-start", 0, Some(0), |_, editor, _| {
        Ok(editor.console_input_start().map_or(Value::Nil, |start| {
            Value::Int(i64::try_from(start).unwrap_or(i64::MAX))
        }))
    });
}

pub fn run(isolate: &mut Isolate, editor: &mut Editor) {
    let heard = consoles(editor)
        .map(|consoles| std::mem::take(&mut consoles.heard))
        .unwrap_or_default();
    for event in heard {
        let (buffer, arguments) = match event {
            Heard::Output(buffer, kind, payload) => {
                let (Ok(kind), Ok(payload)) = (
                    isolate.heap_mut().string(&kind),
                    isolate.heap_mut().string(&payload),
                ) else {
                    continue;
                };
                (buffer, Some(vec![kind, payload]))
            }
            Heard::Idle(buffer) => (buffer, None),
        };
        let hold = editor.plugin_state::<Consoles>().and_then(|consoles| {
            let known = consoles.known.iter().find(|known| known.buffer == buffer)?;
            if arguments.is_some() {
                known.output
            } else {
                known.idle
            }
        });
        let Some(handler) = hold.and_then(|hold| isolate.held(hold)) else {
            continue;
        };
        if let Err(error) = isolate.spawn(handler, &arguments.unwrap_or_default()) {
            log::report(editor, &error, None);
        }
    }
    resized(isolate, editor);
}

fn resized(isolate: &mut Isolate, editor: &mut Editor) {
    let buffers: Vec<BufferId> = editor
        .plugin_state::<Consoles>()
        .map(|consoles| consoles.known.iter().map(|known| known.buffer).collect())
        .unwrap_or_default();
    let mut calls = Vec::new();
    for buffer in buffers {
        let Some(width) = editor.buffer_width(buffer) else {
            continue;
        };
        let Some(known) = editor.plugin_state_mut::<Consoles>().and_then(|consoles| {
            consoles
                .known
                .iter_mut()
                .find(|known| known.buffer == buffer)
        }) else {
            continue;
        };
        if known.width != Some(width) {
            known.width = Some(width);
            calls.extend(known.resized.map(|hold| (hold, width)));
        }
    }
    for (hold, width) in calls {
        let Some(handler) = isolate.held(hold) else {
            continue;
        };
        let width = Value::Int(i64::try_from(width).unwrap_or(i64::MAX));
        if let Err(error) = isolate.spawn(handler, &[width]) {
            log::report(editor, &error, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::{Editor, Effect, JobId, Outcome};

    use crate::Ox;
    use crate::testing::{press, settle, typed};

    const PROMPT: &str = "\u{1}>\u{2}$ \u{3}";

    fn started(source: &str) -> (Editor, Ox, JobId) {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, source).unwrap();
        settle(&mut editor, &mut ox);
        let job = editor
            .take_effects()
            .into_iter()
            .find_map(|effect| match effect {
                Effect::StartSession(job) => Some(job),
                _ => None,
            })
            .unwrap();
        assert_eq!(job.program, "shell");
        assert_eq!(job.args, ["-i"]);
        assert_eq!(job.env, [("TERM".to_owned(), "dumb".to_owned())]);
        (editor, ox, job.id)
    }

    const SHELL: &str = r#"(def heard (atom []))
(defcommand shell-send "Send" (run-command 'comint-send-input))
(def (start)
  (console-start "*shell*" :program "shell" :args ["-i"] :env {TERM: "dumb"} :label "Shell"
                 :keys {"RET" 'shell-send}
                 :on-output (fn [kind payload] (swap! heard conj [kind payload]))
                 :on-idle (fn [] (swap! heard conj :idle))))
(start)"#;

    fn sent(editor: &mut Editor) -> Vec<String> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Send(_, text) => Some(text),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn plugins_start_consoles_and_hear_their_markers_and_prompts() {
        let (mut editor, mut ox, job) = started(SHELL);

        editor.session_output(job, &format!("hi\n\u{1}note\u{2}one\u{3}{PROMPT}"));
        settle(&mut editor, &mut ox);

        assert_eq!(
            ox.eval(&mut editor, "@heard").unwrap(),
            r#"[["note" "one"] :idle]"#
        );
        assert_eq!(
            editor
                .buffer_by_id(editor.buffer_named("*shell*").unwrap())
                .unwrap()
                .contents(),
            "hi\n$ "
        );
        assert_eq!(
            ox.eval(
                &mut editor,
                r#"[(consoles "Shell") (console-running? "*shell*") (console-waiting? "*shell*") (console-running? "*scratch*")]"#
            )
            .unwrap(),
            r#"[["*shell*"] true true false]"#
        );
    }

    #[test]
    fn consoles_know_prompts_and_watched_lines_by_their_patterns() {
        let (mut editor, mut ox, job) = started(
            r##"(def heard (atom []))
(console-start "*shell*" :program "shell" :args ["-i"] :env {TERM: "dumb"}
               :prompts [#r#"^Browse\[\d+\]> $"#] :watch {debug: "^debug at "}
               :on-output (fn [kind payload] (swap! heard conj [kind payload]))
               :on-idle (fn [] (swap! heard conj :idle)))"##,
        );

        editor.session_output(job, "debug at a.R#2: x\nBrowse[1]> ");
        settle(&mut editor, &mut ox);

        assert_eq!(
            ox.eval(&mut editor, "@heard").unwrap(),
            r#"[["debug" "debug at a.R#2: x"] :idle]"#
        );
        assert_eq!(
            ox.eval(
                &mut editor,
                r#"[(console-prompt "*shell*") (console-prompt "*scratch*")]"#
            )
            .unwrap(),
            r#"["Browse[1]> " nil]"#
        );
        assert!(
            ox.eval(
                &mut editor,
                r#"(try (console-start "*bad*" :program "x" :prompts ["("]) (catch error e e.message))"#
            )
            .unwrap()
            .contains("console-start can't read the pattern (")
        );
    }

    #[test]
    fn consoles_hear_when_their_window_changes_width() {
        let (mut editor, mut ox, _) = started(SHELL);
        ox.eval(
            &mut editor,
            r#"(def widths (atom [])) (console-start "*shell*" :program "shell" :args ["-i"] :env {TERM: "dumb"} :on-resize (fn [width] (swap! widths conj width)))"#,
        )
        .unwrap();
        press(&mut editor, &mut ox, "C-x o");

        editor.resize_pane(editor.selected_window(), 20, 41);
        settle(&mut editor, &mut ox);
        editor.resize_pane(editor.selected_window(), 30, 41);
        settle(&mut editor, &mut ox);
        editor.resize_pane(editor.selected_window(), 30, 51);
        settle(&mut editor, &mut ox);

        assert_eq!(ox.eval(&mut editor, "@widths").unwrap(), "[40 50]");
    }

    #[test]
    fn plugins_read_and_give_back_console_history() {
        let (mut editor, mut ox, job) = started(SHELL);
        editor.session_output(job, PROMPT);
        settle(&mut editor, &mut ox);
        press(&mut editor, &mut ox, "C-x o");
        typed(&mut editor, &mut ox, "ls");
        press(&mut editor, &mut ox, "RET");

        assert_eq!(
            ox.eval(&mut editor, r#"(console-history "*shell*")"#)
                .unwrap(),
            r#"["ls"]"#
        );
        ox.eval(
            &mut editor,
            r#"(set-console-history "*shell*" ["old" "older"])"#,
        )
        .unwrap();
        assert_eq!(
            ox.eval(&mut editor, r#"(console-history "*shell*")"#)
                .unwrap(),
            r#"["old" "older"]"#
        );
    }

    #[test]
    fn code_is_sent_echoed_or_asked_quietly() {
        let (mut editor, mut ox, job) = started(SHELL);
        editor.session_output(job, PROMPT);
        settle(&mut editor, &mut ox);

        ox.eval(&mut editor, r#"(console-send "*shell*" "ls\npwd")"#)
            .unwrap();
        assert_eq!(sent(&mut editor), ["ls\npwd\n"]);
        assert_eq!(
            ox.eval(&mut editor, r#"(console-request "*shell*" "quiet()")"#)
                .unwrap(),
            "true"
        );
        assert_eq!(sent(&mut editor), ["quiet()\n"]);
        editor.session_output(job, &format!("{PROMPT}{PROMPT}"));
        let id = editor.buffer_named("*shell*").unwrap();
        assert_eq!(editor.buffer_by_id(id).unwrap().contents(), "$ ls\n$ pwd\n");
    }

    #[test]
    fn an_error_line_after_a_send_reports_the_code_again_as_failed() {
        let (mut editor, mut ox, job) = started(
            r#"(console-start "*shell*" :program "shell" :args ["-i"] :env {TERM: "dumb"}
               :prompts ["^> $"] :errors ["^Error"])"#,
        );
        editor.session_output(job, "> ");
        settle(&mut editor, &mut ox);
        editor.buffer_mut().insert_at(0, "ok\nboom\nlate\n");
        ox.eval(
            &mut editor,
            "(def ran (atom [])) (on-evaluate :test (fn [b start end failed] (swap! ran conj [start end failed])))",
        )
        .unwrap();
        let send = |editor: &mut Editor, ox: &mut Ox, code: &str, output: &str| {
            ox.eval(editor, &format!("(console-send \"*shell*\" \"{code}\")"))
                .unwrap();
            editor.session_output(job, output);
            settle(editor, ox);
        };

        send(&mut editor, &mut ox, "ok", "fine\n> ");
        send(&mut editor, &mut ox, "boom", "Error: boom\n> ");
        send(&mut editor, &mut ox, "late", "> ");
        editor.session_output(job, "Error: from earlier\n> ");
        settle(&mut editor, &mut ox);

        assert_eq!(
            ox.eval(&mut editor, "@ran").unwrap(),
            "[[0 2 false] [3 7 false] [3 7 true] [8 12 false]]"
        );
    }

    #[test]
    fn code_sent_from_a_buffer_that_holds_it_is_reported_where_it_sits() {
        let (mut editor, mut ox, job) = started(SHELL);
        editor.session_output(job, PROMPT);
        settle(&mut editor, &mut ox);
        editor.buffer_mut().insert_at(0, "ls\npwd\nls\n");
        editor.buffer_mut().set_point(3);
        ox.eval(
            &mut editor,
            "(def ran (atom [])) (on-evaluate :test (fn [b start end failed] (swap! ran conj [(with-buffer b (buffer-name)) start end failed])))",
        )
        .unwrap();

        ox.eval(
            &mut editor,
            r#"(console-send "*shell*" "ls\n") (console-send "*shell*" "echo") (with-buffer "*shell*" (console-send "*shell*" "pwd"))"#,
        )
        .unwrap();
        settle(&mut editor, &mut ox);

        assert_eq!(
            ox.eval(&mut editor, "@ran").unwrap(),
            r#"[["*scratch*" 0 2 false]]"#
        );
        assert_eq!(sent(&mut editor), ["ls\n", "echo\n", "pwd\n"]);
    }

    #[test]
    fn console_keys_belong_to_the_console_and_restarting_keeps_the_buffer() {
        let (mut editor, mut ox, job) = started(SHELL);
        editor.session_output(job, PROMPT);
        settle(&mut editor, &mut ox);
        let id = editor.buffer_named("*shell*").unwrap();
        press(&mut editor, &mut ox, "C-x o");
        assert_eq!(editor.buffer().id(), id);

        typed(&mut editor, &mut ox, "ls");
        assert_eq!(ox.eval(&mut editor, "(console-input-start)").unwrap(), "2");
        press(&mut editor, &mut ox, "RET");
        assert_eq!(sent(&mut editor), ["ls\n"]);

        editor.job_finished(job, &Outcome::Exited(Some(0)));
        assert_eq!(
            ox.eval(&mut editor, r#"(console-running? "*shell*")"#)
                .unwrap(),
            "false"
        );
        ox.eval(&mut editor, "(start)").unwrap();
        assert_eq!(editor.buffer_named("*shell*"), Some(id));
        assert_eq!(
            ox.eval(&mut editor, r#"(console-running? "*shell*")"#)
                .unwrap(),
            "true"
        );
        assert!(
            ox.eval(&mut editor, r#"(console-start "*x*" :args [])"#)
                .unwrap_err()
                .message
                .contains("needs a :program")
        );
    }
}
