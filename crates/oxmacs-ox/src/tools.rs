use std::collections::VecDeque;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use ox::span::Span;
use ox::task::TaskId;
use ox::value::Value;
use ox::{Error, read};
use oxmacs_core::command::{Command, CommandId, DynamicSpec, Scope, ScopeId};
use oxmacs_core::editor::{Editor, InputLine};
use oxmacs_core::plugin::PluginBuffer;
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;

use crate::{Ox, help, log, mode, plugins, stacktrace, xref};

pub const REPL: &str = "*ox repl*";
const PROMPT: &str = "ox> ";
const GREETING: &str =
    "Ox. Type an expression and press RET; M-p and M-n step through what you typed.\n";
const NOTE_LIMIT: usize = 200;

#[derive(Debug, Clone)]
enum Request {
    EvalLastForm,
    EvalTopForm,
    EvalBuffer,
    LoadFile,
    Interrupt,
    Doc,
    Complete(bool),
    IndentOrComplete,
    FindDefinition,
    Usages,
    OpenRepl,
    Send,
    Recall(bool),
    Saved(PathBuf),
    ReloadInit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Finish {
    Quietly,
    Note,
    Echo,
    Loaded(String),
}

#[derive(Debug)]
struct Form {
    buffer: BufferId,
    revision: u64,
    name: String,
    module: String,
    text: Rc<str>,
    span: Range<usize>,
    end: usize,
    finish: Finish,
}

impl Form {
    fn new(editor: &Editor, text: &Rc<str>, span: Range<usize>, finish: Finish) -> Self {
        let buffer = editor.buffer();
        Self {
            buffer: buffer.id(),
            revision: buffer.revision(),
            name: source_name(editor),
            module: "user".to_owned(),
            end: buffer.byte_to_char(span.end),
            text: Rc::clone(text),
            span,
            finish,
        }
    }

