use crate::builtins::collections::{self, is_list};
use crate::error::Error;
use crate::forms::BUILT_IN;
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::symbol::Sym;
use crate::value::Value;
use crate::value::equal::equal;
use crate::vm::build::{make_map, make_vector};
use crate::vm::modules::PRELUDE;

use super::testing::PLAIN_HEADS;

fn word(name: &str) -> Value {
    Value::symbol(name)
}

fn list(heap: &mut Heap, items: &[Value]) -> Result<Value, Error> {
    heap.list(items.iter().copied()).map_err(Error::from)
}

fn call(heap: &mut Heap, operator: &str, items: &[Value]) -> Result<Value, Error> {
    let mut all = Vec::with_capacity(items.len() + 1);
    all.push(word(operator));
    all.extend_from_slice(items);
    list(heap, &all)
}

fn spliced(heap: &mut Heap, value: Value) -> Result<Vec<Value>, Error> {
    collections::sequence(heap, "~@", value)
}

fn quoted(heap: &mut Heap, form: Value) -> Result<Value, Error> {
    call(heap, "quote", &[form])
}

fn binding(heap: &mut Heap, name: Value, value: Value) -> Result<Value, Error> {
    make_vector(heap, &[name, value])
}

fn when(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let body = call(heap, "do", &arguments[1..])?;
    call(heap, "if", &[arguments[0], body, Value::Nil])
}

fn unless(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let body = call(heap, "do", &arguments[1..])?;
    call(heap, "if", &[arguments[0], Value::Nil, body])
}

fn chained(
    heap: &mut Heap,
    forms: &[Value],
    empty: Value,
    name: &str,
    shortcut: bool,
) -> Result<Value, Error> {
    match forms {
        [] => Ok(empty),
        [only] => Ok(*only),
        [first, more @ ..] => {
            let bound = binding(heap, word("value"), *first)?;
            let rest = call(heap, name, more)?;
            let test = if shortcut {
                call(heap, "if", &[word("value"), word("value"), rest])?
            } else {
                call(heap, "if", &[word("value"), rest, word("value")])?
            };
            call(heap, "let", &[bound, test])
        }
    }
}

fn cond(heap: &mut Heap, clauses: &[Value]) -> Result<Value, Error> {
    let Some((clause, more)) = clauses.split_first() else {
        return Ok(Value::Nil);
    };
    let test = collections::first(heap, *clause)?;
    let rest = collections::rest(heap, *clause)?;
    let body = spliced(heap, rest)?;
    let body = call(heap, "do", &body)?;
    if equal(heap, test, word("else")) {
        return Ok(body);
    }
    let otherwise = call(heap, "cond", more)?;
    call(heap, "if", &[test, body, otherwise])
}

fn case(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let mut branches = vec![word("cond")];
    for clause in &arguments[1..] {
        let parts = if heap.vector_of(*clause).is_some() || heap.pair_of(*clause).is_some() {
            spliced(heap, *clause)?
        } else {
            Vec::new()
        };
        let Some((constants, body)) = parts.split_first() else {
            return Err(Error::new("case clauses are [value body...] vectors"));
        };
        let test = if equal(heap, *constants, word("else")) {
            word("else")
        } else {
            let options = if heap.pair_of(*constants).is_some() {
                spliced(heap, *constants)?
            } else {
                vec![*constants]
            };
            let mut checks = vec![word("or")];
            for option in options {
                let constant = quoted(heap, option)?;
                checks.push(call(heap, "=", &[word("value"), constant])?);
            }
            list(heap, &checks)?
        };
        let mut branch = vec![test];
        branch.extend_from_slice(body);
        branches.push(make_vector(heap, &branch)?);
    }
    let bound = binding(heap, word("value"), arguments[0])?;
    let chosen = list(heap, &branches)?;
    call(heap, "let", &[bound, chosen])
}

enum Step {
    Over(Value, Value),
    When(Value),
    Let(Value),
}

