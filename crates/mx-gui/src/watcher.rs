use std::collections::HashSet;
use std::ffi::OsString;
use std::future;
use std::path::{Path, PathBuf};

use iced::Subscription;
use iced::futures::Stream;
use iced::futures::channel::mpsc;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

#[derive(Debug, Hash)]
struct Files(Vec<PathBuf>);

pub fn changes(files: Vec<PathBuf>) -> Subscription<()> {
    if files.is_empty() {
        return Subscription::none();
    }
    Subscription::run_with(Files(files), watch)
}

fn watch(files: &Files) -> impl Stream<Item = ()> + use<> {
    let files = files.0.clone();
    iced::stream::channel(1, async move |sender| {
        let _watcher = start(&files, sender);
        future::pending::<()>().await;
    })
}

fn start(files: &[PathBuf], mut sender: mpsc::Sender<()>) -> notify::Result<RecommendedWatcher> {
    let names = file_names(files);
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        if result.is_ok_and(|event| concerns(&event, &names)) {
            let _ = sender.try_send(());
        }
    })?;
    for directory in directories(files) {
        let _ = watcher.watch(directory, RecursiveMode::NonRecursive);
    }
    Ok(watcher)
}

fn file_names(files: &[PathBuf]) -> HashSet<OsString> {
    files
        .iter()
        .filter_map(|file| file.file_name())
        .map(ToOwned::to_owned)
        .collect()
}

fn directories(files: &[PathBuf]) -> Vec<&Path> {
    let mut directories: Vec<&Path> = files.iter().filter_map(|file| file.parent()).collect();
    directories.sort();
    directories.dedup();
    directories
}

fn concerns(event: &Event, names: &HashSet<OsString>) -> bool {
    !matches!(event.kind, EventKind::Access(_))
        && event
            .paths
            .iter()
            .any(|path| path.file_name().is_some_and(|name| names.contains(name)))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::thread;
    use std::time::{Duration, Instant};

    use notify::event::{AccessKind, ModifyKind};

    use super::*;

    #[test]
    fn each_parent_directory_is_watched_once() {
        let files = [
            PathBuf::from("/a/x.rs"),
            PathBuf::from("/b/z.rs"),
            PathBuf::from("/a/y.rs"),
        ];

        assert_eq!(directories(&files), [Path::new("/a"), Path::new("/b")]);
    }

    #[test]
    fn only_changes_to_visited_file_names_matter() {
        let names = file_names(&[PathBuf::from("/a/x.rs")]);
        let event = |kind, path: &str| Event::new(kind).add_path(PathBuf::from(path));
        let modify = EventKind::Modify(ModifyKind::Any);

        assert!(concerns(&event(modify, "/a/x.rs"), &names));
        assert!(!concerns(&event(modify, "/a/other.rs"), &names));
        assert!(!concerns(
            &event(EventKind::Access(AccessKind::Any), "/a/x.rs"),
            &names
        ));
    }

    #[test]
    fn writing_a_visited_file_sends_a_signal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        fs::write(&path, "old").unwrap();
        let (sender, mut receiver) = mpsc::channel(1);
        let _watcher = start(std::slice::from_ref(&path), sender).unwrap();

        fs::write(&path, "new text").unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while receiver.try_recv().is_err() {
            assert!(Instant::now() < deadline, "no change was signalled");
            thread::sleep(Duration::from_millis(20));
        }
    }
}
