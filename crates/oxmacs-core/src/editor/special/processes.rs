use std::path::{Path, PathBuf};

use jiff::Timestamp;

use crate::editor::{Editor, Event, Job, JobId, Outcome, ServerId};
use crate::external::lsp::Launch;
use crate::external::process::Process;
use crate::text::BufferId;

const KEPT: usize = 10;
const HISTORY: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    Starting,
    Running,
    Stopping,
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
    Server(ServerId),
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
    handle: Option<Process>,
    doomed: Option<bool>,
}

#[derive(Debug, Default)]
pub(in crate::editor) struct Processes {
    tracked: Vec<Tracked>,
    next_id: u64,
    watching_usage: bool,
}

fn kept<T: Copy>(history: &mut Vec<T>, value: T) {
    history.push(value);
    if history.len() > HISTORY {
        history.remove(0);
    }
}

impl Editor {
    fn processes_changed(&mut self) {
        self.record(Event::Processes);
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

    fn track(&mut self, source: Source, info: ProcessInfo) {
        self.processes
            .tracked
            .retain(|known| known.source != source);
        self.processes.tracked.push(Tracked {
            source,
            info,
            handle: None,
            doomed: None,
        });
        self.prune_processes();
        self.processes_changed();
    }

    fn untracked_info(
        &mut self,
        kind: &str,
        name: String,
        program: &str,
        args: Vec<String>,
        directory: PathBuf,
    ) -> ProcessInfo {
        let id = self.processes.next_id;
        self.processes.next_id += 1;
        ProcessInfo {
            id,
            kind: kind.to_owned(),
            name,
            program: Path::new(program).file_stem().map_or_else(
                || program.to_owned(),
                |stem| stem.to_string_lossy().into_owned(),
            ),
            args,
            directory: Some(directory),
            env: Vec::new(),
            status: ProcessStatus::Starting,
            pid: None,
            started: Timestamp::now(),
            ended: None,
            cpu: Vec::new(),
            memory: Vec::new(),
            buffer: None,
        }
    }

    pub(in crate::editor) fn track_job(
        &mut self,
        job: &Job,
        kind: &'static str,
        name: String,
        buffer: Option<BufferId>,
    ) {
        let info = ProcessInfo {
            env: job.env.clone(),
            buffer,
            ..self.untracked_info(
                kind,
                name,
                &job.program,
                job.args.clone(),
                job.directory.clone(),
            )
        };
        self.track(Source::Job(job.id), info);
    }

    pub(in crate::editor) fn track_server(&mut self, id: ServerId, name: String, launch: &Launch) {
        let info = self.untracked_info(
            "language server",
            name,
            &launch.program.to_string_lossy(),
            launch.args.clone(),
            launch.directory.clone(),
        );
        self.track(Source::Server(id), info);
    }

    fn tracked_mut(&mut self, source: Source) -> Option<&mut Tracked> {
        self.processes
            .tracked
            .iter_mut()
            .rev()
            .find(|entry| entry.source == source)
    }

    fn started(&mut self, source: Source, pid: u32, handle: Option<Process>) {
        let Some(entry) = self.tracked_mut(source) else {
            return;
        };
        entry.info.pid = Some(pid);
        if entry.info.status == ProcessStatus::Starting {
            entry.info.status = ProcessStatus::Running;
        }
        entry.handle = handle;
        if let Some(force) = entry.doomed.take() {
            Self::end(entry, force);
        }
        self.processes_changed();
    }

    pub fn job_started(&mut self, job: JobId, process: Process) {
        self.started(Source::Job(job), process.pid(), Some(process));
    }

    pub(crate) fn server_started(&mut self, id: ServerId, process: Process) {
        self.started(Source::Server(id), process.pid(), Some(process));
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn job_started_as(&mut self, job: JobId, pid: u32) {
        self.started(Source::Job(job), pid, None);
    }

    fn ended(&mut self, source: Source, status: ProcessStatus) {
        let now = Timestamp::now();
        if let Some(entry) = self.tracked_mut(source) {
            entry.handle = None;
            entry.doomed = None;
            if !entry.info.status.finished() {
                entry.info.status = status;
                entry.info.ended = Some(now);
            }
            self.processes_changed();
        }
        self.prune_processes();
    }

    pub(in crate::editor) fn job_ended(&mut self, job: JobId, outcome: &Outcome) {
        let status = match outcome {
            Outcome::Exited(code) => ProcessStatus::Exited(*code),
            Outcome::Failed(_) => ProcessStatus::Failed,
        };
        self.ended(Source::Job(job), status);
    }

    pub(in crate::editor) fn server_ended(&mut self, id: ServerId, failure: Option<&str>) {
        let status = if failure.is_some() {
            ProcessStatus::Failed
        } else {
            ProcessStatus::Exited(None)
        };
        self.ended(Source::Server(id), status);
    }

    fn end(entry: &mut Tracked, force: bool) {
        match &entry.handle {
            Some(handle) if force => handle.kill(),
            Some(handle) => handle.terminate(),
            None => entry.doomed = Some(force || entry.doomed == Some(true)),
        }
    }

    fn stop(&mut self, source: Source, force: bool) {
        if let Some(entry) = self.tracked_mut(source)
            && !entry.info.status.finished()
        {
            entry.info.status = ProcessStatus::Stopping;
            Self::end(entry, force);
            self.processes_changed();
        }
    }

    pub(in crate::editor) fn kill_job(&mut self, job: JobId) {
        self.stop(Source::Job(job), true);
    }

    pub(in crate::editor) fn kill_server(&mut self, id: ServerId) {
        self.stop(Source::Server(id), true);
    }

    pub(crate) fn job_process(&self, job: JobId) -> Option<&Process> {
        self.handle_of(Source::Job(job))
    }

    pub(crate) fn server_process(&self, id: ServerId) -> Option<&Process> {
        self.handle_of(Source::Server(id))
    }

    fn handle_of(&self, source: Source) -> Option<&Process> {
        self.processes
            .tracked
            .iter()
            .rev()
            .find(|entry| entry.source == source)?
            .handle
            .as_ref()
    }

    pub fn kill_every_process(&mut self) {
        for entry in &mut self.processes.tracked {
            if let Some(handle) = entry.handle.take() {
                handle.kill();
            }
        }
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn job_status(&self, job: JobId) -> Option<ProcessStatus> {
        self.status_of(Source::Job(job))
    }

    #[cfg(test)]
    pub(crate) fn server_status(&self, id: ServerId) -> Option<ProcessStatus> {
        self.status_of(Source::Server(id))
    }

    #[cfg(any(test, feature = "testing"))]
    fn status_of(&self, source: Source) -> Option<ProcessStatus> {
        self.processes
            .tracked
            .iter()
            .rev()
            .find(|entry| entry.source == source)
            .map(|entry| entry.info.status)
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
        let source = entry.source;
        self.stop(source, force);
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

    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::ProcessStatus;
    use crate::editor::{Editor, Event, EventKind, Job, JobId, Outcome};
    use crate::external::process::Process;

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
        editor.job_started_as(JobId(0), 4242);
        editor.job_started_as(JobId(1), 4343);
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

    fn long() -> Process {
        let mut command = Command::new(if cfg!(windows) { "ping" } else { "sleep" });
        if cfg!(windows) {
            command.args(["-n", "60", "127.0.0.1"]);
        } else {
            command.arg("60");
        }
        command.stdout(Stdio::null());
        oxmacs_process::spawn(&mut command).unwrap().process
    }

    fn ends(process: &Process) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(30) {
            if matches!(process.try_wait(), Ok(Some(_))) {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn stopping_ends_the_process_even_when_asked_before_it_started() {
        let mut editor = Editor::new();
        for (number, name) in [(7, "cargo build"), (8, "cargo test")] {
            editor.track_job(
                &job(number, "cargo", &["build"]),
                "compile",
                name.to_owned(),
                None,
            );
        }
        let ids: Vec<u64> = editor.processes().iter().map(|info| info.id).collect();
        let (built, tested) = (long(), long());

        editor.stop_process(ids[1], true).unwrap();
        editor.job_started(JobId(7), built.clone());
        editor.stop_process(ids[0], false).unwrap();
        editor.job_started(JobId(8), tested.clone());

        assert_eq!(editor.job_status(JobId(7)), Some(ProcessStatus::Stopping));
        assert!(ends(&built) && ends(&tested));
        assert_eq!(editor.take_effects(), []);
        editor.job_ended(JobId(7), &Outcome::Exited(None));
        assert_eq!(
            editor.stop_process(ids[0], false),
            Err("cargo build is not running".to_owned())
        );
        assert_eq!(
            editor.stop_process(ids[0] + 50, false),
            Err(format!("No process {}", ids[0] + 50))
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

    #[test]
    fn process_changes_come_back_once_per_drain() {
        let mut editor = Editor::new();
        editor.subscribe(EventKind::Processes);

        editor.track_job(
            &job(1, "cargo", &["test"]),
            "compile",
            "cargo test".to_owned(),
            None,
        );
        editor.job_started_as(JobId(1), 5);
        assert_eq!(editor.take_events(), [Event::Processes]);
        editor.processes_tick(&[]);

        assert_eq!(editor.take_events(), [Event::Processes]);
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
