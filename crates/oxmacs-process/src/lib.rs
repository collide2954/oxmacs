use std::io::{self, BufRead, BufReader, Read, Write};
use std::process::{
    Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Output, Stdio,
};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

mod lookup;
#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod tree;

pub use lookup::{find, which};

const POLL: Duration = Duration::from_millis(20);
const GRACE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone)]
pub struct Process(Arc<Shared>);

#[derive(Debug)]
struct Shared {
    pid: u32,
    child: Mutex<Child>,
    input: Mutex<Option<Sender<Vec<u8>>>>,
    tree: tree::Tree,
}

#[derive(Debug)]
pub struct Started {
    pub process: Process,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn broken() -> io::Error {
    io::Error::from(io::ErrorKind::BrokenPipe)
}

fn queue(stdin: Option<ChildStdin>) -> Option<Sender<Vec<u8>>> {
    let mut stdin = stdin?;
    let (sender, queued) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || {
        for bytes in queued {
            if stdin
                .write_all(&bytes)
                .and_then(|()| stdin.flush())
                .is_err()
            {
                return;
            }
        }
    });
    Some(sender)
}

pub fn spawn(command: &mut Command) -> io::Result<Started> {
    tree::hide_console(command);
    tree::prepare(command);
    let mut child = command.spawn()?;
    let tree = tree::Tree::of(&child);
    let input = queue(child.stdin.take());
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let process = Process(Arc::new(Shared {
        pid: child.id(),
        child: Mutex::new(child),
        input: Mutex::new(input),
        tree,
    }));
    Ok(Started {
        process,
        stdout,
        stderr,
    })
}

pub fn run(command: &mut Command, input: Option<&[u8]>) -> io::Result<Output> {
    tree::hide_console(command);
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdin = child.stdin.take();
    thread::scope(|scope| {
        if let (Some(mut stdin), Some(input)) = (stdin, input) {
            scope.spawn(move || stdin.write_all(input));
        }
        child.wait_with_output()
    })
}

pub fn read_lines(reader: impl Read, mut each: impl FnMut(String) -> bool) {
    let mut reader = BufReader::new(reader);
    let mut bytes = Vec::new();
    while matches!(reader.read_until(b'\n', &mut bytes), Ok(read) if read > 0) {
        let line = String::from_utf8_lossy(&bytes);
        if !each(line.trim_end_matches(['\n', '\r']).to_owned()) {
            return;
        }
        bytes.clear();
    }
}

impl Shared {
    fn try_wait(&self) -> io::Result<Option<ExitStatus>> {
        lock(&self.child).try_wait()
    }

    fn kill(&self) {
        if !self.tree.kill() {
            let _ = lock(&self.child).kill();
        }
    }
}

impl Process {
    pub fn pid(&self) -> u32 {
        self.0.pid
    }

    pub fn send(&self, bytes: impl Into<Vec<u8>>) -> io::Result<()> {
        lock(&self.0.input)
            .as_ref()
            .ok_or_else(broken)?
            .send(bytes.into())
            .map_err(|_| broken())
    }

    pub fn close_input(&self) {
        lock(&self.0.input).take();
    }

    pub fn try_wait(&self) -> io::Result<Option<ExitStatus>> {
        self.0.try_wait()
    }