    fn source(&self) -> String {
        let padding: String = self.text[..self.span.start]
            .chars()
            .map(|character| if character == '\n' { '\n' } else { ' ' })
            .collect();
        format!("{padding}{}", &self.text[self.span.clone()])
    }
}

#[derive(Debug, Default)]
pub(crate) struct Evaluation {
    waiting: VecDeque<Form>,
    running: Option<(TaskId, Form)>,
    interrupted: bool,
}

fn prefixed(text: &str, at: usize) -> usize {
    let mut start = at;
    while text[..start].ends_with(['\'', '`', '~', '@', '#']) {
        start -= 1;
    }
    start
}

fn last_expression(text: &str) -> Option<Range<usize>> {
    let mut opens: Vec<usize> = Vec::new();
    let mut last = None;
    let mut chars = text.char_indices().peekable();
    while let Some((at, character)) = chars.next() {
        match character {
            ';' => while chars.next_if(|(_, next)| *next != '\n').is_some() {},
            '"' => {
                let mut end = text.len();
                while let Some((inside, next)) = chars.next() {
                    if next == '\\' {
                        chars.next();
                    } else if next == '"' {
                        end = inside + 1;
                        break;
                    }
                }
                last = Some(prefixed(text, at)..end);
            }
            '(' | '[' | '{' => opens.push(at),
            ')' | ']' | '}' => {
                if let Some(open) = opens.pop() {
                    last = Some(prefixed(text, open)..at + 1);
                }
            }
            _ if mode::delimits(character) => {}
            _ => {
                let mut end = at + character.len_utf8();
                if character == '\\'
                    && let Some((next_at, next)) = chars.next()
                {
                    end = next_at + next.len_utf8();
                }
                while let Some((next_at, next)) = chars.peek().copied() {
                    if mode::delimits(next) {
                        break;
                    }
                    end = next_at + next.len_utf8();
                    chars.next();
                }
                if !text[at..end].chars().all(|found| "'`~@#".contains(found)) {
                    last = Some(at..end);
                }
            }
        }
    }
    last
}

fn bytes(span: Span, text: &str) -> Range<usize> {
    let end = usize::try_from(span.end).unwrap_or(0).min(text.len());
    let start = usize::try_from(span.start).unwrap_or(0).min(end);
    start..end
}

fn note(editor: &mut Editor, form: &Form, text: &str, face: Highlight) {
    if editor
        .buffer_by_id(form.buffer)
        .is_some_and(|buffer| buffer.revision() == form.revision)
    {
        let line = text.lines().next().unwrap_or_default();
        let shown: String = if line.chars().count() > NOTE_LIMIT || line.len() < text.len() {
            line.chars().take(NOTE_LIMIT).chain(['…']).collect()
        } else {
            line.to_owned()
        };
        editor.set_inline_note(form.buffer, form.end, &shown, face);
    }
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

pub(crate) fn requests_waiting(editor: &Editor) -> bool {
    editor
        .plugin_state::<Requests>()
        .is_some_and(|requests| !requests.waiting.is_empty())
}

pub(crate) fn reload_init(editor: &mut Editor) {
    queue(editor, Request::ReloadInit);
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

#[derive(Debug, Clone, Copy)]
enum Home {
    Mode,
    Repl,
    Everywhere,
}

type Keys = &'static [(Home, &'static str)];

type Spec = (&'static str, &'static str, Home, Request, Keys);

fn looking_up() -> [Spec; 5] {
    [
        (
            "ox-find-definition",
            "Go to the definition of the Ox name at point; M-, comes back",
            Home::Everywhere,
            Request::FindDefinition,
            &[(Home::Mode, "M-."), (Home::Repl, "M-.")],
        ),
        (
            "ox-usages",
            "List the usages of the Ox name at point",
            Home::Everywhere,
            Request::Usages,
            &[(Home::Mode, "M-?"), (Home::Repl, "M-?")],
        ),
        (
            "ox-doc",
            "Show the docs of the Ox name at point",
            Home::Everywhere,
            Request::Doc,
            &[(Home::Mode, "C-c C-d"), (Home::Repl, "C-c C-d")],
        ),
        (
            "ox-complete",
            "Complete the Ox name before point",
            Home::Everywhere,
            Request::Complete(true),
            &[
                (Home::Mode, "C-M-i"),
                (Home::Repl, "C-M-i"),
                (Home::Repl, "TAB"),
            ],
        ),
        (
            "ox-indent-or-complete",
            "Complete the Ox name before point, or indent",
            Home::Mode,
            Request::IndentOrComplete,
            &[(Home::Mode, "TAB")],
        ),
    ]
}

fn evaluating() -> [Spec; 5] {
    [
        (
            "ox-eval-last-form",
            "Evaluate the Ox form before point",
            Home::Mode,
            Request::EvalLastForm,
            &[(Home::Mode, "C-x C-e")],
        ),
        (
            "ox-eval-top-form",
            "Evaluate the top-level Ox form around point",
            Home::Mode,
            Request::EvalTopForm,
            &[(Home::Mode, "C-M-x")],
        ),
        (
            "ox-eval-buffer",
            "Evaluate this Ox buffer",
            Home::Mode,
            Request::EvalBuffer,
            &[(Home::Mode, "C-c C-b")],
        ),
        (
            "ox-load-file",
            "Save this Ox file and load it",
            Home::Mode,
            Request::LoadFile,
            &[(Home::Mode, "C-c C-k")],
        ),
        (
            "ox-interrupt",
            "Stop the Ox evaluation that is running",
            Home::Mode,
            Request::Interrupt,
            &[(Home::Mode, "C-c C-c")],
        ),
    ]
}

fn repl_commands() -> [Spec; 4] {
    [
        (
            "ox-repl",
            "Open a REPL for Ox",
            Home::Everywhere,
            Request::OpenRepl,
            &[(Home::Mode, "C-c C-z")],
        ),
        (
            "ox-repl-send",
            "Evaluate the input, or start a new line while it is unfinished",
            Home::Repl,
            Request::Send,
            &[(Home::Repl, "RET")],
        ),
        (
            "ox-repl-previous",
            "Recall the previous input",
            Home::Repl,
            Request::Recall(true),
            &[(Home::Repl, "M-p")],
        ),
        (
            "ox-repl-next",
            "Recall the next input",
            Home::Repl,
            Request::Recall(false),
            &[(Home::Repl, "M-n")],
        ),
    ]
}

pub fn install(editor: &mut Editor) {
    let mode_scope = editor.add_scope("Ox");
    let repl_scope = editor.add_scope("Ox REPL");
    editor.set_plugin_state(Requests {
        waiting: Vec::new(),
        repl: Some(repl_scope),
    });
    mode::install(editor, mode_scope);
    let scope = |home: Home| match home {
        Home::Mode => mode_scope,
        Home::Repl => repl_scope,
        Home::Everywhere => Scope::Global.into(),
    };
    let specs = evaluating()
        .into_iter()
        .chain(looking_up())
        .chain(repl_commands());
    for (name, summary, home, request, keys) in specs {
        let id = match request {
            Request::IndentOrComplete => editor.add_dynamic_command(DynamicSpec {
                name: name.to_owned(),
                summary: summary.to_owned(),
                scope: scope(home),
                run: Arc::new(indent_or_complete),
            }),
            request => command(editor, name, summary, scope(home), request),
        };
        for (place, keys) in keys {
            editor.bind(scope(*place), keys, id).ok();
        }
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

fn indent_or_complete(editor: &mut Editor) {
    let buffer = editor.buffer();
    let before = (buffer.revision(), buffer.point());
    let selecting = buffer.selection().is_some();
    editor.execute(Command::IndentForTabCommand);
    let buffer = editor.buffer();
    let unchanged = before == (buffer.revision(), buffer.point());
    let after_name = buffer
        .chars_before(buffer.point())
        .next()
        .is_some_and(|character| !mode::delimits(character) && !"'`~@#".contains(character));
    if unchanged && after_name && !selecting {
        queue(editor, Request::Complete(true));
    }
}

pub(crate) fn in_repl(editor: &Editor) -> bool {
    editor.pane().plugin_buffer::<ReplView>().is_some()
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
                Request::EvalTopForm => self.eval_top_form(editor),
                Request::EvalBuffer => self.eval_buffer(editor),
                Request::LoadFile => self.load_file(editor),
                Request::Interrupt => self.interrupt(editor),
                Request::Doc => help::show_doc(&self.isolate, editor, self.home.as_deref()),
                Request::Complete(explicit) => {
                    help::complete(&self.isolate, editor, self.home.as_deref(), explicit);
                }
                Request::IndentOrComplete => indent_or_complete(editor),
                Request::FindDefinition => {
                    xref::find_definition(&self.isolate, editor, self.home.as_deref());
                }
                Request::Usages => xref::show_usages(editor, self.home.as_deref()),
                Request::OpenRepl => open_repl(editor),
                Request::Send => self.send(editor),
                Request::Recall(older) => recall(editor, older),
                Request::Saved(path) => {
                    self.reload(editor, &path);
                }
                Request::ReloadInit => {
                    let home = self.home.clone();
                    self.run_init(editor, home.as_deref());
                }
            }
        }
    }

