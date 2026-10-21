use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, LazyLock};
use std::thread;

use oxmacs_core::editor::Editor;
use rhai::packages::{Package, StandardPackage};
use rhai::{AST, Dynamic, Engine, EvalAltResult, FnPtr, FuncArgs, INT, Module, Scope, Shared};

use crate::api;

const STOPPED: &str = "The script stopped unexpectedly";
const NESTED: &str = "Scripts are nested too deeply";
const GOTO: &str = "goto";
const MAX_NESTING: usize = 8;

static STANDARD: LazyLock<Shared<Module>> =
    LazyLock::new(|| StandardPackage::new().as_shared_module());

pub(crate) type Failure = Box<EvalAltResult>;
pub(crate) type Reply = Result<Dynamic, String>;
type Job = Box<dyn FnOnce(&mut Editor) -> Reply + Send>;
type Link = Sender<(Job, Sender<Reply>)>;

#[derive(Debug)]
struct Nesting(usize);

#[derive(Debug, Default)]
pub(crate) struct Session {
    variables: Scope<'static>,
    functions: Arc<AST>,
}

#[derive(Clone)]
pub(crate) struct Callback {
    function: FnPtr,
    lib: Arc<AST>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Origin {
    pub(crate) line: usize,
    pub(crate) column: usize,
}

fn sandbox() -> Engine {
    let mut engine = Engine::new_raw();
    engine.register_global_module(Shared::clone(&STANDARD));
    engine
        .register_custom_syntax([GOTO, "(", "$expr$", ")"], false, |context, inputs| {
            let position = context.eval_expression_tree(&inputs[0])?;
            context.call_native_fn::<Dynamic>(GOTO, (position,))
        })
        .ok();
    engine
}

fn on_editor(
    link: &Link,
    job: impl FnOnce(&mut Editor) -> Reply + Send + 'static,
) -> Result<Dynamic, Failure> {
    let (reply, answer) = mpsc::channel();
    link.send((Box::new(job), reply)).map_err(|_| STOPPED)?;
    answer.recv().map_err(|_| STOPPED)?.map_err(Into::into)
}

fn with_editor(link: Link, lib: &Arc<AST>) -> Engine {
    let mut engine = sandbox();
    let to = link.clone();
    engine.register_fn("insert", move |text: &str| {
        let text = text.to_owned();
        on_editor(&to, move |editor| api::insert(editor, &text))
    });
    let to = link.clone();
    engine.register_fn("text", move || {
        on_editor(&to, |editor| Ok(api::text(editor)))
    });
    let to = link.clone();
    engine.register_fn("region", move || {
        on_editor(&to, |editor| Ok(api::region(editor)))
    });
    let to = link.clone();
    engine.register_fn("point", move || {
        on_editor(&to, |editor| Ok(api::point(editor)))
    });
    let to = link.clone();
    engine.register_fn(GOTO, move |position: INT| {
        on_editor(&to, move |editor| Ok(api::goto(editor, position)))
    });
    let to = link.clone();
    engine.register_fn("line", move || {
        on_editor(&to, |editor| Ok(api::line(editor)))
    });
    let to = link.clone();
    engine.register_fn("message", move |text: &str| {
        let text = text.to_owned();
        on_editor(&to, move |editor| Ok(api::message(editor, text)))
    });
    let to = link.clone();
    engine.register_fn("run", move |name: &str| {
        let name = name.to_owned();
        on_editor(&to, move |editor| api::run(editor, &name))
    });
    let (to, shared) = (link.clone(), Arc::clone(lib));
    engine.register_fn("ask", move |prompt: &str, function: FnPtr| {
        let callback = Callback {
            function,
            lib: Arc::clone(&shared),
        };
        let prompt = prompt.to_owned();
        on_editor(&to, move |editor| Ok(api::ask(editor, &prompt, callback)))
    });
    let (to, shared) = (link.clone(), Arc::clone(lib));
    engine.register_fn(
        "command",
        move |name: &str, summary: &str, function: FnPtr| {
            let callback = Callback {
                function,
                lib: Arc::clone(&shared),
            };
            let (name, summary) = (name.to_owned(), summary.to_owned());
            on_editor(&to, move |editor| {
                api::command(editor, name, summary, callback)
            })
        },
    );
    engine.register_fn("bind", move |keys: &str, name: &str| {
        let (keys, name) = (keys.to_owned(), name.to_owned());
        on_editor(&link, move |editor| api::bind(editor, &keys, &name))
    });
    engine
}

fn serve(
    editor: &mut Editor,
    lib: &Arc<AST>,
    work: impl FnOnce(&Engine) -> Result<Dynamic, Failure> + Send,
) -> Result<Dynamic, Failure> {
    let (link, requests) = mpsc::channel();
    let engine = with_editor(link, lib);
    thread::scope(|scope| {
        let worker = scope.spawn(move || work(&engine));
        for (job, reply) in &requests {
            reply.send(job(editor)).ok();
        }
        worker.join().unwrap_or_else(|_| Err(STOPPED.into()))
    })
}

fn run(
    editor: &mut Editor,
    lib: &Arc<AST>,
    work: impl FnOnce(&Engine) -> Result<Dynamic, Failure> + Send,
) -> Result<Dynamic, Failure> {
    let depth = editor
        .applet_state::<Nesting>()
        .map_or(0, |nesting| nesting.0);
    if depth >= MAX_NESTING {
        return Err(NESTED.into());
    }
    editor.set_applet_state(Nesting(depth + 1));
    let outcome = serve(editor, lib, work);
    editor.set_applet_state(Nesting(depth));
    outcome
}

impl Session {
    pub(crate) fn evaluate(&mut self, editor: &mut Editor, code: &str) -> Result<Dynamic, Failure> {
        let ast = self.functions.merge(&sandbox().compile(code)?);
        self.functions = Arc::new(ast.clone_functions_only());
        let variables = &mut self.variables;
        run(editor, &self.functions, move |engine| {
            engine.eval_ast_with_scope(variables, &ast)
        })
    }
}

pub(crate) fn evaluate(editor: &mut Editor, code: &str) -> Result<Dynamic, Failure> {
    Session::default().evaluate(editor, code)
}

pub(crate) fn call_back(editor: &mut Editor, callback: &Callback, args: impl FuncArgs + Send) {
    let Callback { function, lib } = callback.clone();
    let outcome = run(editor, &callback.lib, move |engine| {
        function.call::<Dynamic>(engine, &lib, args)
    });
    if let Err(error) = outcome {
        editor.show_message(describe(*error, Origin::default()));
    }
}

fn innermost(error: EvalAltResult) -> EvalAltResult {
    match error {
        EvalAltResult::ErrorInFunctionCall(_, _, inner, _)
        | EvalAltResult::ErrorInModule(_, inner, _) => innermost(*inner),
        other => other,
    }
}

pub(crate) fn describe(error: EvalAltResult, origin: Origin) -> String {
    let mut error = innermost(error);
    let position = error.take_position();
    let text = match &error {
        EvalAltResult::ErrorRuntime(value, _) => value.to_string(),
        other => other.to_string(),
    };
    let Some(line) = position.line() else {
        return text;
    };
    let shown = line + origin.line;
    match position.position() {
        Some(column) if line == 1 => {
            let column = column + origin.column;
            format!("{text} (line {shown}, column {column})")
        }
        Some(column) => format!("{text} (line {shown}, column {column})"),
        None => format!("{text} (line {shown})"),
    }
}
