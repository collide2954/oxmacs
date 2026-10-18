use std::io::{self, BufRead, BufReader, PipeReader, PipeWriter};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::editor::{Job, Outcome};
use crate::tools;

const POLL: Duration = Duration::from_millis(50);
const LINGER: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Line(String),
    Finished(Outcome),
}

#[derive(Debug, Clone)]
pub struct Killer(Arc<Mutex<Child>>);

impl Killer {
    pub fn kill(&self) {
        let _ = lock(&self.0).kill();
    }
}

pub fn spawn(job: &Job, report: impl FnMut(Event) + Send + 'static) -> io::Result<Killer> {
    let (reader, writer) = io::pipe()?;
    let child = Arc::new(Mutex::new(command(job, writer)?.spawn()?));
    let (sender, lines) = mpsc::channel();
    thread::spawn(move || read_lines(reader, &sender));
    let watched = Arc::clone(&child);
    thread::spawn(move || supervise(&watched, &lines, report));
    Ok(Killer(child))
}

fn command(job: &Job, output: PipeWriter) -> io::Result<Command> {
    let mut command = Command::new(&job.program);
    command
        .args(&job.args)
        .current_dir(&job.directory)
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output);
    tools::hide_console(&mut command);
    Ok(command)
}

fn lock(child: &Mutex<Child>) -> MutexGuard<'_, Child> {
    child.lock().unwrap_or_else(PoisonError::into_inner)
}

fn read_lines(reader: PipeReader, lines: &Sender<String>) {
    let mut reader = BufReader::new(reader);
    let mut bytes = Vec::new();
    while matches!(reader.read_until(b'\n', &mut bytes), Ok(read) if read > 0) {
        let line = String::from_utf8_lossy(&bytes);
        if lines
            .send(line.trim_end_matches(['\n', '\r']).to_owned())
            .is_err()
        {
            return;
        }
        bytes.clear();
    }
}

fn supervise(child: &Mutex<Child>, lines: &Receiver<String>, mut report: impl FnMut(Event)) {
    let code = loop {
        match lock(child).try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) => {}
            Err(_) => break None,
        }
        match lines.recv_timeout(POLL) {
            Ok(line) => report(Event::Line(line)),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => thread::sleep(POLL),
        }
    };
    let deadline = Instant::now() + LINGER;
    while let Some(left) = deadline.checked_duration_since(Instant::now())
        && let Ok(line) = lines.recv_timeout(left)
    {
        report(Event::Line(line));
    }
    report(Event::Finished(Outcome::Exited(code)));
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::path::PathBuf;

    use super::*;
    use crate::editor::JobId;

    const PATIENCE: Duration = Duration::from_secs(30);

    fn job(program: &str, args: &[&str]) -> Job {
        Job {
            id: JobId(0),
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            directory: env::temp_dir(),
        }
    }

    fn started(job: &Job) -> (Killer, Receiver<Event>) {
        let (sender, events) = mpsc::channel();
        let killer = spawn(job, move |event| {
            let _ = sender.send(event);
        })
        .unwrap();
        (killer, events)
    }

    fn collect(events: &Receiver<Event>) -> Vec<Event> {
        let mut seen = Vec::new();
        while let Ok(event) = events.recv_timeout(PATIENCE) {
            let finished = matches!(event, Event::Finished(_));
            seen.push(event);
            if finished {
                break;
            }
        }
        seen
    }

    #[test]
    fn output_lines_and_the_exit_code_are_reported() {
        let (_, events) = started(&job(env!("CARGO"), &["--version"]));

        let events = collect(&events);

        assert!(
            matches!(
                &events[..],
                [Event::Line(line), Event::Finished(Outcome::Exited(Some(0)))]
                    if line.starts_with("cargo ")
            ),
            "{events:?}"
        );
    }

    #[test]
    fn errors_arrive_with_the_output() {
        let (_, events) = started(&job(env!("CARGO"), &["--oxmacs-no-such-flag"]));

        let events = collect(&events);

        assert!(
            events.iter().any(
                |event| matches!(event, Event::Line(line) if line.contains("--oxmacs-no-such-flag"))
            ),
            "{events:?}"
        );
        assert!(matches!(
            events.last(),
            Some(Event::Finished(Outcome::Exited(Some(code)))) if *code != 0
        ));
    }

    #[test]
    fn programs_that_cannot_start_say_why() {
        let missing = job("oxmacs-no-such-program", &[]);

        assert!(spawn(&missing, |_| {}).is_err());
    }

    #[test]
    fn running_jobs_can_be_killed() {
        let long = if cfg!(windows) {
            job("ping", &["-n", "60", "127.0.0.1"])
        } else {
            job("sleep", &["60"])
        };
        let (killer, events) = started(&long);
        let start = Instant::now();

        killer.kill();

        assert!(matches!(
            collect(&events).last(),
            Some(Event::Finished(Outcome::Exited(_)))
        ));
        assert!(start.elapsed() < PATIENCE);
    }

    #[test]
    fn jobs_run_in_their_directory() {
        let dir = tempfile::tempdir().unwrap();
        let (program, args): (&str, &[&str]) = if cfg!(windows) {
            ("cmd", &["/C", "cd"])
        } else {
            ("pwd", &[])
        };
        let mut job = job(program, args);
        job.directory = dir.path().to_owned();
        let (_, events) = started(&job);

        let events = collect(&events);

        let Some(Event::Line(line)) = events.first() else {
            panic!("{events:?}");
        };
        assert_eq!(
            PathBuf::from(line).canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
    }
}
