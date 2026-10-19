use std::io::{self, BufRead, BufReader, PipeReader, PipeWriter, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::editor::{Job, Outcome};
use crate::external::tools;

const POLL: Duration = Duration::from_millis(50);
const LINGER: Duration = Duration::from_millis(250);
const CHUNK: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Line(String),
    Output(String),
    Finished(Outcome),
}

#[derive(Debug, Clone)]
pub struct Handle {
    child: Arc<Mutex<Child>>,
    input: Arc<Mutex<Option<ChildStdin>>>,
}

impl Handle {
    pub fn kill(&self) {
        let _ = lock(&self.child).kill();
    }

    pub fn send(&self, text: &str) -> io::Result<()> {
        let mut input = lock(&self.input);
        let stdin = input
            .as_mut()
            .ok_or_else(|| io::Error::from(io::ErrorKind::BrokenPipe))?;
        stdin.write_all(text.as_bytes())?;
        stdin.flush()
    }

    pub fn close_input(&self) {
        lock(&self.input).take();
    }
}

pub fn spawn(job: &Job, report: impl FnMut(Event) + Send + 'static) -> io::Result<Handle> {
    launch(job, false, report)
}

pub fn start_session(job: &Job, report: impl FnMut(Event) + Send + 'static) -> io::Result<Handle> {
    launch(job, true, report)
}

fn launch(
    job: &Job,
    interactive: bool,
    report: impl FnMut(Event) + Send + 'static,
) -> io::Result<Handle> {
    let (reader, writer) = io::pipe()?;
    let mut child = command(job, writer, interactive)?.spawn()?;
    let input = Arc::new(Mutex::new(child.stdin.take()));
    let child = Arc::new(Mutex::new(child));
    let (sender, events) = mpsc::channel();
    thread::spawn(move || {
        if interactive {
            read_chunks(reader, &sender);
        } else {
            read_lines(reader, &sender);
        }
    });
    let watched = Arc::clone(&child);
    thread::spawn(move || supervise(&watched, &events, report));
    Ok(Handle { child, input })
}

fn command(job: &Job, output: PipeWriter, interactive: bool) -> io::Result<Command> {
    let mut command = Command::new(&job.program);
    command
        .args(&job.args)
        .envs(job.env.iter().map(|(key, value)| (key, value)))
        .current_dir(&job.directory)
        .stdin(if interactive {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(output.try_clone()?)
        .stderr(output);
    tools::hide_console(&mut command);
    Ok(command)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn read_lines(reader: PipeReader, events: &Sender<Event>) {
    let mut reader = BufReader::new(reader);
    let mut bytes = Vec::new();
    while matches!(reader.read_until(b'\n', &mut bytes), Ok(read) if read > 0) {
        let line = String::from_utf8_lossy(&bytes);
        let line = line.trim_end_matches(['\n', '\r']).to_owned();
        if events.send(Event::Line(line)).is_err() {
            return;
        }
        bytes.clear();
    }
}

fn read_chunks(mut reader: PipeReader, events: &Sender<Event>) {
    let mut buffer = [0; CHUNK];
    let mut pending = Vec::new();
    while let Ok(read) = reader.read(&mut buffer)
        && read > 0
    {
        pending.extend_from_slice(&buffer[..read]);
        let complete = match std::str::from_utf8(&pending) {
            Ok(_) => pending.len(),
            Err(error) if error.error_len().is_some() => pending.len(),
            Err(error) => error.valid_up_to(),
        };
        let text = String::from_utf8_lossy(&pending[..complete]).replace('\r', "");
        pending.drain(..complete);
        if !text.is_empty() && events.send(Event::Output(text)).is_err() {
            return;
        }
    }
}

fn supervise(child: &Mutex<Child>, events: &Receiver<Event>, mut report: impl FnMut(Event)) {
    let code = loop {
        match lock(child).try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) => {}
            Err(_) => break None,
        }
        match events.recv_timeout(POLL) {
            Ok(event) => report(event),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => thread::sleep(POLL),
        }
    };
    let deadline = Instant::now() + LINGER;
    while let Some(left) = deadline.checked_duration_since(Instant::now())
        && let Ok(event) = events.recv_timeout(left)
    {
        report(event);
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
            env: Vec::new(),
        }
    }

    fn started(job: &Job) -> (Handle, Receiver<Event>) {
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

    fn session(job: &Job) -> (Handle, Receiver<Event>) {
        let (sender, events) = mpsc::channel();
        let handle = start_session(job, move |event| {
            let _ = sender.send(event);
        })
        .unwrap();
        (handle, events)
    }

    #[test]
    fn sessions_take_input_and_stream_output() {
        let echo = if cfg!(windows) {
            job("findstr", &["^"])
        } else {
            job("cat", &[])
        };
        let (handle, events) = session(&echo);

        handle.send("b\na\n").unwrap();
        handle.close_input();

        let events = collect(&events);
        let output: String = events
            .iter()
            .filter_map(|event| match event {
                Event::Output(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(output, "b\na\n");
        assert!(matches!(
            events.last(),
            Some(Event::Finished(Outcome::Exited(Some(0))))
        ));
    }

    #[test]
    fn jobs_without_a_session_refuse_input() {
        let (handle, events) = started(&job(env!("CARGO"), &["--version"]));

        assert!(handle.send("x").is_err());
        collect(&events);
    }

    #[test]
    fn jobs_get_their_environment() {
        let (program, args): (&str, &[&str]) = if cfg!(windows) {
            ("cmd", &["/C", "echo %OXMACS_TEST%"])
        } else {
            ("sh", &["-c", "echo $OXMACS_TEST"])
        };
        let mut job = job(program, args);
        job.env = vec![("OXMACS_TEST".to_owned(), "hello".to_owned())];
        let (_, events) = started(&job);

        assert_eq!(
            collect(&events).first(),
            Some(&Event::Line("hello".to_owned()))
        );
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
