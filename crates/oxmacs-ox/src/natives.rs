use ox::Isolate;
use ox::heap::Heap;
use ox::value::{Value, print};
use oxmacs_core::editor::Editor;

#[derive(Debug, Default)]
struct ModeLine(Vec<(String, String)>);

fn segment(editor: &Editor) -> Option<String> {
    let texts: Vec<&str> = editor
        .applet_state::<ModeLine>()?
        .0
        .iter()
        .map(|(_, text)| text.as_str())
        .collect();
    (!texts.is_empty()).then(|| texts.join("  "))
}

fn set_mode_line(heap: &Heap, editor: &mut Editor, arguments: &[Value]) {
    let key = print::display(heap, arguments[0]);
    let text = (arguments[1] != Value::Nil).then(|| print::display(heap, arguments[1]));
    if editor.applet_state::<ModeLine>().is_none() {
        editor.set_applet_state(ModeLine::default());
        editor.add_mode_line_segment(segment);
    }
    let Some(segments) = editor.applet_state_mut::<ModeLine>() else {
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
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

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
}
