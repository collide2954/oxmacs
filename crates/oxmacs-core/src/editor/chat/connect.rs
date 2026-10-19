use crate::config::{BouncerMode, IrcServer, PasswordSource};
use crate::editor::chat::Link;
use crate::editor::prompts::minibuffer::Purpose;
use crate::editor::{ConnectionId, Editor, Effect};
use crate::external::net::Endpoint;
use crate::irc::network::{Network, Target};
use crate::irc::session::{Identity, Sasl, Session};

const BOUNCER_NAME: &str = "irc-bouncer";
const BOUNCER_ACCOUNT: &str = "bouncer";
const STARTING: &str = "Starting the oxmacs bouncer…";
const NOT_STARTED: &str =
    "The oxmacs bouncer did not start; irc-daemon.log in the oxmacs data folder says why";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::editor) enum Secret {
    Sasl,
    Bouncer,
}

enum Route {
    Direct(Option<String>),
    Bouncer(Endpoint, String),
}

pub(in crate::editor) fn endpoint(address: &str, tls: bool) -> Option<Endpoint> {
    let (host, port) = address.rsplit_once(':')?;
    Some(Endpoint {
        host: host.trim_matches(['[', ']']).to_owned(),
        port: port.parse().ok()?,
        tls,
    })
}

impl Editor {
    pub(in crate::editor) fn connect_irc(&mut self, server: &IrcServer) {
        let open = self
            .irc
            .links
            .iter()
            .any(|link| link.server.name == server.name && link.open);
        if open {
            self.show_chat(&server.name, &Target::Server);
            return;
        }
        match self.config.irc_bouncer.mode {
            BouncerMode::Off => match server.sasl.clone() {
                Some(account) => {
                    let source = server.password.clone();
                    let name = server.name.clone();
                    self.with_password(server, Secret::Sasl, &source, &name, &account);
                }
                None => self.start_irc(server, Route::Direct(None)),
            },
            BouncerMode::Local => match (self.irc.local_daemon)() {
                Some(daemon) => match endpoint(&daemon.address, false) {
                    Some(endpoint) => {
                        self.start_irc(server, Route::Bouncer(endpoint, daemon.token));
                    }
                    None => self.show_message(format!(
                        "The bouncer reported an odd address: {}",
                        daemon.address
                    )),
                },
                None => self.launch_daemon(server),
            },
            BouncerMode::Remote => {
                let source = self.config.irc_bouncer.password.clone();
                self.with_password(
                    server,
                    Secret::Bouncer,
                    &source,
                    BOUNCER_NAME,
                    BOUNCER_ACCOUNT,
                );
            }
        }
    }

    fn with_password(
        &mut self,
        server: &IrcServer,
        secret: Secret,
        source: &PasswordSource,
        name: &str,
        account: &str,
    ) {
        match source {
            PasswordSource::Text(text) => self.continue_irc(server, secret, text.clone()),
            PasswordSource::Env(variable) => match std::env::var(variable) {
                Ok(password) => self.continue_irc(server, secret, password),
                Err(_) => {
                    self.show_message(format!("Set {variable} to the password for {name}"));
                }
            },
            PasswordSource::Ask => self.ask_irc_password(server, secret, name, account, false),
            PasswordSource::Keychain => match (self.irc.keychain.read)(name, account) {
                Ok(Some(password)) => self.continue_irc(server, secret, password),
                Ok(None) => self.ask_irc_password(server, secret, name, account, true),
                Err(problem) => {
                    self.show_message(format!("Keychain: {problem}"));
                    self.ask_irc_password(server, secret, name, account, false);
                }
            },
        }
    }

    fn ask_irc_password(
        &mut self,
        server: &IrcServer,
        secret: Secret,
        name: &str,
        account: &str,
        remember: bool,
    ) {
        let Some(index) = self
            .config
            .irc_server
            .iter()
            .position(|known| known.name == server.name)
        else {
            return;
        };
        let prompt = format!("Password for {account} on {name}: ");
        self.open_minibuffer(&prompt, Purpose::IrcPassword(index, secret, remember));
    }

    pub(in crate::editor) fn irc_password_entered(
        &mut self,
        index: usize,
        secret: Secret,
        remember: bool,
        password: String,
    ) {
        let Some(server) = self.config.irc_server.get(index).cloned() else {
            return;
        };
        let (name, account) = match secret {
            Secret::Sasl => (server.name.clone(), server.sasl.clone().unwrap_or_default()),
            Secret::Bouncer => (BOUNCER_NAME.to_owned(), BOUNCER_ACCOUNT.to_owned()),
        };
        if remember && let Err(problem) = (self.irc.keychain.write)(&name, &account, &password) {
            self.show_message(format!("Keychain: {problem}"));
        }
        self.continue_irc(&server, secret, password);
    }

