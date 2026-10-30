use std::sync::{Arc, Mutex, PoisonError};

use ox_lang::error::ErrorKind;
use ox_lang::heap::{Heap, Hold};
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::{Editor, Offer};

#[derive(Debug)]
pub(crate) enum Answer {
    Text(String),
    Choice(bool),
    Failed(String),
    Quit,
}

pub(crate) type Inbox = Arc<Mutex<Vec<(Hold, Answer)>>>;

pub(crate) struct Reply {
    hold: Hold,
    inbox: Inbox,
    sent: bool,
}

impl Reply {
    fn post(&self, answer: Answer) {
        self.inbox
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((self.hold, answer));
    }

    pub(crate) fn send(mut self, answer: Answer) {
        self.post(answer);
        self.sent = true;
    }
}

impl Drop for Reply {
    fn drop(&mut self) {
        if !self.sent {
            self.post(Answer::Quit);
        }
    }
}

pub(crate) fn reply(heap: &mut Heap, inbox: &Inbox) -> Result<(Value, Reply), Error> {
    let promise = heap.new_promise()?;
    let hold = heap.hold(promise);
    Ok((
        promise,
        Reply {
            hold,
            inbox: Arc::clone(inbox),
            sent: false,
        },
    ))
}

fn offers(heap: &Heap, value: Value) -> Result<Vec<Offer>, Error> {
    let name = |value: Value| match value {
        Value::Keyword(name) | Value::Symbol(name) => Some(name.name().to_owned()),
        _ => heap.text(value).map(str::to_owned),
    };
    let found = match heap.entries(value) {
        Some(entries) => entries
            .into_iter()
            .map(|(key, note)| Some(Offer::noted(name(key)?, print::display(heap, note))))
            .collect(),
        None => heap.elements(value).and_then(|items| {
            items
                .into_iter()
                .map(|item| name(item).map(Offer::from))
                .collect()
        }),
    };
    found.ok_or_else(|| {
        Error::new(format!(
            ":choices must be a vector of names or a map of names to notes, got {}",
            print::brief(heap, value)
        ))
    })
}

fn shown(heap: &Heap, value: Value) -> String {
    match value {
        Value::Nil => String::new(),
        given => print::display(heap, given),
    }
}

fn prompt(
    heap: &mut Heap,
    editor: &mut Editor,
    arguments: &[Value],
    inbox: &Inbox,
) -> Result<Value, Error> {
    let question = print::display(heap, arguments[0]);
    let initial = shown(heap, arguments[1]);
    let choices = match arguments[2] {
        Value::Nil => None,
        offered => Some(offers(heap, offered)?),
    };
    let secret = arguments[3].truthy();
    let (promise, reply) = reply(heap, inbox)?;
    let done = move |_: &mut Editor, text| reply.send(Answer::Text(text));
    match choices {
        _ if secret => editor.prompt_secret(&question, done),
        Some(offered) => editor.prompt_with(&question, move |_| offered.clone(), done),
        None => editor.prompt_with_text(&question, &initial, done),
    }
    Ok(promise)
}

pub fn install(isolate: &mut Isolate, inbox: &Inbox) {
    let asking = Arc::clone(inbox);
    isolate
        .native("ox", "prompt")
        .suspends()
        .arity(1, Some(1))
        .keywords(&["initial", "choices", "secret"])
        .host::<Editor>(move |heap, editor, arguments| prompt(heap, editor, arguments, &asking));
    let locating = Arc::clone(inbox);
    isolate
        .native("ox", "prompt-path")
        .suspends()
        .arity(1, Some(1))
        .keywords(&["directories", "initial"])
        .host::<Editor>(move |heap, editor, arguments| {
            let question = print::display(heap, arguments[0]);
            let directories = arguments[1].truthy();
            let initial = shown(heap, arguments[2]);
            let (promise, reply) = reply(heap, &locating)?;
            editor.prompt_path_with(&question, directories, &initial, move |_, path| {
                reply.send(Answer::Text(path.display().to_string()));
            });
            Ok(promise)
        });
    let deciding = Arc::clone(inbox);
    isolate
        .native("ox", "yes-or-no")
        .suspends()
        .arity(1, Some(1))
        .host::<Editor>(move |heap, editor, arguments| {
            let question = print::display(heap, arguments[0]);
            let (promise, reply) = reply(heap, &deciding)?;
            editor.ask(&question, move |_, yes| reply.send(Answer::Choice(yes)));
            Ok(promise)
        });
}

