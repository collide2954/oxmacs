use crate::text;
use ox_lang::Isolate;
use ox_lang::build::full;
use ox_lang::error::Error;
use ox_lang::heap::Heap;
use ox_lang::value::Value;

const MODULE: &str = "ox/csv";

fn rows(text: &str) -> Result<Vec<Vec<String>>, Error> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if quoted {
            match character {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => quoted = false,
                _ => field.push(character),
            }
            continue;
        }
        match character {
            '"' => {
                quoted = true;
                started = true;
            }
            ',' => {
                row.push(std::mem::take(&mut field));
                started = true;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                started = false;
            }
            _ => {
                field.push(character);
                started = true;
            }
        }
    }
    if quoted {
        return Err(Error::new("csv/read found a quote that never closes"));
    }
    if started || !field.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

fn read(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let found = rows(text(heap, "csv/read", arguments[0])?)?;
    let header = arguments[1].truthy();
    let mut built = Vec::with_capacity(found.len());
    if header {
        let mut found = found.into_iter();
        let names = found.next().unwrap_or_default();
        for row in found {
            let mut items = Vec::with_capacity(names.len() * 2);
            for (name, value) in names
                .iter()
                .zip(row.iter().chain(std::iter::repeat(&String::new())))
            {
                items.push(heap.string(name).map_err(full)?);
                items.push(heap.string(value).map_err(full)?);
            }
            built.push(heap.map_from(&items)?);
        }
    } else {
        for row in found {
            let fields = row
                .iter()
                .map(|field| heap.string(field))
                .collect::<Result<Vec<_>, _>>()
                .map_err(full)?;
            built.push(heap.vector_from(&fields).map_err(full)?);
        }
    }
    heap.vector_from(&built).map_err(full)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "read")
        .arity(1, Some(1))
        .keywords(&["header"])
        .function(read);
}
