use std::backtrace::Backtrace;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jiff::Timestamp;

pub fn report(panic: &str, backtrace: &str) -> String {
    format!(
        "oxmacs {} crashed\n\n{panic}\n\n{backtrace}\n",
        env!("CARGO_PKG_VERSION")
    )
}

pub fn write(folder: &Path, report: &str, at: Timestamp) -> io::Result<PathBuf> {
    fs::create_dir_all(folder)?;
    let path = folder.join(format!("crash-{}.log", at.strftime("%Y%m%d-%H%M%S")));
    fs::write(&path, report)?;
    Ok(path)
}

pub fn install(folder: Option<PathBuf>) {
    let Some(folder) = folder else {
        return;
    };
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = Backtrace::force_capture().to_string();
        let _ = write(
            &folder,
            &report(&info.to_string(), &backtrace),
            Timestamp::now(),
        );
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_logs_name_the_version_the_panic_and_the_backtrace() {
        let folder = tempfile::tempdir().unwrap();
        let at: Timestamp = "2026-10-23T21:05:09Z".parse().unwrap();

        let written = write(
            &folder.path().join("crashes"),
            &report("panicked at src/lib.rs:1:1:\nboom", "0: main"),
            at,
        )
        .unwrap();

        assert_eq!(
            written.file_name().unwrap().to_string_lossy(),
            "crash-20261023-210509.log"
        );
        let text = fs::read_to_string(written).unwrap();
        assert!(text.starts_with(&format!("oxmacs {} crashed", env!("CARGO_PKG_VERSION"))));
        assert!(text.contains("boom") && text.contains("0: main"), "{text}");
    }
}
