use std::path::{Path, PathBuf};

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::Value;
use crate::value::print;
use crate::vm::build::full;

const MODULE: &str = "ox/path";

pub(crate) fn text<'a>(heap: &'a Heap, name: &str, value: Value) -> Result<&'a str, Error> {
    heap.text(value).ok_or_else(|| {
        Error::new(format!(
            "{name} expects a string, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })
}

fn shown(heap: &mut Heap, path: Option<&Path>) -> Result<Value, Error> {
    match path {
        Some(path) => heap.string(&path.display().to_string()).map_err(full),
        None => Ok(Value::Nil),
    }
}

fn part(
    heap: &mut Heap,
    name: &str,
    value: Value,
    pick: fn(&Path) -> Option<&std::ffi::OsStr>,
) -> Result<Value, Error> {
    let path = PathBuf::from(text(heap, name, value)?);
    match pick(&path).and_then(|part| part.to_str()) {
        Some(part) => heap.string(part).map_err(full),
        None => Ok(Value::Nil),
    }
}

fn home() -> Option<PathBuf> {
    std::env::home_dir()
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "join")
        .arity(1, None)
        .function(|heap, arguments| {
            let mut joined = PathBuf::new();
            for argument in arguments {
                joined.push(text(heap, "path/join", *argument)?);
            }
            shown(heap, Some(&joined))
        });
    isolate
        .native(MODULE, "parent")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let path = PathBuf::from(text(heap, "path/parent", arguments[0])?);
            shown(
                heap,
                path.parent()
                    .filter(|parent| !parent.as_os_str().is_empty()),
            )
        });
    isolate
        .native(MODULE, "name")
        .arity(1, Some(1))
        .function(|heap, arguments| part(heap, "path/name", arguments[0], Path::file_name));
    isolate
        .native(MODULE, "stem")
        .arity(1, Some(1))
        .function(|heap, arguments| part(heap, "path/stem", arguments[0], Path::file_stem));
    isolate
        .native(MODULE, "extension")
        .arity(1, Some(1))
        .function(|heap, arguments| part(heap, "path/extension", arguments[0], Path::extension));
    isolate
        .native(MODULE, "home")
        .arity(0, Some(0))
        .function(|heap, _| shown(heap, home().as_deref()));
    isolate
        .native(MODULE, "expand")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let given = text(heap, "path/expand", arguments[0])?.to_owned();
            let expanded = match (given.strip_prefix('~'), home()) {
                (Some(rest), Some(home)) => home.join(rest.trim_start_matches(['/', '\\'])),
                _ => PathBuf::from(given),
            };
            shown(heap, Some(&expanded))
        });
    isolate
        .native(MODULE, "temp")
        .arity(0, Some(0))
        .function(|heap, _| shown(heap, Some(&std::env::temp_dir())));
    isolate
        .native(MODULE, "separator")
        .arity(0, Some(0))
        .function(|heap, _| heap.string(std::path::MAIN_SEPARATOR_STR).map_err(full));
    isolate
        .native(MODULE, "abbreviate")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let path = PathBuf::from(text(heap, "path/abbreviate", arguments[0])?);
            let shown = match home()
                .and_then(|home| path.strip_prefix(&home).ok().map(Path::to_path_buf))
            {
                Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
                Some(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
                None => path.display().to_string(),
            };
            heap.string(&shown).map_err(full)
        });
    isolate
        .native(MODULE, "absolute?")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            Ok(Value::Bool(
                Path::new(text(heap, "path/absolute?", arguments[0])?).is_absolute(),
            ))
        });
}
