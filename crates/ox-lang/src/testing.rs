use std::fmt::Write as _;
use std::time::Duration;

use crate::Isolate;
use crate::error::{Error, ErrorKind};
use crate::native::field;

pub const TASKS: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub enum Status {
    Passed,
    Failed(Error),
    Skipped,
}

#[derive(Debug)]
pub struct Outcome {
    pub name: String,
    pub status: Status,
}

fn judged(ran: Result<(), Error>) -> Status {
    match ran {
        Ok(()) => Status::Passed,
        Err(error) if error.kind == ErrorKind::Refused => Status::Skipped,
        Err(error) => Status::Failed(error),
    }
}

pub fn run_file(isolate: &mut Isolate, file: &str, source: &str) -> Vec<Outcome> {
    match isolate.eval_named(file, source) {
        Ok(_) => run_registered(isolate, file),
        Err(error) => vec![Outcome {
            name: file.to_owned(),
            status: judged(Err(error)),
        }],
    }
}

pub fn run_registered(isolate: &mut Isolate, file: &str) -> Vec<Outcome> {
    let Ok(tests) = isolate.eval("(vec @registered-tests)") else {
        return Vec::new();
    };
    let mut outcomes = Vec::new();
    for test in isolate.heap().elements(tests).unwrap_or_default() {
        let run = field(isolate.heap(), test, "run").unwrap_or_default();
        if isolate.file_of(run).is_some_and(|defined| defined != file) {
            continue;
        }
        let name = isolate.write(field(isolate.heap(), test, "name").unwrap_or_default());
        let ran = isolate.call(run, &[]).and_then(|_| {
            match isolate.finish_tasks(Some(TASKS)).into_iter().next() {
                Some(error) => Err(error),
                None => Ok(()),
            }
        });
        outcomes.push(Outcome {
            name: name.trim_matches('"').to_owned(),
            status: judged(ran),
        });
    }
    outcomes
}

pub fn report(outcomes: &[Outcome], file: &str, source: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for outcome in outcomes {
        let name = &outcome.name;
        match &outcome.status {
            Status::Passed => lines.push(format!("ok    {name}")),
            Status::Skipped => lines.push(format!("skip  {name}: it needs the editor")),
            Status::Failed(error) => {
                lines.push(format!("FAIL  {name}"));
                if let Some(place) = error.place(Some((file, source))) {
                    lines.push(format!("      {place}"));
                }
                lines.extend(error.message.lines().map(|line| format!("      {line}")));
            }
        }
    }
    lines
}

pub fn failed(outcomes: &[Outcome]) -> usize {
    outcomes
        .iter()
        .filter(|outcome| matches!(outcome.status, Status::Failed(_)))
        .count()
}

pub fn summary(outcomes: &[Outcome]) -> String {
    let skipped = outcomes
        .iter()
        .filter(|outcome| matches!(outcome.status, Status::Skipped))
        .count();
    let ran = outcomes.len() - skipped;
    let mut shown = match (ran, failed(outcomes)) {
        (0, _) => "No tests ran".to_owned(),
        (1, 0) => "1 test passed".to_owned(),
        (_, 0) => format!("{ran} tests passed"),
        (_, failed) => format!("{ran} tests, {failed} failed"),
    };
    if skipped > 0 {
        let _ = write!(shown, ", {skipped} skipped");
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::{Outcome, Status, run_file, run_registered, summary};
    use crate::Isolate;
    use crate::error::{Error, ErrorKind};

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
            .map(|outcome| {
                (
                    outcome.name.as_str(),
                    matches!(outcome.status, Status::Passed),
                )
            })
            .collect();
        assert_eq!(names, [("adds", true), ("breaks", false)]);
    }

    #[test]
    fn a_file_that_does_not_load_is_one_failure() {
        let mut isolate = Isolate::new();

        let outcomes = run_file(&mut isolate, "broken.ox", "(+ 1");

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].name, "broken.ox");
        assert!(matches!(outcomes[0].status, Status::Failed(_)));
    }

    fn outcome(status: Status) -> Outcome {
        Outcome {
            name: "t".to_owned(),
            status,
        }
    }

    #[test]
    fn the_summary_counts_what_ran_and_what_was_skipped() {
        let refused = || Status::Failed(Error::new("no").of(ErrorKind::Refused));
        assert_eq!(summary(&[]), "No tests ran");
        assert_eq!(summary(&[outcome(Status::Passed)]), "1 test passed");
        assert_eq!(
            summary(&[outcome(Status::Passed), outcome(Status::Skipped)]),
            "1 test passed, 1 skipped"
        );
        assert_eq!(
            summary(&[outcome(Status::Passed), outcome(refused())]),
            "2 tests, 1 failed"
        );
        assert_eq!(
            summary(&[outcome(Status::Skipped)]),
            "No tests ran, 1 skipped"
        );
    }
}
