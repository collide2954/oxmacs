use crate::command::Command;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::irc::message::Message;
use crate::irc::network::Target;

pub(super) const HELP: &str = "Commands: /join #channel [key] · /part [#channel] [reason] · /msg nick text · /query nick [text] · /me action · /nick name · /topic [text] · /whois nick · /names · /away [text] · /notice target text · /quote raw line · /quit [reason] · //text sends text starting with /";
const ACTION: char = '\u{1}';

#[derive(Debug)]
pub(in crate::editor) struct Cycle {
    start: usize,
    candidates: Vec<String>,
    index: usize,
    suffix: &'static str,
}

fn split_word(text: &str) -> (&str, &str) {
    let (word, rest) = text.split_once(' ').unwrap_or((text, ""));
    (word, rest.trim())
}

impl Editor {
    pub(in crate::editor) fn irc_command(&mut self, index: usize, target: &Target, line: &str) {
        let (name, rest) = split_word(line);
        let here = target.name().map(str::to_owned);
        let channel = match target {
            Target::Channel(name) => Some(name.clone()),
            Target::Server | Target::Query(_) => None,
        };
        let server = self.irc.links[index].server.name.clone();
        let message = match name.to_ascii_lowercase().as_str() {
            "join" | "j" => self.irc_join(server, rest),
            "part" | "leave" => self.irc_part(index, channel, rest),
            "msg" => {
                let (nick, text) = split_word(rest);
                if nick.is_empty() || text.is_empty() {
                    self.show_message("Usage: /msg nick text");
                    return;
                }
                self.irc_say(index, nick, text);
                None
            }
            "query" => {
                let (nick, text) = split_word(rest);
                if nick.is_empty() {
                    self.show_message("Usage: /query nick [text]");
                    return;
                }
                self.irc_open(index, &Target::Query(nick.to_owned()));
                if !text.is_empty() {
                    self.irc_say(index, nick, text);
                }
                None
            }
            "me" => {
                let Some(here) = here else {
                    self.show_message("/me works in channels and private chats");
                    return;
                };
                self.irc_say(index, &here, &format!("{ACTION}ACTION {rest}{ACTION}"));
                None
            }
            "nick" => Some(Message::new("NICK", [rest])),
            "topic" => {
                let Some(channel) = channel else {
                    self.show_message("/topic works in channels");
                    return;
                };
                let mut params = vec![channel];
                params.extend((!rest.is_empty()).then(|| rest.to_owned()));
                Some(Message::new("TOPIC", params))
            }
            "whois" => Some(Message::new("WHOIS", [rest])),
            "names" => Some(Message::new(
                "NAMES",
                channel.or_else(|| (!rest.is_empty()).then(|| rest.to_owned())),
            )),
            "away" => Some(Message::new(
                "AWAY",
                (!rest.is_empty()).then(|| rest.to_owned()),
            )),
            "notice" => {
                let (to, text) = split_word(rest);
                Some(Message::new("NOTICE", [to, text]))
            }
            "quote" | "raw" => {
                self.irc.links[index].session.send_raw(rest);
                None
            }
            "quit" => {
                let reason = if rest.is_empty() { "oxmacs" } else { rest };
                self.irc_quit(index, reason);
                return;
            }
            "help" => {
                self.irc_help(index, target);
                return;
            }
            _ => {
                self.show_message(format!("Unknown command /{name}; /help lists them"));
                return;
            }
        };
        if let Some(message) = message {
            self.irc.links[index].session.send(&message);
        }
    }

    fn irc_join(&mut self, server: String, rest: &str) -> Option<Message> {
        let (channels, key) = split_word(rest);
        if channels.is_empty() {
            self.show_message("Which channel? /join #channel");
            return None;
        }
        if let Some(first) = channels.split(',').next() {
            self.irc.focus = Some((server, Target::Channel(first.to_owned())));
        }
        let mut params = vec![channels.to_owned()];
        params.extend((!key.is_empty()).then(|| key.to_owned()));
        Some(Message::new("JOIN", params))
    }

    fn irc_part(&mut self, index: usize, channel: Option<String>, rest: &str) -> Option<Message> {
        let (first, reason) = split_word(rest);
        let session = &self.irc.links[index].session;
        let (chosen, reason) = if session.is_channel(first) {
            (Some(first.to_owned()), reason)
        } else {
            (channel, rest)
        };
        let Some(chosen) = chosen else {
            self.show_message("Which channel? /part #channel");
            return None;
        };
        let mut params = vec![chosen];
        params.extend((!reason.is_empty()).then(|| reason.to_owned()));
        Some(Message::new("PART", params))
    }

