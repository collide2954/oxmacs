use std::env;

use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::irc::settings::{self, IrcServer, PasswordSource};
use crate::text::Buffer;
use crate::theme::Highlight;

const NAME: &str = "*IRC Setup*";
const HELP: &str = "TAB next field · RET change · C-c C-c save · C-c C-k cancel";
const LABEL: usize = 12;
const MARGIN: &str = "  ";
const PRESETS: [(&str, &str, &str); 3] = [
    ("Libera.Chat", "libera", "irc.libera.chat"),
    ("OFTC", "oftc", "irc.oftc.net"),
    ("EFnet", "efnet", "irc.efnet.org"),
];
const PASSWORDS: [&str; 3] = ["keychain", "ask", "env"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Network,
    Name,
    Host,
    Port,
    Tls,
    Nick,
    Account,
    Password,
    Variable,
    Channels,
    KeepAlive,
}

impl Field {
    fn label(self) -> &'static str {
        match self {
            Self::Network => "Network",
            Self::Name => "Name",
            Self::Host => "Host",
            Self::Port => "Port",
            Self::Tls => "TLS",
            Self::Nick => "Nick",
            Self::Account => "Account",
            Self::Password => "Password",
            Self::Variable => "Variable",
            Self::Channels => "Channels",
            Self::KeepAlive => "Keep alive",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Network => "RET picks Libera.Chat, OFTC, EFnet or another server",
            Self::Name => "What M-x irc calls this server",
            Self::Host => "The server's address",
            Self::Port => "6697 is the usual port for TLS",
            Self::Tls => "Encrypt the connection",
            Self::Nick => "Your nickname on the network",
            Self::Account => "Registered account to sign in with (SASL); empty for none",
            Self::Password => {
                "keychain: asked once and kept in the OS keychain · ask: every time · env: from a variable"
            }
            Self::Variable => "Environment variable that holds the password",
            Self::Channels => "Channels to join, separated by spaces, such as #emacs #rust",
            Self::KeepAlive => "Stay connected through the built-in bouncer when oxmacs closes",
        }
    }
}

#[derive(Debug)]
pub(in crate::editor) struct SetupForm {
    network: Option<usize>,
    name: String,
    host: String,
    port: u16,
    tls: bool,
    nick: String,
    account: String,
    password: usize,
    variable: String,
    channels: String,
    keep_alive: bool,
    rows: Vec<(usize, Field)>,
    styles: Vec<LineStyle>,
}

impl SetupForm {
    pub(in crate::editor) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }

    fn fields(&self) -> Vec<Field> {
        let mut fields = vec![
            Field::Network,
            Field::Name,
            Field::Host,
            Field::Port,
            Field::Tls,
            Field::Nick,
            Field::Account,
            Field::Password,
        ];
        if PASSWORDS[self.password] == "env" {
            fields.push(Field::Variable);
        }
        fields.extend([Field::Channels, Field::KeepAlive]);
        fields
    }

    fn value(&self, field: Field) -> String {
        let text = |text: &str| {
            if text.is_empty() {
                "(none)".to_owned()
            } else {
                text.to_owned()
            }
        };
        let check = |on: bool| if on { "[x]" } else { "[ ]" }.to_owned();
        match field {
            Field::Network => format!(
                "[{}]",
                self.network.map_or("Other", |index| PRESETS[index].0)
            ),
            Field::Name => text(&self.name),
            Field::Host => text(&self.host),
            Field::Port => self.port.to_string(),
            Field::Tls => check(self.tls),
            Field::Nick => text(&self.nick),
            Field::Account => text(&self.account),
            Field::Password => format!("[{}]", PASSWORDS[self.password]),
            Field::Variable => text(&self.variable),
            Field::Channels => text(&self.channels),
            Field::KeepAlive => check(self.keep_alive),
        }
    }

    fn editable(&self, field: Field) -> Option<String> {
        match field {
            Field::Name => Some(self.name.clone()),
            Field::Host => Some(self.host.clone()),
            Field::Port => Some(self.port.to_string()),
            Field::Nick => Some(self.nick.clone()),
            Field::Account => Some(self.account.clone()),
            Field::Variable => Some(self.variable.clone()),
            Field::Channels => Some(self.channels.clone()),
            Field::Network | Field::Tls | Field::Password | Field::KeepAlive => None,
        }
    }

    fn server(&self) -> IrcServer {
        let password = match PASSWORDS[self.password] {
            "ask" => PasswordSource::Ask,
            "env" => PasswordSource::Env(self.variable.trim().to_owned()),
            _ => PasswordSource::Keychain,
        };
        let channels = self
            .channels
            .split_whitespace()
            .map(|channel| {
                if channel.starts_with(['#', '&']) {
                    channel.to_owned()
                } else {
                    format!("#{channel}")
                }
            })
            .collect();
        let account = self.account.trim();
        IrcServer {
            name: self.name.trim().to_owned(),
            host: self.host.trim().to_owned(),
            port: self.port,
            tls: self.tls,
            nick: self.nick.trim().to_owned(),
            username: None,
            realname: None,
            channels,
            sasl: (!account.is_empty()).then(|| account.to_owned()),
            password,
        }
    }
}

