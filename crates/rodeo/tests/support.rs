pub fn evaluated(imports: &str, source: &str) -> String {
    let mut isolate = ox::Isolate::new();
    rodeo::script::install(&mut isolate);
    match isolate.eval(&format!("(module (import {imports})) {source}")) {
        Ok(value) => isolate.write(value),
        Err(error) => format!("error: {}", error.message),
    }
}
