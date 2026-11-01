use std::time::Duration;

use ox_lang::heap::Heap;
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::Editor;

#[derive(Debug, Default)]
struct ModeLine(Vec<(String, String)>);

fn segment(editor: &Editor) -> Option<String> {
    let texts: Vec<&str> = editor
        .plugin_state::<ModeLine>()?
        .0
        .iter()
        .map(|(_, text)| text.as_str())
        .collect();
    (!texts.is_empty()).then(|| texts.join("  "))
}

fn set_mode_line(heap: &Heap, editor: &mut Editor, arguments: &[Value]) {
    let key = print::display(heap, arguments[0]);
    let text = (arguments[1] != Value::Nil).then(|| print::display(heap, arguments[1]));
    if editor.plugin_state::<ModeLine>().is_none() {
        editor.set_plugin_state(ModeLine::default());
        editor.add_mode_line_segment(segment);
    }
    let Some(segments) = editor.plugin_state_mut::<ModeLine>() else {
        return;
    };
    match (segments.0.iter_mut().find(|(known, _)| *known == key), text) {
        (Some((_, shown)), Some(text)) => *shown = text,
        (None, Some(text)) => segments.0.push((key, text)),
        (_, None) => segments.0.retain(|(known, _)| *known != key),
    }
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "message")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            editor.show_message(print::display(heap, arguments[0]));
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "set-mode-line")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            set_mode_line(heap, editor, arguments);
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "clipboard-set")
        .arity(1, Some(1))
        .keywords(&["clear-after"])
        .host::<Editor>(|heap, editor, arguments| {
            let clear_after = match arguments[1] {
                Value::Nil => None,
                Value::Int(seconds) => {
                    Some(Duration::from_secs(u64::try_from(seconds).unwrap_or(0)))
                }
                Value::Float(seconds) => {
                    Some(Duration::try_from_secs_f64(seconds).unwrap_or_default())
                }
                other => {
                    return Err(Error::new(format!(
                        "clipboard-set's :clear-after is a number of seconds, got {}",
                        print::brief(heap, other)
                    )));
                }
            };
            editor.copy_secret(print::display(heap, arguments[0]), clear_after);
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "copy-text")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            editor.copy_text(print::display(heap, arguments[0]));
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "notify")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            let title = print::display(heap, arguments[0]);
            editor.notify(title, print::display(heap, arguments[1]));
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "window-focused?")
        .arity(0, Some(0))
        .host::<Editor>(|_, editor, _| Ok(Value::Bool(editor.focused())));
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::{Editor, Effect};

    use crate::Ox;

    #[test]
    fn ox_sets_and_clears_mode_line_text() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        ox.eval(
            &mut editor,
            r#"(set-mode-line :weather "☀ 21°") (set-mode-line :mail "3 new")"#,
        )
        .unwrap();
        assert_eq!(editor.mode_line_segments(), ["☀ 21°  3 new"]);

        ox.eval(&mut editor, r#"(set-mode-line :weather "☁ 18°")"#)
            .unwrap();
        assert_eq!(editor.mode_line_segments(), ["☁ 18°  3 new"]);

        ox.eval(
            &mut editor,
            "(set-mode-line :weather nil) (set-mode-line :mail nil)",
        )
        .unwrap();
        assert_eq!(editor.mode_line_segments(), Vec::<String>::new());
    }

    #[test]
    fn ox_copies_text_to_the_kill_ring_and_clipboard() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        ox.eval(&mut editor, r#"(copy-text "a passage")"#).unwrap();

        assert_eq!(editor.clipboard_history(), ["a passage"]);
        assert_eq!(
            editor.take_effects(),
            [Effect::WriteClipboard("a passage".to_owned())]
        );
    }

    #[test]
    fn ox_copies_secrets_notifies_and_knows_about_focus() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        editor.set_focused(false);

        let focused = ox
            .eval(
                &mut editor,
                r##"(clipboard-set "hunter2") (notify "#oxmacs" "ana: hi") (window-focused?)"##,
            )
            .unwrap();

        assert_eq!(focused, "false");
        assert_eq!(
            editor.take_effects(),
            [
                Effect::WriteClipboard("hunter2".to_owned()),
                Effect::Notify {
                    title: "#oxmacs".to_owned(),
                    body: "ana: hi".to_owned()
                }
            ]
        );
        assert_eq!(editor.clipboard_history(), Vec::<String>::new());
    }

    #[test]
    fn secrets_copied_with_clear_after_are_cleared_at_the_deadline() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        ox.eval(&mut editor, r#"(clipboard-set "hunter2" :clear-after 0)"#)
            .unwrap();
        editor.take_effects();
        let now = std::time::Instant::now();
        editor.idle(now, now);

        assert_eq!(
            editor.take_effects(),
            [Effect::ClearClipboard("hunter2".to_owned())]
        );
        let refused = ox
            .eval(&mut editor, r#"(clipboard-set "x" :clear-after "soon")"#)
            .unwrap_err();
        assert!(
            refused
                .message
                .contains(":clear-after is a number of seconds"),
            "{}",
            refused.message
        );
    }
}
