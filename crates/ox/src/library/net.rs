use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::library::paths::text;
use crate::portable::{Channel, Failure, Portable, Sent};
use crate::symbol::Sym;
use crate::value::{Value, print};

const MODULE: &str = "ox/net";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(30);
const KEPT_LINES: usize = 4096;
const CHUNK: usize = 8192;

#[derive(Debug)]
enum Command {
    Send(String),
    Close,
}

#[derive(Debug)]
struct Socket {
    commands: Mutex<Sender<Command>>,
}

trait Wire: Read + Write + Send {}

impl<T: Read + Write + Send> Wire for T {}

fn keyword(name: &str) -> Portable {
    Portable::Keyword(Sym::intern(name))
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

fn open(host: &str, port: u16, tls: bool) -> io::Result<Box<dyn Wire>> {
    let mut failure = io::Error::new(io::ErrorKind::NotFound, "no address for the host");
    let mut connected = None;
    for address in (host, port).to_socket_addrs()? {
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
    if !tls {
        stream.set_read_timeout(Some(POLL))?;
        return Ok(Box::new(stream));
    }
    let name = ServerName::try_from(host.to_owned())
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

fn deliver(lines: &Channel, line: &str) {
    let item = Portable::String(line.into());
    loop {
        match lines.send(item.clone()) {
            Sent::Done | Sent::Closed => return,
            Sent::Full => thread::sleep(POLL),
        }
    }
}

fn pump(mut stream: Box<dyn Wire>, commands: &Receiver<Command>, lines: &Channel) {
    let mut pending = Vec::new();
    let mut buffer = [0; CHUNK];
    loop {
        loop {
            match commands.try_recv() {
                Ok(Command::Send(line)) => {
                    let written = stream
                        .write_all(format!("{line}\r\n").as_bytes())
                        .and_then(|()| stream.flush());
                    if written.is_err() {
                        return;
                    }
                }
                Ok(Command::Close) | Err(TryRecvError::Disconnected) => return,
                Err(TryRecvError::Empty) => break,
            }
        }
        match stream.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => {
                pending.extend_from_slice(&buffer[..read]);
                while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                    let bytes: Vec<u8> = pending.drain(..=end).collect();
                    let line = String::from_utf8_lossy(&bytes);
                    deliver(lines, line.trim_end_matches(['\r', '\n']));
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return,
        }
    }
}

fn connect(host: String, port: u16, tls: bool) -> Result<Portable, Failure> {
    let stream = open(&host, port, tls).map_err(|error| Failure {
        value: None,
        message: format!("Couldn't connect to {host}:{port}: {error}"),
    })?;
    let lines = Arc::new(Channel::new(KEPT_LINES));
    let (sender, commands) = mpsc::channel();
    let fed = Arc::clone(&lines);
    let started = thread::Builder::new()
        .name(format!("ox net {host}"))
        .spawn(move || {
            pump(stream, &commands, &fed);
            drop(commands);
            fed.close();
        });
    if let Err(error) = started {
        return Err(Failure {
            value: None,
            message: format!("Couldn't start the connection to {host}: {error}"),
        });
    }
    let socket = Socket {
        commands: Mutex::new(sender),
    };
    Ok(Portable::Map(vec![
        (keyword("lines"), Portable::Channel(lines)),
        (
            keyword("socket"),
            Portable::Foreign(Sym::intern("socket"), Arc::new(socket)),
        ),
        (keyword("host"), Portable::String(host.into())),
        (keyword("port"), Portable::Int(i64::from(port))),
    ]))
}

fn wrong(heap: &Heap, name: &str, wanted: &str, value: Value) -> Error {
    Error::new(format!(
        "{name} expects {wanted}, got {}",
        print::brief(heap, value)
    ))
    .of(ErrorKind::Type)
}

fn socket<'a>(heap: &'a Heap, name: &str, connection: Value) -> Result<&'a Socket, Error> {
    heap.entries(connection)
        .and_then(|entries| {
            entries.into_iter().find_map(|(key, value)| match key {
                Value::Keyword(key) if key.name() == "socket" => Some(value),
                _ => None,
            })
        })
        .and_then(|value| heap.foreign_of::<Socket>(value))
        .ok_or_else(|| wrong(heap, name, "a connection from net/connect", connection))
}

fn command(heap: &Heap, name: &str, connection: Value, command: Command) -> Result<Value, Error> {
    let socket = socket(heap, name, connection)?;
    let sent = socket
        .commands
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .send(command)
        .is_ok();
    Ok(Value::Bool(sent))
}

fn options(heap: &Heap, given: &[Value]) -> Result<bool, Error> {
    let mut tls = false;
    for pair in given.chunks(2) {
        match pair {
            [Value::Keyword(option), value] if option.name() == "tls" => tls = value.truthy(),
            _ => {
                return Err(Error::new(format!(
                    "net/connect takes :tls, not {}",
                    print::brief(heap, pair[0])
                )));
            }
        }
    }
    Ok(tls)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native_in(MODULE, "connect", 2, None, |heap, arguments| {
        let host = text(heap, "net/connect", arguments[0])?.to_owned();
        let port = match arguments[1] {
            Value::Int(port) => u16::try_from(port).ok(),
            _ => None,
        }
        .ok_or_else(|| wrong(heap, "net/connect", "a port number", arguments[1]))?;
        let tls = options(heap, &arguments[2..])?;
        heap.background(&format!("ox net/connect {host}"), move || {
            connect(host, port, tls)
        })
    });
    isolate.define_native_in(MODULE, "send", 2, Some(2), |heap, arguments| {
        let line = print::display(heap, arguments[1]);
        command(heap, "net/send", arguments[0], Command::Send(line))
    });
    isolate.define_native_in(MODULE, "close", 1, Some(1), |heap, arguments| {
        command(heap, "net/close", arguments[0], Command::Close)
    });
}