    fn continue_irc(&mut self, server: &IrcServer, secret: Secret, password: String) {
        match secret {
            Secret::Sasl => self.start_irc(server, Route::Direct(Some(password))),
            Secret::Bouncer => {
                let bouncer = &self.config.irc_bouncer;
                match bouncer
                    .address
                    .as_deref()
                    .and_then(|address| endpoint(address, bouncer.tls))
                {
                    Some(endpoint) => self.start_irc(server, Route::Bouncer(endpoint, password)),
                    None => self.show_message("irc-bouncer: address should look like host:port"),
                }
            }
        }
    }

    fn start_irc(&mut self, server: &IrcServer, route: Route) {
        let (endpoint, identity, via_bouncer) = match route {
            Route::Direct(password) => (
                Endpoint {
                    host: server.host.clone(),
                    port: server.port,
                    tls: server.tls,
                },
                Identity {
                    nick: server.nick.clone(),
                    username: server.username().to_owned(),
                    realname: server.realname().to_owned(),
                    password: None,
                    sasl: server
                        .sasl
                        .clone()
                        .zip(password)
                        .map(|(account, password)| Sasl { account, password }),
                },
                false,
            ),
            Route::Bouncer(endpoint, password) => (
                endpoint,
                Identity {
                    nick: server.nick.clone(),
                    username: format!("{}/{}", server.username(), server.name),
                    realname: server.realname().to_owned(),
                    password: Some(password),
                    sasl: None,
                },
                true,
            ),
        };
        let id = ConnectionId(self.irc.next);
        self.irc.next += 1;
        let session = Session::new(identity);
        let existing = self
            .irc
            .links
            .iter()
            .position(|link| link.server.name == server.name);
        if let Some(index) = existing {
            let link = &mut self.irc.links[index];
            link.id = id;
            link.session = session;
            link.server = server.clone();
            link.open = true;
            link.connected = false;
            link.via_bouncer = via_bouncer;
        } else {
            self.irc.links.push(Link {
                id,
                server: server.clone(),
                session,
                network: Network::new(&server.name),
                open: true,
                connected: false,
                via_bouncer,
            });
        }
        let note = if via_bouncer {
            format!(
                "Connecting through the bouncer at {}:{}…",
                endpoint.host, endpoint.port
            )
        } else {
            format!("Connecting to {}:{}…", endpoint.host, endpoint.port)
        };
        let route = if via_bouncer { " via the bouncer" } else { "" };
        self.track_connection(
            id,
            &server.name,
            format!("{}:{}{route}", endpoint.host, endpoint.port),
        );
        if via_bouncer && self.config.irc_bouncer.mode == BouncerMode::Local {
            let pid = (self.irc.local_daemon)().map(|daemon| daemon.pid);
            self.track_daemon(pid, "oxmacs irc-daemon".to_owned());
        }
        self.effects.push(Effect::Connect(id, endpoint));
        self.irc_note(&server.name, &note, false);
        self.show_chat(&server.name, &Target::Server);
    }

    pub(in crate::editor) fn launch_daemon(&mut self, server: &IrcServer) {
        if !self.irc.waiting.contains(&server.name) {
            self.irc.waiting.push(server.name.clone());
        }
        if !self.irc.launching {
            self.irc.launching = true;
            self.effects.push(Effect::LaunchDaemon);
        }
        self.show_message(STARTING);
    }

    pub fn irc_daemon_ready(&mut self, ready: bool) {
        self.irc.launching = false;
        let waiting = std::mem::take(&mut self.irc.waiting);
        if !ready {
            self.show_message(NOT_STARTED);
            return;
        }
        let pid = (self.irc.local_daemon)().map(|daemon| daemon.pid);
        self.track_daemon(pid, "oxmacs irc-daemon".to_owned());
        for name in waiting {
            let server = self
                .config
                .irc_server
                .iter()
                .find(|server| server.name == name)
                .cloned();
            if let Some(server) = server {
                self.connect_irc(&server);
            }
        }
    }

    pub(in crate::editor) fn irc_stop_daemon(&mut self) {
        let Some(index) = self
            .irc
            .links
            .iter()
            .position(|link| link.via_bouncer && link.open)
        else {
            self.show_message("Not connected through a bouncer");
            return;
        };
        self.irc.links[index].session.send_raw("OXMACS STOP");
        self.irc_flush(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_split_into_host_and_port() {
        assert_eq!(
            endpoint("bnc.example.org:6697", true),
            Some(Endpoint {
                host: "bnc.example.org".to_owned(),
                port: 6697,
                tls: true
            })
        );
        assert_eq!(endpoint("[::1]:6667", false).unwrap().host, "::1");
        assert_eq!(endpoint("no-port", true), None);
    }
}