    fn evaluate(&mut self, editor: &mut Editor, mut forms: Vec<Form>) {
        let module = help::module(editor, self.home.as_deref());
        for form in &mut forms {
            form.module.clone_from(&module);
        }
        self.evaluation.waiting.extend(forms);
        self.advance(editor);
    }

    pub(crate) fn advance(&mut self, editor: &mut Editor) {
        while self.evaluation.running.is_none() {
            let Some(form) = self.evaluation.waiting.pop_front() else {
                return;
            };
            match self
                .isolate
                .spawn_eval_in(editor, &form.module, &form.name, &form.source())
            {
                Ok(Some(task)) => self.evaluation.running = Some((task, form)),
                Ok(None) => self.succeeded(editor, &form, Value::Nil),
                Err(error) => self.failed(editor, &form, &error),
            }
        }
    }

    pub(crate) fn evaluating(&self) -> Option<TaskId> {
        self.evaluation.running.as_ref().map(|(task, _)| *task)
    }

    pub(crate) fn evaluated(
        &mut self,
        editor: &mut Editor,
        finished: Vec<(TaskId, Result<Value, Error>)>,
    ) -> Vec<Error> {
        let mut others = Vec::new();
        for (task, result) in finished {
            let Some((_, form)) = self
                .evaluation
                .running
                .take_if(|(running, _)| *running == task)
            else {
                others.extend(result.err());
                continue;
            };
            if std::mem::take(&mut self.evaluation.interrupted) {
                editor.show_message("Evaluation interrupted");
                continue;
            }
            match result {
                Ok(value) => self.succeeded(editor, &form, value),
                Err(error) => self.failed(editor, &form, &error),
            }
        }
        others
    }

