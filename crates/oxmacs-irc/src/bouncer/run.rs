use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use jiff::Timestamp;
use rustls::ServerConfig;

use crate::bouncer::history::History;
use crate::bouncer::relay::{Action, Bouncer, ClientId};
use crate::settings::IrcServer;
use oxmacs_core::external::net::{self, Connection, Event};

const IDLE: Duration = Duration::from_secs(60);
const FAREWELL: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub struct Daemon<H> {
    pub servers: Vec<IrcServer>,
    pub passwords: Vec<Option<String>>,
    pub password: String,
    pub history: H,
    pub tls: Option<Arc<ServerConfig>>,
}

enum Input {
    Accepted(TcpStream),
    Client(ClientId, Event),
    Upstream(usize, Event),
}

struct Runtime {
    inputs: Sender<Input>,
    tls: Option<Arc<ServerConfig>>,
    upstreams: HashMap<usize, Connection>,
    clients: HashMap<ClientId, Connection>,
    timers: Vec<(Instant, usize)>,
    next_client: u64,
}

impl Runtime {
    fn perform(&mut self, actions: Vec<Action>) -> bool {
        for action in actions {
            match action {
                Action::Connect { network, endpoint } => {
                    let inputs = self.inputs.clone();
                    let connection = net::connect(&endpoint, move |event| {
                        let _ = inputs.send(Input::Upstream(network, event));
                    });
                    self.upstreams.insert(network, connection);
                }
                Action::Reconnect { network, after } => {
                    self.timers.push((Instant::now() + after, network));
                }
                Action::Upstream { network, line } => {
                    if let Some(connection) = self.upstreams.get(&network) {
                        connection.send(&line);
                    }
                }
                Action::Client { client, line } => {
                    if let Some(connection) = self.clients.get(&client) {
                        connection.send(&line);
                    }
                }
                Action::Drop(client) => {
                    if let Some(connection) = self.clients.remove(&client) {
                        connection.close();
                    }
                }
                Action::Stop => {
                    for connection in self.upstreams.values().chain(self.clients.values()) {
                        connection.close();
                    }
                    for connection in self.upstreams.values() {
                        connection.wait(FAREWELL);
                    }
                    return false;
                }
            }
        }
        true
    }

    fn accept(&mut self, stream: TcpStream) -> ClientId {
        let id = ClientId(self.next_client);
        self.next_client += 1;
        let inputs = self.inputs.clone();
        let connection = net::accept(stream, self.tls.clone(), move |event| {
            let _ = inputs.send(Input::Client(id, event));
        });
        self.clients.insert(id, connection);
        id
    }

    fn patience(&self) -> Duration {
        let now = Instant::now();
        self.timers
            .iter()
            .map(|(due, _)| due.saturating_duration_since(now))
            .min()
            .unwrap_or(IDLE)
    }

    fn due(&mut self) -> Vec<usize> {
        let now = Instant::now();
        let (due, waiting): (Vec<_>, Vec<_>) =
            self.timers.drain(..).partition(|(when, _)| *when <= now);
        self.timers = waiting;
        due.into_iter().map(|(_, network)| network).collect()
    }
}

