#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub tags: Vec<(String, Option<String>)>,
    pub source: Option<String>,
    pub command: String,
    pub params: Vec<String>,
}

impl Message {
    pub fn new<P: Into<String>>(command: &str, params: impl IntoIterator<Item = P>) -> Self {
        Self {
            tags: Vec::new(),
            source: None,
            command: command.to_ascii_uppercase(),
            params: params.into_iter().map(Into::into).collect(),
        }
    }

    #[must_use]
    pub fn with_tag(mut self, key: &str, value: Option<&str>) -> Self {
        self.tags.push((key.to_owned(), value.map(str::to_owned)));
        self
    }

    pub fn parse(line: &str) -> Option<Self> {
        let mut rest = line.trim_end_matches(['\r', '\n']).trim_start_matches(' ');
        let mut tags = Vec::new();
        if let Some(after) = rest.strip_prefix('@') {
            let (raw, remainder) = after.split_once(' ')?;
            tags = raw
                .split(';')
                .filter(|tag| !tag.is_empty())
                .map(|tag| match tag.split_once('=') {
                    Some((key, value)) => (key.to_owned(), Some(unescape(value))),
                    None => (tag.to_owned(), None),
                })
                .collect();
            rest = remainder.trim_start_matches(' ');
        }
        let mut source = None;
        if let Some(after) = rest.strip_prefix(':') {
            let (raw, remainder) = after.split_once(' ')?;
            source = Some(raw.to_owned());
            rest = remainder.trim_start_matches(' ');
        }
        let (command, mut rest) = rest.split_once(' ').unwrap_or((rest, ""));
        if command.is_empty() {
            return None;
        }
        let mut params = Vec::new();
        loop {
            rest = rest.trim_start_matches(' ');
            if rest.is_empty() {
                break;
            }
            if let Some(trailing) = rest.strip_prefix(':') {
                params.push(trailing.to_owned());
                break;
            }
            let (param, remainder) = rest.split_once(' ').unwrap_or((rest, ""));
            params.push(param.to_owned());
            rest = remainder;
        }
        Some(Self {
            tags,
            source,
            command: command.to_ascii_uppercase(),
            params,
        })
    }

    pub fn to_line(&self) -> String {
        let mut line = String::new();
        if !self.tags.is_empty() {
            let tags: Vec<String> = self
                .tags
                .iter()
                .map(|(key, value)| match value {
                    Some(value) => format!("{key}={}", escape(value)),
                    None => key.clone(),
                })
                .collect();
            line.push('@');
            line.push_str(&tags.join(";"));
            line.push(' ');
        }
        if let Some(source) = &self.source {
            line.push(':');
            line.push_str(source);
            line.push(' ');
        }
        line.push_str(&self.command);
        for (index, param) in self.params.iter().enumerate() {
            let param = param.replace(['\r', '\n'], " ");
            line.push(' ');
            let last = index + 1 == self.params.len();
            if last && (param.is_empty() || param.contains(' ') || param.starts_with(':')) {
                line.push(':');
            }
            line.push_str(&param);
        }
        line
    }

    pub fn tag(&self, key: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_deref().unwrap_or_default())
    }

    pub fn has_tag(&self, key: &str) -> bool {
        self.tags.iter().any(|(name, _)| name == key)
    }

    pub fn nick(&self) -> Option<&str> {
        self.source
            .as_deref()
            .and_then(|source| source.split(['!', '@']).next())
    }

    pub fn param(&self, index: usize) -> &str {
        self.params.get(index).map_or("", String::as_str)
    }
}

