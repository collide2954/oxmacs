use std::mem;

use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::editor::{Editor, InputLine};

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
    if let Some(id) = editor.find_applet_buffer::<ConsoleView>(|_| true) {
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
        }
    });
    editor.show_buffer(id);
}

fn outcome(editor: &mut Editor, session: &mut Session, code: &str) -> Option<String> {
    if code.trim().is_empty() {
        return None;
    }
    match session.evaluate(editor, code) {
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
    let mut session = mem::take(&mut view.session);
    let printed = outcome(editor, &mut session, &code);
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
