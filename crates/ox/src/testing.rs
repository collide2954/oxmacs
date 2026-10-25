use std::time::Duration;

use crate::Isolate;
use crate::error::Error;
use crate::value::Value;

pub const TASKS: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct Outcome {
    pub name: String,
    pub error: Option<Error>,
}

fn field(isolate: &Isolate, map: Value, key: &str) -> Value {
    isolate
        .heap()
        .entries(map)
        .unwrap_or_default()
        .into_iter()
        .find(|(found, _)| *found == Value::keyword(key))
        .map_or(Value::Nil, |(_, value)| value)
}

pub fn run_registered(isolate: &mut Isolate, file: &str) -> Vec<Outcome> {
    let Ok(tests) = isolate.eval("(vec @registered-tests)") else {
        return Vec::new();
    };
    let mut outcomes = Vec::new();
    for test in isolate.heap().elements(tests).unwrap_or_default() {
        let run = field(isolate, test, "run");
        if isolate.file_of(run).is_some_and(|defined| defined != file) {
            continue;
        }
        let name = isolate.write(field(isolate, test, "name"));
        let ran = isolate.call(run, &[]).and_then(|_| {
            match isolate.finish_tasks(Some(TASKS)).into_iter().next() {
                Some(error) => Err(error),
                None => Ok(()),
            }
        });
        outcomes.push(Outcome {
            name: name.trim_matches('"').to_owned(),
            error: ran.err(),
        });
    }
    outcomes
}

#[cfg(test)]
mod tests {
    use super::run_registered;
    use crate::Isolate;

    #[test]
    fn the_tests_a_file_defines_run_and_say_how_they_went() {
        let mut isolate = Isolate::new();
        isolate
            .eval_named("other.ox", r#"(test "elsewhere" (expect false))"#)
            .unwrap();
        isolate
            .eval_named(
                "mine.ox",
                r#"(test "adds" (expect (= (+ 1 2) 3)))
                   (test "breaks" (expect (= 1 2)))"#,
            )
            .unwrap();

        let outcomes = run_registered(&mut isolate, "mine.ox");

        let names: Vec<(&str, bool)> = outcomes
            .iter()
            .map(|outcome| (outcome.name.as_str(), outcome.error.is_none()))
            .collect();
        assert_eq!(names, [("adds", true), ("breaks", false)]);
    }
}