fn default_nick() -> String {
    let user = env::var("USERNAME")
        .or_else(|_| env::var("USER"))
        .unwrap_or_default()
        .to_lowercase();
    let nick: String = user
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        .collect();
    if nick.starts_with(|c: char| c.is_ascii_alphabetic()) {
        nick
    } else {
        "oxmacs-user".to_owned()
    }
}

fn page(form: &SetupForm) -> (Page, Vec<(usize, Field)>) {
    let mut page = Page::default();
    page.heading("IRC Setup");
    page.line(&[(HELP, Some(Highlight::Comment))]);
    page.line(&[]);
    let mut rows = Vec::new();
    for field in form.fields() {
        rows.push((page.styles.len(), field));
        let label = format!("{MARGIN}{:<LABEL$}", field.label());
        let value = form.value(field);
        let style = if value == "(none)" {
            Highlight::Comment
        } else {
            Highlight::Link
        };
        page.line(&[(&label, None), (&value, Some(style))]);
    }
    (page, rows)
}

impl Editor {
    fn setup_index(&self) -> Option<usize> {
        self.buffers
            .find_mode(|mode| matches!(mode, Mode::IrcSetup(_)))
    }

    fn unique_server_name(&self, base: &str) -> String {
        let taken = |name: &str| {
            self.irc_settings()
                .server
                .iter()
                .any(|server| server.name == name)
        };
        if !taken(base) {
            return base.to_owned();
        }
        (2..=self.irc_settings().server.len() + 2)
            .map(|number| format!("{base}-{number}"))
            .find(|name| !taken(name))
            .unwrap_or_else(|| base.to_owned())
    }

    pub(in crate::editor) fn irc_setup(&mut self) {
        if let Some(index) = self.setup_index() {
            self.select_buffer(index);
            return;
        }
        let (_, name, host) = PRESETS[0];
        let form = SetupForm {
            network: Some(0),
            name: self.unique_server_name(name),
            host: host.to_owned(),
            port: 6697,
            tls: true,
            nick: default_nick(),
            account: String::new(),
            password: 0,
            variable: String::new(),
            channels: String::new(),
            keep_alive: false,
            rows: Vec::new(),
            styles: Vec::new(),
        };
        let mut buffer = Buffer::with_text(NAME, "");
        buffer.set_read_only(true);
        self.add_buffer_with(buffer, Mode::IrcSetup(Box::new(form)));
        self.render_setup();
        self.move_to_field(0);
    }

    fn setup_form(&self) -> Option<&SetupForm> {
        match self.buffers.mode(0)? {
            Mode::IrcSetup(form) => Some(form),
            _ => None,
        }
    }

    fn setup_form_mut(&mut self) -> Option<&mut SetupForm> {
        let index = self.setup_index()?;
        match self.buffers.mode_mut(index)? {
            Mode::IrcSetup(form) => Some(form),
            _ => None,
        }
    }

