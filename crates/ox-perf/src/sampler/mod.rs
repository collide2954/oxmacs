use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::{benches, tables};

const ROWS: usize = 40;

pub fn profile(
    path: &Path,
    name: &str,
    roots: &[PathBuf],
    length: Duration,
) -> Result<String, String> {
    let mut isolate = oxmacs_api::plain_isolate();
    let found = benches::load(&mut isolate, path, roots)?;
    let benchmark = found
        .iter()
        .find(|benchmark| benchmark.name == name)
        .ok_or_else(|| benches::missing(name, &found))?;
    let function = isolate
        .held(benchmark.thunk)
        .ok_or("A benchmark lost its function")?;
    let samples = native::sampled(|| {
        let started = Instant::now();
        while started.elapsed() < length {
            isolate
                .call(function, &[])
                .map_err(|error| format!("{name}: {error}"))?;
        }
        Ok(())
    })?;
    Ok(tables::report(&samples, ROWS))
}

#[cfg(not(all(windows, target_arch = "x86_64")))]
mod native {
    use crate::tables::Frame;

    pub fn sampled(_work: impl FnOnce() -> Result<(), String>) -> Result<Vec<Vec<Frame>>, String> {
        Err("Sampling needs Windows on x86-64 for now".to_owned())
    }
}

#[cfg(all(windows, target_arch = "x86_64"))]
mod native;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::profile;

    fn suite() -> (tempfile::TempDir, std::path::PathBuf) {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("suite.ox");
        std::fs::write(
            &file,
            "(def (spin i) (if (< i 20000) (spin (+ i 1)) i)) {spin: (fn [] (spin 0))}",
        )
        .unwrap();
        (folder, file)
    }

    #[test]
    #[cfg(all(windows, target_arch = "x86_64"))]
    fn sampling_names_the_vm_functions_that_run() {
        let (_folder, file) = suite();

        let report = profile(&file, "spin", &[], Duration::from_millis(500)).unwrap();

        assert!(report.starts_with("Self time by function ("), "{report}");
        assert!(report.contains("ox_lang::vm::Runtime::"), "{report}");
    }

    #[test]
    #[cfg(all(windows, target_arch = "x86_64"))]
    fn sampling_code_that_allocates_all_the_time_finishes() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("suite.ox");
        std::fs::write(
            &file,
            "(def numbers (vec (range 2000))) {grow: (fn [] (mapcat (fn [x] [x x]) numbers))}",
        )
        .unwrap();

        let report = profile(&file, "grow", &[], Duration::from_secs(2)).unwrap();

        assert!(report.contains("ox_lang::vm::Runtime::"), "{report}");
    }

    #[test]
    fn only_benchmarks_in_the_file_can_be_sampled() {
        let (_folder, file) = suite();

        assert_eq!(
            profile(&file, "nope", &[], Duration::from_millis(10)).unwrap_err(),
            "There is no benchmark named nope; the file has spin"
        );
    }
}
