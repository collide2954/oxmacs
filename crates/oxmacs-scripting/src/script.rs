use std::sync::LazyLock;
use std::sync::mpsc::{self, Sender};
use std::thread;

use oxmacs_core::editor::Editor;
use rhai::packages::{Package, StandardPackage};
use rhai::{Dynamic, Engine, EvalAltResult, INT, Module, Shared};

use crate::api;

const STOPPED: &str = "The script stopped unexpectedly";
const GOTO: &str = "goto";

static STANDARD: LazyLock<Shared<Module>> =
    LazyLock::new(|| StandardPackage::new().as_shared_module());

pub(crate) type Failure = Box<EvalAltResult>;
pub(crate) type Reply = Result<Dynamic, String>;
type Job = Box<dyn FnOnce(&mut Editor) -> Reply + Send>;
type Link = Sender<(Job, Sender<Reply>)>;

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

fn with_editor(link: Link) -> Engine {
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
    engine.register_fn("line", move || {
        on_editor(&link, |editor| Ok(api::line(editor)))
    });
    engine
}

fn run(
    editor: &mut Editor,
    work: impl FnOnce(&Engine) -> Result<Dynamic, Failure> + Send,
) -> Result<Dynamic, Failure> {
    let (link, requests) = mpsc::channel();
    let engine = with_editor(link);
    thread::scope(|scope| {
        let worker = scope.spawn(move || work(&engine));
        for (job, reply) in &requests {
            reply.send(job(editor)).ok();
        }
        worker.join().unwrap_or_else(|_| Err(STOPPED.into()))
    })
}

pub(crate) fn evaluate(editor: &mut Editor, code: &str) -> Result<Dynamic, Failure> {
    let ast = sandbox().compile(code)?;
    run(editor, move |engine| engine.eval_ast(&ast))
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
