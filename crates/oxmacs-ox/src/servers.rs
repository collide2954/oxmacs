use ox::heap::Heap;
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::editor::{Editor, ServerSpec};

const NATIVE: &str = "define-language-server";

fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

fn leak_all<T>(items: Vec<T>) -> &'static [T] {
    Box::leak(items.into_boxed_slice())
}

fn name_of(heap: &Heap, value: Value) -> String {
    match value {
        Value::Keyword(name) | Value::Symbol(name) => name.name().to_owned(),
        _ => print::display(heap, value),
    }
}

fn texts(heap: &Heap, option: &str, value: Value) -> Result<Vec<String>, Error> {
    heap.elements(value)
        .filter(|_| heap.text(value).is_none())
        .map(|items| items.into_iter().map(|item| name_of(heap, item)).collect())
        .ok_or_else(|| {
            Error::new(format!(
                "{NATIVE} expects a vector of names for :{option}, got {}",
                print::brief(heap, value)
            ))
        })
}

#[derive(Default)]
struct Options {
    languages: Vec<(String, String)>,
    program: Option<String>,
    args: Vec<String>,
    roots: Vec<String>,
    install: String,
}

fn options(heap: &Heap, given: &[Value]) -> Result<Options, Error> {
    let mut found = Options::default();
    for pair in given.chunks(2) {
        let [Value::Keyword(option), value] = pair else {
            return Err(unknown(heap, pair[0]));
        };
        match option.name() {
            "languages" => {
                found.languages = heap
                    .entries(*value)
                    .map(|entries| {
                        entries
                            .into_iter()
                            .map(|(mode, id)| (name_of(heap, mode), name_of(heap, id)))
                            .collect()
                    })
                    .ok_or_else(|| {
                        Error::new(format!(
                            "{NATIVE} expects a map of mode names to language ids for :languages, got {}",
                            print::brief(heap, *value)
                        ))
                    })?;
            }
            "program" => found.program = Some(print::display(heap, *value)),
            "args" => found.args = texts(heap, "args", *value)?,
            "roots" => found.roots = texts(heap, "roots", *value)?,
            "install" => found.install = print::display(heap, *value),
            _ => return Err(unknown(heap, pair[0])),
        }
    }
    Ok(found)
}

fn unknown(heap: &Heap, option: Value) -> Error {
    Error::new(format!(
        "{NATIVE} takes :languages, :program, :args, :roots and :install, not {}",
        print::brief(heap, option)
    ))
}

fn define(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = print::display(heap, arguments[0]);
    let given = options(heap, &arguments[1..])?;
    let program = given
        .program
        .ok_or_else(|| Error::new(format!("{NATIVE} needs a :program")))?;
    let args: Vec<&'static str> = given.args.into_iter().map(leak).collect();
    let spec = ServerSpec {
        name: leak(name),
        languages: leak_all(
            given
                .languages
                .into_iter()
                .map(|(mode, id)| (leak(mode), leak(id)))
                .collect(),
        ),
        programs: leak_all(vec![(leak(program), leak_all(args))]),
        roots: leak_all(given.roots.into_iter().map(leak).collect()),
        install: leak(given.install),
        runs: false,
    };
    editor.add_language_servers(&[spec]);
    Ok(Value::Nil)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>(NATIVE, 1, None, define);
    isolate.define_host_native::<Editor>("language-server-ready?", 0, Some(0), |_, editor, _| {
        Ok(Value::Bool(editor.lsp_serves_here()))
    });
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::{Editor, Effect};

    use crate::Ox;
    use crate::testing::settle;

    fn eval(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
        match ox.eval(editor, source) {
            Ok(shown) => shown,
            Err(error) => format!("error: {}", error.message),
        }
    }

    #[test]
    fn plugins_define_language_servers_that_start_for_their_modes() {
        let folder = tempfile::tempdir().unwrap();
        let program = folder.path().join("serve-r");
        std::fs::write(&program, "").unwrap();
        std::fs::write(folder.path().join("DESCRIPTION"), "Package: a\n").unwrap();
        std::fs::create_dir_all(folder.path().join("R")).unwrap();
        let script = folder.path().join("R").join("a.R");
        std::fs::write(&script, "x <- 1\n").unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        assert_eq!(
            eval(
                &mut editor,
                &mut ox,
                &format!(
                    r#"(define-language-server "serve-r" :languages {{"R" "r"}} :program {:?} :args ["-e" "run()"] :roots ["DESCRIPTION"] :install "Install it in R")"#,
                    program.display().to_string()
                )
            ),
            "nil"
        );
        editor.open_file(&script);
        settle(&mut editor, &mut ox);

        let started: Vec<_> = editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::StartServer(_, launch) => Some(launch),
                _ => None,
            })
            .collect();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].program, program);
        assert_eq!(started[0].args, ["-e", "run()"]);
        assert_eq!(started[0].directory, folder.path());
        assert_eq!(
            eval(&mut editor, &mut ox, "(language-server-ready?)"),
            "false"
        );
        assert!(
            eval(
                &mut editor,
                &mut ox,
                r#"(define-language-server "x" :languages ["R"] :program "x")"#
            )
            .contains("define-language-server expects a map of mode names to language ids")
        );
        assert!(
            eval(
                &mut editor,
                &mut ox,
                r#"(define-language-server "x" :colour 1)"#
            )
            .contains(
                "define-language-server takes :languages, :program, :args, :roots and :install"
            )
        );
    }
}