fn steps(heap: &mut Heap, name: &str, bindings: Value) -> Result<Vec<Step>, Error> {
    let Some(vector) = heap.vector_of(bindings) else {
        return Err(Error::new(format!(
            "{name} takes a binding vector and a body"
        )));
    };
    let items = vector.items(heap);
    if items.len() % 2 == 1 {
        return Err(Error::new(format!(
            "{name} bindings come in name and collection pairs"
        )));
    }
    items
        .chunks(2)
        .map(|pair| match pair[0] {
            Value::Keyword(option) if option.name() == "when" => Ok(Step::When(pair[1])),
            Value::Keyword(option) if option.name() == "let" => Ok(Step::Let(pair[1])),
            Value::Keyword(option) => Err(Error::new(format!(
                "{name} takes :when and :let, not :{}",
                option.name()
            ))),
            name => Ok(Step::Over(name, pair[1])),
        })
        .collect()
}

fn each(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let mut inner = call(heap, "do", &arguments[1..])?;
    for step in steps(heap, "each", arguments[0])?.into_iter().rev() {
        inner = match step {
            Step::Over(name, items) => {
                let parameters = make_vector(heap, &[word("_"), name])?;
                let function = call(heap, "fn", &[parameters, inner])?;
                call(heap, "reduce", &[function, Value::Nil, items])?
            }
            Step::When(test) => call(heap, "when", &[test, inner])?,
            Step::Let(bindings) => call(heap, "let", &[bindings, inner])?,
        };
    }
    call(heap, "do", &[inner, Value::Nil])
}

fn comprehension(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let body = call(heap, "do", &arguments[1..])?;
    let mut inner = make_vector(heap, &[body])?;
    for step in steps(heap, "for", arguments[0])?.into_iter().rev() {
        inner = match step {
            Step::Over(name, items) => {
                let parameters = make_vector(heap, &[name])?;
                let function = call(heap, "fn", &[parameters, inner])?;
                call(heap, "mapcat", &[function, items])?
            }
            Step::When(test) => {
                let none = make_vector(heap, &[])?;
                call(heap, "if", &[test, inner, none])?
            }
            Step::Let(bindings) => call(heap, "let", &[bindings, inner])?,
        };
    }
    call(heap, "vec", &[inner])
}

fn repeated(heap: &mut Heap, arguments: &[Value], guard: &str) -> Result<Value, Error> {
    let mut body = vec![word(guard)];
    body.extend_from_slice(arguments);
    body.push(call(heap, "again", &[])?);
    let body = list(heap, &body)?;
    let bindings = make_vector(heap, &[])?;
    call(heap, "loop", &[bindings, body])
}

fn thread(heap: &mut Heap, arguments: &[Value], last: bool) -> Result<Value, Error> {
    let mut threaded = arguments[0];
    for form in &arguments[1..] {
        threaded = if !is_list(heap, *form) {
            list(heap, &[*form, threaded])?
        } else if last {
            let mut items = spliced(heap, *form)?;
            items.push(threaded);
            list(heap, &items)?
        } else {
            let operator = collections::first(heap, *form)?;
            let rest = collections::rest(heap, *form)?;
            let mut items = vec![operator, threaded];
            items.extend(spliced(heap, rest)?);
            list(heap, &items)?
        };
    }
    Ok(threaded)
}

fn if_let(heap: &mut Heap, arguments: &[Value], when: bool) -> Result<Value, Error> {
    let value = second(heap, arguments[0])?;
    let name = collections::first(heap, arguments[0])?;
    let bound = binding(heap, word("value"), value)?;
    let inner = binding(heap, name, word("value"))?;
    let test = if when {
        let mut body = vec![word("let"), inner];
        body.extend_from_slice(&arguments[1..]);
        let then = list(heap, &body)?;
        call(heap, "if", &[word("value"), then, Value::Nil])?
    } else {
        let then = call(heap, "let", &[inner, arguments[1]])?;
        let mut branches = vec![word("if"), word("value"), then];
        branches.extend_from_slice(&arguments[2..]);
        list(heap, &branches)?
    };
    call(heap, "let", &[bound, test])
}

