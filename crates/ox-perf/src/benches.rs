use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ox::Isolate;
use ox::heap::Hold;
use ox::value::Value;

use crate::counting;

const MINIMUM_ROUNDS: u32 = 5;
const MAXIMUM_ROUNDS: u32 = 100_000;
const BUDGET: Duration = Duration::from_secs(1);
const WRONG: &str =
    "A benchmark file ends with a map of names to functions or [count function] pairs";

#[derive(Debug)]
pub struct Benchmark {
    pub name: String,
    pub count: Option<u64>,
    pub thunk: Hold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measure {
    pub best: Duration,
    pub rounds: u32,
    pub allocations: usize,
}

pub fn load(
    isolate: &mut Isolate,
    path: &Path,
    roots: &[PathBuf],
) -> Result<Vec<Benchmark>, String> {
    let source =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if let Some(folder) = path.parent() {
        isolate.add_root(folder);
    }
    for root in roots {
        isolate.add_root(root);
    }
    let value = isolate
        .eval_named(&path.display().to_string(), &source)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    benchmarks(isolate, value)
}

fn name_of(isolate: &Isolate, key: Value) -> Option<String> {
    match key {
        Value::Keyword(name) | Value::Symbol(name) => Some(name.name().to_owned()),
        _ => isolate.heap().text(key).map(str::to_owned),
    }
}

pub fn benchmarks(isolate: &mut Isolate, value: Value) -> Result<Vec<Benchmark>, String> {
    let entries = isolate.heap().entries(value).ok_or(WRONG)?;
    let mut found = Vec::with_capacity(entries.len());
    for (key, item) in entries {
        let name = name_of(isolate, key).ok_or(WRONG)?;
        let (count, thunk) = match isolate.heap().elements(item).as_deref() {
            Some([Value::Int(count), thunk]) => {
                (Some(u64::try_from(*count).map_err(|_| WRONG)?), *thunk)
            }
            Some(_) => return Err(WRONG.to_owned()),
            None => (None, item),
        };
        found.push(Benchmark {
            name,
            count,
            thunk: isolate.hold(thunk),
        });
    }
    Ok(found)
}

pub fn measure(
    isolate: &mut Isolate,
    benchmark: &Benchmark,
    budget: Duration,
) -> Result<Measure, String> {
    let function = isolate
        .held(benchmark.thunk)
        .ok_or("A benchmark lost its function")?;
    counting::start();
    let started = Instant::now();
    let mut measured = Measure {
        best: Duration::MAX,
        rounds: 0,
        allocations: 0,
    };
    while measured.rounds < MINIMUM_ROUNDS
        || (started.elapsed() < budget && measured.rounds < MAXIMUM_ROUNDS)
    {
        let before = counting::allocations();
        let clock = Instant::now();
        isolate
            .call(function, &[])
            .map_err(|error| format!("{}: {error}", benchmark.name))?;
        measured.best = measured.best.min(clock.elapsed());
        measured.allocations = counting::allocations() - before;
        measured.rounds += 1;
    }
    Ok(measured)
}

pub fn shown(time: Duration) -> String {
    let seconds = time.as_secs_f64();
    if time < Duration::from_micros(1) {
        format!("{} ns", time.as_nanos())
    } else if time < Duration::from_millis(1) {
        format!("{:.2} µs", seconds * 1e6)
    } else if time < Duration::from_secs(1) {
        format!("{:.2} ms", seconds * 1e3)
    } else {
        format!("{seconds:.2} s")
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "allocation and item counts stay far below 2^52"
)]
fn per(allocations: usize, count: u64) -> f64 {
    allocations as f64 / count.max(1) as f64
}

pub fn row(benchmark: &Benchmark, measure: &Measure) -> String {
    let (each, allocations) = match benchmark.count {
        Some(count) => {
            let nanos = measure.best.as_nanos() / u128::from(count.max(1));
            let each = Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX));
            (
                format!("{} /item", shown(each)),
                format!("{:.1} allocs/item", per(measure.allocations, count)),
            )
        }
        None => (String::new(), format!("{} allocs", measure.allocations)),
    };
    format!(
        "{:<20}{:>12}{each:>16}{allocations:>20}  best of {}",
        benchmark.name,
        shown(measure.best),
        measure.rounds
    )
}

