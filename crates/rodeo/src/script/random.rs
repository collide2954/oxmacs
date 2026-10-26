use ox::Isolate;
use ox::build::{full, make_vector};
use ox::error::{Error, ErrorKind};
use ox::heap::Heap;
use ox::value::Value;

use super::{bits, number, signed, wrong};
use crate::random::{Random, noise};

const MODULE: &str = "rodeo/random";

fn whole(heap: &Heap, name: &str, value: Value) -> Result<i64, Error> {
    match value {
        Value::Int(number) => Ok(number),
        _ => Err(wrong(heap, name, "a whole number", value)),
    }
}

fn state(heap: &Heap, name: &str, rng: Value) -> Result<Random, Error> {
    let missing = || wrong(heap, name, "a generator from random/new", rng);
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
        .ok_or_else(|| wrong(heap, "random/pick", "a collection", arguments[1]))?;
    let chosen = draw(heap, "random/pick", arguments[0], |random| {
        random.pick(&items).copied()
    })?;
    Ok(chosen.unwrap_or(Value::Nil))
}

fn shuffle(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let mut items = heap
        .elements(arguments[1])
        .ok_or_else(|| wrong(heap, "random/shuffle", "a collection", arguments[1]))?;
    draw(heap, "random/shuffle", arguments[0], |random| {
        random.shuffle(&mut items);
    })?;
    make_vector(heap, &items)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native_in(MODULE, "new", 1, Some(1), |heap, arguments| {
        let seed = whole(heap, "random/new", arguments[0])?;
        let random = Random::new(bits(seed));
        let words = random.state().map(|word| Value::Int(signed(word)));
        let held = make_vector(heap, &words)?;
        heap.atom(held).map_err(full)
    });
    isolate.define_native_in(MODULE, "float", 1, Some(1), |heap, arguments| {
        Ok(Value::Float(draw(
            heap,
            "random/float",
            arguments[0],
            Random::float,
        )?))
    });
    isolate.define_native_in(MODULE, "range", 3, Some(3), |heap, arguments| {
        let low = number(heap, "random/range", arguments[1])?;
        let high = number(heap, "random/range", arguments[2])?;
        Ok(Value::Float(draw(
            heap,
            "random/range",
            arguments[0],
            |random| random.range(low, high),
        )?))
    });
    isolate.define_native_in(MODULE, "int", 2, Some(3), int);
    isolate.define_native_in(MODULE, "chance?", 2, Some(2), |heap, arguments| {
        let probability = number(heap, "random/chance?", arguments[1])?;
        Ok(Value::Bool(draw(
            heap,
            "random/chance?",
            arguments[0],
            |random| random.chance(probability),
        )?))
    });
    isolate.define_native_in(MODULE, "pick", 2, Some(2), pick);
    isolate.define_native_in(MODULE, "shuffle", 2, Some(2), shuffle);
    isolate.define_native_in(MODULE, "noise", 2, Some(2), |heap, arguments| {
        let seed = whole(heap, "random/noise", arguments[0])?;
        let index = whole(heap, "random/noise", arguments[1])?;
        Ok(Value::Float(noise(bits(seed), bits(index))))
    });
}
