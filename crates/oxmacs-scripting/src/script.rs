use std::sync::LazyLock;

use rhai::packages::{Package, StandardPackage};
use rhai::{Dynamic, Engine, EvalAltResult, Module, Shared};

static STANDARD: LazyLock<Shared<Module>> =
    LazyLock::new(|| StandardPackage::new().as_shared_module());

pub(crate) type Failure = Box<EvalAltResult>;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Origin {
    pub(crate) line: usize,
    pub(crate) column: usize,
}

fn sandbox() -> Engine {
    let mut engine = Engine::new_raw();
    engine.register_global_module(Shared::clone(&STANDARD));
    engine
}

pub(crate) fn evaluate(code: &str) -> Result<Dynamic, Failure> {
    let engine = sandbox();
    let ast = engine.compile(code)?;
    engine.eval_ast(&ast)
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
