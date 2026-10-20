use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const TABLE: &str = "irc";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct IrcServer {
    pub name: String,
    pub host: String,
    #[serde(default = "irc_port")]
    pub port: u16,
    #[serde(default = "enabled")]
    pub tls: bool,
    pub nick: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realname: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sasl: Option<String>,
    #[serde(default)]
    pub password: PasswordSource,
}

impl IrcServer {
    pub fn username(&self) -> &str {
        self.username.as_deref().unwrap_or(&self.nick)
    }

    pub fn realname(&self) -> &str {
        self.realname.as_deref().unwrap_or("oxmacs")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct IrcBouncer {
    pub mode: BouncerMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    pub tls: bool,
    pub password: PasswordSource,
    pub listen: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<PathBuf>,
}

impl Default for IrcBouncer {
    fn default() -> Self {
        Self {
            mode: BouncerMode::Off,
            address: None,
            tls: true,
            password: PasswordSource::Keychain,
            listen: "127.0.0.1:0".to_owned(),
            certificate: None,
            key: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BouncerMode {
    #[default]
    Off,
    Local,
    Remote,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PasswordSource {
    #[default]
    Keychain,
    Ask,
    Env(String),
    Text(String),
}

fn irc_port() -> u16 {
    6697
}

fn enabled() -> bool {
    true
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct Unchecked {
    server: Vec<IrcServer>,
    bouncer: IrcBouncer,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "Unchecked", into = "Unchecked")]
pub struct IrcSettings {
    pub server: Vec<IrcServer>,
    pub bouncer: IrcBouncer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem(String);

impl fmt::Display for Problem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<Unchecked> for IrcSettings {
    type Error = Problem;

    fn try_from(unchecked: Unchecked) -> Result<Self, Problem> {
        if unchecked.bouncer.mode == BouncerMode::Remote && unchecked.bouncer.address.is_none() {
            return Err(Problem(
                "bouncer mode = \"remote\" needs an address".to_owned(),
            ));
        }
        for (index, server) in unchecked.server.iter().enumerate() {
            if server.nick.trim().is_empty() {
                return Err(Problem(format!("{} needs a nick", server.name)));
            }
            if unchecked.server[..index]
                .iter()
                .any(|earlier| earlier.name == server.name)
            {
                return Err(Problem(format!("two servers are named {}", server.name)));
            }
        }
        Ok(Self {
            server: unchecked.server,
            bouncer: unchecked.bouncer,
        })
    }
}

impl From<IrcSettings> for Unchecked {
    fn from(settings: IrcSettings) -> Self {
        Self {
            server: settings.server,
            bouncer: settings.bouncer,
        }
    }
}

fn server_table(server: &IrcServer) -> Result<toml_edit::Table, String> {
    let fields = toml::to_string(server).map_err(|error| error.to_string())?;
    let parsed: toml_edit::DocumentMut = fields
        .parse()
        .map_err(|error: toml_edit::TomlError| error.message().to_owned())?;
    let mut table = parsed.as_table().clone();
    table.set_implicit(false);
    if let Some(password) = table.remove("password") {
        let value = match password {
            toml_edit::Item::Table(inner) => {
                toml_edit::Value::InlineTable(inner.into_inline_table())
            }
            other => other
                .into_value()
                .map_err(|_| "the password source is not a value".to_owned())?,
        };
        table.insert("password", toml_edit::Item::Value(value));
    }
    Ok(table)
}

pub fn add_server(
    document: &mut toml_edit::DocumentMut,
    server: &IrcServer,
    keep_alive: bool,
) -> Result<(), String> {
    let table = server_table(server)?;
    let irc = document
        .entry(TABLE)
        .or_insert_with(|| {
            let mut table = toml_edit::Table::new();
            table.set_implicit(true);
            toml_edit::Item::Table(table)
        })
        .as_table_mut()
        .ok_or("[irc] is not a table")?;
    match irc.get_mut("server") {
        Some(toml_edit::Item::ArrayOfTables(servers)) => servers.push(table),
        Some(_) => return Err("[irc] server is not a list of servers".to_owned()),
        None => {
            let mut servers = toml_edit::ArrayOfTables::new();
            servers.push(table);
            irc.insert("server", toml_edit::Item::ArrayOfTables(servers));
        }
    }
    if keep_alive {
        let bouncer = irc
            .entry("bouncer")
            .or_insert_with(toml_edit::table)
            .as_table_mut()
            .ok_or("[irc] bouncer is not a table")?;
        bouncer.insert("mode", toml_edit::value("local"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::config::Config;

    fn irc(text: &str) -> Result<IrcSettings, String> {
        Config::parse(text)
            .map_err(|error| error.to_string())?
            .applet::<IrcSettings>(TABLE)
    }

    #[test]
    fn servers_are_tables_with_sensible_defaults() {
        let settings = irc(r##"
[[irc.server]]
name = "libera"
host = "irc.libera.chat"
nick = "oxtest"
channels = ["#oxmacs-test"]
sasl = "oxtest"

[[irc.server]]
name = "local"
host = "localhost"
port = 6667
tls = false
nick = "me"
password = { env = "LOCAL_IRC" }
"##)
        .unwrap();

        let libera = &settings.server[0];
        assert_eq!(libera.port, 6697);
        assert!(libera.tls);
        assert_eq!(libera.username(), "oxtest");
        assert_eq!(libera.realname(), "oxmacs");
        assert_eq!(libera.channels, ["#oxmacs-test"]);
        assert_eq!(libera.password, PasswordSource::Keychain);

        let local = &settings.server[1];
        assert_eq!((local.port, local.tls), (6667, false));
        assert_eq!(local.password, PasswordSource::Env("LOCAL_IRC".to_owned()));
        assert_eq!(local.sasl, None);
    }

    #[test]
    fn the_old_irc_keys_still_work() {
        let settings = irc(
            "[[irc-server]]\nname = \"n\"\nhost = \"h\"\nnick = \"x\"\n\n[irc-bouncer]\nmode = \"local\"\n",
        )
        .unwrap();

        assert_eq!(settings.server[0].name, "n");
        assert_eq!(settings.bouncer.mode, BouncerMode::Local);
    }

    #[test]
    fn password_sources_are_spelled_plainly() {
        let read = |password: &str| {
            irc(&format!(
                "[[irc.server]]\nname = \"n\"\nhost = \"h\"\nnick = \"x\"\npassword = {password}\n"
            ))
            .map(|settings| settings.server[0].password.clone())
        };

        assert_eq!(read("\"ask\"").unwrap(), PasswordSource::Ask);
        assert_eq!(read("\"keychain\"").unwrap(), PasswordSource::Keychain);
        assert_eq!(
            read("{ text = \"hunter2\" }").unwrap(),
            PasswordSource::Text("hunter2".to_owned())
        );
        assert!(read("\"sometimes\"").is_err());
    }

    #[test]
    fn the_bouncer_is_off_unless_configured() {
        let bouncer = irc("").unwrap().bouncer;
        assert_eq!(bouncer.mode, BouncerMode::Off);
        assert_eq!(bouncer.listen, "127.0.0.1:0");

        let bouncer = irc(
            "[irc.bouncer]\nmode = \"remote\"\naddress = \"bnc.example.org:6697\"\npassword = \"ask\"\n",
        )
        .unwrap()
        .bouncer;
        assert_eq!(bouncer.mode, BouncerMode::Remote);
        assert_eq!(bouncer.address.as_deref(), Some("bnc.example.org:6697"));
        assert!(bouncer.tls);
        assert_eq!(bouncer.password, PasswordSource::Ask);

        let daemon = irc(
            "[irc.bouncer]\nlisten = \"0.0.0.0:6697\"\ncertificate = \"cert.pem\"\nkey = \"key.pem\"\n",
        )
        .unwrap()
        .bouncer;
        assert_eq!(daemon.listen, "0.0.0.0:6697");
        assert_eq!(daemon.certificate.as_deref(), Some(Path::new("cert.pem")));
    }

    fn libera(password: PasswordSource) -> IrcServer {
        IrcServer {
            name: "libera".to_owned(),
            host: "irc.libera.chat".to_owned(),
            port: 6697,
            tls: true,
            nick: "brett".to_owned(),
            username: None,
            realname: None,
            channels: vec!["#emacs".to_owned(), "#rust".to_owned()],
            sasl: Some("brett".to_owned()),
            password,
        }
    }

    fn added(text: &str, server: &IrcServer, keep_alive: bool) -> String {
        let mut document: toml_edit::DocumentMut = text.parse().unwrap();
        add_server(&mut document, server, keep_alive).unwrap();
        document.to_string()
    }

    #[test]
    fn servers_are_added_without_disturbing_the_file() {
        let text = "# my settings\ntheme = \"adwaita\" # light\n";

        let added = added(text, &libera(PasswordSource::Keychain), false);

        assert_eq!(
            added,
            "# my settings\ntheme = \"adwaita\" # light\n\n[[irc.server]]\nname = \"libera\"\nhost = \"irc.libera.chat\"\nport = 6697\ntls = true\nnick = \"brett\"\nchannels = [\"#emacs\", \"#rust\"]\nsasl = \"brett\"\npassword = \"keychain\"\n"
        );
        let settings = irc(&added).unwrap();
        assert_eq!(settings.server, [libera(PasswordSource::Keychain)]);
        assert_eq!(settings.bouncer, IrcBouncer::default());
    }

    #[test]
    fn added_servers_can_turn_on_the_local_bouncer_and_read_env_passwords() {
        let text = "[irc.bouncer]\ntls = true\n";
        let server = libera(PasswordSource::Env("LIBERA_PASSWORD".to_owned()));

        let added = added(text, &server, true);

        let settings = irc(&added).unwrap();
        assert_eq!(settings.bouncer.mode, BouncerMode::Local);
        assert_eq!(settings.server, [server]);
        assert!(
            added.contains("password = { env = \"LIBERA_PASSWORD\" }"),
            "{added}"
        );
        let again = self::added(
            &added,
            &IrcServer {
                name: "oftc".to_owned(),
                host: "irc.oftc.net".to_owned(),
                ..libera(PasswordSource::Ask)
            },
            false,
        );
        assert_eq!(irc(&again).unwrap().server.len(), 2);
        assert_eq!(
            irc(&self::added(&again, &libera(PasswordSource::Ask), false)).unwrap_err(),
            "[irc] two servers are named libera"
        );
    }

    #[test]
    fn remote_bouncers_need_an_address() {
        assert_eq!(
            irc("[irc.bouncer]\nmode = \"remote\"\n").unwrap_err(),
            "[irc] bouncer mode = \"remote\" needs an address"
        );
    }

    #[test]
    fn servers_need_distinct_names_and_a_nick() {
        assert_eq!(
            irc("[[irc.server]]\nname = \"a\"\nhost = \"h\"\nnick = \"x\"\n[[irc.server]]\nname = \"a\"\nhost = \"h\"\nnick = \"y\"\n")
                .unwrap_err(),
            "[irc] two servers are named a"
        );
        assert_eq!(
            irc("[[irc.server]]\nname = \"a\"\nhost = \"h\"\nnick = \"\"\n").unwrap_err(),
            "[irc] a needs a nick"
        );
    }
}
