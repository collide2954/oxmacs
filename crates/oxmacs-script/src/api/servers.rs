use ox_lang::heap::Heap;
use ox_lang::native::{given, name_of};
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::{Editor, ServerSpec};

const NATIVE: &str = "define-language-server";

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
    programs: Vec<(String, Vec<String>)>,
    install: String,
    runs: bool,
}

fn programs(heap: &Heap, value: Value) -> Result<Vec<(String, Vec<String>)>, Error> {
    let wrong = || {
        Error::new(format!(
            "{NATIVE} expects a vector of [program arg ...] vectors for :programs, got {}",
            print::brief(heap, value)
        ))
    };
    heap.elements(value)
        .ok_or_else(wrong)?
        .into_iter()
        .map(|choice| {
            let words = texts(heap, "programs", choice)?;
            let (program, args) = words.split_first().ok_or_else(wrong)?;
            Ok((program.clone(), args.to_vec()))
        })
        .collect()
}

const DEFINE: [&str; 6] = [
    "languages",
    "program",
    "args",
    "programs",
    "install",
    "runs",
];

fn options(heap: &Heap, slots: &[Value]) -> Result<Options, Error> {
    let mut found = Options::default();
    for (option, value) in given(&DEFINE, slots) {
        match option {
            "languages" => {
                found.languages = heap
                    .entries(value)
                    .map(|entries| {
                        entries
                            .into_iter()
                            .map(|(mode, id)| (name_of(heap, mode), name_of(heap, id)))
                            .collect()
                    })
                    .ok_or_else(|| {
                        Error::new(format!(
                            "{NATIVE} expects a map of mode names to language ids for :languages, got {}",
                            print::brief(heap, value)
                        ))
                    })?;
            }
            "program" => found.program = Some(print::display(heap, value)),
            "args" => found.args = texts(heap, "args", value)?,
            "programs" => found.programs = programs(heap, value)?,
            "install" => found.install = print::display(heap, value),
            _ => found.runs = value.truthy(),
        }
    }
    Ok(found)
}

fn define(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = print::display(heap, arguments[0]);
    let given = options(heap, &arguments[1..])?;
    let choices = match given.program {
        Some(program) => vec![(program, given.args)],
        None if !given.programs.is_empty() => given.programs,
        None => {
            return Err(Error::new(format!(
                "{NATIVE} needs a :program or :programs"
            )));
        }
    };
    let spec = ServerSpec {
        name,
        languages: given.languages,
        programs: choices,
        install: given.install,
        runs: given.runs,
    };
    editor.add_language_servers(&[spec]);
    Ok(Value::Nil)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", NATIVE)
        .arity(1, Some(1))
        .keywords(&DEFINE)
        .host::<Editor>(define);
    isolate
        .native("ox", "language-server-ready?")
        .arity(0, Some(0))
        .host::<Editor>(|_, editor, _| Ok(Value::Bool(editor.lsp_serves_here())));
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
        let program = folder.path().join(if cfg!(windows) {
            "serve-r.exe"
        } else {
            "serve-r"
        });
        std::fs::write(&program, "").unwrap();
        std::fs::write(folder.path().join("DESCRIPTION"), "Package: a\n").unwrap();
        std::fs::create_dir_all(folder.path().join("R")).unwrap();
        let script = folder.path().join("R").join("a.R");
        std::fs::write(&script, "x <- 1\n").unwrap();
        let mut editor = oxmacs_core::testing::kits();
        let mut ox = Ox::new();

        assert_eq!(
            eval(
                &mut editor,
                &mut ox,
                &format!(
                    r#"(define-language-server "serve-r" :languages {{"R" "r"}} :program {:?} :args ["-e" "run()"] :install "Install it in R")"#,
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
            .contains("define-language-server has no keyword parameter :colour")
        );
    }

    #[test]
    fn servers_try_their_programs_in_turn_and_say_when_list_tools_can_install_them() {
        let folder = tempfile::tempdir().unwrap();
        let second = folder.path().join(if cfg!(windows) {
            "second-server.exe"
        } else {
            "second-server"
        });
        std::fs::write(&second, "").unwrap();
        let script = folder.path().join("a.R");
        std::fs::write(&script, "x <- 1\n").unwrap();
        let mut editor = oxmacs_core::testing::kits();
        let mut ox = Ox::new();
        eval(
            &mut editor,
            &mut ox,
            &format!(
                r#"(define-language-server "turns" :languages {{"R" "r"}} :programs [["oxmacs-no-such-server" "--stdio"] [{:?} "--serve"]])
                   (define-language-server "missing" :languages {{"R" "r"}} :programs [["oxmacs-no-such-server"]] :install "npm install -g missing" :runs true)"#,
                second.display().to_string()
            ),
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
        assert_eq!(started.len(), 1, "{started:?}");
        assert_eq!(started[0].program, second);
        assert_eq!(started[0].args, ["--serve"]);
        let listed = editor.command_named("list-tools").unwrap();
        editor.execute(listed);
        let shown = editor.buffer().contents();
        assert!(
            shown.contains("not installed; i runs npm install -g missing"),
            "{shown}"
        );
    }
}
