use ox_lang::Isolate;
use ox_lang::heap::Heap;
use ox_lang::value::{Value, print};
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
    isolate.define_host_native::<Editor>("message", 1, Some(1), |heap, editor, arguments| {
        editor.show_message(print::display(heap, arguments[0]));
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>("set-mode-line", 2, Some(2), |heap, editor, arguments| {
        set_mode_line(heap, editor, arguments);
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>("clipboard-set", 1, Some(1), |heap, editor, arguments| {
        editor.copy_secret(print::display(heap, arguments[0]));
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>("copy-text", 1, Some(1), |heap, editor, arguments| {
        editor.copy_text(print::display(heap, arguments[0]));
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>("notify", 2, Some(2), |heap, editor, arguments| {
        let title = print::display(heap, arguments[0]);
        editor.notify(title, print::display(heap, arguments[1]));
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>("window-focused?", 0, Some(0), |_, editor, _| {
        Ok(Value::Bool(editor.focused()))
    });
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
}
