use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

use crate::bouncer::history::DiskHistory;
use crate::bouncer::run::{self, Daemon};
use crate::keychain;
use crate::settings::{BouncerMode, IrcSettings, PasswordSource, TABLE};
use oxmacs_core::config::{self, Config};
use oxmacs_core::external::net;

const TOKEN_BYTES: usize = 32;
const KNOCK: Duration = Duration::from_millis(100);
const HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LocalDaemon {
    pub address: String,
    pub token: String,
    pub pid: u32,
}

impl LocalDaemon {
    pub fn file() -> Option<PathBuf> {
        data_directory().map(|directory| directory.join("irc-daemon.toml"))
    }

    pub fn read(path: &Path) -> Option<Self> {
        toml::from_str(&fs::read_to_string(path).ok()?).ok()
    }

    pub fn write(&self, path: &Path) -> Result<(), String> {
        let text = toml::to_string(self).map_err(|error| error.to_string())?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        private_file(path)
            .and_then(|mut file| file.write_all(text.as_bytes()))
            .map_err(|error| format!("{}: {error}", path.display()))
    }
}

fn data_directory() -> Option<PathBuf> {
    dirs::data_local_dir().map(|directory| directory.join("oxmacs"))
}

fn private_file(path: &Path) -> io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

pub fn launch() -> Result<(), String> {
    let program = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut command = Command::new(program);
    command
        .arg("irc-daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut command);
    command.spawn().map(drop).map_err(|error| error.to_string())
}

#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

pub fn wait_until_running(patience: Duration) -> bool {
    LocalDaemon::file().is_some_and(|file| wait_for(&file, patience))
}

fn wait_for(file: &Path, patience: Duration) -> bool {
    let deadline = Instant::now() + patience;
    loop {
        let answering = LocalDaemon::read(file)
            .and_then(|daemon| daemon.address.parse::<SocketAddr>().ok())
            .is_some_and(|address| TcpStream::connect_timeout(&address, KNOCK).is_ok());
        if answering {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(KNOCK);
    }
}

pub fn token() -> Result<String, String> {
    let mut bytes = [0u8; TOKEN_BYTES];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "no source of randomness".to_owned())?;
    Ok(bytes
        .iter()
        .flat_map(|byte| [HEX[usize::from(byte >> 4)], HEX[usize::from(byte & 0xF)]])
        .map(char::from)
        .collect())
}

fn headless_password(source: &PasswordSource, name: &str, account: &str) -> Result<String, String> {
    match source {
        PasswordSource::Text(text) => Ok(text.clone()),
        PasswordSource::Env(variable) => {
            std::env::var(variable).map_err(|_| format!("{name}: set {variable} to the password"))
        }
        PasswordSource::Keychain => keychain::read(name, account)?.ok_or_else(|| {
            format!("{name}: no password for {account} in the keychain; connect once from oxmacs to keep it")
        }),
        PasswordSource::Ask => Err(format!(
            "{name}: a background bouncer cannot ask for a password; use the keychain, an environment variable or text"
        )),
    }
}

pub fn main() -> Result<(), String> {
    let result = serve();
    if let Err(error) = &result
        && let Some(directory) = data_directory()
    {
        let _ = fs::create_dir_all(&directory);
        if let Ok(mut log) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("irc-daemon.log"))
        {
            let _ = writeln!(log, "{}: {error}", jiff::Timestamp::now());
        }
    }
    result
}

fn serve() -> Result<(), String> {
    let path = config::default_path().ok_or("no config directory")?;
    let settings = Config::load(&path)
        .map_err(|error| error.to_string())?
        .applet::<IrcSettings>(TABLE)?;
    let bouncer = &settings.bouncer;
    let local = bouncer.mode == BouncerMode::Local;
    let password = if local {
        token()?
    } else {
        headless_password(&bouncer.password, "irc-bouncer", "bouncer")?
    };
    let passwords = settings
        .server
        .iter()
        .map(|server| {
            server
                .sasl
                .as_ref()
                .map(|account| headless_password(&server.password, &server.name, account))
                .transpose()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let listener = TcpListener::bind(&bouncer.listen)
        .map_err(|error| format!("{}: {error}", bouncer.listen))?;
    let tls = match (&bouncer.certificate, &bouncer.key) {
        (Some(certificate), Some(key)) => {
            Some(net::server_tls(certificate, key).map_err(|error| error.to_string())?)
        }
        _ => None,
    };
    let file = if local { LocalDaemon::file() } else { None };
    if let Some(file) = &file {
        let address = listener
            .local_addr()
            .map_err(|error| error.to_string())?
            .to_string();
        LocalDaemon {
            address,
            token: password.clone(),
            pid: std::process::id(),
        }
        .write(file)?;
    }
    let history = DiskHistory::new(data_directory().ok_or("no data directory")?.join("irc"));
    run::run(
        Daemon {
            servers: settings.server.clone(),
            passwords,
            password,
            history,
            tls,
        },
        listener,
    );
    if let Some(file) = file {
        let _ = fs::remove_file(file);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::PasswordSource;

    #[test]
    fn tokens_are_long_random_and_hexadecimal() {
        let first = token().unwrap();
        let second = token().unwrap();

        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }

    #[test]
    fn the_local_daemon_file_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oxmacs").join("irc-daemon.toml");
        let daemon = LocalDaemon {
            address: "127.0.0.1:41234".to_owned(),
            token: "abc".to_owned(),
            pid: 42,
        };

        daemon.write(&path).unwrap();

        assert_eq!(LocalDaemon::read(&path), Some(daemon));
        assert_eq!(LocalDaemon::read(&dir.path().join("missing.toml")), None);
    }

    #[test]
    fn waiting_succeeds_once_the_daemon_answers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("irc-daemon.toml");
        assert!(!wait_for(&path, Duration::from_millis(300)));

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        LocalDaemon {
            address: listener.local_addr().unwrap().to_string(),
            token: "t".to_owned(),
            pid: 1,
        }
        .write(&path)
        .unwrap();

        assert!(wait_for(&path, Duration::from_secs(5)));
    }

    #[test]
    fn the_daemon_cannot_ask_for_passwords() {
        assert_eq!(
            headless_password(&PasswordSource::Ask, "libera", "me"),
            Err("libera: a background bouncer cannot ask for a password; use the keychain, an environment variable or text".to_owned())
        );
        assert_eq!(
            headless_password(&PasswordSource::Text("pw".to_owned()), "libera", "me"),
            Ok("pw".to_owned())
        );
    }
}
