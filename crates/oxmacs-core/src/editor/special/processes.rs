use std::path::Path;

use jiff::{SignedDuration, Timestamp};

use crate::display::LineStyle;
use crate::editor::buffers::Mode;
use crate::editor::page::{Page, clip};
use crate::editor::{ConnectionId, ConnectionSpec, Editor, Effect, Job, JobId, Outcome};
use crate::text::{Buffer, BufferId};
use crate::theme::Highlight;

const NAME: &str = "*Processes*";
const HELP: &str = "RET visit  k stop  g refresh  q quit";
const KEPT: usize = 10;
const MEBIBYTE: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessKey(u64);

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub kind: &'static str,
    pub name: String,
    pub detail: String,
    pub pid: Option<u32>,
    pub stop: Option<fn(&mut Editor)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::editor) enum Source {
    Job(JobId),
    Connection(ConnectionId),
    Plugin(ProcessKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Starting,
    Running,
    Exited(Option<i32>),
    Failed,
    Connected,
    Closed,
}

impl Status {
    fn finished(self) -> bool {
        matches!(self, Self::Exited(_) | Self::Failed | Self::Closed)
    }

    fn shown(self) -> (String, Highlight) {
        match self {
            Self::Starting => ("starting".to_owned(), Highlight::Warning),
            Self::Running => ("running".to_owned(), Highlight::Success),
            Self::Exited(Some(0) | None) => ("exited".to_owned(), Highlight::Comment),
            Self::Exited(Some(code)) => (format!("exit {code}"), Highlight::Error),
            Self::Failed => ("failed".to_owned(), Highlight::Error),
            Self::Connected => ("connected".to_owned(), Highlight::Success),
            Self::Closed => ("closed".to_owned(), Highlight::Comment),
        }
    }
}

#[derive(Debug, Clone)]
struct Tracked {
    source: Source,
    kind: &'static str,
    name: String,
    detail: String,
    started: Timestamp,
    ended: Option<Timestamp>,
    pid: Option<u32>,
    status: Status,
    buffer: Option<BufferId>,
    cpu: Option<f32>,
    memory: Option<u64>,
    stop: Option<fn(&mut Editor)>,
}

#[derive(Debug, Default)]
pub(in crate::editor) struct Processes {
    tracked: Vec<Tracked>,
    now: Option<Timestamp>,
    next_key: u64,
}

#[derive(Debug, Default)]
pub(in crate::editor) struct ProcessList {
    rows: Vec<(usize, Source)>,
    styles: Vec<LineStyle>,
}

impl ProcessList {
    pub(in crate::editor) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }

    fn row(&self, line: usize) -> Option<Source> {
        self.rows
            .iter()
            .find(|(row, _)| *row == line)
            .map(|(_, source)| *source)
    }
}

fn runtime(elapsed: SignedDuration) -> String {
    let seconds = elapsed.as_secs().max(0);
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds:02}s")
    } else {
        format!("{seconds}s")
    }
}

fn memory(bytes: u64) -> String {
    format!("{} MB", bytes.div_ceil(MEBIBYTE))
}

const COLUMNS: [&str; 8] = [
    "Name", "Kind", "Status", "PID", "Time", "CPU", "Memory", "Details",
];
const DROPPED: [usize; 6] = [7, 1, 4, 3, 6, 5];
const DETAILS: usize = 7;
const GAP: usize = 2;
const MIN_DETAILS: usize = 12;
const ROOM: usize = 120;

fn cells(entry: &Tracked, now: Timestamp) -> [String; 8] {
    let live = !entry.status.finished();
    let usage = |value: Option<String>| value.filter(|_| live).unwrap_or_else(|| "-".to_owned());
    [
        entry.name.clone(),
        entry.kind.to_owned(),
        entry.status.shown().0,
        entry
            .pid
            .map_or_else(|| "-".to_owned(), |pid| pid.to_string()),
        runtime(entry.ended.unwrap_or(now).duration_since(entry.started)),
        usage(entry.cpu.map(|cpu| format!("{cpu:.1}%"))),
        usage(entry.memory.map(memory)),
        entry.detail.clone(),
    ]
}