fn unescape(value: &str) -> String {
    let mut unescaped = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            unescaped.push(c);
            continue;
        }
        match chars.next() {
            Some(':') => unescaped.push(';'),
            Some('s') => unescaped.push(' '),
            Some('r') => unescaped.push('\r'),
            Some('n') => unescaped.push('\n'),
            Some(other) => unescaped.push(other),
            None => {}
        }
    }
    unescaped
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            ';' => escaped.push_str("\\:"),
            ' ' => escaped.push_str("\\s"),
            '\\' => escaped.push_str("\\\\"),
            '\r' => escaped.push_str("\\r"),
            '\n' => escaped.push_str("\\n"),
            other => escaped.push(other),
        }
    }
    escaped
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_messages_split_into_source_command_and_params() {
        let message = Message::parse(":alice!a@host PRIVMSG #rust :hello there").unwrap();

        assert_eq!(message.source.as_deref(), Some("alice!a@host"));
        assert_eq!(message.nick(), Some("alice"));
        assert_eq!(message.command, "PRIVMSG");
        assert_eq!(message.params, ["#rust", "hello there"]);
        assert_eq!(message.tags, Vec::new());
    }

    #[test]
    fn tags_are_unescaped() {
        let message = Message::parse(
            "@time=2026-10-19T12:00:00.000Z;msgid=abc;+typing=active;bot;note=a\\sb\\:c\\\\d\\r\\ne :srv NOTICE * :hi",
        )
        .unwrap();

        assert_eq!(message.tag("time"), Some("2026-10-19T12:00:00.000Z"));
        assert_eq!(message.tag("+typing"), Some("active"));
        assert_eq!(message.tag("bot"), Some(""));
        assert!(message.has_tag("bot"));
        assert_eq!(message.tag("note"), Some("a b;c\\d\r\ne"));
        assert_eq!(message.tag("missing"), None);
    }

    #[test]
    fn odd_escapes_and_empty_values_follow_the_spec() {
        let message = Message::parse("@a=x\\y\\;b=;c=end\\ PING").unwrap();

        assert_eq!(message.tag("a"), Some("xy"));
        assert_eq!(message.tag("b"), Some(""));
        assert_eq!(message.tag("c"), Some("end"));
    }

    #[test]
    fn commands_are_uppercased_and_numerics_kept() {
        assert_eq!(Message::parse("ping :x").unwrap().command, "PING");
        assert_eq!(
            Message::parse(":srv 001 me :Welcome").unwrap().params,
            ["me", "Welcome"]
        );
    }

    #[test]
    fn extra_spaces_and_line_endings_are_tolerated() {
        let message = Message::parse(":srv  MODE   #c  +o   bob\r\n").unwrap();

        assert_eq!(message.params, ["#c", "+o", "bob"]);
    }

    #[test]
    fn empty_and_broken_lines_are_rejected() {
        assert_eq!(Message::parse(""), None);
        assert_eq!(Message::parse("   "), None);
        assert_eq!(Message::parse(":onlysource"), None);
        assert_eq!(Message::parse("@tags-only"), None);
    }

    #[test]
    fn lines_are_written_with_a_trailing_param_when_needed() {
        assert_eq!(Message::new("JOIN", ["#rust"]).to_line(), "JOIN #rust");
        assert_eq!(
            Message::new("PRIVMSG", ["#rust", "hello there"]).to_line(),
            "PRIVMSG #rust :hello there"
        );
        assert_eq!(
            Message::new("PRIVMSG", ["#rust", ":)"]).to_line(),
            "PRIVMSG #rust ::)"
        );
        assert_eq!(Message::new("AWAY", [""]).to_line(), "AWAY :");
        assert_eq!(Message::new("QUIT", Vec::<String>::new()).to_line(), "QUIT");
    }

    #[test]
    fn client_tags_are_escaped_when_written() {
        let message = Message::new("TAGMSG", ["#rust"])
            .with_tag("+typing", Some("active"))
            .with_tag("+note", Some("a b;c"))
            .with_tag("+flag", None);

        assert_eq!(
            message.to_line(),
            "@+typing=active;+note=a\\sb\\:c;+flag TAGMSG #rust"
        );
    }

    #[test]
    fn writing_then_parsing_round_trips() {
        let original =
            Message::new("PRIVMSG", ["#c", "multi word :text"]).with_tag("+x", Some("1\\2"));

        assert_eq!(Message::parse(&original.to_line()).unwrap(), original);
    }

    #[test]
    fn line_breaks_never_reach_the_wire() {
        assert_eq!(
            Message::new("PRIVMSG", ["#c", "one\r\ntwo"]).to_line(),
            "PRIVMSG #c :one  two"
        );
    }
}
