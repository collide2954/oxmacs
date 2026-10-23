use std::ffi::OsStr;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::external::tools;

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

#[derive(Debug, Clone)]
pub struct Server {
    child: Arc<Mutex<Child>>,
    input: Arc<Mutex<ChildStdin>>,
}

fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Server {
    pub fn send(&self, body: &str) -> io::Result<()> {
        let mut input = lock(&self.input);
        input.write_all(&frame(body))?;
        input.flush()
    }

    pub fn kill(&self) {
        let _ = lock(&self.child).kill();
    }

    pub fn pid(&self) -> u32 {
        lock(&self.child).id()
    }
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

pub fn locate(program: &str, search_path: Option<&OsStr>) -> Option<PathBuf> {
    let given = Path::new(program);
    if given.components().count() > 1 {
        return given.is_file().then(|| given.to_owned());
    }
    let script = cfg!(windows)
        .then(|| {
            env_paths(search_path)
                .map(|directory| directory.join(format!("{program}.cmd")))
                .find(|candidate| candidate.is_file())
        })
        .flatten();
    tools::find(program, search_path).or(script)
}

fn env_paths(search_path: Option<&OsStr>) -> impl Iterator<Item = PathBuf> {
    search_path.map(std::env::split_paths).into_iter().flatten()
}

fn command(launch: &Launch) -> Command {
    let script = launch
        .program
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("cmd"));
    let mut command = if script {
        let mut command = Command::new("cmd");
        command.arg("/C").arg(&launch.program);
        command
    } else {
        Command::new(&launch.program)
    };
    command
        .args(&launch.args)
        .current_dir(&launch.directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    tools::hide_console(&mut command);
    command
}

fn ending(child: &Mutex<Child>) -> Option<String> {
    let deadline = Instant::now() + SETTLE;
    loop {
        match lock(child).try_wait() {
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

pub fn start(launch: &Launch, report: impl FnMut(Event) + Send + 'static) -> io::Result<Server> {
    let mut child = command(launch).spawn()?;
    let missing = || io::Error::other("the server's pipes are missing");
    let input = child.stdin.take().ok_or_else(missing)?;
    let output = child.stdout.take().ok_or_else(missing)?;
    let errors = child.stderr.take().ok_or_else(missing)?;
    let report = Arc::new(Mutex::new(report));
    let logged = Arc::clone(&report);
    thread::spawn(move || {
        for line in BufReader::new(errors).lines() {
            let Ok(line) = line else {
                break;
            };
            (lock(&logged))(Event::Log(line));
        }
    });
    let child = Arc::new(Mutex::new(child));
    let watched = Arc::clone(&child);
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
    Ok(Server {
        child,
        input: Arc::new(Mutex::new(input)),
    })
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
    fn programs_are_found_on_the_path_or_given_by_path() {
        let dir = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) {
            "fake-ls.exe"
        } else {
            "fake-ls"
        };
        std::fs::write(dir.path().join(name), "").unwrap();
        let search = dir.path().as_os_str();

        assert_eq!(locate("fake-ls", Some(search)), Some(dir.path().join(name)));
        assert_eq!(locate("absent-ls", Some(search)), None);
        let full = dir.path().join(name);
        assert_eq!(locate(full.to_str().unwrap(), None), Some(full.clone()));
    }

    #[test]
    #[cfg(windows)]
    fn npm_scripts_are_found_on_windows() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pyright-langserver.cmd"), "").unwrap();

        assert_eq!(
            locate("pyright-langserver", Some(dir.path().as_os_str())),
            Some(dir.path().join("pyright-langserver.cmd"))
        );
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