fn fit(widths: &[usize; 8], room: usize) -> (Vec<usize>, usize) {
    let total = |shown: &[usize], details: usize| {
        let cells: usize = shown
            .iter()
            .map(|column| {
                if *column == DETAILS {
                    details
                } else {
                    widths[*column]
                }
            })
            .sum();
        cells + GAP * shown.len().saturating_sub(1)
    };
    let mut shown: Vec<usize> = (0..COLUMNS.len()).collect();
    let mut details = widths[DETAILS];
    for dropped in DROPPED {
        if total(&shown, details) <= room {
            break;
        }
        shown.retain(|column| *column != dropped);
        if dropped == DETAILS {
            let spare = room.saturating_sub(total(&shown, 0) + GAP);
            if spare >= MIN_DETAILS {
                details = spare;
                shown.push(DETAILS);
                break;
            }
        }
    }
    (shown, details)
}

fn page(tracked: &[Tracked], now: Timestamp, room: usize) -> (Page, Vec<(usize, Source)>) {
    let mut page = Page::default();
    page.heading("Processes");
    page.line(&[(HELP, Some(Highlight::Comment))]);
    page.line(&[]);
    let mut rows = Vec::new();
    if tracked.is_empty() {
        page.line(&[("Nothing running", Some(Highlight::Comment))]);
        return (page, rows);
    }
    let header = COLUMNS.map(str::to_owned);
    let table: Vec<[String; 8]> = tracked.iter().map(|entry| cells(entry, now)).collect();
    let widths: [usize; 8] = std::array::from_fn(|column| {
        table
            .iter()
            .chain(std::iter::once(&header))
            .map(|row| row[column].chars().count())
            .max()
            .unwrap_or(0)
    });
    let (shown, details) = fit(&widths, room);
    let layout = |row: &[String; 8]| -> Vec<(usize, String)> {
        shown
            .iter()
            .enumerate()
            .map(|(place, column)| {
                let gap = if place + 1 < shown.len() { "  " } else { "" };
                let cell = &row[*column];
                let text = match *column {
                    DETAILS => clip(cell, details),
                    3..=6 => format!("{cell:>width$}", width = widths[*column]),
                    _ => format!("{cell:<width$}", width = widths[*column]),
                };
                (*column, format!("{text}{gap}"))
            })
            .collect()
    };
    let titles: String = layout(&header).into_iter().map(|(_, text)| text).collect();
    page.line(&[(titles.trim_end(), Some(Highlight::Comment))]);
    for (entry, row) in tracked.iter().zip(&table) {
        let (_, status) = entry.status.shown();
        let quiet = entry.status.finished().then_some(Highlight::Comment);
        let parts: Vec<(usize, String)> = layout(row);
        let styled: Vec<(&str, Option<Highlight>)> = parts
            .iter()
            .map(|(column, text)| {
                let style = match *column {
                    0 => quiet.or(Some(Highlight::Link)),
                    2 => Some(status),
                    DETAILS => Some(Highlight::Comment),
                    _ => quiet,
                };
                (text.as_str(), style)
            })
            .collect();
        rows.push((page.styles.len(), entry.source));
        page.line(&styled);
    }
    (page, rows)
}
impl Editor {
    fn track(&mut self, entry: Tracked) {
        let tracked = &mut self.processes.tracked;
        tracked.retain(|known| known.source != entry.source);
        tracked.push(entry);
        self.prune_processes();
        self.render_processes();
    }