    fn render_setup(&mut self) {
        let Some(index) = self.setup_index() else {
            return;
        };
        let Some(Mode::IrcSetup(form)) = self.buffers.mode(index) else {
            return;
        };
        let (page, rows) = page(form);
        if let Some((buffer, Mode::IrcSetup(form))) = self.buffers.entry_mut(index) {
            page.fill(buffer);
            form.rows = rows;
            form.styles = page.styles;
        }
    }

    fn field_at_point(&self) -> Option<(usize, Field)> {
        let line = self.point_line();
        self.setup_form()?
            .rows
            .iter()
            .enumerate()
            .find(|(_, (row, _))| *row == line)
            .map(|(place, (_, field))| (place, *field))
    }

    fn move_to_field(&mut self, place: usize) {
        let Some((line, field)) = self
            .setup_form()
            .and_then(|form| form.rows.get(place).copied())
        else {
            return;
        };
        let buffer = self.buffers.current_mut();
        let start = buffer.line_start(line);
        buffer.set_point((start + MARGIN.len() + LABEL).min(buffer.line_end(line)));
        self.show_message(field.hint());
    }

    pub(in crate::editor) fn irc_setup_next_field(&mut self, forward: bool) {
        let Some(count) = self.setup_form().map(|form| form.rows.len()) else {
            return;
        };
        let line = self.point_line();
        let rows: Vec<usize> = self
            .setup_form()
            .map(|form| form.rows.iter().map(|(row, _)| *row).collect())
            .unwrap_or_default();
        let place = if forward {
            rows.iter().position(|row| *row > line).unwrap_or(0)
        } else {
            rows.iter()
                .rposition(|row| *row < line)
                .unwrap_or(count.saturating_sub(1))
        };
        self.move_to_field(place);
    }

    pub(in crate::editor) fn irc_setup_change(&mut self) {
        let Some((place, field)) = self.field_at_point() else {
            self.show_message("No field here");
            return;
        };
        let editable = self.setup_form().and_then(|form| form.editable(field));
        if let Some(text) = editable {
            self.prompt_with_text(
                &format!("{}: ", field.label()),
                &text,
                move |editor, input| {
                    editor.set_setup_field(field, &input);
                },
            );
            return;
        }
        let presets = PRESETS.len();
        let names: Vec<String> = PRESETS
            .iter()
            .map(|(_, name, _)| self.unique_server_name(name))
            .collect();
        if let Some(form) = self.setup_form_mut() {
            match field {
                Field::Network => {
                    let next = form.network.map_or(0, |index| index + 1);
                    if next < presets {
                        let (_, _, host) = PRESETS[next];
                        form.network = Some(next);
                        form.name.clone_from(&names[next]);
                        host.clone_into(&mut form.host);
                        form.port = 6697;
                        form.tls = true;
                    } else {
                        form.network = None;
                    }
                }
                Field::Tls => form.tls = !form.tls,
                Field::KeepAlive => form.keep_alive = !form.keep_alive,
                Field::Password => form.password = (form.password + 1) % PASSWORDS.len(),
                _ => {}
            }
        }
        self.render_setup();
        self.move_to_field(place);
    }

    fn set_setup_field(&mut self, field: Field, input: &str) {
        let input = input.trim();
        if field == Field::Port {
            match input.parse::<u16>() {
                Ok(port) if port > 0 => {
                    if let Some(form) = self.setup_form_mut() {
                        form.port = port;
                    }
                }
                _ => {
                    self.show_message("Port needs a number from 1 to 65535");
                    return;
                }
            }
        }
        if let Some(form) = self.setup_form_mut() {
            let text = input.to_owned();
            match field {
                Field::Name => form.name = text,
                Field::Host => {
                    if form.network.is_some_and(|index| PRESETS[index].2 != text) {
                        form.network = None;
                    }
                    form.host = text;
                }
                Field::Nick => form.nick = text,
                Field::Account => form.account = text,
                Field::Variable => form.variable = text,
                Field::Channels => form.channels = text,
                _ => {}
            }
        }
        self.render_setup();
        if let Some(place) = self
            .setup_form()
            .and_then(|form| form.rows.iter().position(|(_, known)| *known == field))
        {
            self.move_to_field(place);
        }
    }

