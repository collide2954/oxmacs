use ox_lang::Isolate;
use ox_lang::build::make_vector;
use ox_lang::error::{Error, ErrorKind};
use ox_lang::heap::Heap;
use ox_lang::native::{expected, number, whole};
use ox_lang::value::Value;

use super::{bits, signed};
use crate::random::{Random, noise};

const MODULE: &str = "rodeo/random";

fn state(heap: &Heap, name: &str, rng: Value) -> Result<Random, Error> {
    let missing = || expected(heap, name, "a generator from random/new", rng);
    let held = heap.atom_get(rng).ok_or_else(missing)?;
    let parts = heap
        .vector_of(held)
        .map(|vector| vector.items(heap))
        .filter(|parts| parts.len() == 4)
        .ok_or_else(missing)?;
    let mut words = [0; 4];
    for (slot, part) in words.iter_mut().zip(parts) {
        let Value::Int(word) = part else {
            return Err(missing());
        };
        *slot = bits(word);
    }
    Ok(Random::from_state(words))
}

fn store(heap: &mut Heap, rng: Value, random: &Random) -> Result<(), Error> {
    let words = random.state().map(|word| Value::Int(signed(word)));
    let held = make_vector(heap, &words)?;
    heap.atom_set(rng, held);
    Ok(())
}

fn draw<T>(
    heap: &mut Heap,
    name: &str,
    rng: Value,
    use_it: impl FnOnce(&mut Random) -> T,
) -> Result<T, Error> {
    let mut random = state(heap, name, rng)?;
    let drawn = use_it(&mut random);
    store(heap, rng, &random)?;
    Ok(drawn)
}

fn int(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let name = "random/int";
    let (low, high) = match arguments {
        [_, bound] => (0, whole(heap, name, *bound)?),
        _ => (
            whole(heap, name, arguments[1])?,
            whole(heap, name, arguments[2])?,
        ),
    };
    if high <= low {
        return Err(Error::new(format!(
            "random/int needs high above low, got {low} and {high}"
        ))
        .of(ErrorKind::Range));
    }
    Ok(Value::Int(draw(heap, name, arguments[0], |random| {
        random.int(low, high)
    })?))
}

fn pick(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let items = heap
        .elements(arguments[1])
        .ok_or_else(|| expected(heap, "random/pick", "a collection", arguments[1]))?;
    let chosen = draw(heap, "random/pick", arguments[0], |random| {
        random.pick(&items).copied()
    })?;
    Ok(chosen.unwrap_or(Value::Nil))
}

fn shuffle(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let mut items = heap
        .elements(arguments[1])
        .ok_or_else(|| expected(heap, "random/shuffle", "a collection", arguments[1]))?;
    draw(heap, "random/shuffle", arguments[0], |random| {
        random.shuffle(&mut items);
    })?;
    make_vector(heap, &items)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "new")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let seed = whole(heap, "random/new", arguments[0])?;
            let random = Random::new(bits(seed));
            let words = random.state().map(|word| Value::Int(signed(word)));
            let held = make_vector(heap, &words)?;
            heap.atom(held).map_err(Error::from)
        });
    isolate
        .native(MODULE, "float")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            Ok(Value::Float(draw(
                heap,
                "random/float",
                arguments[0],
                Random::float,
            )?))
        });
    isolate
        .native(MODULE, "range")
        .arity(3, Some(3))
        .function(|heap, arguments| {
            let low = number(heap, "random/range", arguments[1])?;
            let high = number(heap, "random/range", arguments[2])?;
            Ok(Value::Float(draw(
                heap,
                "random/range",
                arguments[0],
                |random| random.range(low, high),
            )?))
        });
    isolate
        .native(MODULE, "int")
        .arity(2, Some(3))
        .function(int);
    isolate
        .native(MODULE, "chance?")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let probability = number(heap, "random/chance?", arguments[1])?;
            Ok(Value::Bool(draw(
                heap,
                "random/chance?",
                arguments[0],
                |random| random.chance(probability),
            )?))
        });
    isolate
        .native(MODULE, "pick")
        .arity(2, Some(2))
        .function(pick);
    isolate
        .native(MODULE, "shuffle")
        .arity(2, Some(2))
        .function(shuffle);
    isolate
        .native(MODULE, "noise")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let seed = whole(heap, "random/noise", arguments[0])?;
            let index = whole(heap, "random/noise", arguments[1])?;
            Ok(Value::Float(noise(bits(seed), bits(index))))
        });
}
