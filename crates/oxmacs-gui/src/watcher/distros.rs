use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use iced::futures::channel::mpsc;
use oxmacs_files::wsl;
use oxmacs_process::{Launch, Process, Started};

use super::Interest;

const EVENTS: &str = "close_write,create,delete,moved_to,moved_from,attrib";
const POLL: Duration = Duration::from_secs(2);
const STEP: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Folder {
    pub(super) share: PathBuf,
    pub(super) linux: String,
}

pub(super) struct Watch {
    stop: Arc<AtomicBool>,
    process: Arc<Mutex<Option<Process>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Watch {
    pub(super) fn start(
        distro: String,
        folders: Vec<Folder>,
        interest: Arc<Interest>,
        sender: mpsc::Sender<()>,
    ) -> Self {
        Self::spawn(distro, folders, interest, sender, true)
    }

    fn spawn(
        distro: String,
        folders: Vec<Folder>,
        interest: Arc<Interest>,
        sender: mpsc::Sender<()>,
        listen: bool,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let process = Arc::new(Mutex::new(None));
        let mut changes = Changes {
            interest,
            sender,
            stop: Arc::clone(&stop),
        };
        let slot = Arc::clone(&process);
        thread::spawn(move || {
            let seen = changes.snapshot(&folders);
            if !(listen && changes.listen(&distro, &folders, &slot)) {
                changes.poll(&folders, seen);
            }
        });
        Self { stop, process }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(process) = lock(&self.process).take() {
            process.kill();
        }
    }
}

type Seen = Vec<(PathBuf, Option<SystemTime>, u64)>;

struct Changes {
    interest: Arc<Interest>,
    sender: mpsc::Sender<()>,
    stop: Arc<AtomicBool>,
}

impl Changes {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn signal(&mut self) {
        let _ = self.sender.try_send(());
    }

    fn listen(&mut self, distro: &str, folders: &[Folder], slot: &Mutex<Option<Process>>) -> bool {
        let Some(first) = folders.first() else {
            return true;
        };
        let mut command = Launch::new("inotifywait")
            .args(["-m", "-q", "-e", EVENTS, "--format", "%w%f"])
            .args(folders.iter().map(|folder| folder.linux.as_str()))
            .directory(&first.share)
            .command();
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let Ok(Started {
            process,
            stdout: Some(stdout),
            ..
        }) = oxmacs_process::spawn(&mut command)
        else {
            return false;
        };
        *lock(slot) = Some(process.clone());
        if self.stopped() {
            process.kill();
            return true;
        }
        oxmacs_process::read_lines(stdout, |line| {
            if self.stopped() {
                return false;
            }
            if self.interest.touches(&path_of(distro, folders, &line)) {
                self.signal();
            }
            true
        });
        self.stopped() || process.wait().is_ok_and(|status| status.success())
    }

    fn poll(&mut self, folders: &[Folder], mut seen: Seen) {
        while !self.stopped() {
            let started = Instant::now();
            while started.elapsed() < POLL {
                if self.stopped() {
                    return;
                }
                thread::sleep(STEP);
            }
            let now = self.snapshot(folders);
            if now != seen {
                seen = now;
                self.signal();
            }
        }
    }

    fn snapshot(&self, folders: &[Folder]) -> Seen {
        let mut seen: Seen = folders
            .iter()
            .filter_map(|folder| fs::read_dir(&folder.share).ok())
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                self.interest.touches(&path).then(|| {
                    let metadata = entry.metadata().ok();
                    let modified = metadata.as_ref().and_then(|found| found.modified().ok());
                    (path, modified, metadata.map_or(0, |found| found.len()))
                })
            })
            .collect();
        seen.sort();
        seen
    }
}

fn path_of(distro: &str, folders: &[Folder], line: &str) -> PathBuf {
    let (parent, name) = line.rsplit_once('/').unwrap_or(("", line));
    let parent = if parent.is_empty() { "/" } else { parent };
    folders
        .iter()
        .find(|folder| folder.linux == parent)
        .map_or_else(
            || wsl::share(distro, line),
            |folder| folder.share.join(name),
        )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::watcher::Watched;

    fn folder(share: &str, linux: &str) -> Folder {
        Folder {
            share: PathBuf::from(share),
            linux: linux.to_owned(),
        }
    }

    #[test]
    fn changed_linux_paths_land_in_the_folder_they_were_watched_as() {
        let folders = [
            folder(r"\\wsl$\fedora\home\ana\x", "/home/ana/x"),
            folder(r"\\wsl.localhost\fedora\", "/"),
        ];

        assert_eq!(
            path_of("fedora", &folders, "/home/ana/x/a.rs"),
            Path::new(r"\\wsl$\fedora\home\ana\x").join("a.rs")
        );
        assert_eq!(
            path_of("fedora", &folders, "/hosts"),
            Path::new(r"\\wsl.localhost\fedora\").join("hosts")
        );
        assert_eq!(
            path_of("fedora", &folders, "/etc/hosts"),
            wsl::share("fedora", "/etc/hosts")
        );
    }

    fn signaled_within(receiver: &mut mpsc::Receiver<()>, limit: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < limit {
            if receiver.try_recv().is_ok() {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn without_a_watcher_inside_changes_to_visited_files_are_found_by_looking() {
        let dir = tempfile::tempdir().unwrap();
        let visited = dir.path().join("a.txt");
        fs::write(&visited, "old").unwrap();
        let interest = Arc::new(Interest::new(&Watched {
            files: vec![visited.clone()],
            directories: Vec::new(),
        }));
        let (sender, mut receiver) = mpsc::channel(1);
        let folders = vec![Folder {
            share: dir.path().to_owned(),
            linux: "/x".to_owned(),
        }];
        let watch = Watch::spawn("fedora".to_owned(), folders, interest, sender, false);
        thread::sleep(STEP * 3);

        fs::write(dir.path().join("other.txt"), "x").unwrap();
        assert!(!signaled_within(&mut receiver, POLL * 2));
        fs::write(&visited, "new text").unwrap();
        assert!(signaled_within(&mut receiver, POLL * 3));

        drop(watch);
        fs::remove_file(&visited).unwrap();
        assert!(!signaled_within(&mut receiver, POLL * 2));
    }
}
