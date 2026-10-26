use std::fs;
use std::path::{Path, PathBuf};
use std::thread;

use ox_lang::Isolate;

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Value(String),
    Error(String),
}

struct Case {
    line: usize,
    source: String,
    expected: Outcome,
}

fn expectation(text: &str) -> Option<(&str, Outcome)> {
    for (marker, error) in [(";=>", false), (";!", true)] {
        let found = text
            .trim_start()
            .strip_prefix(marker)
            .map(|rest| ("", rest))
            .or_else(|| text.split_once(&format!(" {marker} ")));
        if let Some((code, rest)) = found {
            let rest = rest.trim().to_owned();
            let outcome = if error {
                Outcome::Error(rest)
            } else {
                Outcome::Value(rest)
            };
            return Some((code, outcome));
        }
    }
    None
}

fn cases(text: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut source = String::new();
    let mut start = 0;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(";;") {
            continue;
        }
        let (code, expected) = match expectation(line) {
            Some((code, expected)) => (code, Some(expected)),
            None => (line, None),
        };
        if !code.trim().is_empty() {
            if source.is_empty() {
                start = index + 1;
            }
            source.push_str(code);
            source.push('\n');
        }
        if let Some(expected) = expected {
            cases.push(Case {
                line: start.max(1),
                source: std::mem::take(&mut source),
                expected,
            });
        }
    }
    cases
}

fn failures(path: &Path) -> Vec<String> {
    let text = fs::read_to_string(path).unwrap_or_default();
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    cases(&text)
        .into_iter()
        .filter_map(|case| {
            let mut isolate = Isolate::new();
            let actual = match isolate.eval(&case.source) {
                Ok(value) => Outcome::Value(isolate.write(value)),
                Err(error) => Outcome::Error(error.message),
            };
            (actual != case.expected).then(|| {
                format!(
                    "{name}:{}: {} gave {actual:?}, expected {:?}",
                    case.line,
                    case.source.trim(),
                    case.expected
                )
            })
        })
        .collect()
}

fn corpus() -> Vec<PathBuf> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("ox");
    let mut files: Vec<PathBuf> = fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "ox"))
        .collect();
    files.sort();
    files
}

#[test]
fn the_corpus_reads_cases_and_their_expectations() {
    let found =
        cases(";; a heading\n(+ 1 2) ;=> 3\n\n(def x 1)\nx\n;=> 1\n(/ 1 0) ;! Division by zero\n");

    let summary: Vec<(usize, &str, &Outcome)> = found
        .iter()
        .map(|case| (case.line, case.source.as_str(), &case.expected))
        .collect();
    assert_eq!(
        summary,
        [
            (2, "(+ 1 2)\n", &Outcome::Value("3".to_owned())),
            (4, "(def x 1)\nx\n", &Outcome::Value("1".to_owned())),
            (
                7,
                "(/ 1 0)\n",
                &Outcome::Error("Division by zero".to_owned())
            ),
        ]
    );
}

#[test]
fn every_corpus_case_gives_its_expected_result() {
    let files = corpus();
    assert!(!files.is_empty(), "no corpus files found");
    let failed: Vec<String> = thread::scope(|scope| {
        let runs: Vec<_> = files
            .iter()
            .map(|path| scope.spawn(move || failures(path)))
            .collect();
        runs.into_iter()
            .flat_map(|run| run.join().unwrap_or_default())
            .collect()
    });
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}
