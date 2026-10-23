use std::collections::HashSet;

use ox::describe::ModuleDocs;
use ox::heap::{Heap, HeapFull, Hold};
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::editor::Editor;

use crate::{DOCS, PRELUDE, mode, tools, views};

#[derive(Debug, Default)]
struct Asked(Vec<Hold>);

fn full(error: HeapFull) -> Error {
    Error::new(error.to_string())
}

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
            items.push(heap.string(&text(at..range.start)).map_err(full)?);
        }
        let shown = heap.string(&text(range.clone())).map_err(full)?;
        items.push(match views::face_name(*highlight) {
            Some(face) => heap
                .vector_from(&[shown, Value::keyword(face)])
                .map_err(full)?,
            None => shown,
        });
        at = range.end;
    }
    if at < chars.len() {
        items.push(heap.string(&text(at..chars.len())).map_err(full)?);
    }
    heap.vector_from(&items).map_err(full)
}

fn highlight(heap: &mut Heap, lines: Value) -> Result<Value, Error> {
    let lines: Vec<String> = heap
        .elements(lines)
        .ok_or_else(|| {
            Error::new(format!(
                "ox-highlight expects a vector of lines, got {}",
                print::write(heap, lines)
            ))
        })?
        .into_iter()
        .map(|line| print::display(heap, line))
        .collect();
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    let spans = mode::highlight_lines(&borrowed);
    let rows = lines
        .iter()
        .zip(&spans)
        .map(|(line, spans)| segments(heap, line, spans))
        .collect::<Result<Vec<_>, _>>()?;
    heap.vector_from(&rows).map_err(full)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("ox-reference", 0, Some(0), |heap, editor, _| {
        let promise = heap.new_promise().map_err(full)?;
        let hold = heap.hold(promise);
        if editor.plugin_state::<Asked>().is_none() {
            editor.set_plugin_state(Asked::default());
        }
        if let Some(asked) = editor.plugin_state_mut::<Asked>() {
            asked.0.push(hold);
        }
        Ok(promise)
    });
    isolate.define_host_native::<Editor>("ox-highlight", 1, Some(1), |heap, _, arguments| {
        highlight(heap, arguments[0])
    });
    isolate.define_host_native::<Editor>("run-in-repl", 1, Some(1), |heap, editor, arguments| {
        tools::run_in_repl(editor, print::display(heap, arguments[0]));
        Ok(Value::Nil)
    });
}

fn editor_names() -> HashSet<String> {
    let documented = DOCS
        .lines()
        .filter_map(|line| line.strip_prefix('('))
        .filter_map(|rest| rest.split([' ', ')']).next());
    let defined = PRELUDE.lines().filter_map(|line| {
        line.strip_prefix("(defmacro ")
            .or_else(|| line.strip_prefix("(def ("))
            .and_then(|rest| rest.split([' ', ')']).next())
    });
    documented.chain(defined).map(str::to_owned).collect()
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
    use crate::testing::{home_with, settle};
    use crate::tools::REPL;

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
