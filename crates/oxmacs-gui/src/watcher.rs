use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::{fs, future};

use iced::Subscription;
use iced::futures::Stream;
use iced::futures::channel::mpsc;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

#[derive(Debug, Clone, Default, Hash, PartialEq, Eq)]
pub struct Watched {
    pub files: Vec<PathBuf>,
    pub directories: Vec<PathBuf>,
}

pub fn changes(watched: Watched) -> Subscription<()> {
    if watched.files.is_empty() && watched.directories.is_empty() {
        return Subscription::none();
    }
    Subscription::run_with(watched, watch)
}

fn watch(watched: &Watched) -> impl Stream<Item = ()> + use<> {
    let watched = watched.clone();
    iced::stream::channel(1, async move |sender| {
        let _watcher = start(&watched, sender);
        future::pending::<()>().await;
    })
}

fn start(watched: &Watched, mut sender: mpsc::Sender<()>) -> notify::Result<RecommendedWatcher> {
    let interest = Interest::new(watched);
    let mut notifier = notify::recommended_watcher(move |result: notify::Result<Event>| {
        if result.is_ok_and(|event| interest.concerns(&event)) {
            let _ = sender.try_send(());
        }
    })?;
    for directory in watched_directories(watched) {
        let _ = notifier.watch(directory, RecursiveMode::NonRecursive);
    }
    Ok(notifier)
}

fn watched_directories(watched: &Watched) -> Vec<&Path> {
    let mut directories: Vec<&Path> = watched
        .files
        .iter()
        .filter_map(|file| file.parent())
        .chain(watched.directories.iter().map(PathBuf::as_path))
        .collect();
    directories.sort();
    directories.dedup();
    directories
}

struct Interest {
    names: HashSet<OsString>,
    directories: HashSet<PathBuf>,
}

impl Interest {
    fn new(watched: &Watched) -> Self {
        Self {
            names: watched
                .files
                .iter()
                .filter_map(|file| file.file_name())
                .map(ToOwned::to_owned)
                .collect(),
            directories: watched
                .directories
                .iter()
                .flat_map(|directory| [Some(directory.clone()), fs::canonicalize(directory).ok()])
                .flatten()
                .collect(),
        }
    }

    fn concerns(&self, event: &Event) -> bool {
        !matches!(event.kind, EventKind::Access(_))
            && event.paths.iter().any(|path| {
                path.file_name()
                    .is_some_and(|name| self.names.contains(name))
                    || path
                        .parent()
                        .is_some_and(|parent| self.directories.contains(parent))
            })
    }
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::{Duration, Instant};

    use notify::event::{AccessKind, ModifyKind};

    use super::*;

    fn watched(files: &[&str], directories: &[&str]) -> Watched {
        Watched {
            files: files.iter().map(PathBuf::from).collect(),
            directories: directories.iter().map(PathBuf::from).collect(),
        }
    }

    fn event(kind: EventKind, path: &str) -> Event {
        Event::new(kind).add_path(PathBuf::from(path))
    }

    const MODIFY: EventKind = EventKind::Modify(ModifyKind::Any);

    #[test]
    fn each_directory_is_watched_once() {
        let watched = watched(&["/a/x.rs", "/b/z.rs", "/a/y.rs"], &["/c", "/a"]);

        assert_eq!(
            watched_directories(&watched),
            [Path::new("/a"), Path::new("/b"), Path::new("/c")]
        );
    }

    #[test]
    fn only_changes_to_visited_file_names_matter() {
        let interest = Interest::new(&watched(&["/a/x.rs"], &[]));

        assert!(interest.concerns(&event(MODIFY, "/a/x.rs")));
        assert!(!interest.concerns(&event(MODIFY, "/a/other.rs")));
        assert!(!interest.concerns(&event(EventKind::Access(AccessKind::Any), "/a/x.rs")));
    }

    #[test]
    fn any_change_inside_a_listed_directory_matters() {
        let interest = Interest::new(&watched(&[], &["/d"]));

        assert!(interest.concerns(&event(MODIFY, "/d/new.txt")));
        assert!(!interest.concerns(&event(MODIFY, "/e/new.txt")));
    }

    fn assert_signaled(receiver: &mut mpsc::Receiver<()>) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while receiver.try_recv().is_err() {
            assert!(Instant::now() < deadline, "no change was signaled");
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn writing_a_visited_file_sends_a_signal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        fs::write(&path, "old").unwrap();
        let (sender, mut receiver) = mpsc::channel(1);
        let watched = Watched {
            files: vec![path.clone()],
            directories: Vec::new(),
        };
        let _watcher = start(&watched, sender).unwrap();

        fs::write(&path, "new text").unwrap();

        assert_signaled(&mut receiver);
    }

    #[test]
    fn creating_a_file_in_a_listed_directory_sends_a_signal() {
        let dir = tempfile::tempdir().unwrap();
        let (sender, mut receiver) = mpsc::channel(1);
        let watched = Watched {
            files: Vec::new(),
            directories: vec![dir.path().to_owned()],
        };
        let _watcher = start(&watched, sender).unwrap();

        fs::write(dir.path().join("new.txt"), "").unwrap();

        assert_signaled(&mut receiver);
    }
}
