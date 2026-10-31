use std::path::Path;

use ox_lang::Isolate;
use ox_lang::error::Error;
use ox_lang::value::Value;
use oxmacs_files::wsl;

use crate::{map_value, text};

const MODULE: &str = "ox/wsl";

pub fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "place")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let given = text(heap, "wsl/place", arguments[0])?;
            let Some(found) = wsl::place(Path::new(given)).or_else(|| wsl::named(given)) else {
                return Ok(Value::Nil);
            };
            let distro = heap.string(&found.distro)?;
            let path = heap.string(&found.path)?;
            map_value(heap, &[("distro", distro), ("path", path)])
        });
    isolate
        .native(MODULE, "share")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let distro = text(heap, "wsl/share", arguments[0])?.to_owned();
            let path = text(heap, "wsl/share", arguments[1])?;
            let shared = wsl::share(&distro, path);
            heap.string(&shared.to_string_lossy()).map_err(Error::from)
        });
    isolate
        .native(MODULE, "route!")
        .arity(1, Some(1))
        .outside()
        .function(|_, arguments| {
            oxmacs_process::run_inside_wsl(arguments[0].truthy());
            Ok(Value::Nil)
        });
}
