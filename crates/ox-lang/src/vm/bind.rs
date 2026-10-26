use crate::bytecode::{Arity, Code};
use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::native::arguments;
use crate::symbol::Sym;
use crate::value::Value;
use crate::value::print;

pub enum Binding {
    Direct,
    Bound { values: Vec<Value>, missing: u64 },
}

fn name(code: &Code) -> &'static str {
    code.name.map_or("fn", Sym::name)
}

fn wrong_count(name: &str, arity: &Arity, count: usize) -> Error {
    let required = usize::from(arity.required);
    let wanted = if arity.rest {
        format!("at least {}", arguments(required))
    } else if arity.optional > 0 {
        format!(
            "{required} to {} arguments",
            required + usize::from(arity.optional)
        )
    } else {
        arguments(required)
    };
    Error::new(format!("{name} takes {wanted}, got {count}")).of(ErrorKind::Arity)
}

fn unknown_keyword(heap: &Heap, name: &str, value: Value) -> Error {
    Error::new(format!(
        "{name} has no keyword parameter {}",
        print::brief(heap, value)
    ))
    .of(ErrorKind::Arity)
}

pub fn bind(heap: &mut Heap, code: &Code, arguments: &[Value]) -> Result<Binding, Error> {
    bind_to(heap, name(code), &code.arity, arguments)
}

pub(crate) fn bind_to(
    heap: &mut Heap,
    name: &str,
    arity: &Arity,
    arguments: &[Value],
) -> Result<Binding, Error> {
    let count = arguments.len();
    let required = usize::from(arity.required);
    if arity.simple() {
        return if count == required {
            Ok(Binding::Direct)
        } else {
            Err(wrong_count(name, arity, count))
        };
    }
    if count < required {
        return Err(wrong_count(name, arity, count));
    }
    let named_start = (required..count)
        .find(|index| {
            matches!(arguments[*index], Value::Keyword(name) if arity.keywords.contains(&name))
        })
        .unwrap_or(count);
    let optional = usize::from(arity.optional);
    let positional = &arguments[required..named_start];
    let mut values = arguments[..required].to_vec();
    let mut missing = 0_u64;
    for index in 0..optional {
        if let Some(value) = positional.get(index) {
            values.push(*value);
        } else {
            values.push(Value::Nil);
            missing |= 1 << index;
        }
    }
    let extra = positional.get(optional..).unwrap_or_default();
    if arity.rest {
        let rest = heap
            .list(extra.iter().copied())
            .map_err(|full| Error::new(full.to_string()))?;
        values.push(rest);
    } else if let Some(first) = extra.first() {
        if !arity.keywords.is_empty() && matches!(first, Value::Keyword(_)) {
            return Err(unknown_keyword(heap, name, *first));
        }
        return Err(wrong_count(name, arity, count));
    }
    let mut named = vec![None; arity.keywords.len()];
    for pair in arguments[named_start..].chunks(2) {
        match pair {
            [Value::Keyword(keyword), value] => {
                let Some(index) = arity.keywords.iter().position(|known| known == keyword) else {
                    return Err(unknown_keyword(heap, name, pair[0]));
                };
                named[index] = Some(*value);
            }
            [Value::Keyword(keyword)] => {
                return Err(Error::new(format!(
                    "{name} was given :{} without a value",
                    keyword.name()
                ))
                .of(ErrorKind::Arity));
            }
            [other, ..] => return Err(unknown_keyword(heap, name, *other)),
            [] => {}
        }
    }
    for (index, value) in named.into_iter().enumerate() {
        if let Some(value) = value {
            values.push(value);
        } else {
            values.push(Value::Nil);
            missing |= 1 << (optional + index);
        }
    }
    Ok(Binding::Bound { values, missing })
}