pub fn deliver(isolate: &mut Isolate, inbox: &Inbox) {
    let answers = std::mem::take(&mut *inbox.lock().unwrap_or_else(PoisonError::into_inner));
    for (hold, answer) in answers {
        let Some(promise) = isolate.release(hold) else {
            continue;
        };
        match answer {
            Answer::Text(text) => {
                if let Ok(value) = isolate.heap_mut().string(&text) {
                    isolate.resolve(promise, value);
                }
            }
            Answer::Choice(yes) => {
                isolate.resolve(promise, Value::Bool(yes));
            }
            Answer::Failed(message) => {
                isolate.reject(promise, &Error::new(message));
            }
            Answer::Quit => {
                isolate.reject(promise, &Error::new("Quit").of(ErrorKind::Quit));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

    use crate::Ox;
    use crate::testing::{press, settle};

    fn run(editor: &mut Editor, ox: &mut Ox, source: &str, command: &str) {
        ox.eval(editor, source).unwrap();
        let id = editor.command_named(command).unwrap();
        editor.execute(id);
        settle(editor, ox);
    }

    #[test]
    fn commands_can_await_an_answer_from_the_minibuffer() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        run(
            &mut editor,
            &mut ox,
            r#"(defcommand rename "Rename" (message (str "to " (await (prompt "New name: " :initial "old")))))"#,
            "rename",
        );
        assert_eq!(editor.minibuffer().unwrap().input().contents(), "old");
        press(&mut editor, &mut ox, "x RET");

        assert_eq!(editor.message(), Some("to oldx"));
    }

    #[test]
    fn prompts_can_offer_choices_with_notes() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        run(
            &mut editor,
            &mut ox,
            r#"(defcommand pick "Pick" (message (str "picked " (await (prompt "Kind: " :choices {starfield: "Stars" matrix: "Rain"})))))"#,
            "pick",
        );
        let offered: Vec<(String, Option<String>)> = editor
            .completions()
            .into_iter()
            .map(|completion| (completion.label, completion.annotation))
            .collect();
        assert_eq!(
            offered,
            [
                ("starfield".to_owned(), Some("Stars".to_owned())),
                ("matrix".to_owned(), Some("Rain".to_owned()))
            ]
        );
        press(&mut editor, &mut ox, "m a RET");
        assert_eq!(editor.message(), Some("picked matrix"));

        run(
            &mut editor,
            &mut ox,
            r#"(defcommand plain "Plain" (message (await (prompt "Fruit: " :choices ["apple" :banana]))))"#,
            "plain",
        );
        press(&mut editor, &mut ox, "b RET");
        assert_eq!(editor.message(), Some("banana"));
        assert_eq!(
            ox.eval(&mut editor, r#"(prompt "Fruit: " :choices 5)"#)
                .unwrap_err()
                .to_string(),
            ":choices must be a vector of names or a map of names to notes, got 5"
        );
    }

    #[test]
    fn secret_prompts_take_passwords() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        run(
            &mut editor,
            &mut ox,
            r#"(defcommand unlock "Unlock" (message (str "got " (count (await (prompt "Password: " :secret true))))))"#,
            "unlock",
        );
        press(&mut editor, &mut ox, "p w RET");

        assert_eq!(editor.message(), Some("got 2"));
    }

    #[test]
    fn yes_or_no_answers_true_or_false() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        run(
            &mut editor,
            &mut ox,
            r#"(defcommand ask "Ask" (message (if (await (yes-or-no "Sure?")) "yes" "no")))"#,
            "ask",
        );
        assert!(editor.question().is_some());
        press(&mut editor, &mut ox, "y");
        assert_eq!(editor.message(), Some("yes"));

        let id = editor.command_named("ask").unwrap();
        editor.execute(id);
        settle(&mut editor, &mut ox);
        press(&mut editor, &mut ox, "n");
        assert_eq!(editor.message(), Some("no"));
    }

    #[test]
    fn paths_are_asked_for_with_completion() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir(folder.path().join("inner")).unwrap();
        let mut editor = Editor::new();
        editor.open_file(&folder.path().join("scratch.txt"));
        let mut ox = Ox::new();

        run(
            &mut editor,
            &mut ox,
            r#"(defcommand where "Where" (message (str "at " (await (prompt-path "Directory: " :directories true)))))"#,
            "where",
        );
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Directory: ");
        press(&mut editor, &mut ox, "i n n e r RET RET");

        let expected = format!("at {}", folder.path().join("inner").display());
        assert_eq!(editor.message(), Some(expected.as_str()));
    }

    #[test]
    fn paths_can_start_with_a_name_to_save_as() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        editor.open_file(&folder.path().join("scratch.txt"));
        let mut ox = Ox::new();

        run(
            &mut editor,
            &mut ox,
            r#"(defcommand save-as "Save as" (message (str "to " (await (prompt-path "Save as: " :initial "plot-1.png")))))"#,
            "save-as",
        );
        let input = editor.minibuffer().unwrap().input().contents();
        assert!(input.ends_with("plot-1.png"), "{input}");
        press(&mut editor, &mut ox, "RET");

        let expected = format!("to {}", folder.path().join("plot-1.png").display());
        assert_eq!(editor.message(), Some(expected.as_str()));
    }

    #[test]
    fn quitting_a_prompt_raises_quit_which_only_catch_quit_stops() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        run(
            &mut editor,
            &mut ox,
            r#"(defcommand careful "Careful" (try (try (await (prompt "Name: ")) (catch error e (message "error"))) (catch quit e (message $"stopped: {e.message}"))))"#,
            "careful",
        );
        press(&mut editor, &mut ox, "C-g");

        assert_eq!(editor.message(), Some("stopped: Quit"));
    }

    #[test]
    fn a_quit_nothing_catches_ends_the_command_quietly() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        run(
            &mut editor,
            &mut ox,
            r#"(defcommand rename "Rename" (message (str "to " (await (prompt "New name: ")))))"#,
            "rename",
        );
        press(&mut editor, &mut ox, "C-g");

        assert_eq!(editor.message(), Some("Quit"));
        assert_eq!(crate::ide::log::reports(&editor), 0);
    }

    #[test]
    fn commands_run_as_tasks_and_report_their_errors() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        run(
            &mut editor,
            &mut ox,
            r#"(def gate (promise)) (defcommand wait "Wait" (message (str "got " (await gate)))) (defcommand broken "Broken" (/ 1 0))"#,
            "wait",
        );
        assert_eq!(editor.message(), None);
        ox.eval(&mut editor, r#"(deliver gate "it")"#).unwrap();
        settle(&mut editor, &mut ox);
        assert_eq!(editor.message(), Some("got it"));

        let id = editor.command_named("broken").unwrap();
        editor.execute(id);
        settle(&mut editor, &mut ox);
        assert_eq!(editor.message(), Some("Division by zero"));
    }
}