fn second(heap: &mut Heap, value: Value) -> Result<Value, Error> {
    let items = collections::sequence(heap, "second", value)?;
    Ok(items.get(1).copied().unwrap_or(Value::Nil))
}

fn rules_macro(
    heap: &mut Heap,
    name: Value,
    literals: Value,
    rules: Value,
) -> Result<Value, Error> {
    let parameters = make_vector(heap, &[word("&"), word("form")])?;
    let quoted_name = quoted(heap, name)?;
    let rules = quoted(heap, rules)?;
    let expanded = call(
        heap,
        "expand-rules",
        &[quoted_name, literals, rules, word("form")],
    )?;
    call(heap, "defmacro", &[name, parameters, expanded])
}

fn defrule(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let (pattern, template) = (arguments[0], arguments[1]);
    let name = collections::first(heap, pattern)?;
    let literals = make_vector(heap, &[])?;
    let rule = make_vector(heap, &[pattern, template])?;
    let rules = make_vector(heap, &[rule])?;
    rules_macro(heap, name, literals, rules)
}

fn defrules(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let literals = quoted(heap, arguments[1])?;
    let rules = make_vector(heap, &arguments[2..])?;
    rules_macro(heap, arguments[0], literals, rules)
}

fn maybe(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let handler = call(heap, "catch", &[word("error"), word("e"), Value::Nil])?;
    call(heap, "try", &[arguments[0], handler])
}

fn atomically(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let enter = call(heap, "atomic-enter", &[])?;
    let leave = call(heap, "atomic-leave", &[])?;
    let body = call(heap, "do", arguments)?;
    let cleanup = call(heap, "finally", &[leave])?;
    let guarded = call(heap, "try", &[body, cleanup])?;
    call(heap, "do", &[enter, guarded])
}

fn wrapped(heap: &mut Heap, operator: &str, body: &[Value]) -> Result<Value, Error> {
    let mut function = vec![word("fn"), make_vector(heap, &[])?];
    function.extend_from_slice(body);
    let function = list(heap, &function)?;
    call(heap, operator, &[function])
}

fn some_thread(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let Some((form, more)) = arguments[1..].split_first() else {
        return Ok(arguments[0]);
    };
    let step = if is_list(heap, *form) {
        let operator = collections::first(heap, *form)?;
        let rest = collections::rest(heap, *form)?;
        let mut items = vec![operator, word("it")];
        items.extend(spliced(heap, rest)?);
        list(heap, &items)?
    } else {
        list(heap, &[*form, word("it")])?
    };
    let bound = binding(heap, word("it"), arguments[0])?;
    let missing = call(heap, "nil?", &[word("it")])?;
    let mut next = vec![word("some->"), step];
    next.extend_from_slice(more);
    let next = list(heap, &next)?;
    let test = call(heap, "if", &[missing, Value::Nil, next])?;
    call(heap, "let", &[bound, test])
}

fn as_thread(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let (value, name) = (arguments[0], arguments[1]);
    let Some((form, more)) = arguments[2..].split_first() else {
        return Ok(value);
    };
    let bound = binding(heap, name, value)?;
    let mut next = vec![word("as->"), *form, name];
    next.extend_from_slice(more);
    let next = list(heap, &next)?;
    call(heap, "let", &[bound, next])
}

fn test(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let mut run = vec![word("fn"), make_vector(heap, &[])?];
    run.extend_from_slice(&arguments[1..]);
    let run = list(heap, &run)?;
    let entry = make_map(
        heap,
        &[
            Value::keyword("name"),
            arguments[0],
            Value::keyword("run"),
            run,
        ],
    )?;
    let register = call(
        heap,
        "swap!",
        &[word("registered-tests"), word("conj"), entry],
    )?;
    call(heap, "do", &[register, Value::Nil])
}