fn listed(names: &[&str]) -> String {
    match names {
        [] => "none".to_owned(),
        [only] => (*only).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

pub fn missing(name: &str, found: &[Benchmark]) -> String {
    let known: Vec<&str> = found
        .iter()
        .map(|benchmark| benchmark.name.as_str())
        .collect();
    format!(
        "There is no benchmark named {name}; the file has {}",
        listed(&known)
    )
}

pub fn bench(path: &Path, names: &[String], roots: &[PathBuf]) -> Result<String, String> {
    let mut isolate = ox_library::isolate();
    let found = load(&mut isolate, path, roots)?;
    if let Some(name) = names
        .iter()
        .find(|name| !found.iter().any(|benchmark| benchmark.name == **name))
    {
        return Err(missing(name, &found));
    }
    let mut out = String::new();
    for benchmark in &found {
        if names.is_empty() || names.contains(&benchmark.name) {
            let measured = measure(&mut isolate, benchmark, BUDGET)?;
            let _ = writeln!(out, "{}", row(benchmark, &measured));
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ox::Isolate;

    use super::{Measure, bench, benchmarks, measure, row, shown};

    #[test]
    fn benchmark_files_end_with_names_of_functions_or_counted_functions() {
        let mut isolate = ox_library::isolate();
        let value = isolate
            .eval("{fast: (fn [] 1) counted: [1000 (fn [] (+ 1 2))]}")
            .unwrap();

        let found = benchmarks(&mut isolate, value).unwrap();

        let summary: Vec<(&str, Option<u64>)> = found
            .iter()
            .map(|benchmark| (benchmark.name.as_str(), benchmark.count))
            .collect();
        assert_eq!(summary, [("fast", None), ("counted", Some(1000))]);
        let wrong = isolate.eval("[1 2]").unwrap();
        assert_eq!(
            benchmarks(&mut isolate, wrong).unwrap_err(),
            "A benchmark file ends with a map of names to functions or [count function] pairs"
        );
    }

    #[test]
    fn measuring_runs_a_benchmark_at_least_five_times_and_counts_its_allocations() {
        let mut isolate = ox_library::isolate();
        let value = isolate
            .eval("(def runs (atom 0)) {grow: (fn [] (swap! runs inc) (vec (range 100)))}")
            .unwrap();
        let found = benchmarks(&mut isolate, value).unwrap();

        let measured = measure(&mut isolate, &found[0], Duration::ZERO).unwrap();

        assert_eq!(measured.rounds, 5);
        assert_eq!(
            isolate.eval("@runs").map(|runs| isolate.write(runs)),
            Ok("5".to_owned())
        );
        assert!(measured.allocations > 0);
        assert!(measured.best > Duration::ZERO);
    }

    #[test]
    fn times_read_in_the_unit_that_fits_them() {
        assert_eq!(shown(Duration::from_nanos(950)), "950 ns");
        assert_eq!(shown(Duration::from_nanos(1_500)), "1.50 µs");
        assert_eq!(shown(Duration::from_micros(2_500)), "2.50 ms");
        assert_eq!(shown(Duration::from_millis(3_250)), "3.25 s");
    }

    #[test]
    fn rows_show_the_time_per_item_when_a_benchmark_has_a_count() {
        let mut isolate = ox_library::isolate();
        let value = isolate
            .eval("{loop: [1000 (fn [] 1)] once: (fn [] 2)}")
            .unwrap();
        let found = benchmarks(&mut isolate, value).unwrap();
        let measured = Measure {
            best: Duration::from_micros(500),
            rounds: 12,
            allocations: 3000,
        };

        assert_eq!(
            row(&found[0], &measured),
            "loop                   500.00 µs    500 ns /item     3.0 allocs/item  best of 12"
        );
        assert_eq!(
            row(&found[1], &measured),
            "once                   500.00 µs                         3000 allocs  best of 12"
        );
    }

    #[test]
    fn bench_runs_the_named_benchmarks_of_a_file_with_its_imports() {
        let folder = tempfile::tempdir().unwrap();
        let library = folder.path().join("library");
        std::fs::create_dir_all(library.join("shapes")).unwrap();
        std::fs::write(
            library.join("shapes").join("square.ox"),
            "(module (export area)) (def (area side) (* side side))",
        )
        .unwrap();
        let file = folder.path().join("suite.ox");
        std::fs::write(
            &file,
            "(module (import (shapes square :as square))) {small: (fn [] (square/area 2)) big: (fn [] (square/area 9))}",
        )
        .unwrap();

        let roots = [library];
        let report = bench(&file, &["big".to_owned()], &roots).unwrap();

        assert!(report.starts_with("big "), "{report}");
        assert!(!report.contains("small"), "{report}");
        assert_eq!(
            bench(&file, &["huge".to_owned()], &roots).unwrap_err(),
            "There is no benchmark named huge; the file has small and big"
        );
    }
}