    pub(in crate::editor) fn guard_chat_input(&mut self, command: Command) -> bool {
        if !command.modifies_text() {
            return false;
        }
        let Some((buffer, Mode::Chat(view))) = self.buffers.entry_mut(0) else {
            return false;
        };
        let Some(start) = view.input.start(buffer) else {
            return false;
        };
        if buffer.point() < start {
            buffer.set_point(buffer.len_chars());
        }
        buffer.point() <= start
            && matches!(
                command,
                Command::DeleteBackwardChar | Command::BackwardDeleteWord
            )
    }

    pub(in crate::editor) fn irc_complete_nick(&mut self, again: bool) {
        let Some(Mode::Chat(view)) = self.buffers.mode(0) else {
            return;
        };
        let nicks = self.irc_members(&view.server.clone(), &view.target.clone());
        let Some((buffer, Mode::Chat(view))) = self.buffers.entry_mut(0) else {
            return;
        };
        let Some(start) = view.input.start(buffer) else {
            return;
        };
        let point = buffer.point();
        if point < start {
            return;
        }
        if again && let Some(cycle) = &mut view.cycle {
            cycle.index = (cycle.index + 1) % cycle.candidates.len();
            let chosen = format!("{}{}", cycle.candidates[cycle.index], cycle.suffix);
            buffer.delete_range(cycle.start..point);
            buffer.insert_at(cycle.start, &chosen);
            buffer.set_point(cycle.start + chosen.chars().count());
            return;
        }
        let typed = buffer.text_in(start..point);
        let word_start = start
            + typed
                .rfind(' ')
                .map_or(0, |space| typed[..=space].chars().count());
        let word = buffer.text_in(word_start..point).to_lowercase();
        let mut candidates: Vec<String> = nicks
            .into_iter()
            .filter(|nick| !word.is_empty() && nick.to_lowercase().starts_with(&word))
            .collect();
        candidates.sort_by_key(|nick| nick.to_lowercase());
        let Some(first) = candidates.first().cloned() else {
            view.cycle = None;
            return;
        };
        let suffix = if word_start == start { ": " } else { " " };
        let chosen = format!("{first}{suffix}");
        buffer.delete_range(word_start..point);
        buffer.insert_at(word_start, &chosen);
        buffer.set_point(word_start + chosen.chars().count());
        view.cycle = Some(Cycle {
            start: word_start,
            candidates,
            index: 0,
            suffix,
        });
    }
}
#[cfg(test)]
mod tests {
    use jiff::tz::TimeZone;

    use crate::config::Config;
    use crate::editor::test_support::{lines, press, type_text};
    use crate::editor::{ConnectionId, Editor, Effect};
    use crate::external::net::Event;

    const LIBERA: &str = "[[irc-server]]\nname = \"libera\"\nhost = \"irc.libera.chat\"\nnick = \"oxtest\"\nchannels = [\"#oxmacs-test\"]\n";

    fn connected() -> (Editor, ConnectionId) {
        let mut editor = Editor::new();
        editor.apply_config(&Config::parse(LIBERA).unwrap());
        editor.irc.zone = TimeZone::UTC;
        press(&mut editor, "M-x");
        type_text(&mut editor, "irc");
        press(&mut editor, "RET");
        let id = ConnectionId(0);
        editor.irc_event(id, Event::Connected);
        for line in [
            ":srv CAP * LS :",
            ":srv 001 oxtest :Welcome",
            ":oxtest!o@h JOIN #oxmacs-test",
            ":srv 353 oxtest = #oxmacs-test :oxtest alice albert @bob",
            ":srv 366 oxtest #oxmacs-test :End",
        ] {
            editor.irc_event(id, Event::Line(line.to_owned()));
        }
        press(&mut editor, "C-x b");
        type_text(&mut editor, "#oxmacs-test");
        press(&mut editor, "RET");
        editor.take_effects();
        (editor, id)
    }

    fn sent(editor: &mut Editor) -> Vec<String> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Transmit(_, lines) => Some(lines),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn run(editor: &mut Editor, input: &str) -> Vec<String> {
        type_text(editor, input);
        press(editor, "RET");
        sent(editor)
    }

    fn input(editor: &Editor) -> String {
        lines(editor).last().cloned().unwrap_or_default()
    }