    pub fn wait(&self) -> io::Result<ExitStatus> {
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(status);
            }
            thread::sleep(POLL);
        }
    }

    pub fn kill(&self) {
        self.0.kill();
    }

    pub fn terminate(&self) {
        self.close_input();
        tree::ask_to_end(&self.0.tree);
        let shared = Arc::clone(&self.0);
        thread::spawn(move || {
            let deadline = Instant::now() + GRACE;
            while Instant::now() < deadline && matches!(shared.try_wait(), Ok(None)) {
                thread::sleep(POLL);
            }
            shared.kill();
        });
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::process::Command;

    use super::*;

    const PATIENCE: Duration = Duration::from_secs(30);

    fn long() -> Command {
        let mut command = Command::new(if cfg!(windows) { "ping" } else { "sleep" });
        if cfg!(windows) {
            command.args(["-n", "60", "127.0.0.1"]);
        } else {
            command.arg("60");
        }
        command.stdout(Stdio::null());
        command
    }

    fn echo() -> Command {
        let mut command = if cfg!(windows) {
            let mut command = Command::new("findstr");
            command.arg("^");
            command
        } else {
            Command::new("cat")
        };
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
        command
    }

    fn ended_within(process: &Process, limit: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < limit {
            if matches!(process.try_wait(), Ok(Some(_))) {
                return true;
            }
            thread::sleep(POLL);
        }
        false
    }

    fn alive(pid: &str) -> bool {
        if cfg!(windows) {
            let listed = Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}"), "/NH"])
                .output()
                .unwrap();
            String::from_utf8_lossy(&listed.stdout).contains(pid)
        } else {
            Command::new("kill")
                .args(["-0", pid])
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
        }
    }

    #[test]
    fn input_is_queued_and_closing_it_ends_the_reader() {
        let Started {
            process, stdout, ..
        } = spawn(&mut echo()).unwrap();
        let line = "x".repeat(1023) + "\n";

        let start = Instant::now();
        for _ in 0..1024 {
            process.send(line.clone()).unwrap();
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        process.close_input();

        let mut output = String::new();
        stdout.unwrap().read_to_string(&mut output).unwrap();
        assert_eq!(output.replace('\r', "").len(), 1024 * 1024);
        assert!(process.wait().unwrap().success());
    }

    #[test]
    fn processes_without_input_refuse_it() {
        let mut command = long();
        command.stdin(Stdio::null());
        let Started { process, .. } = spawn(&mut command).unwrap();

        assert!(process.send("x").is_err());
        process.kill();
        assert!(ended_within(&process, PATIENCE));
    }

    #[test]
    fn programs_that_cannot_start_say_why() {
        assert!(spawn(&mut Command::new("oxmacs-no-such-program")).is_err());
    }

    #[test]
    fn terminating_ends_a_process_that_ignores_its_input() {
        let Started { process, .. } = spawn(&mut long()).unwrap();

        process.terminate();

        assert!(ended_within(&process, PATIENCE));
    }

    #[test]
    fn killing_ends_the_whole_tree() {
        let mut command = if cfg!(windows) {
            let mut command = Command::new("powershell");
            command.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$p = Start-Process ping -ArgumentList '-n','60','127.0.0.1' -WindowStyle Hidden -PassThru; $p.Id; Wait-Process -Id $p.Id",
            ]);
            command
        } else {
            let mut command = Command::new("sh");
            command.args(["-c", "sleep 60 & echo $!; wait"]);
            command
        };
        command.stdout(Stdio::piped());
        let Started {
            process, stdout, ..
        } = spawn(&mut command).unwrap();
        let mut first = None;
        read_lines(stdout.unwrap(), |line| {
            first = Some(line.trim().to_owned());
            false
        });
        let grandchild = first.unwrap();
        assert!(alive(&grandchild), "{grandchild}");

        process.kill();

        assert!(ended_within(&process, PATIENCE));
        let start = Instant::now();
        while alive(&grandchild) && start.elapsed() < PATIENCE {
            thread::sleep(Duration::from_millis(100));
        }
        assert!(!alive(&grandchild), "{grandchild} outlived the kill");
    }

    #[test]
    fn running_feeds_the_input_and_collects_the_output() {
        let mut command = if cfg!(windows) {
            let mut command = Command::new("findstr");
            command.arg("^");
            command
        } else {
            Command::new("cat")
        };

        let output = run(&mut command, Some(b"b\na\n")).unwrap();

        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).replace('\r', ""),
            "b\na\n"
        );
    }

    #[test]
    fn lines_are_read_lossily_and_keep_going_past_bad_bytes() {
        let mut seen = Vec::new();

        read_lines(Cursor::new(b"a\r\n\xffz\nb".to_vec()), |line| {
            seen.push(line);
            true
        });

        assert_eq!(seen, ["a", "\u{fffd}z", "b"]);
    }
}