fn checked(heap: &Heap, form: Value) -> bool {
    if !is_list(heap, form) {
        return false;
    }
    let Some(pair) = heap.pair_of(form) else {
        return false;
    };
    let operator = match pair.head {
        Value::Symbol(name) => Some(name),
        other => heap.identifier_of(other).map(|identifier| identifier.name),
    };
    operator.is_some_and(|name| !PLAIN_HEADS.contains(&name.name()))
}

fn expect(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let form = arguments[0];
    let quoted_form = quoted(heap, form)?;
    let failed = |heap: &mut Heap, shown: Value| -> Result<Value, Error> {
        let failure = call(heap, "expectation-failed", &[shown])?;
        call(heap, "raise", &[failure])
    };
    if !checked(heap, form) {
        let shown = call(heap, "pr-str", &[quoted_form])?;
        let failure = failed(heap, shown)?;
        return call(heap, "if", &[form, Value::Bool(true), failure]);
    }
    let operator = collections::first(heap, form)?;
    let operands = collections::rest(heap, form)?;
    let mut values = vec![word("vector")];
    values.extend(spliced(heap, operands)?);
    let values = list(heap, &values)?;
    let bound = binding(heap, word("values"), values)?;
    let applied = call(heap, "apply", &[operator, word("values")])?;
    let shown = call(heap, "explain", &[quoted_form, word("values")])?;
    let failure = failed(heap, shown)?;
    let test = call(heap, "if", &[applied, Value::Bool(true), failure])?;
    call(heap, "let", &[bound, test])
}

pub(super) fn install(isolate: &mut Isolate) {
    isolate.define_expander("when", 1, None, when);
    isolate.define_expander("unless", 1, None, unless);
    isolate.define_expander("and", 0, None, |heap, arguments| {
        chained(heap, arguments, Value::Bool(true), "and", false)
    });
    isolate.define_expander("or", 0, None, |heap, arguments| {
        chained(heap, arguments, Value::Nil, "or", true)
    });
    isolate.define_expander("cond", 0, None, cond);
    isolate.define_expander("case", 1, None, case);
    isolate.define_expander("each", 1, None, each);
    isolate.define_expander("for", 1, None, comprehension);
    isolate.define_expander("while", 1, None, |heap, arguments| {
        repeated(heap, arguments, "when")
    });
    isolate.define_expander("until", 1, None, |heap, arguments| {
        repeated(heap, arguments, "unless")
    });
    isolate.define_expander("->", 1, None, |heap, arguments| {
        thread(heap, arguments, false)
    });
    isolate.define_expander("->>", 1, None, |heap, arguments| {
        thread(heap, arguments, true)
    });
    isolate.define_expander("if-let", 2, None, |heap, arguments| {
        if_let(heap, arguments, false)
    });
    isolate.define_expander("when-let", 1, None, |heap, arguments| {
        if_let(heap, arguments, true)
    });
    isolate.define_expander("defrule", 2, Some(2), defrule);
    isolate.define_expander("defrules", 2, None, defrules);
    isolate.define_expander("maybe", 1, Some(1), maybe);
    isolate.define_expander("atomically", 0, None, atomically);
    isolate.define_expander("gen", 0, None, |heap, arguments| {
        wrapped(heap, "generator", arguments)
    });
    isolate.define_expander("spawn", 0, None, |heap, arguments| {
        wrapped(heap, "spawn-call", arguments)
    });
    isolate.define_expander("some->", 1, None, some_thread);
    isolate.define_expander("as->", 2, None, as_thread);
    isolate.define_expander("test", 1, None, test);
    isolate.define_expander("expect", 1, Some(1), expect);
    for (name, form) in BUILT_IN {
        isolate
            .runtime_mut()
            .set_form(PRELUDE, Sym::intern(name), Some(*form));
    }
}
