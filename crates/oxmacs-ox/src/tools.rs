use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ox::read;
use oxmacs_core::command::{CommandId, DynamicSpec, Scope, ScopeId};
use oxmacs_core::editor::{Editor, InputLine};
use oxmacs_core::plugin::PluginBuffer;
use oxmacs_core::text::BufferId;

use crate::{Ox, log, mode, plugins};

pub const REPL: &str = "*ox repl*";
const PROMPT: &str = "ox> ";
const GREETING: &str =
    "Ox. Type an expression and press RET; M-p and M-n step through what you typed.\n";

#[derive(Debug, Clone)]
enum Request {
    EvalLastForm,
    EvalBuffer,
    OpenRepl,
    Send,
    Recall(bool),
    Saved(PathBuf),
}

#[derive(Debug, Default)]
struct Requests {
    waiting: Vec<Request>,
    repl: Option<ScopeId>,
}

#[derive(Debug)]
struct ReplView {
    scope: ScopeId,
    input: InputLine,
}

impl PluginBuffer for ReplView {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn input_line(&mut self) -> Option<&mut InputLine> {
        Some(&mut self.input)
    }
}

fn queue(editor: &mut Editor, request: Request) {
    if let Some(requests) = editor.plugin_state_mut::<Requests>() {
        requests.waiting.push(request);
    }
}

fn command(
    editor: &mut Editor,
    name: &str,
    summary: &str,
    scope: ScopeId,
    request: Request,
) -> CommandId {
    editor.add_dynamic_command(DynamicSpec {
        name: name.to_owned(),
        summary: summary.to_owned(),
        scope,
        run: Arc::new(move |editor: &mut Editor| queue(editor, request.clone())),
    })
}

fn saved(editor: &mut Editor, id: BufferId) {
    let Some(path) = editor
        .buffer_by_id(id)
        .and_then(|buffer| buffer.file())
        .map(|file| file.path.clone())
    else {
        return;
    };
    if path.extension().is_some_and(|extension| extension == "ox") {
        queue(editor, Request::Saved(path));
    }
}