    fn prune_processes(&mut self) {
        let tracked = &mut self.processes.tracked;
        let finished = tracked
            .iter()
            .filter(|known| known.status.finished())
            .count();
        let mut excess = finished.saturating_sub(KEPT);
        tracked.retain(|known| {
            let drop = excess > 0 && known.status.finished();
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
        let detail = std::iter::once(program.as_str())
            .chain(job.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ");
        self.track(Tracked {
            source: Source::Job(job.id),
            kind,
            name,
            detail,
            started: Timestamp::now(),
            ended: None,
            pid: None,
            status: Status::Starting,
            buffer,
            cpu: None,
            memory: None,
            stop: None,
        });
    }

    pub(in crate::editor) fn track_connection(&mut self, id: ConnectionId, spec: &ConnectionSpec) {
        self.track(Tracked {
            source: Source::Connection(id),
            kind: spec.kind,
            name: spec.name.clone(),
            detail: spec.detail.clone(),
            started: Timestamp::now(),
            ended: None,
            pid: None,
            status: Status::Starting,
            buffer: None,
            cpu: None,
            memory: None,
            stop: None,
        });
    }

    pub fn track_process(&mut self, spec: ProcessSpec) -> ProcessKey {
        let running = self
            .processes
            .tracked
            .iter()
            .rev()
            .find_map(|entry| match entry.source {
                Source::Plugin(key)
                    if entry.kind == spec.kind
                        && entry.name == spec.name
                        && !entry.status.finished() =>
                {
                    Some(key)
                }
                _ => None,
            });
        if let Some(key) = running {
            self.update_tracked(Source::Plugin(key), |entry| {
                entry.pid = spec.pid.or(entry.pid);
            });
            return key;
        }
        let key = ProcessKey(self.processes.next_key);
        self.processes.next_key += 1;
        self.track(Tracked {
            source: Source::Plugin(key),
            kind: spec.kind,
            name: spec.name,
            detail: spec.detail,
            started: Timestamp::now(),
            ended: None,
            pid: spec.pid,
            status: Status::Running,
            buffer: None,
            cpu: None,
            memory: None,
            stop: spec.stop,
        });
        key
    }

    pub fn process_ended(&mut self, key: ProcessKey) {
        self.end_tracked(Source::Plugin(key), Status::Exited(Some(0)));
    }

    fn update_tracked(&mut self, source: Source, change: impl FnOnce(&mut Tracked)) {
        if let Some(entry) = self
            .processes
            .tracked
            .iter_mut()
            .rev()
            .find(|entry| entry.source == source)
        {
            change(entry);
            self.render_processes();
        }
    }

    pub fn job_started(&mut self, job: JobId, pid: u32) {
        self.update_tracked(Source::Job(job), |entry| {
            entry.pid = Some(pid);
            entry.status = Status::Running;
        });
    }

    pub(in crate::editor) fn job_ended(&mut self, job: JobId, outcome: &Outcome) {
        let status = match outcome {
            Outcome::Exited(code) => Status::Exited(*code),
            Outcome::Failed(_) => Status::Failed,
        };
        self.end_tracked(Source::Job(job), status);
    }

    pub(in crate::editor) fn connection_changed(&mut self, id: ConnectionId, open: bool) {
        if open {
            self.update_tracked(Source::Connection(id), |entry| {
                entry.status = Status::Connected;
            });
        } else {
            self.end_tracked(Source::Connection(id), Status::Closed);
        }
    }

    fn end_tracked(&mut self, source: Source, status: Status) {
        let now = Timestamp::now();
        self.update_tracked(source, |entry| {
            if !entry.status.finished() {
                entry.status = status;
                entry.ended = Some(now);
            }
        });
        self.prune_processes();
        self.render_processes();
    }

    pub fn process_pids(&self) -> Vec<u32> {
        self.processes
            .tracked
            .iter()
            .filter(|entry| !entry.status.finished())
            .filter_map(|entry| entry.pid)
            .collect()
    }

    pub fn wants_process_usage(&self) -> bool {
        self.processes_index()
            .and_then(|index| self.buffers.get(index))
            .is_some_and(|buffer| self.window_showing(buffer.id()).is_some())
    }

    pub fn processes_tick(&mut self, now: Timestamp, usage: &[(u32, f32, u64)]) {
        self.processes.now = Some(now);
        for entry in &mut self.processes.tracked {
            let sample = usage
                .iter()
                .find(|(pid, ..)| Some(*pid) == entry.pid)
                .filter(|_| !entry.status.finished());
            if let Some((_, cpu, bytes)) = sample {
                entry.cpu = Some(*cpu);
                entry.memory = Some(*bytes);
            }
        }
        self.render_processes();
    }

    fn processes_index(&self) -> Option<usize> {
        self.buffers
            .find_mode(|mode| matches!(mode, Mode::Processes(_)))
    }

    pub(in crate::editor) fn list_processes(&mut self) {
        if self.processes_index().is_none() {
            self.beside(|editor| {
                let mut buffer = Buffer::with_text(NAME, "");
                buffer.set_read_only(true);
                editor.add_buffer_with(buffer, Mode::Processes(ProcessList::default()));
            });
        }
        self.render_processes();
        self.show_beside(Self::processes_index);
    }

    pub(in crate::editor) fn render_processes(&mut self) {
        let Some(index) = self.processes_index() else {
            return;
        };
        let now = self.processes.now.unwrap_or_else(Timestamp::now);
        let room = self
            .buffers
            .get(index)
            .and_then(|buffer| self.width_showing(buffer.id()))
            .filter(|width| *width > 0)
            .unwrap_or(ROOM);
        let (page, rows) = page(&self.processes.tracked, now, room);
        if let Some(buffer) = self.buffers.get_mut(index) {
            page.fill(buffer);
        }
        if let Some(Mode::Processes(list)) = self.buffers.mode_mut(index) {
            list.rows = rows;
            list.styles = page.styles;
        }
    }

    fn process_at_point(&mut self) -> Option<Tracked> {
        let line = self.point_line();
        let Some(Mode::Processes(list)) = self.buffers.mode(0) else {
            return None;
        };
        let source = list.row(line);
        let found = source.and_then(|source| {
            self.processes
                .tracked
                .iter()
                .rev()
                .find(|entry| entry.source == source)
                .cloned()
        });
        if found.is_none() {
            self.show_message("No process on this line");
        }
        found
    }

    pub(in crate::editor) fn visit_process(&mut self) {
        let Some(entry) = self.process_at_point() else {
            return;
        };
        if let Source::Connection(id) = entry.source {
            self.visit_connection(id);
            return;
        }
        match (entry.kind, entry.buffer) {
            ("lint", _) => self.list_diagnostics(),
            (_, Some(buffer))
                if self
                    .buffers
                    .position(|known| known.id() == buffer)
                    .is_some() =>
            {
                self.show_beside(move |editor| {
                    editor.buffers.position(|known| known.id() == buffer)
                });
            }
            _ => self.show_message(format!("{} has no buffer", entry.name)),
        }
    }

    pub(in crate::editor) fn stop_process(&mut self) {
        let Some(entry) = self.process_at_point() else {
            return;
        };
        if entry.status.finished() {
            self.show_message(format!("{} is not running", entry.name));
            return;
        }
        self.ask_to_stop(&entry.name, entry.source);
    }

    pub(in crate::editor) fn finish_stop(&mut self, source: Source) {
        match source {
            Source::Job(job) => self.effects.push(Effect::Kill(job)),
            Source::Connection(id) => self.stop_connection(id),
            Source::Plugin(key) => {
                let stop = self
                    .processes
                    .tracked
                    .iter()
                    .find(|entry| entry.source == source)
                    .and_then(|entry| entry.stop);
                if let Some(stop) = stop {
                    stop(self);
                }
                self.process_ended(key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use jiff::{SignedDuration, Timestamp};

    use super::ProcessSpec;
    use crate::editor::test_support::{lines, press, question, type_text};
    use crate::editor::{Editor, Effect, Job, JobId, Outcome};

    fn job(id: u64, program: &str, args: &[&str]) -> Job {
        Job {
            id: JobId(id),
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            directory: PathBuf::from("."),
            env: Vec::new(),
        }
    }

    fn listed(editor: &mut Editor) {
        press(editor, "M-x");
        type_text(editor, "list-processes");
        press(editor, "RET");
    }

    fn later(editor: &mut Editor, seconds: i64, usage: &[(u32, f32, u64)]) {
        let now = Timestamp::now() + SignedDuration::from_secs(seconds);
        editor.processes_tick(now, usage);
    }

    #[test]
    fn running_and_finished_processes_are_listed_with_their_usage() {
        let mut editor = Editor::new();
        editor.track_job(
            &job(0, "Rterm", &["--ess"]),
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

        listed(&mut editor);
        later(&mut editor, 125, &[(4242, 1.5, 85 * 1024 * 1024)]);

        assert_eq!(editor.buffer().name(), "*Processes*");
        let shown = lines(&editor);
        assert!(shown[3].starts_with("Name"), "{shown:?}");
        assert!(shown[3].ends_with("Details"), "{shown:?}");
        let r = &shown[4];
        assert!(r.starts_with("*R:analysis*"), "{r}");
        for part in [
            "R session",
            "running",
            "4242",
            "2m 05s",
            "1.5%",
            "85 MB",
            "Rterm --ess",
        ] {
            assert!(r.contains(part), "{part} in {r}");
        }
        let lint = &shown[5];
        for part in ["lint", "exit 101", "4343", "cargo check"] {
            assert!(lint.contains(part), "{part} in {lint}");
        }
    }

    #[test]
    fn narrow_windows_keep_the_most_useful_columns() {
        let mut editor = Editor::new();
        editor.track_job(
            &job(0, "Rterm", &["--ess"]),
            "R session",
            "*R:analysis*".to_owned(),
            None,
        );
        editor.job_started(JobId(0), 4242);
        listed(&mut editor);

        editor.resize_pane(editor.selected_window(), 20, 41);

        let shown = lines(&editor);
        assert!(
            shown.iter().all(|line| line.chars().count() <= 40),
            "{shown:?}"
        );
        assert!(shown[3].starts_with("Name"), "{shown:?}");
        assert!(
            shown[3].contains("Status") && shown[3].contains("CPU"),
            "{shown:?}"
        );
        assert!(
            !shown[3].contains("Details") && !shown[3].contains("Kind"),
            "{shown:?}"
        );
    }

    #[test]
    fn nothing_running_says_so() {
        let mut editor = Editor::new();

        listed(&mut editor);

        assert_eq!(lines(&editor)[3], "Nothing running");
    }

    #[test]
    fn k_asks_then_stops_the_process_on_its_line() {
        let mut editor = Editor::new();
        editor.track_job(
            &job(7, "cargo", &["build"]),
            "compile",
            "cargo build".to_owned(),
            None,
        );
        editor.job_started(JobId(7), 99);
        listed(&mut editor);
        editor.take_effects();

        press(&mut editor, "M-< C-n C-n C-n C-n k");
        assert_eq!(
            question(&editor),
            Some("Stop cargo build? (y or n) ".to_owned())
        );
        press(&mut editor, "y");

        assert_eq!(editor.take_effects(), [Effect::Kill(JobId(7))]);
    }

    #[test]
    fn finished_processes_cannot_be_stopped_and_old_ones_drop_off() {
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
        listed(&mut editor);

        let shown = lines(&editor);
        assert_eq!(shown.len(), 4 + 10);
        assert!(shown[4].starts_with("check 5"), "{shown:?}");
        press(&mut editor, "M-< C-n C-n C-n C-n k");
        assert_eq!(editor.message(), Some("check 5 is not running"));
    }

    #[test]
    fn the_list_follows_changes_while_it_is_open() {
        let mut editor = Editor::new();
        listed(&mut editor);

        editor.track_job(
            &job(3, "cargo", &["test"]),
            "compile",
            "cargo test".to_owned(),
            None,
        );

        assert!(lines(&editor)[4].starts_with("cargo test"));
        assert!(lines(&editor)[4].contains("starting"));
    }

    #[derive(Debug, Default)]
    struct Stopped(usize);

    fn daemon(pid: Option<u32>) -> ProcessSpec {
        ProcessSpec {
            kind: "helper",
            name: "sync-daemon".to_owned(),
            detail: "oxmacs sync".to_owned(),
            pid,
            stop: Some(|editor| {
                if let Some(stopped) = editor.plugin_state_mut::<Stopped>() {
                    stopped.0 += 1;
                }
            }),
        }
    }

    #[test]
    fn plugins_track_their_own_processes_once_and_can_stop_them() {
        let mut editor = Editor::new();
        editor.set_plugin_state(Stopped::default());

        let first = editor.track_process(daemon(None));
        let again = editor.track_process(daemon(Some(321)));
        listed(&mut editor);

        assert_eq!(first, again);
        let shown = lines(&editor);
        assert_eq!(shown.len(), 5, "{shown:?}");
        for part in ["sync-daemon", "helper", "running", "321", "oxmacs sync"] {
            assert!(shown[4].contains(part), "{part} in {}", shown[4]);
        }

        press(&mut editor, "M-< C-n C-n C-n C-n k y");

        assert_eq!(
            editor.plugin_state::<Stopped>().map(|stopped| stopped.0),
            Some(1)
        );
        assert!(lines(&editor)[4].contains("exited"), "{:?}", lines(&editor));
        let later = editor.track_process(daemon(Some(400)));
        assert_ne!(later, first);
        editor.process_ended(later);
        assert!(!lines(&editor).iter().any(|line| line.contains("running")));
    }

    #[test]
    fn usage_is_only_wanted_while_the_list_is_shown() {
        let mut editor = Editor::new();
        assert!(!editor.wants_process_usage());

        listed(&mut editor);

        assert!(editor.wants_process_usage());
    }
}
