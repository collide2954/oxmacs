use std::sync::{Arc, Mutex, PoisonError};

use ox::heap::{Heap, Hold};
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::editor::Editor;

#[derive(Debug)]
pub(crate) enum Answer {
    Text(String),
    Choice(bool),
    Quit,
}

pub(crate) type Inbox = Arc<Mutex<Vec<(Hold, Answer)>>>;

struct Reply {
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

    fn send(mut self, answer: Answer) {
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

fn reply(heap: &mut Heap, inbox: &Inbox) -> Result<(Value, Reply), Error> {
    let promise = heap
        .new_promise()
        .map_err(|full| Error::new(full.to_string()))?;
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

fn prompt(
    heap: &mut Heap,
    editor: &mut Editor,
    arguments: &[Value],
    inbox: &Inbox,
) -> Result<Value, Error> {
    let question = print::display(heap, arguments[0]);
    let mut initial = String::new();
    for pair in arguments[1..].chunks(2) {
        match pair {
            [Value::Keyword(option), value] if option.name() == "initial" => {
                initial = print::display(heap, *value);
            }
            _ => {
                return Err(Error::new(format!(
                    "prompt takes :initial, not {}",
                    print::write(heap, pair[0])
                )));
            }
        }
    }
    let (promise, reply) = reply(heap, inbox)?;
    editor.prompt_with_text(&question, &initial, move |_, text| {
        reply.send(Answer::Text(text));
    });
    Ok(promise)
}

pub fn install(isolate: &mut Isolate, inbox: &Inbox) {
    let asking = Arc::clone(inbox);
    isolate.define_host_native::<Editor>("prompt", 1, None, move |heap, editor, arguments| {
        prompt(heap, editor, arguments, &asking)
    });
    let deciding = Arc::clone(inbox);
    isolate.define_host_native::<Editor>(
        "yes-or-no",
        1,
        Some(1),
        move |heap, editor, arguments| {
            let question = print::display(heap, arguments[0]);
            let (promise, reply) = reply(heap, &deciding)?;
            editor.ask(&question, move |_, yes| reply.send(Answer::Choice(yes)));
            Ok(promise)
        },
    );
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
            Answer::Quit => {
                isolate.reject(promise, "Quit");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::input::KeySequence;

    use crate::Ox;

    fn press(editor: &mut Editor, ox: &mut Ox, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
        ox.tick(editor);
    }

    fn run(editor: &mut Editor, ox: &mut Ox, source: &str, command: &str) {
        ox.eval(editor, source).unwrap();
        let id = editor.command_named(command).unwrap();
        editor.execute(id);
        ox.tick(editor);
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
        ox.tick(&mut editor);
        press(&mut editor, &mut ox, "n");
        assert_eq!(editor.message(), Some("no"));
    }

    #[test]
    fn quitting_a_prompt_raises_quit_in_the_task() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        run(
            &mut editor,
            &mut ox,
            r#"(defcommand careful "Careful" (try (await (prompt "Name: ")) (catch error e (message "stopped: {e.message}"))))"#,
            "careful",
        );
        press(&mut editor, &mut ox, "C-g");

        assert_eq!(editor.message(), Some("stopped: Quit"));
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
        ox.tick(&mut editor);
        assert_eq!(editor.message(), Some("got it"));

        let id = editor.command_named("broken").unwrap();
        editor.execute(id);
        ox.tick(&mut editor);
        assert_eq!(editor.message(), Some("Ox: Division by zero"));
    }
}