    #[test]
    fn slash_commands_send_their_irc_messages() {
        let (mut editor, _) = connected();

        for (typed, expected) in [
            ("/join #rust", "JOIN #rust"),
            ("/JOIN #a,#b key", "JOIN #a,#b key"),
            ("/part", "PART #oxmacs-test"),
            ("/part #rust gone for lunch", "PART #rust :gone for lunch"),
            ("/msg alice hi there", "PRIVMSG alice :hi there"),
            ("/nick oxtest2", "NICK oxtest2"),
            ("/topic", "TOPIC #oxmacs-test"),
            (
                "/topic Testing oxmacs",
                "TOPIC #oxmacs-test :Testing oxmacs",
            ),
            ("/whois alice", "WHOIS alice"),
            ("/away lunch", "AWAY lunch"),
            ("/away", "AWAY"),
            ("/notice alice ping", "NOTICE alice ping"),
            ("/names", "NAMES #oxmacs-test"),
            ("/quote MODE #oxmacs-test +i", "MODE #oxmacs-test +i"),
            ("/me waves", "PRIVMSG #oxmacs-test :\u{1}ACTION waves\u{1}"),
        ] {
            assert_eq!(run(&mut editor, typed), [expected], "{typed}");
        }
    }

    #[test]
    fn actions_show_as_actions() {
        let (mut editor, _) = connected();

        run(&mut editor, "/me waves");

        assert!(
            lines(&editor)
                .iter()
                .any(|line| line.ends_with("* │ oxtest waves"))
        );
    }

    #[test]
    fn double_slashes_send_a_literal_slash() {
        let (mut editor, _) = connected();

        assert_eq!(
            run(&mut editor, "//etc/hosts is fine"),
            ["PRIVMSG #oxmacs-test :/etc/hosts is fine"]
        );
    }

    #[test]
    fn query_opens_a_private_buffer_and_sends_any_text() {
        let (mut editor, _) = connected();

        assert_eq!(
            run(&mut editor, "/query alice hello"),
            ["PRIVMSG alice hello"]
        );

        assert_eq!(editor.buffer().name(), "alice");
    }

    #[test]
    fn joining_from_a_command_shows_the_channel() {
        let (mut editor, id) = connected();

        run(&mut editor, "/join #rust");
        editor.irc_event(id, Event::Line(":oxtest!o@h JOIN #rust".to_owned()));

        assert_eq!(editor.buffer().name(), "#rust");
    }

    #[test]
    fn quit_says_goodbye_and_disconnects() {
        let (mut editor, id) = connected();

        type_text(&mut editor, "/quit see you");
        press(&mut editor, "RET");

        let effects = editor.take_effects();
        assert!(effects.contains(&Effect::Transmit(id, vec!["QUIT :see you".to_owned()])));
        assert!(effects.contains(&Effect::Disconnect(id)));
    }

    #[test]
    fn unknown_commands_are_explained() {
        let (mut editor, _) = connected();

        assert_eq!(run(&mut editor, "/frobnicate"), Vec::<String>::new());

        assert_eq!(
            editor.message(),
            Some("Unknown command /frobnicate; /help lists them")
        );
    }

    #[test]
    fn help_lists_the_commands() {
        let (mut editor, _) = connected();

        run(&mut editor, "/help");

        assert!(lines(&editor).iter().any(|line| line.contains("/join")));
    }

    #[test]
    fn tab_completes_nicks_and_cycles() {
        let (mut editor, _) = connected();

        type_text(&mut editor, "al");
        press(&mut editor, "TAB");
        assert_eq!(input(&editor), "#oxmacs-test › albert: ");
        press(&mut editor, "TAB");
        assert_eq!(input(&editor), "#oxmacs-test › alice: ");
        press(&mut editor, "TAB");
        assert_eq!(input(&editor), "#oxmacs-test › albert: ");

        type_text(&mut editor, "ask b");
        press(&mut editor, "TAB");
        assert_eq!(input(&editor), "#oxmacs-test › albert: ask bob ");
    }

    #[test]
    fn typing_in_the_history_goes_to_the_prompt() {
        let (mut editor, _) = connected();
        press(&mut editor, "M-<");

        type_text(&mut editor, "hi");

        assert_eq!(input(&editor), "#oxmacs-test › hi");
    }

    #[test]
    fn deleting_stops_at_the_prompt() {
        let (mut editor, _) = connected();
        type_text(&mut editor, "a");

        press(&mut editor, "DEL DEL DEL");

        assert_eq!(input(&editor), "#oxmacs-test › ");
    }
}
