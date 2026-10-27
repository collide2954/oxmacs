use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use crate::command::{Scope, ScopeId};
use crate::display::LineStyle;
use crate::editor::buffers::Mode;
use crate::editor::build::errors::Errors;
use crate::editor::{Editor, JobEvent, JobId, JobSpec, Outcome};
use crate::files::paths;
use crate::special_buffer::SpecialBuffer;
use crate::text::{Buffer, BufferId};
use crate::theme::Highlight;

pub type OnLine = Arc<dyn Fn(&mut Editor, &str) + Send + Sync>;
pub type OnExit = Arc<dyn Fn(&mut Editor, Option<i32>) + Send + Sync>;

#[derive(Clone)]
pub struct RunSpec {
    pub buffer: String,
    pub program: String,
    pub args: Vec<String>,
    pub directory: PathBuf,
    pub env: Vec<(String, String)>,
    pub title: Option<String>,
    pub input: Option<String>,
    pub kind: &'static str,
    pub errors: bool,
    pub noun: Option<&'static str>,
    pub on_line: Option<OnLine>,
    pub on_exit: Option<OnExit>,
}

impl RunSpec {
    pub fn new(buffer: impl Into<String>, program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            buffer: buffer.into(),
            program: program.into(),
            args,
            directory: PathBuf::new(),
            env: Vec::new(),
            title: None,
            input: None,
            kind: "job",
            errors: false,
            noun: None,
            on_line: None,
            on_exit: None,
        }
    }

    fn command(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl fmt::Debug for RunSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RunSpec")
            .field("buffer", &self.buffer)
            .field("program", &self.program)
            .field("args", &self.args)
            .field("directory", &self.directory)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(in crate::editor) struct Run {
    pub(in crate::editor) spec: RunSpec,
    pub(in crate::editor) job: Option<JobId>,
    killed: bool,
    pub(in crate::editor) errors: Errors,
    ended: Option<(usize, Highlight)>,
}

fn summary(noun: Option<&str>, outcome: &Outcome, killed: bool) -> String {
    let rest = match outcome {
        _ if killed => "killed".to_owned(),
        Outcome::Exited(Some(0)) => "finished".to_owned(),
        Outcome::Exited(Some(code)) => format!("exited abnormally with code {code}"),
        Outcome::Exited(None) => "exited abnormally".to_owned(),
        Outcome::Failed(error) => format!("failed to start: {error}"),
    };
    if let Some(noun) = noun {
        return format!("{noun} {rest}");
    }
    let mut letters = rest.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

impl Editor {
    pub(in crate::editor) fn run_index(&self, job: JobId) -> Option<usize> {
        self.buffers.find_special::<Run>(|run| run.job == Some(job))
    }

    fn run_named(&self, name: &str) -> Option<usize> {
        self.buffers
            .find_special::<Run>(|run| run.spec.buffer == name)
    }

    pub fn run_in_buffer(&mut self, spec: RunSpec) -> Option<BufferId> {
        if let Some(running) = self
            .run_named(&spec.buffer)
            .and_then(|index| self.buffers.special::<Run>(index))
            .and_then(|run| run.job)
        {
            self.kill_job(running);
        }
        let shown = paths::abbreviate(&spec.directory, self.home.as_deref());
        let header = format!(
            "Directory: {shown}\n{}\n\n",
            spec.title
                .clone()
                .unwrap_or_else(|| format!("$ {}", spec.command()))
        );
        let name = spec.buffer.clone();
        let run = Run {
            spec: spec.clone(),
            job: None,
            killed: false,
            errors: Errors::default(),
            ended: None,
        };
        match self.run_named(&name) {
            Some(index) => {
                if let Some(buffer) = self.buffers.get_mut(index) {
                    buffer.set_text(&header);
                    buffer.set_point(buffer.len_chars());
                }
                self.buffers.set_mode(index, Mode::special(run));
                let shown = self.buffers.get(index).map(Buffer::id);
                if shown.and_then(|id| self.window_showing(id)).is_none() {
                    self.beside(|editor| {
                        if let Some(index) = editor.run_named(&name) {
                            editor.select_buffer(index);
                        }
                    });
                }
            }
            None => self.beside(|editor| {
                let mut buffer = Buffer::with_text(&name, &header);
                buffer.set_read_only(true);
                buffer.set_point(buffer.len_chars());
                editor.add_buffer_with(buffer, Mode::special(run));
            }),
        }
        let buffer = self
            .run_named(&name)
            .and_then(|index| self.buffers.get(index))
            .map(Buffer::id);
        let label = std::iter::once(spec.program.as_str())
            .chain(spec.args.iter().map(String::as_str))
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        let job = JobSpec {
            program: spec.program,
            args: spec.args,
            directory: spec.directory,
            env: spec.env,
            label: spec.kind,
            name: label,
            buffer,
            interactive: false,
            input: spec.input,
        };
        let id = self.start_job(job, Self::run_event);
        if let Some(run) = self
            .run_named(&name)
            .and_then(|index| self.buffers.special_mut::<Run>(index))
        {
            run.job = Some(id);
        }
        buffer
    }

    fn current_run(&self) -> Option<&Run> {
        self.buffers.special::<Run>(0)
    }

    pub(in crate::editor) fn run_again(&mut self) -> bool {
        let Some(spec) = self.current_run().map(|run| run.spec.clone()) else {
            return false;
        };
        self.run_in_buffer(spec);
        true
    }

    pub(in crate::editor) fn stop_run(&mut self, index: usize) -> bool {
        let Some(run) = self
            .buffers
            .special_mut::<Run>(index)
            .filter(|run| run.job.is_some())
        else {
            return false;
        };
        run.killed = true;
        if let Some(job) = run.job {
            self.kill_job(job);
        }
        true
    }

    fn run_event(&mut self, job: JobId, event: JobEvent<'_>) {
        match event {
            JobEvent::Line(line) | JobEvent::Output(line) => self.run_line(job, line),
            JobEvent::Finished(outcome) => self.run_finished(job, outcome),
        }
    }

    fn run_line(&mut self, job: JobId, line: &str) {
        let Some(index) = self.run_index(job) else {
            return;
        };
        let at = self
            .buffers
            .get(index)
            .map_or(0, |buffer| buffer.line_of(buffer.len_chars()));
        let patterns = self.builds.patterns().to_vec();
        let mut on_line = None;
        if let Some(run) = self.buffers.special_mut::<Run>(index) {
            if run.spec.errors {
                run.errors.scan(at, line, &run.spec.directory, &patterns);
            }
            on_line.clone_from(&run.spec.on_line);
        }
        self.append_output(index, &format!("{line}\n"));
        if let Some(on_line) = on_line {
            on_line(self, line);
        }
    }

    fn run_finished(&mut self, job: JobId, outcome: &Outcome) {
        let Some(index) = self.run_index(job) else {
            return;
        };
        let Some(run) = self.buffers.special::<Run>(index) else {
            return;
        };
        let (noun, killed, on_exit) = (run.spec.noun, run.killed, run.spec.on_exit.clone());
        let summary = summary(noun, outcome, killed);
        let line = self
            .buffers
            .get(index)
            .map_or(0, |buffer| buffer.line_of(buffer.len_chars()) + 1);
        self.append_output(index, &format!("\n{summary}\n"));
        let face = if matches!(outcome, Outcome::Exited(Some(0))) && !killed {
            Highlight::Success
        } else {
            Highlight::Error
        };
        if let Some(run) = self.buffers.special_mut::<Run>(index) {
            run.job = None;
            run.ended = Some((line, face));
        }
        if noun.is_some() {
            self.show_message(summary);
        }
        if let Some(on_exit) = on_exit {
            let code = match outcome {
                Outcome::Exited(code) => *code,
                Outcome::Failed(_) => None,
            };
            on_exit(self, code);
        }
    }

    fn append_output(&mut self, index: usize, text: &str) {
        let Some(buffer) = self.buffers.get_mut(index) else {
            return;
        };
        let end = buffer.len_chars();
        buffer.insert_at(end, text);
        let id = buffer.id();
        self.follow_end(id, end);
    }
}

impl SpecialBuffer for Run {
    fn scope(&self) -> ScopeId {
        Scope::Compilation.into()
    }

    fn line_style(&self, line: usize, text: &str) -> LineStyle {
        match self.ended {
            Some((ended, face)) if ended == line => LineStyle {
                background: None,
                spans: vec![(0..text.chars().count(), face)],
            },
            _ => self.errors.line_style(line, text),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use super::RunSpec;
    use crate::editor::{Editor, Effect, Outcome, ProcessStatus};
    use crate::testing::{lines, press};

    #[test]
    fn runs_stream_into_their_buffer_and_tell_their_hooks() {
        let mut editor = Editor::new();
        let heard = Arc::new(Mutex::new(Vec::new()));
        let (lines_heard, exits_heard) = (Arc::clone(&heard), Arc::clone(&heard));
        let spec = RunSpec {
            directory: PathBuf::from("work"),
            title: Some("Rscript job.R".to_owned()),
            on_line: Some(Arc::new(move |_, line| {
                lines_heard.lock().unwrap().push(line.to_owned());
            })),
            on_exit: Some(Arc::new(move |_, code| {
                exits_heard.lock().unwrap().push(format!("exit {code:?}"));
            })),
            ..RunSpec::new("*R job*", "Rscript", vec!["job.R".to_owned()])
        };

        editor.run_in_buffer(spec);
        let job = match editor.take_effects().pop() {
            Some(Effect::Spawn(job)) => job,
            other => panic!("{other:?}"),
        };
        editor.job_output(job.id, "job says 42");
        editor.job_finished(job.id, &Outcome::Exited(Some(0)));

        press(&mut editor, "C-x o");
        assert_eq!(editor.buffer().name(), "*R job*");
        assert_eq!(
            lines(&editor),
            [
                "Directory: work",
                "Rscript job.R",
                "",
                "job says 42",
                "",
                "Finished"
            ]
        );
        assert_eq!(*heard.lock().unwrap(), ["job says 42", "exit Some(0)"]);
        assert_eq!(editor.message(), None);
    }

    #[test]
    fn g_runs_again_and_c_c_c_k_stops() {
        let mut editor = Editor::new();
        editor.run_in_buffer(RunSpec::new("*job*", "sleep", vec!["60".to_owned()]));
        let first = editor.take_effects();
        press(&mut editor, "C-x o g");
        let second = match editor.take_effects().pop() {
            Some(Effect::Spawn(job)) => job,
            other => panic!("{other:?}"),
        };
        assert!(matches!(&first[0], Effect::Spawn(job) if job.id != second.id));

        press(&mut editor, "C-c C-k");
        assert_eq!(editor.job_status(second.id), Some(ProcessStatus::Stopping));
        editor.job_finished(second.id, &Outcome::Exited(None));
        assert_eq!(lines(&editor).last().unwrap(), "Killed");
    }
}
