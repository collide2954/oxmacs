use std::time::Duration;

use ox_lang::heap::{Heap, Hold};
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::{Editor, EventKind, ModeLinePart};

#[derive(Debug)]
struct Shown {
    key: String,
    text: String,
    click: Option<Hold>,
}

#[derive(Debug, Default)]
struct ModeLine(Vec<Shown>);

fn segment(editor: &Editor) -> Vec<ModeLinePart> {
    editor
        .plugin_state::<ModeLine>()
        .map(|shown| {
            shown
                .0
                .iter()
                .map(|shown| ModeLinePart {
                    text: shown.text.clone(),
                    click: shown.click.map(|_| shown.key.clone()),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn set_mode_line(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) {
    let key = match arguments[0] {
        Value::Keyword(name) => name.name().to_owned(),
        other => print::display(heap, other),
    };
    let text = (arguments[1] != Value::Nil).then(|| print::display(heap, arguments[1]));
    let click = (text.is_some() && arguments[2] != Value::Nil).then(|| heap.hold(arguments[2]));
    if click.is_some() {
        editor.subscribe(EventKind::ModeLineClicked);
    }
    if editor.plugin_state::<ModeLine>().is_none() {
        editor.set_plugin_state(ModeLine::default());
        editor.add_mode_line_segment(segment);
    }
    let Some(segments) = editor.plugin_state_mut::<ModeLine>() else {
        return;
    };
    let at = segments.0.iter().position(|shown| shown.key == key);
    let old = match (at, text) {
        (Some(at), Some(text)) => {
            let shown = &mut segments.0[at];
            shown.text = text;
            std::mem::replace(&mut shown.click, click)
        }
        (None, Some(text)) => {
            segments.0.push(Shown { key, text, click });
            None
        }
        (Some(at), None) => segments.0.remove(at).click,
        (None, None) => None,
    };
    if let Some(old) = old {
        heap.release(old);
    }
}

pub(crate) fn clicked(isolate: &mut Isolate, editor: &mut Editor, key: &str) {
    let Some(handler) = editor
        .plugin_state::<ModeLine>()
        .and_then(|shown| shown.0.iter().find(|shown| shown.key == key))
        .and_then(|shown| shown.click)
        .and_then(|hold| isolate.held(hold))
    else {
        return;
    };
    if let Err(error) = isolate.spawn(handler, &[]) {
        crate::ide::log::report(editor, &error, None);
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
        .keywords(&["on-click"])
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
        assert_eq!(editor.mode_line_segments(), ["☀ 21°", "3 new"]);

        ox.eval(&mut editor, r#"(set-mode-line :weather "☁ 18°")"#)
            .unwrap();
        assert_eq!(editor.mode_line_segments(), ["☁ 18°", "3 new"]);

        ox.eval(
            &mut editor,
            "(set-mode-line :weather nil) (set-mode-line :mail nil)",
        )
        .unwrap();
        assert_eq!(editor.mode_line_segments(), Vec::<String>::new());
    }

    #[test]
    fn clicking_mode_line_text_runs_its_function() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        ox.eval(
            &mut editor,
            r#"(defonce clicks (atom 0)) (set-mode-line :git "main ↑2" :on-click (fn [] (swap! clicks inc))) (set-mode-line :clock "12:00")"#,
        )
        .unwrap();
        let parts = editor.mode_line_parts();
        assert_eq!(parts[0].click.as_deref(), Some("git"));
        assert_eq!(parts[1].click, None);

        editor.click_mode_line("git");
        editor.click_mode_line("clock");
        crate::testing::settle(&mut editor, &mut ox);

        assert_eq!(ox.eval(&mut editor, "@clicks").unwrap(), "1");

        ox.eval(&mut editor, r#"(set-mode-line :git "main")"#)
            .unwrap();
        editor.click_mode_line("git");
        crate::testing::settle(&mut editor, &mut ox);

        assert_eq!(editor.mode_line_parts()[0].click, None);
        assert_eq!(ox.eval(&mut editor, "@clicks").unwrap(), "1");
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
    fn image_colors_know_the_themes_faces() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        let keyword = editor
            .theme()
            .highlight(oxmacs_core::theme::Highlight::Keyword);

        let shown = ox
            .eval(
                &mut editor,
                "(module (import (ox image))) (image/color :keyword)",
            )
            .unwrap();

        assert_eq!(
            shown,
            format!("[{} {} {} 255]", keyword.r, keyword.g, keyword.b)
        );
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
