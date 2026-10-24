use std::path::{Path, PathBuf};

use jiff::Timestamp;

use crate::editor::{Editor, Effect, Job, JobId, Outcome};
use crate::text::BufferId;

const KEPT: usize = 10;
const HISTORY: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    Starting,
    Running,
    Exited(Option<i32>),
    Failed,
}

impl ProcessStatus {
    pub fn finished(self) -> bool {
        matches!(self, Self::Exited(_) | Self::Failed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Job(JobId),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessInfo {
    pub id: u64,
    pub kind: String,
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
    pub directory: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    pub status: ProcessStatus,
    pub pid: Option<u32>,
    pub started: Timestamp,
    pub ended: Option<Timestamp>,
    pub cpu: Vec<f32>,
    pub memory: Vec<u64>,
    pub buffer: Option<BufferId>,
}

#[derive(Debug, Clone)]
struct Tracked {
    source: Source,
    info: ProcessInfo,
}

#[derive(Debug, Default)]
pub(in crate::editor) struct Processes {
    tracked: Vec<Tracked>,
    next_id: u64,
    watching_usage: bool,
    watchers: Vec<fn(&mut Editor)>,
}

fn kept<T: Copy>(history: &mut Vec<T>, value: T) {
    history.push(value);
    if history.len() > HISTORY {
        history.remove(0);
    }
}

impl Editor {
    fn processes_changed(&mut self) {
        for watcher in self.processes.watchers.clone() {
            watcher(self);
        }
    }

    pub fn on_processes_change(&mut self, watcher: fn(&mut Editor)) {
        if !self.processes.watchers.contains(&watcher) {
            self.processes.watchers.push(watcher);
        }
    }

    pub fn processes(&self) -> Vec<ProcessInfo> {
        self.processes
            .tracked
            .iter()
            .map(|entry| entry.info.clone())
            .collect()
    }

    fn prune_processes(&mut self) {
        let tracked = &mut self.processes.tracked;
        let finished = tracked
            .iter()
            .filter(|known| known.info.status.finished())
            .count();
        let mut excess = finished.saturating_sub(KEPT);
        tracked.retain(|known| {
            let drop = excess > 0 && known.info.status.finished();
            excess -= usize::from(drop);
            !drop
        });
    }

    pub(in crate::editor) fn track_job(
        &mut self,
        job: &Job,
        kind: &'static str,
        name: String,
        buffer: Option<BufferId>,
    ) {
        let program = Path::new(&job.program).file_stem().map_or_else(
            || job.program.clone(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let source = Source::Job(job.id);
        let id = self.processes.next_id;
        self.processes.next_id += 1;
        self.processes
            .tracked
            .retain(|known| known.source != source);
        self.processes.tracked.push(Tracked {
            source,
            info: ProcessInfo {
                id,
                kind: kind.to_owned(),
                name,
                program,
                args: job.args.clone(),
                directory: Some(job.directory.clone()),
                env: job.env.clone(),
                status: ProcessStatus::Starting,
                pid: None,
                started: Timestamp::now(),
                ended: None,
                cpu: Vec::new(),
                memory: Vec::new(),
                buffer,
            },
        });
        self.prune_processes();
        self.processes_changed();
    }

    fn update_tracked(&mut self, source: Source, change: impl FnOnce(&mut ProcessInfo)) {
        if let Some(entry) = self
            .processes
            .tracked
            .iter_mut()
            .rev()
            .find(|entry| entry.source == source)
        {
            change(&mut entry.info);
            self.processes_changed();
        }
    }

    pub fn job_started(&mut self, job: JobId, pid: u32) {
        self.update_tracked(Source::Job(job), |info| {
            info.pid = Some(pid);
            info.status = ProcessStatus::Running;
        });
    }

    pub(in crate::editor) fn job_ended(&mut self, job: JobId, outcome: &Outcome) {
        let status = match outcome {
            Outcome::Exited(code) => ProcessStatus::Exited(*code),
            Outcome::Failed(_) => ProcessStatus::Failed,
        };
        let now = Timestamp::now();
        self.update_tracked(Source::Job(job), |info| {
            if !info.status.finished() {
                info.status = status;
                info.ended = Some(now);
            }
        });
        self.prune_processes();
    }

    pub fn process_pids(&self) -> Vec<u32> {
        self.processes
            .tracked
            .iter()
            .filter(|entry| !entry.info.status.finished())
            .filter_map(|entry| entry.info.pid)
            .collect()
    }

    pub fn watch_process_usage(&mut self, watching: bool) {
        self.processes.watching_usage = watching;
    }

    pub fn wants_process_usage(&self) -> bool {
        self.processes.watching_usage
    }

    pub fn processes_tick(&mut self, usage: &[(u32, f32, u64)]) {
        for entry in &mut self.processes.tracked {
            let sample = usage
                .iter()
                .find(|(pid, ..)| Some(*pid) == entry.info.pid)
                .filter(|_| !entry.info.status.finished());
            if let Some((_, cpu, bytes)) = sample {
                kept(&mut entry.info.cpu, *cpu);
                kept(&mut entry.info.memory, *bytes);
            }
        }
        self.processes_changed();
    }

    fn tracked(&self, id: u64) -> Result<&Tracked, String> {
        self.processes
            .tracked
            .iter()
            .find(|entry| entry.info.id == id)
            .ok_or_else(|| format!("No process {id}"))
    }

    pub fn stop_process(&mut self, id: u64, force: bool) -> Result<(), String> {
        let entry = self.tracked(id)?;
        if entry.info.status.finished() {
            return Err(format!("{} is not running", entry.info.name));
        }
        let Source::Job(job) = entry.source;
        self.effects.push(if force {
            Effect::Kill(job)
        } else {
            Effect::Terminate(job)
        });
        Ok(())
    }

    pub fn visit_process(&mut self, id: u64) -> Result<(), String> {
        let entry = self.tracked(id)?;
        let name = entry.info.name.clone();
        let Some(buffer) = entry.info.buffer.filter(|buffer| {
            self.buffers
                .position(|known| known.id() == *buffer)
                .is_some()
        }) else {
            return Err(format!("{name} has no buffer"));
        };
        self.show_beside(move |editor| editor.buffers.position(|known| known.id() == buffer));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::ProcessStatus;
    use crate::editor::{Editor, Effect, Job, JobId, Outcome};

    fn job(id: u64, program: &str, args: &[&str]) -> Job {
        Job {
            id: JobId(id),
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            directory: PathBuf::from("work"),
            env: Vec::new(),
        }
    }

    #[test]
    fn jobs_are_listed_with_their_status_and_usage() {
        let mut editor = Editor::new();
        editor.track_job(
            &job(0, "/usr/bin/Rterm", &["--ess"]),
            "R session",
            "*R:analysis*".to_owned(),
            None,
        );
        editor.track_job(
            &job(1, "cargo", &["check"]),
            "lint",
            "cargo check".to_owned(),
            None,
        );
        editor.job_started(JobId(0), 4242);
        editor.job_started(JobId(1), 4343);
        editor.job_ended(JobId(1), &Outcome::Exited(Some(101)));
        editor.processes_tick(&[(4242, 1.5, 85), (4343, 9.0, 9)]);
        editor.processes_tick(&[(4242, 2.5, 90)]);

        let listed = editor.processes();
        assert_eq!(listed.len(), 2);
        let r = &listed[0];
        assert_eq!(
            (r.kind.as_str(), r.name.as_str(), r.program.as_str()),
            ("R session", "*R:analysis*", "Rterm")
        );
        assert_eq!(r.args, ["--ess"]);
        assert_eq!(r.directory, Some(PathBuf::from("work")));
        assert_eq!((r.status, r.pid), (ProcessStatus::Running, Some(4242)));
        assert_eq!(
            (r.cpu.as_slice(), r.memory.as_slice()),
            (&[1.5, 2.5][..], &[85, 90][..])
        );
        let lint = &listed[1];
        assert_eq!(lint.status, ProcessStatus::Exited(Some(101)));
        assert!(lint.ended.is_some() && lint.cpu.is_empty());
        assert_ne!(r.id, lint.id);
    }

    #[test]
    fn only_the_last_ten_finished_processes_are_kept() {
        let mut editor = Editor::new();
        for id in 0..15 {
            editor.track_job(
                &job(id, "cargo", &["check"]),
                "lint",
                format!("check {id}"),
                None,
            );
            editor.job_ended(JobId(id), &Outcome::Exited(Some(0)));
        }

        let listed = editor.processes();
        assert_eq!(listed.len(), 10);
        assert_eq!(listed[0].name, "check 5");
    }

    #[test]
    fn stopping_asks_politely_and_killing_does_not() {
        let mut editor = Editor::new();
        editor.track_job(
            &job(7, "cargo", &["build"]),
            "compile",
            "cargo build".to_owned(),
            None,
        );
        editor.job_started(JobId(7), 99);
        let id = editor.processes()[0].id;
        editor.take_effects();

        editor.stop_process(id, false).unwrap();
        editor.stop_process(id, true).unwrap();

        assert_eq!(
            editor.take_effects(),
            [Effect::Terminate(JobId(7)), Effect::Kill(JobId(7))]
        );
        editor.job_ended(JobId(7), &Outcome::Exited(None));
        assert_eq!(
            editor.stop_process(id, false),
            Err("cargo build is not running".to_owned())
        );
        assert_eq!(
            editor.stop_process(id + 50, false),
            Err(format!("No process {}", id + 50))
        );
    }

    #[test]
    fn usage_is_wanted_only_while_something_watches_it() {
        let mut editor = Editor::new();
        assert!(!editor.wants_process_usage());

        editor.watch_process_usage(true);
        assert!(editor.wants_process_usage());
        editor.watch_process_usage(false);
        assert!(!editor.wants_process_usage());
    }

    #[derive(Debug, Default)]
    struct Changes(usize);

    #[test]
    fn watchers_hear_about_every_change() {
        let mut editor = Editor::new();
        editor.set_plugin_state(Changes::default());
        let count = |editor: &mut Editor| {
            if let Some(changes) = editor.plugin_state_mut::<Changes>() {
                changes.0 += 1;
            }
        };
        editor.on_processes_change(count);
        editor.on_processes_change(count);

        editor.track_job(
            &job(1, "cargo", &["test"]),
            "compile",
            "cargo test".to_owned(),
            None,
        );
        editor.job_started(JobId(1), 5);
        editor.processes_tick(&[]);

        assert_eq!(
            editor.plugin_state::<Changes>().map(|changes| changes.0),
            Some(3)
        );
    }

    #[test]
    fn visiting_shows_the_buffer_beside_or_says_there_is_none() {
        let mut editor = Editor::new();
        let console = editor.buffer().id();
        editor.track_job(
            &job(2, "R", &[]),
            "R session",
            "*R*".to_owned(),
            Some(console),
        );
        editor.track_job(
            &job(3, "cargo", &["fmt"]),
            "format",
            "cargo fmt".to_owned(),
            None,
        );
        let listed = editor.processes();

        assert_eq!(editor.visit_process(listed[0].id), Ok(()));
        assert_eq!(
            editor.visit_process(listed[1].id),
            Err("cargo fmt has no buffer".to_owned())
        );
    }
}
