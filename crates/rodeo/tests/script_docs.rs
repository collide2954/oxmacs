#[test]
fn every_native_is_documented_with_its_arity() {
    let mut isolate = ox_lang::Isolate::new();
    rodeo::script::install(&mut isolate);
    let problems = isolate.undocumented();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn modules_are_listed() {
    let names: Vec<&str> = rodeo::script::MODULES
        .iter()
        .map(|(name, _)| *name)
        .collect();
    assert!(names.contains(&"rodeo/vec"));
}
