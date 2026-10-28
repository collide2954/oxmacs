use std::collections::HashSet;

use ox_lang::describe::ModuleDocs;
use ox_lang::heap::{Heap, Hold};
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::Editor;

use crate::{DOCS, PRELUDE, ide::ox_mode, ide::repl};

#[derive(Debug, Default)]
struct Asked(Vec<Hold>);

fn segments(
    heap: &mut Heap,
    line: &str,
    spans: &[(std::ops::Range<usize>, oxmacs_core::theme::Highlight)],
) -> Result<Value, Error> {
    let chars: Vec<char> = line.chars().collect();
    let text = |range: std::ops::Range<usize>| chars[range].iter().collect::<String>();
    let mut items = Vec::new();
    let mut at = 0;
    for (range, highlight) in spans {
        if range.start > at {
            items.push(heap.string(&text(at..range.start))?);
        }
        let shown = heap.string(&text(range.clone()))?;
        items.push(match oxmacs_core::theme::face_name(*highlight) {
            Some(face) => heap.vector_from(&[shown, Value::keyword(face)])?,
            None => shown,
        });
        at = range.end;
    }
    if at < chars.len() {
        items.push(heap.string(&text(at..chars.len()))?);
    }
    heap.vector_from(&items).map_err(Error::from)
}

fn highlight(heap: &mut Heap, lines: Value) -> Result<Value, Error> {
    let lines: Vec<String> = heap
        .elements(lines)
        .ok_or_else(|| {
            Error::new(format!(
                "ox-highlight expects a vector of lines, got {}",
                print::brief(heap, lines)
            ))
        })?
        .into_iter()
        .map(|line| print::display(heap, line))
        .collect();
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    let spans = ox_mode::highlight_lines(&borrowed);
    let rows = lines
        .iter()
        .zip(&spans)
        .map(|(line, spans)| segments(heap, line, spans))
        .collect::<Result<Vec<_>, _>>()?;
    heap.vector_from(&rows).map_err(Error::from)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "ox-reference")
        .arity(0, Some(0))
        .host::<Editor>(|heap, editor, _| {
            let promise = heap.new_promise()?;
            let hold = heap.hold(promise);
            if editor.plugin_state::<Asked>().is_none() {
                editor.set_plugin_state(Asked::default());
            }
            if let Some(asked) = editor.plugin_state_mut::<Asked>() {
                asked.0.push(hold);
            }
            Ok(promise)
        });
    isolate
        .native("ox", "ox-highlight")
        .arity(1, Some(1))
        .host::<Editor>(|heap, _, arguments| highlight(heap, arguments[0]));
    isolate
        .native("ox", "run-in-repl")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            repl::run_in_repl(editor, print::display(heap, arguments[0]));
            Ok(Value::Nil)
        });
}

fn editor_names() -> HashSet<String> {
    let documented = DOCS
        .lines()
        .filter_map(|line| line.strip_prefix('('))
        .filter_map(|rest| rest.split([' ', ')']).next())
        .map(str::to_owned);
    let defined = ox_lang::names::definitions_with(PRELUDE, &ox_mode::EDITOR_FORMS)
        .into_iter()
        .map(|definition| definition.name);
    documented.chain(defined).collect()
}

fn grouped(mut docs: Vec<ModuleDocs>) -> Vec<ModuleDocs> {
    let names = editor_names();
    let Some(core) = docs.iter_mut().find(|module| module.module == "ox") else {
        return docs;
    };
    let (host, library): (Vec<_>, Vec<_>) = core
        .entries
        .drain(..)
        .partition(|entry| names.contains(&entry.name));
    core.entries = library;
    docs.insert(
        1,
        ModuleDocs {
            module: "oxmacs".to_owned(),
            entries: host,
        },
    );
    docs
}

pub(crate) fn deliver(isolate: &mut Isolate, editor: &mut Editor) {
    let Some(asked) = editor.plugin_state_mut::<Asked>() else {
        return;
    };
    let holds = std::mem::take(&mut asked.0);
    if holds.is_empty() {
        return;
    }
    let docs = grouped(isolate.reference());
    for hold in holds {
        let Some(promise) = isolate.release(hold) else {
            continue;
        };
        match isolate.docs_value(&docs) {
            Ok(value) => isolate.resolve(promise, value),
            Err(error) => isolate.reject(promise, &error.message),
        };
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

    use crate::Ox;
    use crate::ide::repl::REPL;
    use crate::testing::{home_with, settle};

    fn started() -> (tempfile::TempDir, Editor, Ox) {
        let home = home_with(&[]);
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.start(&mut editor, Some(home.path()));
        settle(&mut editor, &mut ox);
        (home, editor, ox)
    }

    fn eval(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
        match ox.eval(editor, source) {
            Ok(value) => value,
            Err(error) => panic!("{}", error.message),
        }
    }

    #[test]
    fn the_reference_comes_back_by_module_with_the_editor_api_on_its_own() {
        let (_home, mut editor, mut ox) = started();

        eval(
            &mut editor,
            &mut ox,
            "(def found (atom nil)) (spawn (reset! found (await (ox-reference))))",
        );
        settle(&mut editor, &mut ox);

        let names = |module: &str| {
            format!(
                r#"(map :name (get (first (filter (fn [m] (= m.module "{module}")) @found)) :entries))"#
            )
        };
        assert_eq!(
            eval(&mut editor, &mut ox, "(vec (take 2 (map :module @found)))"),
            r#"["ox" "oxmacs"]"#
        );
        let core = eval(&mut editor, &mut ox, &names("ox"));
        let host = eval(&mut editor, &mut ox, &names("oxmacs"));
        assert!(core.contains(r#""map""#), "{core}");
        assert!(!core.contains(r#""open-buffer""#), "{core}");
        assert!(host.contains(r#""open-buffer""#), "{host}");
        assert!(host.contains(r#""defcommand""#), "{host}");
    }

    #[test]
    fn ox_code_is_highlighted_into_segments() {
        let (_home, mut editor, mut ox) = started();

        let shown = eval(
            &mut editor,
            &mut ox,
            r#"(ox-highlight ["(def x 1) ; one" "(str \"a" "b\")"])"#,
        );

        assert_eq!(
            shown,
            r#"[["(" ["def" :keyword] " " ["x" :function] " " ["1" :constant] ") " ["; one" :comment]] ["(" ["str" :function] " " ["\"a" :string]] [["b\"" :string] ")"]]"#
        );
    }

    #[test]
    fn code_runs_in_the_repl_beside() {
        let (_home, mut editor, mut ox) = started();

        let before = editor.buffer().name().to_owned();
        eval(&mut editor, &mut ox, r#"(run-in-repl "(+ 1 2)")"#);
        settle(&mut editor, &mut ox);

        assert_eq!(editor.buffer().name(), before);
        let id = editor.buffer_named(REPL).expect("a REPL");
        assert!(editor.buffer_shown(id));
        let text = editor.buffer_by_id(id).unwrap().contents();
        assert!(text.ends_with("ox> (+ 1 2)\n3\nox> "), "{text}");
    }
}
