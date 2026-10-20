use std::fmt;
use std::path::PathBuf;

use crate::editor::special::processes::Kind;
use crate::editor::{Editor, Effect};
use crate::text::BufferId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: JobId,
    pub program: String,
    pub args: Vec<String>,
    pub directory: PathBuf,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Exited(Option<i32>),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobSpec {
    pub program: String,
    pub args: Vec<String>,
    pub directory: PathBuf,
    pub env: Vec<(String, String)>,
    pub label: &'static str,
    pub name: String,
    pub buffer: Option<BufferId>,
    pub interactive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobEvent<'a> {
    Line(&'a str),
    Output(&'a str),
    Finished(&'a Outcome),
}

type Handler = Box<dyn FnMut(&mut Editor, JobId, JobEvent<'_>) + Send>;

#[derive(Default)]
pub(super) struct JobHandlers(Vec<(JobId, Handler)>);

impl fmt::Debug for JobHandlers {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JobHandlers")
            .field("running", &self.0.len())
            .finish()
    }
}

impl Editor {
    pub fn start_job(
        &mut self,
        spec: JobSpec,
        handler: impl FnMut(&mut Editor, JobId, JobEvent<'_>) + Send + 'static,
    ) -> JobId {
        let id = JobId(self.next_job);
        self.next_job += 1;
        let job = Job {
            id,
            program: spec.program,
            args: spec.args,
            directory: spec.directory,
            env: spec.env,
        };
        self.track_job(&job, Kind::Named(spec.label), spec.name, spec.buffer);
        self.job_handlers.0.push((id, Box::new(handler)));
        self.effects.push(if spec.interactive {
            Effect::StartSession(job)
        } else {
            Effect::Spawn(job)
        });
        id
    }

    pub fn job_running(&self, job: JobId) -> bool {
        self.job_handlers.0.iter().any(|(known, _)| *known == job)
    }

    pub fn send_job(&mut self, job: JobId, text: impl Into<String>) {
        self.effects.push(Effect::Send(job, text.into()));
    }

    pub fn stop_job(&mut self, job: JobId) {
        self.effects.push(Effect::Kill(job));
    }

    pub(in crate::editor) fn notify_job(&mut self, job: JobId, event: JobEvent<'_>) -> bool {
        let Some(at) = self
            .job_handlers
            .0
            .iter()
            .position(|(known, _)| *known == job)
        else {
            return false;
        };
        let (_, mut handler) = self.job_handlers.0.remove(at);
        handler(self, job, event);
        if !matches!(event, JobEvent::Finished(_)) {
            self.job_handlers.0.push((job, handler));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Editor, Effect};

    #[derive(Debug, Default)]
    struct Heard(Vec<String>);

    fn spec(interactive: bool) -> JobSpec {
        JobSpec {
            program: "ruff".to_owned(),
            args: vec!["check".to_owned()],
            directory: PathBuf::from("project"),
            env: vec![("NO_COLOR".to_owned(), "1".to_owned())],
            label: "lint",
            name: "ruff check".to_owned(),
            buffer: None,
            interactive,
        }
    }

    fn listen(editor: &mut Editor, job: JobId, event: JobEvent<'_>) {
        let heard = match event {
            JobEvent::Line(line) => format!("{} line {line}", job.0),
            JobEvent::Output(text) => format!("output {text}"),
            JobEvent::Finished(outcome) => format!("finished {outcome:?}"),
        };
        if let Some(state) = editor.applet_state_mut::<Heard>() {
            state.0.push(heard);
        }
    }

    fn heard(editor: &Editor) -> Vec<String> {
        editor
            .applet_state::<Heard>()
            .map(|state| state.0.clone())
            .unwrap_or_default()
    }

    #[test]
    fn jobs_tell_their_handler_each_line_and_how_they_ended() {
        let mut editor = Editor::new();
        editor.set_applet_state(Heard::default());

        let id = editor.start_job(spec(false), listen);

        assert_eq!(
            editor.take_effects(),
            [Effect::Spawn(Job {
                id,
                program: "ruff".to_owned(),
                args: vec!["check".to_owned()],
                directory: PathBuf::from("project"),
                env: vec![("NO_COLOR".to_owned(), "1".to_owned())],
            })]
        );
        assert!(editor.job_running(id));
        editor.job_output(id, "a.py:1:1: F401");
        editor.job_finished(id, &Outcome::Exited(Some(1)));
        editor.job_output(id, "late");

        assert_eq!(
            heard(&editor),
            ["0 line a.py:1:1: F401", "finished Exited(Some(1))"]
        );
        assert!(!editor.job_running(id));
    }

    #[test]
    fn interactive_jobs_take_input_and_stream_output() {
        let mut editor = Editor::new();
        editor.set_applet_state(Heard::default());

        let id = editor.start_job(spec(true), listen);
        editor.send_job(id, "print(1)\n");
        editor.session_output(id, "1\n>>> ");
        editor.stop_job(id);

        let effects = editor.take_effects();
        assert!(matches!(&effects[0], Effect::StartSession(job) if job.id == id));
        assert_eq!(
            effects[1..],
            [Effect::Send(id, "print(1)\n".to_owned()), Effect::Kill(id)]
        );
        assert_eq!(heard(&editor), ["output 1\n>>> "]);
    }

    #[test]
    fn jobs_show_in_the_process_list_under_their_label() {
        let mut editor = Editor::new();

        let id = editor.start_job(spec(false), |_, _, _| {});
        editor.job_started(id, 4242);

        assert_eq!(editor.process_pids(), [4242]);
    }
}