    fn setup_problem(&self, server: &IrcServer, keep_alive: bool) -> Option<String> {
        if server.name.is_empty() {
            return Some("The server needs a name".to_owned());
        }
        if self
            .irc_settings()
            .server
            .iter()
            .any(|known| known.name == server.name)
        {
            return Some(format!("A server named {} already exists", server.name));
        }
        if server.host.is_empty() {
            return Some("The server needs a host".to_owned());
        }
        if server.nick.is_empty() {
            return Some("Pick a nick".to_owned());
        }
        if server.password == PasswordSource::Env(String::new()) {
            return Some("Name the environment variable that holds the password".to_owned());
        }
        if keep_alive && server.sasl.is_some() && server.password == PasswordSource::Ask {
            return Some(
                "The bouncer cannot ask for a password; keep it in the keychain or a variable"
                    .to_owned(),
            );
        }
        None
    }

    pub(in crate::editor) fn irc_setup_save(&mut self) {
        let Some((server, keep_alive)) = self
            .setup_form()
            .map(|form| (form.server(), form.keep_alive))
        else {
            return;
        };
        if let Some(problem) = self.setup_problem(&server, keep_alive) {
            self.show_message(problem);
            return;
        }
        if let Err(error) =
            self.edit_config_file(|document| settings::add_server(document, &server, keep_alive))
        {
            self.show_message(error);
            return;
        }
        if let Some(index) = self.setup_index() {
            self.finish_kill(index);
        }
        match &server.sasl {
            Some(account) if keep_alive && server.password == PasswordSource::Keychain => {
                let prompt = format!(
                    "Password for {account} on {}, kept in the keychain for the bouncer: ",
                    server.name
                );
                let name = server.name.clone();
                self.prompt_secret(&prompt, move |editor, password| {
                    editor.keep_irc_password(&name, &password);
                });
            }
            _ => self.ask_to_connect_irc(&server.name),
        }
    }

    fn keep_irc_password(&mut self, name: &str, password: &str) {
        let Some(server) = self
            .irc_settings()
            .server
            .iter()
            .find(|server| server.name == name)
            .cloned()
        else {
            return;
        };
        if let Some(account) = &server.sasl
            && let Err(problem) = (self.irc.keychain.write)(&server.name, account, password)
        {
            self.show_message(format!("Keychain: {problem}"));
            return;
        }
        self.ask_to_connect_irc(&server.name);
    }

    fn ask_to_connect_irc(&mut self, name: &str) {
        let name = name.to_owned();
        self.ask(&format!("Connect to {name} now?"), move |editor, yes| {
            if yes {
                editor.connect_named_irc(&name);
            }
        });
    }

    pub(in crate::editor) fn irc_setup_cancel(&mut self) {
        if let Some(index) = self.setup_index() {
            self.finish_kill(index);
        }
        self.show_message("IRC setup canceled");
    }

