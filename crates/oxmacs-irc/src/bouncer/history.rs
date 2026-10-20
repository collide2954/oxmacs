use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use jiff::Timestamp;

use crate::protocol::message::Message;

pub trait History {
    fn append(&mut self, network: &str, target: &str, line: &str);
    fn since(
        &self,
        network: &str,
        target: &str,
        after: Option<Timestamp>,
        limit: usize,
    ) -> Vec<String>;
    fn targets(&self, network: &str) -> Vec<String>;
}

fn stamped(line: &str) -> Option<Timestamp> {
    Message::parse(line)?.tag("time")?.parse().ok()
}

fn recent(lines: Vec<String>, after: Option<Timestamp>, limit: usize) -> Vec<String> {
    let mut kept: Vec<String> = lines
        .into_iter()
        .filter(|line| after.is_none_or(|after| stamped(line).is_some_and(|time| time > after)))
        .collect();
    let excess = kept.len().saturating_sub(limit);
    kept.drain(..excess);
    kept
}

#[derive(Debug, Default)]
pub struct MemoryHistory {
    lines: BTreeMap<(String, String), Vec<String>>,
}

impl History for MemoryHistory {
    fn append(&mut self, network: &str, target: &str, line: &str) {
        self.lines
            .entry((network.to_owned(), target.to_ascii_lowercase()))
            .or_default()
            .push(line.to_owned());
    }

    fn since(
        &self,
        network: &str,
        target: &str,
        after: Option<Timestamp>,
        limit: usize,
    ) -> Vec<String> {
        let lines = self
            .lines
            .get(&(network.to_owned(), target.to_ascii_lowercase()))
            .cloned()
            .unwrap_or_default();
        recent(lines, after, limit)
    }

    fn targets(&self, network: &str) -> Vec<String> {
        self.lines
            .keys()
            .filter(|(known, _)| known == network)
            .map(|(_, target)| target.clone())
            .collect()
    }
}

#[derive(Debug)]
pub struct DiskHistory {
    directory: PathBuf,
}

impl DiskHistory {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    fn file(&self, network: &str, target: &str) -> PathBuf {
        let name: String = target
            .to_ascii_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || "#&+!-_.".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.directory.join(network).join(format!("{name}.log"))
    }
}

impl History for DiskHistory {
    fn append(&mut self, network: &str, target: &str, line: &str) {
        let path = self.file(network, target);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{line}");
        }
    }

    fn since(
        &self,
        network: &str,
        target: &str,
        after: Option<Timestamp>,
        limit: usize,
    ) -> Vec<String> {
        let lines = fs::read_to_string(self.file(network, target))
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default();
        recent(lines, after, limit)
    }

    fn targets(&self, network: &str) -> Vec<String> {
        fs::read_dir(self.directory.join(network))
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|entry| {
                        entry
                            .file_name()
                            .to_str()
                            .and_then(|name| name.strip_suffix(".log"))
                            .map(str::to_owned)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(time: &str, text: &str) -> String {
        format!("@time={time} :alice!a@h PRIVMSG #rust :{text}")
    }

    fn exercise(history: &mut impl History) {
        history.append("libera", "#Rust", &line("2026-10-19T10:00:00Z", "one"));
        history.append("libera", "#rust", &line("2026-10-19T11:00:00Z", "two"));
        history.append("libera", "#rust", &line("2026-10-19T12:00:00Z", "three"));
        history.append("libera", "alice", &line("2026-10-19T12:00:00Z", "psst"));

        let after = "2026-10-19T10:30:00Z".parse().ok();
        assert_eq!(
            history.since("libera", "#RUST", after, 10),
            [
                line("2026-10-19T11:00:00Z", "two"),
                line("2026-10-19T12:00:00Z", "three")
            ]
        );
        assert_eq!(
            history.since("libera", "#rust", None, 1),
            [line("2026-10-19T12:00:00Z", "three")]
        );
        let mut targets = history.targets("libera");
        targets.sort();
        assert_eq!(targets, ["#rust", "alice"]);
        assert_eq!(history.targets("other"), Vec::<String>::new());
    }

    #[test]
    fn memory_history_keeps_lines_per_target() {
        exercise(&mut MemoryHistory::default());
    }

    #[test]
    fn disk_history_keeps_lines_in_files_per_target() {
        let dir = tempfile::tempdir().unwrap();
        exercise(&mut DiskHistory::new(dir.path().to_owned()));

        assert!(dir.path().join("libera").join("#rust.log").exists());
    }
}
