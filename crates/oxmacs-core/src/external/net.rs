use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(50);
const BURST: u32 = 4;
const PACE: Duration = Duration::from_secs(1);
const CHUNK: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub tls: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Connected,
    Line(String),
    Closed(Option<String>),
}

#[derive(Debug)]
enum Command {
    Send(String),
    Close,
}

#[derive(Debug, Clone)]
pub struct Connection {
    commands: Sender<Command>,
}

impl Connection {
    pub fn send(&self, line: &str) {
        let _ = self.commands.send(Command::Send(line.to_owned()));
    }

    pub fn close(&self) {
        let _ = self.commands.send(Command::Close);
    }
}

#[derive(Debug, Clone, Copy)]
struct Throttle {
    burst: u32,
    pace: Duration,
    tokens: u32,
    since: Instant,
}

impl Throttle {
    fn new(burst: u32, pace: Duration, now: Instant) -> Self {
        Self {
            burst,
            pace,
            tokens: burst,
            since: now,
        }
    }

    fn take(&mut self, now: Instant) -> bool {
        while self.tokens < self.burst && now.duration_since(self.since) >= self.pace {
            self.tokens += 1;
            self.since += self.pace;
        }
        if self.tokens == self.burst {
            self.since = now;
        }
        if self.tokens == 0 {
            return false;
        }
        self.tokens -= 1;
        true
    }
}

trait Wire: Read + Write + Send {}

impl<T: Read + Write + Send> Wire for T {}

pub fn connect(endpoint: &Endpoint, mut report: impl FnMut(Event) + Send + 'static) -> Connection {
    let (commands, received) = mpsc::channel();
    let endpoint = endpoint.clone();
    thread::spawn(move || {
        let closed = match open(&endpoint) {
            Ok(stream) => {
                report(Event::Connected);
                run(stream, &received, &mut report)
            }
            Err(error) => Some(error.to_string()),
        };
        report(Event::Closed(closed));
    });
    Connection { commands }
}

fn open(endpoint: &Endpoint) -> io::Result<Box<dyn Wire>> {
    let mut failure = io::Error::new(io::ErrorKind::NotFound, "no address for the server");
    let mut connected = None;
    for address in (endpoint.host.as_str(), endpoint.port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(stream) => {
                connected = Some(stream);
                break;
            }
            Err(error) => failure = error,
        }
    }
    let stream = connected.ok_or(failure)?;
    stream.set_nodelay(true)?;
    if !endpoint.tls {
        stream.set_read_timeout(Some(POLL))?;
        return Ok(Box::new(stream));
    }
    let name = ServerName::try_from(endpoint.host.clone())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let connection = ClientConnection::new(tls_config()?, name).map_err(io::Error::other)?;
    let mut secure = StreamOwned::new(connection, stream);
    secure.sock.set_read_timeout(Some(CONNECT_TIMEOUT))?;
    while secure.conn.is_handshaking() {
        secure.conn.complete_io(&mut secure.sock)?;
    }
    secure.sock.set_read_timeout(Some(POLL))?;
    Ok(Box::new(secure))
}

