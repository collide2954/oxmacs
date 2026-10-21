use std::mem;

use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::editor::{Editor, InputLine};
use oxmacs_core::text::BufferId;

use crate::script::{self, Origin, Session};

pub(crate) const NAME: &str = "*Script*";
const PROMPT: &str = "> ";

#[derive(Debug)]
pub(crate) struct ConsoleScope(pub(crate) ScopeId);

#[derive(Debug)]
struct ConsoleView {
    scope: ScopeId,
    input: InputLine,
    session: Session,
    working: BufferId,
}

impl AppletBuffer for ConsoleView {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn input_line(&mut self) -> Option<&mut InputLine> {
        Some(&mut self.input)
    }
}

pub(crate) fn open(editor: &mut Editor) {
    let from = editor.buffer().id();
    if let Some(id) = editor.find_applet_buffer::<ConsoleView>(|_| true) {
        if id != from
            && let Some((_, view)) = editor.applet_buffer_mut::<ConsoleView>(id)
        {
            view.working = from;
        }
        editor.show_buffer(id);
        return;
    }
    let Some(scope) = editor.applet_state::<ConsoleScope>().map(|known| known.0) else {
        return;
    };
    let id = editor.add_applet_buffer_with(NAME, |buffer| {
        buffer.insert_at(0, PROMPT);
        buffer.set_point(buffer.len_chars());
        ConsoleView {
            scope,
            input: InputLine::new(buffer),
            session: Session::default(),
            working: from,
        }
    });
    editor.show_buffer(id);
}

fn outcome(
    editor: &mut Editor,
    session: &mut Session,
    code: &str,
    working: BufferId,
) -> Option<String> {
    if code.trim().is_empty() {
        return None;
    }
    let console = editor.buffer().id();
    let elsewhere = working != console && editor.buffer_by_id(working).is_some();
    if elsewhere {
        editor.show_buffer(working);
    }
    let evaluated = session.evaluate(editor, code);
    if elsewhere {
        editor.show_buffer(console);
    }
    match evaluated {
        Ok(value) if value.is_unit() => None,
        Ok(value) => Some(format!("{value:?}")),
        Err(error) => Some(script::describe(*error, Origin::default())),
    }
}

pub(crate) fn send(editor: &mut Editor) {
    let id = editor.buffer().id();
    let Some((buffer, view)) = editor.applet_buffer_mut::<ConsoleView>(id) else {
        return;
    };
    let Some(code) = view.input.text(buffer) else {
        return;
    };
    view.input.remember(&code);
    let working = view.working;
    let mut session = mem::take(&mut view.session);
    let printed = outcome(editor, &mut session, &code, working);
    let Some((buffer, view)) = editor.applet_buffer_mut::<ConsoleView>(id) else {
        return;
    };
    view.session = session;
    let shown = match printed {
        Some(text) => format!("\n{text}\n{PROMPT}"),
        None => format!("\n{PROMPT}"),
    };
    let end = buffer.len_chars();
    buffer.insert_at(end, &shown);
    let end = buffer.len_chars();
    buffer.set_marker(view.input.mark(), end);
    buffer.set_point(end);
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::text::Buffer;

    use crate::testing::{installed, m_x, press, type_text};

    fn console() -> Editor {
        let mut editor = installed();
        m_x(&mut editor, "script-console");
        editor
    }

    fn enter(editor: &mut Editor, code: &str) {
        type_text(editor, code);
        press(editor, "RET");
    }

    #[test]
    fn the_console_prints_results_and_keeps_variables() {
        let mut editor = console();
        assert_eq!(editor.buffer().name(), "*Script*");

        enter(&mut editor, "let x = 40;");
        enter(&mut editor, "x + 2");

        assert_eq!(editor.buffer().contents(), "> let x = 40;\n> x + 2\n42\n> ");
    }

    #[test]
    fn errors_print_and_earlier_input_comes_back() {
        let mut editor = console();

        enter(&mut editor, "nope");
        assert_eq!(
            editor.buffer().contents(),
            "> nope\nVariable not found: nope (line 1, column 1)\n> "
        );
        press(&mut editor, "M-p");

        assert!(editor.buffer().contents().ends_with("\n> nope"));
    }

    #[test]
    fn functions_and_closures_last_between_inputs() {
        let mut editor = console();

        enter(&mut editor, "fn twice(n) { n * 2 }");
        enter(&mut editor, "let next = |n| n + 1;");
        enter(&mut editor, "twice(next.call(20))");

        assert!(
            editor.buffer().contents().ends_with("\n42\n> "),
            "{}",
            editor.buffer().contents()
        );
    }

    #[test]
    fn scripts_in_the_console_act_on_the_buffer_it_was_opened_from() {
        let mut editor = installed();
        let scratch = editor.buffer().id();
        editor.buffer_mut().insert("abc");
        m_x(&mut editor, "script-console");

        enter(&mut editor, "insert(\"!\"); text()");

        assert_eq!(editor.buffer().name(), "*Script*");
        assert!(
            editor.buffer().contents().ends_with("\n\"abc!\"\n> "),
            "{}",
            editor.buffer().contents()
        );
        assert_eq!(
            editor.buffer_by_id(scratch).map(Buffer::contents),
            Some("abc!".to_owned())
        );
    }

    #[test]
    fn opening_the_console_from_another_buffer_works_on_that_one() {
        let mut editor = console();
        press(&mut editor, "C-x b");
        type_text(&mut editor, "notes");
        press(&mut editor, "RET");
        editor.buffer_mut().insert("n");
        m_x(&mut editor, "script-console");

        enter(&mut editor, "text()");

        assert!(
            editor.buffer().contents().ends_with("\n\"n\"\n> "),
            "{}",
            editor.buffer().contents()
        );
    }

    #[test]
    fn coming_back_to_the_console_keeps_its_session() {
        let mut editor = console();
        enter(&mut editor, "let kept = 7;");
        press(&mut editor, "C-x b");
        type_text(&mut editor, "*scratch*");
        press(&mut editor, "RET");

        m_x(&mut editor, "script-console");
        enter(&mut editor, "kept");

        assert_eq!(editor.buffer().contents(), "> let kept = 7;\n> kept\n7\n> ");
    }
}