pub fn install(editor: &mut Editor) {
    let mode_scope = editor.add_scope("Ox");
    let repl_scope = editor.add_scope("Ox REPL");
    editor.set_plugin_state(Requests {
        waiting: Vec::new(),
        repl: Some(repl_scope),
    });
    mode::install(editor, mode_scope);
    let last = command(
        editor,
        "ox-eval-last-form",
        "Evaluate the Ox form before point",
        mode_scope,
        Request::EvalLastForm,
    );
    let whole = command(
        editor,
        "ox-eval-buffer",
        "Evaluate this Ox buffer",
        mode_scope,
        Request::EvalBuffer,
    );
    let repl = command(
        editor,
        "ox-repl",
        "Open a REPL for Ox",
        Scope::Global.into(),
        Request::OpenRepl,
    );
    let send = command(
        editor,
        "ox-repl-send",
        "Evaluate the input, or start a new line while it is unfinished",
        repl_scope,
        Request::Send,
    );
    let previous = command(
        editor,
        "ox-repl-previous",
        "Recall the previous input",
        repl_scope,
        Request::Recall(true),
    );
    let next = command(
        editor,
        "ox-repl-next",
        "Recall the next input",
        repl_scope,
        Request::Recall(false),
    );
    for (scope, keys, id) in [
        (mode_scope, "C-x C-e", last),
        (mode_scope, "C-c C-b", whole),
        (mode_scope, "C-c C-z", repl),
        (repl_scope, "RET", send),
        (repl_scope, "M-p", previous),
        (repl_scope, "M-n", next),
    ] {
        editor.bind(scope, keys, id).ok();
    }
    editor.on_saved(saved);
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn source_name(editor: &Editor) -> String {
    let buffer = editor.buffer();
    buffer.file().map_or_else(
        || buffer.name().to_owned(),
        |file| file.path.display().to_string(),
    )
}

fn open_repl(editor: &mut Editor) {
    if let Some(id) = editor.plugin_buffers::<ReplView>().first().copied() {
        editor.show_buffer(id);
        return;
    }
    let Some(scope) = editor
        .plugin_state::<Requests>()
        .and_then(|requests| requests.repl)
    else {
        return;
    };
    let id = editor.add_plugin_buffer_with(REPL, |buffer| {
        buffer.insert_at(0, &format!("{GREETING}{PROMPT}"));
        let input = InputLine::new(buffer);
        buffer.set_point(buffer.len_chars());
        ReplView { scope, input }
    });
    editor.show_buffer(id);
}

fn recall(editor: &mut Editor, older: bool) {
    let Some(id) = editor.plugin_buffers::<ReplView>().first().copied() else {
        return;
    };
    if let Some((buffer, view)) = editor.plugin_buffer_mut::<ReplView>(id)
        && let Some(text) = view.input.recall(older)
    {
        view.input.replace(buffer, &text);
    }
}

impl Ox {
    pub(crate) fn handle_requests(&mut self, editor: &mut Editor) {
        let waiting = editor
            .plugin_state_mut::<Requests>()
            .map(|requests| std::mem::take(&mut requests.waiting))
            .unwrap_or_default();
        for request in waiting {
            match request {
                Request::EvalLastForm => self.eval_last_form(editor),
                Request::EvalBuffer => self.eval_buffer(editor),
                Request::OpenRepl => open_repl(editor),
                Request::Send => self.send(editor),
                Request::Recall(older) => recall(editor, older),
                Request::Saved(path) => {
                    self.reload(editor, &path);
                }
            }
        }
    }

    fn evaluate_and_show(&mut self, editor: &mut Editor, name: &str, source: &str) {
        match self.isolate.eval_named_with(editor, name, source) {
            Ok(value) => {
                let shown = self.isolate.write(value);
                editor.show_message(format!("=> {shown}"));
            }
            Err(error) => log::report(editor, &error, Some((name, source))),
        }
    }

    fn eval_last_form(&mut self, editor: &mut Editor) {
        let name = source_name(editor);
        let buffer = editor.buffer();
        let before = buffer.text_in(0..buffer.point());
        let (forms, _) = read::read(&before);
        let Some(form) = forms.last() else {
            editor.show_message("There is no Ox form before point");
            return;
        };
        let start = usize::try_from(form.span.start)
            .unwrap_or(0)
            .min(before.len());
        let end = usize::try_from(form.span.end)
            .unwrap_or(0)
            .clamp(start, before.len());
        let padding: String = before[..start]
            .chars()
            .map(|character| if character == '\n' { '\n' } else { ' ' })
            .collect();
        let source = format!("{padding}{}", &before[start..end]);
        self.evaluate_and_show(editor, &name, &source);
    }

    fn eval_buffer(&mut self, editor: &mut Editor) {
        if let Some(path) = editor.buffer().file().map(|file| file.path.clone())
            && self.reload(editor, &path)
        {
            return;
        }
        let name = source_name(editor);
        let source = editor.buffer().contents();
        self.evaluate_and_show(editor, &name, &source);
    }

    fn send(&mut self, editor: &mut Editor) {
        let Some(id) = editor.plugin_buffers::<ReplView>().first().copied() else {
            return;
        };
        let Some((buffer, view)) = editor.plugin_buffer_mut::<ReplView>(id) else {
            return;
        };
        let input = view.input.text(buffer).unwrap_or_default();
        if read::read(&input).1.iter().any(|error| error.incomplete) {
            let end = buffer.len_chars();
            buffer.insert_at(end, "\n");
            buffer.set_point(end + 1);
            return;
        }
        view.input.remember(&input);
        let shown =
            (!input.trim().is_empty()).then(|| match self.isolate.eval_with(editor, &input) {
                Ok(value) => self.isolate.write(value),
                Err(error) => format!("error: {}", error.message),
            });
        let Some((buffer, view)) = editor.plugin_buffer_mut::<ReplView>(id) else {
            return;
        };
        let mut text = String::from("\n");
        if let Some(shown) = shown {
            text.push_str(&shown);
            text.push('\n');
        }
        text.push_str(PROMPT);
        let end = buffer.len_chars();
        buffer.insert_at(end, &text);
        let end = buffer.len_chars();
        buffer.set_marker(view.input.mark(), end);
        buffer.set_point(end);
    }

    fn reload(&mut self, editor: &mut Editor, path: &Path) -> bool {
        let Some(home) = self.home.clone() else {
            return false;
        };
        if same_file(path, &home.join("init.ox")) {
            if self.run_init(editor, Some(&home)) {
                editor.show_message("Reloaded init.ox");
            }
            return true;
        }
        let Some((plugin, file)) = plugin_file(&home, path) else {
            return false;
        };
        plugins::reload_file(&mut self.isolate, editor, &plugin, &file, path);
        true
    }
}

fn plugin_file(home: &Path, path: &Path) -> Option<(String, String)> {
    let folder = path.parent()?;
    let plugins = folder.parent()?;
    if !same_file(plugins, &home.join("plugins")) {
        return None;
    }
    let plugin = folder.file_name()?.to_str()?.to_owned();
    let file = path.file_name()?.to_str()?.to_owned();
    Some((plugin, file))
}
#[cfg(test)]
mod tests {
    use std::path::Path;

    use oxmacs_core::editor::Editor;
    use oxmacs_core::input::KeySequence;
    use oxmacs_core::text::Buffer;

    use super::REPL;
    use crate::{Ox, log};

    fn press(editor: &mut Editor, ox: &mut Ox, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
        ox.tick(editor);
    }

    fn typed(editor: &mut Editor, ox: &mut Ox, text: &str) {
        let notation: Vec<String> = text
            .chars()
            .map(|character| match character {
                ' ' => "SPC".to_owned(),
                other => other.to_string(),
            })
            .collect();
        press(editor, ox, &notation.join(" "));
    }

    fn home_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        for (name, text) in files {
            let path = home.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        home
    }

    fn started(home: &Path) -> (Editor, Ox) {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.start(&mut editor, Some(home));
        (editor, ox)
    }

    fn visiting(home: &Path, name: &str) -> (Editor, Ox) {
        let (mut editor, ox) = started(home);
        editor.open_file(&home.join(name));
        let end = editor.buffer().len_chars();
        editor.buffer_mut().set_point(end);
        (editor, ox)
    }

    fn rewrite(editor: &mut Editor, ox: &mut Ox, text: &str) {
        let buffer = editor.buffer_mut();
        let end = buffer.len_chars();
        buffer.delete_range(0..end);
        buffer.insert_at(0, text);
        let id = editor.buffer().id();
        editor.save_buffer(id);
        ox.tick(editor);
    }

    fn logged(editor: &Editor) -> String {
        editor
            .buffer_named(log::LOG)
            .and_then(|id| editor.buffer_by_id(id))
            .map(Buffer::contents)
            .unwrap_or_default()
    }

    #[test]
    fn c_x_c_e_evaluates_the_form_before_point() {
        let home = home_with(&[("scratch.ox", "(def x 5)\n(* x 2)")]);
        let (mut editor, mut ox) = visiting(home.path(), "scratch.ox");

        press(&mut editor, &mut ox, "C-x C-e");
        assert_eq!(editor.message(), Some("Ox: x is not defined"));

        editor.buffer_mut().set_point(9);
        press(&mut editor, &mut ox, "C-x C-e");
        assert_eq!(editor.message(), Some("=> x"));
        let end = editor.buffer().len_chars();
        editor.buffer_mut().set_point(end);
        press(&mut editor, &mut ox, "C-x C-e");
        assert_eq!(editor.message(), Some("=> 10"));
    }

    #[test]
    fn c_c_c_b_evaluates_the_whole_buffer() {
        let home = home_with(&[(
            "scratch.ox",
            "(defcommand hello \"Say hello\" (message \"hello\"))\n(+ 1 2)",
        )]);
        let (mut editor, mut ox) = visiting(home.path(), "scratch.ox");

        press(&mut editor, &mut ox, "C-c C-b");

        assert_eq!(editor.message(), Some("=> 3"));
        assert!(editor.command_named("hello").is_some());
    }

    #[test]
    fn the_repl_evaluates_what_you_type() {
        let home = home_with(&[]);
        let (mut editor, mut ox) = started(home.path());

        press(&mut editor, &mut ox, "M-x");
        typed(&mut editor, &mut ox, "ox-repl");
        press(&mut editor, &mut ox, "RET");
        assert_eq!(editor.buffer().name(), REPL);

        typed(&mut editor, &mut ox, "(+ 1 2)");
        press(&mut editor, &mut ox, "RET");
        typed(&mut editor, &mut ox, "(/ 1 0)");
        press(&mut editor, &mut ox, "RET");
        typed(&mut editor, &mut ox, "(+ 1");
        press(&mut editor, &mut ox, "RET");
        typed(&mut editor, &mut ox, "2)");
        press(&mut editor, &mut ox, "RET");

        assert!(
            editor.buffer().contents().ends_with(
                "ox> (+ 1 2)\n3\nox> (/ 1 0)\nerror: Division by zero\nox> (+ 1\n2)\n3\nox> "
            ),
            "{}",
            editor.buffer().contents()
        );
        press(&mut editor, &mut ox, "M-p");
        assert!(editor.buffer().contents().ends_with("ox> (+ 1\n2)"));
        press(&mut editor, &mut ox, "M-p");
        assert!(editor.buffer().contents().ends_with("ox> (/ 1 0)"));
    }

    #[test]
    fn saving_init_ox_runs_it_again() {
        let home = home_with(&[("init.ox", "(def greeting \"hi\")")]);
        let (mut editor, mut ox) = visiting(home.path(), "init.ox");

        rewrite(
            &mut editor,
            &mut ox,
            "(defcommand wave \"Wave\" (message \"waving\"))",
        );

        assert!(editor.command_named("wave").is_some());
        assert_eq!(editor.message(), Some("Reloaded init.ox"));
    }

    #[test]
    fn saving_a_plugin_file_reloads_its_module() {
        let home = home_with(&[
            (
                "plugins/greeter/plugin.ox",
                "(plugin greeter :commands {greet: \"Say hello\"} :entry \"greeter.ox\")",
            ),
            (
                "plugins/greeter/greeter.ox",
                "(module (import (greeter words)))\n(defonce greeted (atom 0))\n(defcommand greet \"Say hello\" (swap! greeted inc) (message (str words/hello \" \" @greeted)))",
            ),
            ("plugins/greeter/words.ox", "(def hello \"Hello\")"),
            ("init.ox", "(use-plugin greeter)"),
        ]);
        let (mut editor, mut ox) = started(home.path());
        let greet = editor.command_named("greet").unwrap();
        editor.execute(greet);
        ox.tick(&mut editor);
        assert_eq!(editor.message(), Some("Hello 1"));

        editor.open_file(&home.path().join("plugins/greeter/words.ox"));
        rewrite(&mut editor, &mut ox, "(def hello \"Howdy\")");
        assert_eq!(editor.message(), Some("Reloaded greeter/words"));
        editor.execute(greet);
        ox.tick(&mut editor);

        assert_eq!(editor.message(), Some("Howdy 2"));
        assert_eq!(logged(&editor), "");
    }

    #[test]
    fn reload_errors_are_logged() {
        let home = home_with(&[("init.ox", "(def greeting \"hi\")")]);
        let (mut editor, mut ox) = visiting(home.path(), "init.ox");

        rewrite(&mut editor, &mut ox, "(/ 1 0)");

        assert!(
            logged(&editor).contains("init.ox:1:1: Division by zero"),
            "{}",
            logged(&editor)
        );
    }
}