    fn succeeded(&mut self, editor: &mut Editor, form: &Form, value: Value) {
        let shown = || format!("=> {}", self.isolate.write(value));
        match &form.finish {
            Finish::Quietly => {}
            Finish::Note => {
                let shown = shown();
                note(editor, form, &shown, Highlight::Comment);
                editor.show_message(shown);
            }
            Finish::Echo => editor.show_message(shown()),
            Finish::Loaded(name) => editor.show_message(format!("Loaded {name}")),
        }
    }

    fn failed(&mut self, editor: &mut Editor, form: &Form, error: &Error) {
        self.evaluation.waiting.clear();
        note(
            editor,
            form,
            &format!("!! {}", error.message),
            Highlight::Error,
        );
        log::report(editor, error, Some((&form.name, &form.source())));
        stacktrace::show(editor, error);
    }

    fn interrupt(&mut self, editor: &mut Editor) {
        let Some(task) = self.evaluating() else {
            editor.show_message("Nothing is being evaluated");
            return;
        };
        self.evaluation.waiting.clear();
        self.evaluation.interrupted = self.isolate.cancel(task);
    }

    fn eval_last_form(&mut self, editor: &mut Editor) {
        let buffer = editor.buffer();
        let before: Rc<str> = buffer.text_in(0..buffer.point()).into();
        let Some(span) = last_expression(&before) else {
            editor.show_message("There is no Ox form before point");
            return;
        };
        let form = Form::new(editor, &before, span, Finish::Note);
        self.evaluate(editor, vec![form]);
    }

    fn eval_top_form(&mut self, editor: &mut Editor) {
        let buffer = editor.buffer();
        let text: Rc<str> = buffer.contents().into();
        let point = buffer.char_to_byte(buffer.point());
        let (forms, _) = read::read(&text);
        let spans: Vec<Range<usize>> = forms.iter().map(|form| bytes(form.span, &text)).collect();
        let Some(span) = spans
            .iter()
            .find(|span| span.contains(&point))
            .or_else(|| spans.iter().rfind(|span| span.end <= point))
        else {
            editor.show_message("There is no Ox form at point");
            return;
        };
        let form = Form::new(editor, &text, span.clone(), Finish::Note);
        self.evaluate(editor, vec![form]);
    }

    fn evaluate_all(&mut self, editor: &mut Editor, finish: Finish) {
        let text: Rc<str> = editor.buffer().contents().into();
        let (forms, errors) = read::read(&text);
        if let Some(error) = errors.into_iter().next() {
            let form = Form::new(editor, &text, bytes(error.span, &text), Finish::Quietly);
            note(
                editor,
                &form,
                &format!("!! {}", error.message),
                Highlight::Error,
            );
            let error = Error::at(error.span, error.message);
            log::report(editor, &error, Some((&form.name, &text)));
            return;
        }
        let Some(last) = forms.len().checked_sub(1) else {
            match finish {
                Finish::Loaded(name) => editor.show_message(format!("Loaded {name}")),
                _ => editor.show_message("There are no Ox forms here"),
            }
            return;
        };
        let forms = forms
            .iter()
            .enumerate()
            .map(|(index, form)| {
                let finish = if index == last {
                    finish.clone()
                } else {
                    Finish::Quietly
                };
                Form::new(editor, &text, bytes(form.span, &text), finish)
            })
            .collect();
        self.evaluate(editor, forms);
    }

    fn eval_buffer(&mut self, editor: &mut Editor) {
        if let Some(path) = editor.buffer().file().map(|file| file.path.clone())
            && self.reload(editor, &path)
        {
            return;
        }
        self.evaluate_all(editor, Finish::Echo);
    }

