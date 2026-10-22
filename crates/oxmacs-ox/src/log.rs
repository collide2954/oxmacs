use std::fmt::Write as _;

use ox::Error;
use ox::span::line_column;
use oxmacs_core::command::{Command, ScopeId};
use oxmacs_core::editor::Editor;
use oxmacs_core::plugin::PluginBuffer;

pub const LOG: &str = "*ox log*";

#[derive(Debug)]
struct LogView {
    scope: ScopeId,
}

impl PluginBuffer for LogView {
    fn scope(&self) -> ScopeId {
        self.scope
    }
}

#[derive(Debug, Default)]
struct Log {
    text: String,
    scope: Option<ScopeId>,
}

fn place(error: &Error, origin: Option<(&str, &str)>) -> String {
    match (error.traceback.first(), error.span, origin) {
        (Some(frame), _, _) => {
            let (line, column) = frame.line_column();
            format!("{}:{line}:{column}: ", frame.file())
        }
        (None, Some(span), Some((file, source))) => {
            let (line, column) = line_column(source, span.start);
            format!("{file}:{line}:{column}: ")
        }
        (None, None, Some((file, _))) => format!("{file}: "),
        (None, _, None) => String::new(),
    }
}

fn now() -> String {
    let now = jiff::Zoned::now();
    format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second())
}

fn scope(editor: &mut Editor) -> ScopeId {
    if let Some(scope) = editor.plugin_state::<Log>().and_then(|log| log.scope) {
        return scope;
    }
    let scope = editor.add_scope("ox log");
    editor.bind(scope, "q", Command::QuitWindow).ok();
    if let Some(log) = editor.plugin_state_mut::<Log>() {
        log.scope = Some(scope);
    }
    scope
}

fn append(editor: &mut Editor, entry: &str) {
    if editor.plugin_state::<Log>().is_none() {
        editor.set_plugin_state(Log::default());
    }
    let scope = scope(editor);
    let text = editor
        .plugin_state_mut::<Log>()
        .map(|log| {
            log.text.push_str(entry);
            log.text.clone()
        })
        .unwrap_or_default();
    let id = editor
        .plugin_buffers::<LogView>()
        .first()
        .copied()
        .unwrap_or_else(|| {
            editor.add_plugin_buffer_with(LOG, |buffer| {
                buffer.set_read_only(true);
                LogView { scope }
            })
        });
    editor.fill_buffer(id, &text);
}

pub fn report(editor: &mut Editor, error: &Error, origin: Option<(&str, &str)>) {
    let mut entry = format!("[{}] {}{}\n", now(), place(error, origin), error.message);
    if error.traceback.len() > 1 {
        for frame in &error.traceback {
            let _ = writeln!(entry, "  in {frame}");
        }
    }
    append(editor, &entry);
    editor.show_message(format!("Ox: {}", error.message));
}

pub fn show(editor: &mut Editor) {
    if let Some(id) = editor.buffer_named(LOG) {
        editor.show_buffer_beside(id);
    }
}

#[cfg(test)]
mod tests {
    use ox::Isolate;
    use oxmacs_core::editor::Editor;

    use super::{LOG, report};

    fn shown(editor: &Editor) -> bool {
        editor
            .buffer_named(LOG)
            .is_some_and(|id| editor.buffer_shown(id))
    }

    fn contents(editor: &Editor) -> String {
        let id = editor.buffer_named(LOG).unwrap();
        editor.buffer_by_id(id).unwrap().contents()
    }

    #[test]
    fn errors_are_logged_with_their_place_and_echoed() {
        let mut editor = Editor::new();
        let error = Isolate::new()
            .eval_named("init.ox", "(def (f) (/ 1 0))\n(f)")
            .unwrap_err();

        report(&mut editor, &error, None);

        let log = contents(&editor);
        assert!(log.contains("init.ox:1:10: Division by zero"), "{log}");
        assert!(log.contains("  in f at init.ox:1:10"), "{log}");
        assert!(log.contains("  in top level at init.ox:2:1"), "{log}");
        assert_eq!(editor.message(), Some("Ox: Division by zero"));
        assert!(!shown(&editor));
    }

    #[test]
    fn errors_without_a_traceback_use_the_source_they_came_from() {
        let mut editor = Editor::new();
        let source = "(+ 1\n  (";
        let error = Isolate::new().eval_named("init.ox", source).unwrap_err();

        report(&mut editor, &error, Some(("init.ox", source)));

        assert!(
            contents(&editor).contains("init.ox:2:3: "),
            "{}",
            contents(&editor)
        );
    }

    #[test]
    fn later_errors_are_added_below_earlier_ones() {
        let mut editor = Editor::new();
        let mut isolate = Isolate::new();
        let first = isolate.eval("(raise (error \"first\"))").unwrap_err();
        let second = isolate.eval("(raise (error \"second\"))").unwrap_err();

        report(&mut editor, &first, None);
        report(&mut editor, &second, None);

        let log = contents(&editor);
        let (one, two) = (log.find("first").unwrap(), log.find("second").unwrap());
        assert!(one < two, "{log}");
    }
}