pub fn run<H: History>(daemon: Daemon<H>, listener: TcpListener) {
    let (inputs, received) = mpsc::channel();
    let accepting = inputs.clone();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if accepting.send(Input::Accepted(stream)).is_err() {
                return;
            }
        }
    });
    let mut bouncer = Bouncer::new(
        daemon.servers,
        daemon.passwords,
        &daemon.password,
        daemon.history,
    );
    let mut runtime = Runtime {
        inputs,
        tls: daemon.tls,
        upstreams: HashMap::new(),
        clients: HashMap::new(),
        timers: Vec::new(),
        next_client: 0,
    };
    bouncer.start();
    while runtime.perform(bouncer.take_actions()) {
        match received.recv_timeout(runtime.patience()) {
            Ok(Input::Accepted(stream)) => {
                let id = runtime.accept(stream);
                bouncer.client_attached(id);
            }
            Ok(Input::Client(id, Event::Line(line))) => {
                bouncer.client_line(id, &line, Timestamp::now());
            }
            Ok(Input::Client(id, Event::Closed(_))) => {
                runtime.clients.remove(&id);
                bouncer.client_closed(id, Timestamp::now());
            }
            Ok(Input::Client(_, Event::Connected)) | Err(RecvTimeoutError::Timeout) => {}
            Ok(Input::Upstream(network, event)) => {
                bouncer.upstream_event(network, event, Timestamp::now());
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
        for network in runtime.due() {
            bouncer.reconnect(network);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::sync::mpsc::Receiver;

    use super::*;
    use crate::bouncer::history::MemoryHistory;
    use crate::settings::{IrcSettings, TABLE};
    use oxmacs_core::config::Config;

    const PATIENCE: Duration = Duration::from_secs(20);

    fn fake_upstream() -> (u16, Receiver<String>, Sender<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (heard, hearing) = mpsc::channel();
        let (say, saying) = mpsc::channel::<String>();
        thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut writer = stream.try_clone().unwrap();
            let speaker = stream.try_clone().unwrap();
            thread::spawn(move || {
                let mut speaker = speaker;
                for line in saying {
                    let _ = speaker.write_all(format!("{line}\r\n").as_bytes());
                }
            });
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else {
                    return;
                };
                let reply = if line.starts_with("CAP LS") {
                    ":fake CAP * LS :\r\n".to_owned()
                } else if line.starts_with("USER") {
                    ":fake 001 oxtest :Welcome\r\n".to_owned()
                } else if line.starts_with("JOIN") {
                    ":oxtest!o@h JOIN #c\r\n:fake 353 oxtest = #c :oxtest alice\r\n:fake 366 oxtest #c :End\r\n".to_owned()
                } else {
                    String::new()
                };
                let _ = writer.write_all(reply.as_bytes());
                let _ = heard.send(line);
            }
        });
        (port, hearing, say)
    }

    fn wait_for(lines: &Receiver<String>, wanted: &str) {
        let start = Instant::now();
        while start.elapsed() < PATIENCE {
            if let Ok(line) = lines.recv_timeout(PATIENCE)
                && line.contains(wanted)
            {
                return;
            }
        }
        panic!("never saw {wanted}");
    }

    fn read_until(reader: &mut BufReader<TcpStream>, wanted: &str) -> Vec<String> {
        let mut seen = Vec::new();
        loop {
            let mut line = String::new();
            assert!(
                reader.read_line(&mut line).unwrap() > 0,
                "closed before {wanted}: {seen:?}"
            );
            let done = line.contains(wanted);
            seen.push(line.trim_end().to_owned());
            if done {
                return seen;
            }
        }
    }

    #[test]
    fn a_client_attaches_through_the_bouncer_to_a_live_upstream() {
        let (port, upstream_heard, upstream_says) = fake_upstream();
        let servers = Config::parse(&format!(
            "[[irc-server]]\nname = \"fake\"\nhost = \"127.0.0.1\"\nport = {port}\ntls = false\nnick = \"oxtest\"\nchannels = [\"#c\"]\n"
        ))
        .unwrap()
        .applet::<IrcSettings>(TABLE)
        .unwrap()
        .server;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let daemon = Daemon {
            servers,
            passwords: Vec::new(),
            password: "pw".to_owned(),
            history: MemoryHistory::default(),
            tls: None,
        };
        let bouncer = thread::spawn(move || run(daemon, listener));
        wait_for(&upstream_heard, "JOIN #c");

        let mut client = TcpStream::connect(address).unwrap();
        client.set_read_timeout(Some(PATIENCE)).unwrap();
        client
            .write_all(b"PASS pw\r\nNICK me\r\nUSER me/fake 0 * :me\r\n")
            .unwrap();
        let mut reader = BufReader::new(client.try_clone().unwrap());
        let burst = read_until(&mut reader, " 366 ");
        assert!(
            burst.iter().any(|line| line.contains(" 001 oxtest ")),
            "{burst:?}"
        );
        assert!(
            burst.iter().any(|line| line.ends_with("JOIN #c")),
            "{burst:?}"
        );

        upstream_says
            .send(":alice!a@h PRIVMSG #c :hello from upstream".to_owned())
            .unwrap();
        read_until(&mut reader, "hello from upstream");

        client.write_all(b"PRIVMSG #c :hi upstream\r\n").unwrap();
        wait_for(&upstream_heard, "PRIVMSG #c :hi upstream");

        client.write_all(b"OXMACS STOP\r\n").unwrap();
        wait_for(&upstream_heard, "QUIT :oxmacs bouncer stopping");
        let start = Instant::now();
        while !bouncer.is_finished() {
            assert!(start.elapsed() < PATIENCE, "the bouncer did not stop");
            thread::sleep(Duration::from_millis(20));
        }
    }
}
