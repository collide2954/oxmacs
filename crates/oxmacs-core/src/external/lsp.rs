use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use oxmacs_process::{Process, Started};

const SETTLE: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub directory: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Message(String),
    Log(String),
    Exited(Option<String>),
}

fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn frame(body: &str) -> Vec<u8> {
    let mut framed = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    framed.extend_from_slice(body.as_bytes());
    framed
}

pub fn read_message(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut length = None;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            if length.is_some() {
                break;
            }
            continue;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0; length.unwrap_or_default()];
    reader.read_exact(&mut body)?;
    Ok(Some(String::from_utf8_lossy(&body).into_owned()))
}

fn command(launch: &Launch) -> Command {
    let mut command = Command::new(&launch.program);
    command
        .args(&launch.args)
        .current_dir(&launch.directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn ending(process: &Process) -> Option<String> {
    let deadline = Instant::now() + SETTLE;
    loop {
        match process.try_wait() {
            Ok(Some(status)) if status.success() => return None,
            Ok(Some(status)) => {
                return Some(status.code().map_or_else(
                    || "stopped by a signal".to_owned(),
                    |code| format!("exited with code {code}"),
                ));
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
            Ok(None) => return Some("stopped answering".to_owned()),
            Err(error) => return Some(error.to_string()),
        }
    }
}

pub fn start(launch: &Launch, report: impl FnMut(Event) + Send + 'static) -> io::Result<Process> {
    let Started {
        process,
        stdout,
        stderr,
    } = oxmacs_process::spawn(&mut command(launch))?;
    let missing = || io::Error::other("the server's pipes are missing");
    let output = stdout.ok_or_else(missing)?;
    let errors = stderr.ok_or_else(missing)?;
    let report = Arc::new(Mutex::new(report));
    let logged = Arc::clone(&report);
    thread::spawn(move || {
        oxmacs_process::read_lines(errors, |line| {
            (lock(&logged))(Event::Log(line));
            true
        });
    });
    let watched = process.clone();
    thread::spawn(move || {
        let mut reader = BufReader::new(output);
        let failure = loop {
            match read_message(&mut reader) {
                Ok(Some(body)) => (lock(&report))(Event::Message(body)),
                Ok(None) => break None,
                Err(error) => break Some(error.to_string()),
            }
        };
        let ended = ending(&watched).or(failure);
        (lock(&report))(Event::Exited(ended));
    });
    Ok(process)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn messages_are_framed_with_their_length_in_bytes() {
        assert_eq!(
            frame("{\"é\":1}"),
            b"Content-Length: 8\r\n\r\n{\"\xc3\xa9\":1}".to_vec()
        );
    }

    #[test]
    fn framed_messages_read_back_one_at_a_time() {
        let mut stream = frame("{\"id\":1}");
        stream.extend(b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\ncontent-length: 2\r\n\r\n{}");
        let mut reader = Cursor::new(stream);

        assert_eq!(
            read_message(&mut reader).unwrap().as_deref(),
            Some("{\"id\":1}")
        );
        assert_eq!(read_message(&mut reader).unwrap().as_deref(), Some("{}"));
        assert_eq!(read_message(&mut reader).unwrap(), None);
    }

    #[test]
    fn a_message_cut_short_is_an_error() {
        let mut reader = Cursor::new(b"Content-Length: 10\r\n\r\n{}".to_vec());

        assert!(read_message(&mut reader).is_err());
    }

    #[test]
    fn servers_that_cannot_start_say_why() {
        let launch = Launch {
            program: PathBuf::from("oxmacs-no-such-server"),
            args: Vec::new(),
            directory: std::env::temp_dir(),
        };

        assert!(start(&launch, |_| {}).is_err());
    }
}
