use crate::editor::chat::Link;
use crate::editor::{ConnectionSpec, Editor, Effect, ProcessSpec};
use crate::external::net::Endpoint;
use crate::irc::network::{Network, Target};
use crate::irc::session::{Identity, Sasl, Session};
use crate::irc::settings::{BouncerMode, IrcServer, PasswordSource};

const BOUNCER_NAME: &str = "irc-bouncer";
const BOUNCER_ACCOUNT: &str = "bouncer";
const STARTING: &str = "Starting the oxmacs bouncer…";
const NOT_STARTED: &str =
    "The oxmacs bouncer did not start; irc-daemon.log in the oxmacs data folder says why";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Secret {
    Sasl,
    Bouncer,
}

enum Route {
    Direct(Option<String>),
    Bouncer(Endpoint, String),
}

fn daemon(pid: Option<u32>) -> ProcessSpec {
    ProcessSpec {
        kind: "bouncer",
        name: "irc-daemon".to_owned(),
        detail: "oxmacs irc-daemon".to_owned(),
        pid,
        stop: Some(Editor::irc_stop_daemon),
    }
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
        match self.irc_settings().bouncer.mode {
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
                let source = self.irc_settings().bouncer.password.clone();
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
            PasswordSource::Env(variable) => {
                if let Ok(password) = std::env::var(variable) {
                    self.continue_irc(server, secret, password);
                } else {
                    let prompt =
                        format!("{variable} is not set; password for {account} on {name}: ");
                    self.ask_irc_password(server, secret, &prompt, false);
                }
            }
            PasswordSource::Ask => {
                let prompt = format!("Password for {account} on {name}: ");
                self.ask_irc_password(server, secret, &prompt, false);
            }
            PasswordSource::Keychain => {
                let prompt = format!("Password for {account} on {name}: ");
                match (self.irc.keychain.read)(name, account) {
                    Ok(Some(password)) => self.continue_irc(server, secret, password),
                    Ok(None) => self.ask_irc_password(server, secret, &prompt, true),
                    Err(problem) => {
                        self.show_message(format!("Keychain: {problem}"));
                        self.ask_irc_password(server, secret, &prompt, false);
                    }
                }
            }
        }
    }

    fn ask_irc_password(
        &mut self,
        server: &IrcServer,
        secret: Secret,
        prompt: &str,
        remember: bool,
    ) {
        let name = server.name.clone();
        self.prompt_secret(prompt, move |editor, password| {
            editor.irc_password_entered(&name, secret, remember, password);
        });
    }

    fn irc_password_entered(
        &mut self,
        name: &str,
        secret: Secret,
        remember: bool,
        password: String,
    ) {
        let Some(server) = self
            .irc_settings()
            .server
            .iter()
            .find(|server| server.name == name)
            .cloned()
        else {
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
                let bouncer = &self.irc_settings().bouncer;
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
        let route = if via_bouncer { " via the bouncer" } else { "" };
        let detail = format!("{}:{}{route}", endpoint.host, endpoint.port);
        let note = if via_bouncer {
            format!(
                "Connecting through the bouncer at {}:{}…",
                endpoint.host, endpoint.port
            )
        } else {
            format!("Connecting to {}:{}…", endpoint.host, endpoint.port)
        };
        let id = self.connect(
            endpoint,
            ConnectionSpec {
                kind: "IRC",
                name: server.name.clone(),
                detail,
                events: |editor, id, event| editor.irc_event(id, event),
                visit: |editor, id| editor.visit_irc(id),
                stop: |editor, id| editor.irc_disconnect(id),
            },
        );
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
        if via_bouncer && self.irc_settings().bouncer.mode == BouncerMode::Local {
            let pid = (self.irc.local_daemon)().map(|daemon| daemon.pid);
            self.track_process(daemon(pid));
        }
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
        self.track_process(daemon(pid));
        for name in waiting {
            let server = self
                .irc_settings()
                .server
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