    pub(in crate::editor) fn connect_named_irc(&mut self, name: &str) {
        if let Some(server) = self
            .irc_settings()
            .server
            .iter()
            .find(|server| server.name == name)
            .cloned()
        {
            self.connect_irc(&server);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use crate::config::Config;
    use crate::editor::test_support::{lines, press, question, type_text};
    use crate::editor::{Editor, Effect};
    use crate::irc::settings::{BouncerMode, IrcSettings, PasswordSource, TABLE};

    fn kept(path: &Path) -> IrcSettings {
        Config::load(path).unwrap().applet(TABLE).unwrap()
    }

    fn opened(config: &str) -> (tempfile::TempDir, PathBuf, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, config).unwrap();
        let mut editor = Editor::new();
        editor.apply_config(&Config::parse(config).unwrap());
        editor.config_path = Some(path.clone());
        press(&mut editor, "M-x");
        type_text(&mut editor, "irc-setup");
        press(&mut editor, "RET");
        (dir, path, editor)
    }

    fn field(editor: &Editor, label: &str) -> String {
        lines(editor)
            .into_iter()
            .find(|line| line.trim_start().starts_with(label))
            .map(|line| line[14..].to_owned())
            .unwrap_or_default()
    }

    fn on_field(editor: &Editor, label: &str) -> bool {
        let buffer = editor.buffer();
        buffer
            .line_text(buffer.line_of(buffer.point()))
            .trim_start()
            .starts_with(label)
    }

    fn go_to(editor: &mut Editor, label: &str) {
        press(editor, "M-<");
        while !on_field(editor, label) {
            press(editor, "TAB");
        }
    }

    fn edit(editor: &mut Editor, label: &str, text: &str) {
        go_to(editor, label);
        press(editor, "RET C-a C-k");
        type_text(editor, text);
        press(editor, "RET");
    }

    #[test]
    fn m_x_irc_setup_opens_a_form_ready_for_libera() {
        let (_dir, _path, editor) = opened("");

        assert_eq!(editor.buffer().name(), "*IRC Setup*");
        assert_eq!(field(&editor, "Network"), "[Libera.Chat]");
        assert_eq!(field(&editor, "Name"), "libera");
        assert_eq!(field(&editor, "Host"), "irc.libera.chat");
        assert_eq!(field(&editor, "Port"), "6697");
        assert_eq!(field(&editor, "TLS"), "[x]");
        assert_ne!(field(&editor, "Nick"), "(none)");
        assert_eq!(field(&editor, "Password"), "[keychain]");
        assert_eq!(field(&editor, "Keep alive"), "[ ]");
        assert_eq!(
            editor.message(),
            Some("RET picks Libera.Chat, OFTC, EFnet or another server")
        );
    }

    #[test]
    fn ret_on_the_network_cycles_the_presets() {
        let (_dir, _path, mut editor) = opened("");

        press(&mut editor, "RET");

        assert_eq!(field(&editor, "Network"), "[OFTC]");
        assert_eq!(field(&editor, "Host"), "irc.oftc.net");
        assert_eq!(field(&editor, "Name"), "oftc");
        press(&mut editor, "RET RET");
        assert_eq!(field(&editor, "Network"), "[Other]");
    }

    #[test]
    fn text_fields_are_edited_in_the_minibuffer() {
        let (_dir, _path, mut editor) = opened("");
        press(&mut editor, "TAB TAB TAB TAB TAB");
        press(&mut editor, "RET");

        assert_eq!(editor.minibuffer().unwrap().prompt(), "Nick: ");
        press(&mut editor, "C-a C-k");
        type_text(&mut editor, "brett");
        press(&mut editor, "RET");

        assert_eq!(field(&editor, "Nick"), "brett");
        assert!(editor.minibuffer().is_none());
    }

    #[test]
    fn saving_writes_config_toml_and_offers_to_connect() {
        let (_dir, path, mut editor) = opened("theme = \"adwaita\"\n");
        edit(&mut editor, "Nick", "brett");
        edit(&mut editor, "Channels", "emacs #rust");

        press(&mut editor, "C-c C-c");

        let saved = Config::load(&path).unwrap();
        assert_eq!(saved.theme.as_deref(), Some("adwaita"));
        let irc = kept(&path);
        let server = &irc.server[0];
        assert_eq!(server.name, "libera");
        assert_eq!(server.nick, "brett");
        assert_eq!(server.sasl, None);
        assert_eq!(server.channels, ["#emacs", "#rust"]);
        assert_eq!(server.password, PasswordSource::Keychain);
        assert_eq!(editor.irc_settings().server, irc.server);
        assert!(
            editor
                .buffers
                .iter()
                .all(|buffer| buffer.name() != "*IRC Setup*")
        );
        assert_eq!(
            question(&editor).as_deref(),
            Some("Connect to libera now? (y or n) ")
        );

        press(&mut editor, "y");
        assert!(
            editor
                .take_effects()
                .iter()
                .any(|effect| matches!(effect, Effect::Connect(..)))
        );
    }

    #[test]
    fn an_account_signs_in_with_sasl() {
        let (_dir, path, mut editor) = opened("");
        edit(&mut editor, "Nick", "brett");
        edit(&mut editor, "Account", "brett");

        press(&mut editor, "C-c C-c");

        assert_eq!(kept(&path).server[0].sasl.as_deref(), Some("brett"));
    }

    #[test]
    fn keep_alive_turns_on_the_local_bouncer() {
        let (_dir, path, mut editor) = opened("");
        edit(&mut editor, "Nick", "brett");
        go_to(&mut editor, "Keep alive");
        press(&mut editor, "RET");

        press(&mut editor, "C-c C-c");

        assert_eq!(kept(&path).bouncer.mode, BouncerMode::Local);
        assert_eq!(editor.irc_settings().bouncer.mode, BouncerMode::Local);
    }

    std::thread_local! {
        static KEPT: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    #[test]
    fn keeping_alive_with_an_account_takes_the_password_now() {
        let (_dir, path, mut editor) = opened("");
        editor.irc.keychain.write = |server, account, password| {
            KEPT.with(|kept| {
                kept.borrow_mut()
                    .push(format!("{account}@{server}={password}"));
            });
            Ok(())
        };
        edit(&mut editor, "Nick", "brett");
        edit(&mut editor, "Account", "brett");
        go_to(&mut editor, "Keep alive");
        press(&mut editor, "RET C-c C-c");

        assert_eq!(kept(&path).server.len(), 1);
        let minibuffer = editor.minibuffer().unwrap();
        assert_eq!(
            minibuffer.prompt(),
            "Password for brett on libera, kept in the keychain for the bouncer: "
        );
        assert!(minibuffer.is_secret());
        type_text(&mut editor, "hunter2");
        press(&mut editor, "RET");

        KEPT.with(|kept| assert_eq!(*kept.borrow(), ["brett@libera=hunter2"]));
        assert_eq!(
            question(&editor).as_deref(),
            Some("Connect to libera now? (y or n) ")
        );
    }

    #[test]
    fn the_bouncer_cannot_ask_for_passwords() {
        let (_dir, path, mut editor) = opened("");
        edit(&mut editor, "Nick", "brett");
        edit(&mut editor, "Account", "brett");
        go_to(&mut editor, "Password");
        press(&mut editor, "RET");
        go_to(&mut editor, "Keep alive");
        press(&mut editor, "RET C-c C-c");

        assert_eq!(
            editor.message(),
            Some("The bouncer cannot ask for a password; keep it in the keychain or a variable")
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn env_passwords_need_a_variable() {
        let (_dir, path, mut editor) = opened("");
        edit(&mut editor, "Nick", "brett");
        go_to(&mut editor, "Password");
        press(&mut editor, "RET RET");
        assert_eq!(field(&editor, "Password"), "[env]");

        press(&mut editor, "C-c C-c");
        assert_eq!(
            editor.message(),
            Some("Name the environment variable that holds the password")
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "");

        edit(&mut editor, "Variable", "LIBERA_PASSWORD");
        press(&mut editor, "C-c C-c");
        assert_eq!(
            kept(&path).server[0].password,
            PasswordSource::Env("LIBERA_PASSWORD".to_owned())
        );
    }

    #[test]
    fn taken_names_get_a_number_and_cannot_be_reused() {
        let existing =
            "[[irc-server]]\nname = \"libera\"\nhost = \"irc.libera.chat\"\nnick = \"old\"\n";
        let (_dir, _path, mut editor) = opened(existing);
        assert_eq!(field(&editor, "Name"), "libera-2");

        edit(&mut editor, "Name", "libera");
        press(&mut editor, "C-c C-c");

        assert_eq!(
            editor.message(),
            Some("A server named libera already exists")
        );
    }

    #[test]
    fn canceling_leaves_config_toml_alone() {
        let (_dir, path, mut editor) = opened("");

        press(&mut editor, "C-c C-k");

        assert_eq!(editor.message(), Some("IRC setup canceled"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
        assert!(
            editor
                .buffers
                .iter()
                .all(|buffer| buffer.name() != "*IRC Setup*")
        );
    }
}