    fn load_file(&mut self, editor: &mut Editor) {
        let buffer = editor.buffer();
        let (id, modified) = (buffer.id(), buffer.is_modified());
        let Some(path) = buffer.file().map(|file| file.path.clone()) else {
            let name = buffer.name().to_owned();
            self.evaluate_all(editor, Finish::Loaded(name));
            return;
        };
        let saved = modified && editor.save_buffer(id);
        if self.reloads(&path) {
            if !saved {
                self.reload(editor, &path);
            }
            return;
        }
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        self.evaluate_all(editor, Finish::Loaded(name));
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

    fn reloads(&self, path: &Path) -> bool {
        self.home.as_ref().is_some_and(|home| {
            init_file(home, path).is_some() || plugin_file(home, path).is_some()
        })
    }

    fn reload(&mut self, editor: &mut Editor, path: &Path) -> bool {
        let Some(home) = self.home.clone() else {
            return false;
        };
        if let Some(name) = init_file(&home, path) {
            if self.run_init(editor, Some(&home)) {
                editor.show_message(format!("Reloaded {name}"));
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

fn init_file(home: &Path, path: &Path) -> Option<&'static str> {
    ["init.ox", "custom.ox"]
        .into_iter()
        .find(|name| same_file(path, &home.join(name)))
}

pub(crate) fn plugin_file(home: &Path, path: &Path) -> Option<(String, String)> {
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
    use oxmacs_core::theme::Highlight;

    use super::{REPL, last_expression};
    use crate::stacktrace::STACKTRACE;
    use crate::{Ox, Pace, log};

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
        crate::testing::settle(&mut editor, &mut ox);
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

    fn note(editor: &Editor, line: usize) -> Option<(String, Highlight)> {
        editor.pane().line_note(line)
    }

    fn noted(text: &str) -> (String, Highlight) {
        (text.to_owned(), Highlight::Comment)
    }

    fn failed(text: &str) -> (String, Highlight) {
        (text.to_owned(), Highlight::Error)
    }

    #[test]
    fn c_x_c_e_evaluates_the_form_before_point_and_notes_its_value() {
        let home = home_with(&[("scratch.ox", "(def x 5)\n(* x 2)")]);
        let (mut editor, mut ox) = visiting(home.path(), "scratch.ox");

        press(&mut editor, &mut ox, "C-x C-e");
        assert_eq!(editor.message(), Some("Ox: x is not defined"));
        assert_eq!(note(&editor, 1), Some(failed("!! x is not defined")));

        editor.buffer_mut().set_point(9);
        press(&mut editor, &mut ox, "C-x C-e");
        assert_eq!(editor.message(), Some("=> x"));
        assert_eq!(note(&editor, 0), Some(noted("=> x")));
        let end = editor.buffer().len_chars();
        editor.buffer_mut().set_point(end);
        press(&mut editor, &mut ox, "C-x C-e");
        assert_eq!(editor.message(), Some("=> 10"));
        assert_eq!(note(&editor, 1), Some(noted("=> 10")));

        editor.buffer_mut().insert_at(0, " ");
        assert_eq!(note(&editor, 1), None);
    }

    #[test]
    fn the_expression_before_point_may_sit_inside_an_unfinished_list() {
        let last = |text: &str| last_expression(text).map(|span| text[span].to_owned());

        assert_eq!(last("(def x 5)\n(* x 2)").as_deref(), Some("(* x 2)"));
        assert_eq!(last("(+ (área 2 3)").as_deref(), Some("(área 2 3)"));
        assert_eq!(last("(+ x").as_deref(), Some("x"));
        assert_eq!(last("(f '(a b)").as_deref(), Some("'(a b)"));
        assert_eq!(last("(str \"a)b\"").as_deref(), Some("\"a)b\""));
        assert_eq!(last("(f \\( ").as_deref(), Some("\\("));
        assert_eq!(last("(f x) ; (g y)\n").as_deref(), Some("(f x)"));
        assert_eq!(last("#{1 2}").as_deref(), Some("#{1 2}"));
        assert_eq!(last("  "), None);
    }

    #[test]
    fn c_m_x_evaluates_the_top_level_form_around_point() {
        let home = home_with(&[("scratch.ox", "(def (área w h)\n  (* w h))\n\n(área 2 3)\n")]);
        let (mut editor, mut ox) = visiting(home.path(), "scratch.ox");

        editor.buffer_mut().set_point(20);
        press(&mut editor, &mut ox, "C-M-x");
        assert_eq!(editor.message(), Some("=> área"));
        assert_eq!(note(&editor, 1), Some(noted("=> área")));

        editor.buffer_mut().set_point(29);
        press(&mut editor, &mut ox, "C-M-x");
        assert_eq!(editor.message(), Some("=> 6"));
        assert_eq!(note(&editor, 3), Some(noted("=> 6")));
        assert_eq!(note(&editor, 1), Some(noted("=> área")));
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
    fn an_error_stops_the_buffer_where_it_happened() {
        let home = home_with(&[("scratch.ox", "(def a 1)\n(/ a 0)\n(def b 2)")]);
        let (mut editor, mut ox) = visiting(home.path(), "scratch.ox");

        press(&mut editor, &mut ox, "C-c C-b");

        assert_eq!(editor.message(), Some("Ox: Division by zero"));
        assert_eq!(note(&editor, 1), Some(failed("!! Division by zero")));
        assert_eq!(ox.eval(&mut editor, "a").unwrap(), "1");
        assert!(ox.eval(&mut editor, "b").is_err());
    }

    fn trace(editor: &Editor) -> Vec<String> {
        let id = editor.buffer_named(STACKTRACE).unwrap();
        let buffer = editor.buffer_by_id(id).unwrap();
        buffer.contents().lines().map(str::to_owned).collect()
    }

    #[test]
    fn errors_from_calls_open_a_stacktrace_whose_frames_visit_their_source() {
        let home = home_with(&[(
            "scratch.ox",
            "(def (inner x)\n  (/ x 0))\n(def (outer) (+ 1 (inner 1)))\n(outer)",
        )]);
        let (mut editor, mut ox) = visiting(home.path(), "scratch.ox");

        press(&mut editor, &mut ox, "C-c C-b");

        assert_eq!(editor.buffer().name(), STACKTRACE);
        assert_eq!(
            trace(&editor),
            [
                "Division by zero",
                "zero-division-error",
                "",
                "  inner      scratch.ox:2:3",
                "  outer      scratch.ox:3:19",
                "  top level  scratch.ox:4:1",
            ]
        );
        press(&mut editor, &mut ox, "RET");
        assert_eq!(editor.buffer().name(), "scratch.ox");
        assert_eq!(editor.buffer().point(), 17);

        press(&mut editor, &mut ox, "C-x o");
        press(&mut editor, &mut ox, "n");
        press(&mut editor, &mut ox, "RET");
        assert_eq!(editor.buffer().point(), 44);

        press(&mut editor, &mut ox, "C-x o");
        press(&mut editor, &mut ox, "q");
        let shown = editor.buffer_named(STACKTRACE).unwrap();
        assert!(!editor.buffer_shown(shown));
    }

    #[test]
    fn frames_without_a_file_open_their_source_read_only() {
        let home = home_with(&[("scratch.ox", "(helper 1)")]);
        let (mut editor, mut ox) = visiting(home.path(), "scratch.ox");
        ox.isolate
            .eval_named("lib/helper.ox", "(def (helper x)\n  (/ x 0))")
            .unwrap();

        press(&mut editor, &mut ox, "C-x C-e");
        assert_eq!(trace(&editor)[3], "  helper     lib/helper.ox:2:3");
        press(&mut editor, &mut ox, "RET");

        assert_eq!(editor.buffer().name(), "lib/helper.ox");
        assert_eq!(editor.buffer().contents(), "(def (helper x)\n  (/ x 0))");
        assert!(editor.buffer().is_read_only());
        assert_eq!(editor.buffer().point(), 18);
    }

    #[test]
    fn c_c_c_k_saves_and_loads_the_file() {
        let home = home_with(&[("scratch.ox", "")]);
        let (mut editor, mut ox) = visiting(home.path(), "scratch.ox");
        editor
            .buffer_mut()
            .insert_at(0, "(def (double x) (* x 2))\n(double 21)");

        press(&mut editor, &mut ox, "C-c C-k");

        assert_eq!(editor.message(), Some("Loaded scratch.ox"));
        assert!(!editor.buffer().is_modified());
        assert_eq!(
            std::fs::read_to_string(home.path().join("scratch.ox")).unwrap(),
            "(def (double x) (* x 2))\n(double 21)\n"
        );
        assert_eq!(ox.eval(&mut editor, "(double 4)").unwrap(), "8");
    }

    #[test]
    fn c_c_c_c_interrupts_a_runaway_evaluation() {
        let home = home_with(&[("scratch.ox", "(def (spin) (spin))\n(spin)\n(def after 1)")]);
        let (mut editor, mut ox) = visiting(home.path(), "scratch.ox");

        press(&mut editor, &mut ox, "C-c C-b");
        assert_eq!(ox.tick(&mut editor), Pace::Running);

        press(&mut editor, &mut ox, "C-c C-c");
        assert_eq!(editor.message(), Some("Evaluation interrupted"));
        assert_eq!(ox.pace(), Pace::Idle);
        assert!(ox.eval(&mut editor, "after").is_err());

        press(&mut editor, &mut ox, "C-c C-c");
        assert_eq!(editor.message(), Some("Nothing is being evaluated"));
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
    fn loading_a_new_plugin_file_starts_its_plugin() {
        let home = home_with(&[("init.ox", "")]);
        let (mut editor, mut ox) = started(home.path());
        let folder = home.path().join("plugins").join("greeter");
        std::fs::create_dir_all(&folder).unwrap();
        editor.open_file(&folder.join("plugin.ox"));
        editor.buffer_mut().insert_at(
            0,
            "(plugin greeter :commands {greet: \"Say hello\"} :entry \"greeter.ox\")",
        );
        press(&mut editor, &mut ox, "C-x C-s");
        assert_eq!(
            editor.message(),
            Some("The greeter plugin starts once greeter.ox exists")
        );
        assert_eq!(logged(&editor), "");
        std::fs::write(
            folder.join("greeter.ox"),
            "(def (twice x) (* 2 x))\n(defcommand greet \"Say hello\" (message \"Hello\"))\n(twice 4)",
        )
        .unwrap();
        editor.open_file(&folder.join("greeter.ox"));
        assert!(editor.command_named("greet").is_none());

        press(&mut editor, &mut ox, "C-c C-k");

        assert_eq!(
            editor.message(),
            Some("Started the greeter plugin; (use-plugin greeter) in init.ox keeps it on")
        );
        assert_eq!(logged(&editor), "");
        let end = editor.buffer().len_chars();
        editor.buffer_mut().set_point(end);
        press(&mut editor, &mut ox, "C-x C-e");
        assert_eq!(editor.message(), Some("=> 8"));
        let greet = editor.command_named("greet").unwrap();
        editor.execute(greet);
        ox.tick(&mut editor);
        assert_eq!(editor.message(), Some("Hello"));
    }

    #[test]
    fn reloading_init_ox_starts_settings_from_their_defaults() {
        let home = home_with(&[
            ("custom.ox", "(settings {clock: false})"),
            ("init.ox", "(settings {tab-width: 4})"),
        ]);
        let (mut editor, mut ox) = visiting(home.path(), "init.ox");
        assert_eq!(editor.config().tab_width, 4);

        ox.eval(&mut editor, "(settings {line-numbers: :off})")
            .unwrap();
        rewrite(&mut editor, &mut ox, "(def quiet true)");

        assert_eq!(editor.config().tab_width, 8);
        assert!(!editor.config().clock);
        assert_eq!(
            editor.config().line_numbers,
            oxmacs_core::config::LineNumbers::Absolute
        );
        assert_eq!(editor.message(), Some("Reloaded init.ox"));
    }

    #[test]
    fn saving_custom_ox_runs_both_files_again() {
        let home = home_with(&[
            ("custom.ox", "(settings {clock: false})"),
            ("init.ox", "(settings {tab-width: 4})"),
        ]);
        let (mut editor, mut ox) = visiting(home.path(), "custom.ox");

        rewrite(
            &mut editor,
            &mut ox,
            "(settings {tab-width: 2 splash: false})",
        );

        assert!(editor.config().clock);
        assert!(!editor.config().splash);
        assert_eq!(editor.config().tab_width, 4);
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