fn tls_config() -> io::Result<Arc<ClientConfig>> {
    let roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(io::Error::other)?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

fn run(
    mut stream: Box<dyn Wire>,
    commands: &Receiver<Command>,
    report: &mut impl FnMut(Event),
) -> Option<String> {
    let mut queue = VecDeque::new();
    let mut throttle = Throttle::new(BURST, PACE, Instant::now());
    let mut pending = Vec::new();
    let mut buffer = [0; CHUNK];
    loop {
        let mut closing = false;
        loop {
            match commands.try_recv() {
                Ok(Command::Send(line)) => queue.push_back(line),
                Ok(Command::Close) | Err(TryRecvError::Disconnected) => {
                    closing = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        while !queue.is_empty() && (closing || throttle.take(Instant::now())) {
            let Some(line) = queue.pop_front() else {
                break;
            };
            let written = stream
                .write_all(format!("{line}\r\n").as_bytes())
                .and_then(|()| stream.flush());
            if let Err(error) = written {
                return Some(error.to_string());
            }
        }
        if closing {
            return None;
        }
        match stream.read(&mut buffer) {
            Ok(0) => return None,
            Ok(read) => {
                pending.extend_from_slice(&buffer[..read]);
                while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                    let bytes: Vec<u8> = pending.drain(..=end).collect();
                    let line = String::from_utf8_lossy(&bytes)
                        .trim_end_matches(['\r', '\n'])
                        .to_owned();
                    if !line.is_empty() {
                        report(Event::Line(line));
                    }
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(error) => return Some(error.to_string()),
        }
    }
}
#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{self, Receiver};
    use std::time::{Duration, Instant};

    use super::*;

    const PATIENCE: Duration = Duration::from_secs(10);

    fn listening() -> (TcpListener, Endpoint) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let endpoint = Endpoint {
            host: "127.0.0.1".to_owned(),
            port,
            tls: false,
        };
        (listener, endpoint)
    }

    fn connected(endpoint: &Endpoint) -> (Connection, Receiver<Event>) {
        let (sender, events) = mpsc::channel();
        let connection = connect(endpoint, move |event| {
            let _ = sender.send(event);
        });
        (connection, events)
    }

    fn next(events: &Receiver<Event>) -> Event {
        events.recv_timeout(PATIENCE).unwrap()
    }

    #[test]
    fn lines_flow_both_ways() {
        let (listener, endpoint) = listening();
        let (connection, events) = connected(&endpoint);
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;

        assert_eq!(next(&events), Event::Connected);
        connection.send("NICK oxtest");
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line, "NICK oxtest\r\n");

        writer
            .write_all(b":srv 001 oxtest :Welcome\r\nPING :x\n")
            .unwrap();
        assert_eq!(
            next(&events),
            Event::Line(":srv 001 oxtest :Welcome".to_owned())
        );
        assert_eq!(next(&events), Event::Line("PING :x".to_owned()));
    }

    #[test]
    fn the_server_closing_ends_the_connection() {
        let (listener, endpoint) = listening();
        let (_connection, events) = connected(&endpoint);
        let (stream, _) = listener.accept().unwrap();
        assert_eq!(next(&events), Event::Connected);

        drop(stream);

        assert_eq!(next(&events), Event::Closed(None));
    }

    #[test]
    fn closing_flushes_queued_lines_first() {
        let (listener, endpoint) = listening();
        let (connection, events) = connected(&endpoint);
        let (stream, _) = listener.accept().unwrap();
        assert_eq!(next(&events), Event::Connected);

        connection.send("QUIT :bye");
        connection.close();

        let mut lines = Vec::new();
        for line in BufReader::new(stream).lines() {
            lines.push(line.unwrap());
        }
        assert_eq!(lines, ["QUIT :bye"]);
        assert_eq!(next(&events), Event::Closed(None));
    }

    #[test]
    fn unreachable_servers_report_why() {
        let (listener, endpoint) = listening();
        drop(listener);
        let (_connection, events) = connected(&endpoint);

        assert!(matches!(next(&events), Event::Closed(Some(_))));
    }

    #[test]
    fn the_throttle_allows_a_burst_then_paces_lines() {
        let start = Instant::now();
        let mut throttle = Throttle::new(2, Duration::from_secs(1), start);

        assert!(throttle.take(start));
        assert!(throttle.take(start));
        assert!(!throttle.take(start));
        assert!(!throttle.take(start + Duration::from_millis(900)));
        assert!(throttle.take(start + Duration::from_millis(1000)));
        assert!(!throttle.take(start + Duration::from_millis(1100)));
        assert!(throttle.take(start + Duration::from_secs(10)));
        assert!(throttle.take(start + Duration::from_secs(10)));
        assert!(!throttle.take(start + Duration::from_secs(10)));
    }

    #[test]
    #[ignore = "talks TLS to example.com"]
    fn tls_reaches_a_public_server() {
        let (connection, events) = connected(&Endpoint {
            host: "example.com".to_owned(),
            port: 443,
            tls: true,
        });
        assert_eq!(next(&events), Event::Connected);

        for line in [
            "HEAD / HTTP/1.1",
            "Host: example.com",
            "Connection: close",
            "",
        ] {
            connection.send(line);
        }

        assert!(matches!(next(&events), Event::Line(line) if line.starts_with("HTTP/1.1 ")));
    }
}
