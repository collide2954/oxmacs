use std::fmt;
use std::sync::Arc;

use crate::editor::{Editor, Effect, Job, JobId, Outcome, ServerId};
use crate::external::lsp::{self, Launch};
use crate::external::process;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    Job(JobId, process::Event),
    Server(ServerId, lsp::Event),
}

type Sink = Arc<dyn Fn(Delivery) + Send + Sync>;

pub struct Runner {
    sink: Sink,
}

impl fmt::Debug for Runner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Runner").finish_non_exhaustive()
    }
}

impl Runner {
    pub fn new(sink: impl Fn(Delivery) + Send + Sync + 'static) -> Self {
        Self {
            sink: Arc::new(sink),
        }
    }

    pub fn perform(&self, editor: &mut Editor, effect: Effect) -> Option<Effect> {
        match effect {
            Effect::Spawn(job) => self.spawn(editor, &job, false),
            Effect::StartSession(job) => self.spawn(editor, &job, true),
            Effect::Send(job, text) => {
                let sent = editor
                    .job_process(job)
                    .is_some_and(|process| process.send(text).is_ok());
                if !sent {
                    editor.show_message("The process is not running");
                }
            }
            Effect::StartServer(id, launch) => self.start_server(editor, id, &launch),
            Effect::ToServer(id, body) => {
                if let Some(process) = editor.server_process(id) {
                    let _ = process.send(lsp::frame(&body));
                }
            }
            other => return Some(other),
        }
        None
    }

    pub fn deliver(&self, editor: &mut Editor, delivery: Delivery) {
        match delivery {
            Delivery::Job(job, process::Event::Line(line)) => editor.job_output(job, &line),
            Delivery::Job(job, process::Event::Output(text)) => editor.session_output(job, &text),
            Delivery::Job(job, process::Event::Finished(outcome)) => {
                editor.job_finished(job, &outcome);
            }
            Delivery::Server(id, event) => editor.server_event(id, event),
        }
    }

    pub fn shutdown(&self, editor: &mut Editor) {
        editor.kill_every_process();
    }

    fn spawn(&self, editor: &mut Editor, job: &Job, session: bool) {
        let id = job.id;
        let sink = Arc::clone(&self.sink);
        let report = move |event| sink(Delivery::Job(id, event));
        let started = if session {
            process::start_session(job, report)
        } else {
            process::spawn(job, report)
        };
        match started {
            Ok(process) => editor.job_started(id, process),
            Err(error) => editor.job_finished(id, &Outcome::Failed(error.to_string())),
        }
    }

    fn start_server(&self, editor: &mut Editor, id: ServerId, launch: &Launch) {
        let sink = Arc::clone(&self.sink);
        match lsp::start(launch, move |event| sink(Delivery::Server(id, event))) {
            Ok(process) => editor.server_started(id, process),
            Err(error) => editor.server_event(id, lsp::Event::Exited(Some(error.to_string()))),
        }
    }
}
