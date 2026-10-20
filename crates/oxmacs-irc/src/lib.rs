pub mod bouncer;
mod chat;
mod keychain;
mod nick_list;
pub mod protocol;
pub mod settings;

use std::thread;
use std::time::Duration;

use iced::futures::channel::oneshot;
use iced::{Element, Length, Task};
use oxmacs_core::applet::{Applet, Payload, Subcommand};
use oxmacs_core::command::{Command, CommandId, CommandSpec, Scope, ScopeId};
use oxmacs_core::editor::{Editor, Pane};
use oxmacs_gui::applet::{Context, GuiApplet};

use chat::{Chat, ChatView, IrcState, chat};

const DAEMON_PATIENCE: Duration = Duration::from_secs(10);
const SUBCOMMANDS: &[Subcommand] = &[Subcommand {
    name: "irc-daemon",
    summary: "Run the IRC bouncer without the editor",
    run: bouncer::daemon::main,
}];

#[derive(Debug)]
pub struct Irc;

#[derive(Debug)]
pub struct LaunchDaemon;

#[derive(Debug)]
pub struct DaemonReady(pub bool);

struct Spec {
    name: &'static str,
    summary: &'static str,
    help: Option<&'static str>,
    keys: &'static [&'static str],
    run: fn(&mut Editor),
}

const GLOBAL: &[Spec] = &[
    Spec {
        name: "irc",
        summary: "Connect to an IRC server from config.toml",
        help: Some(
            "Connects to an [[irc.server]] in config.toml, asking which one when there are several, and joins its channels. Each server, channel and private chat gets its own buffer; type after the prompt and press RET to send. Already connected, it shows the server buffer.",
        ),
        keys: &[],
        run: |editor| {
            chat(editor, Chat::irc);
        },
    },
    Spec {
        name: "irc-setup",
        summary: "Add an IRC server with a short form",
        help: Some(
            "Opens *IRC Setup*: pick Libera.Chat, OFTC, EFnet or another server, then your nick, an optional account to sign in with, where the password is kept (the OS keychain, asked each time, or an environment variable), channels to join, and whether the built-in bouncer keeps you connected when oxmacs closes. TAB moves between fields and RET changes one; C-c C-c adds the server to config.toml and offers to connect, C-c C-k cancels.",
        ),
        keys: &[],
        run: |editor| {
            chat(editor, Chat::irc_setup);
        },
    },
    Spec {
        name: "irc-stop-daemon",
        summary: "Stop the IRC bouncer and its server connections",
        help: Some(
            "Asks the bouncer you are connected through to leave every IRC server and exit. A local bouncer starts again the next time you use M-x irc.",
        ),
        keys: &[],
        run: |editor| {
            chat(editor, Chat::irc_stop_daemon);
        },
    },
    Spec {
        name: "irc-switch",
        summary: "Switch to an IRC buffer, busiest first",
        help: Some(
            "Lists every IRC server, channel and private chat with its unread messages and mentions; chats that mention you come first.",
        ),
        keys: &[],
        run: |editor| {
            chat(editor, Chat::irc_switch);
        },
    },
];

const CHATS: &[Spec] = &[
    Spec {
        name: "irc-send-input",
        summary: "Send the message or /command after the prompt",
        help: Some(
            "Text is sent to the channel or person of this buffer. A line starting with / is a command, such as /join #channel, /msg nick text, /me waves or /quit; /help lists them all, and // sends text that starts with a slash.",
        ),
        keys: &["RET"],
        run: |editor| {
            chat(editor, Chat::irc_send_input);
        },
    },
    Spec {
        name: "irc-complete-nick",
        summary: "Complete the nick before the cursor; again cycles",
        help: None,
        keys: &["TAB"],
        run: |editor| {
            chat(editor, Chat::irc_complete_nick);
        },
    },
    Spec {
        name: "irc-toggle-nick-list",
        summary: "Show or hide the member list beside channels",
        help: None,
        keys: &["C-c C-n"],
        run: |editor| {
            chat(editor, Chat::irc_toggle_members);
        },
    },
];

const SETUP: &[Spec] = &[
    Spec {
        name: "irc-setup-next-field",
        summary: "Move to the next field of the IRC setup",
        help: None,
        keys: &["TAB", "n"],
        run: |editor| {
            chat(editor, |chat| chat.irc_setup_next_field(true));
        },
    },
    Spec {
        name: "irc-setup-previous-field",
        summary: "Move to the previous field of the IRC setup",
        help: None,
        keys: &["S-TAB", "p"],
        run: |editor| {
            chat(editor, |chat| chat.irc_setup_next_field(false));
        },
    },
    Spec {
        name: "irc-setup-change",
        summary: "Change the IRC setup field at point",
        help: None,
        keys: &["RET"],
        run: |editor| {
            chat(editor, Chat::irc_setup_change);
        },
    },
    Spec {
        name: "irc-setup-save",
        summary: "Save the IRC server to config.toml",
        help: None,
        keys: &["C-c C-c"],
        run: |editor| {
            chat(editor, Chat::irc_setup_save);
        },
    },
    Spec {
        name: "irc-setup-cancel",
        summary: "Close the IRC setup without saving",
        help: None,
        keys: &["C-c C-k"],
        run: |editor| {
            chat(editor, Chat::irc_setup_cancel);
        },
    },
];

fn register(editor: &mut Editor, scope: ScopeId, specs: &[Spec]) -> Vec<CommandId> {
    let mut added = Vec::new();
    for spec in specs {
        let command = editor.add_command(CommandSpec {
            name: spec.name,
            summary: spec.summary,
            help: spec.help,
            scope,
            run: spec.run,
        });
        for keys in spec.keys {
            editor.bind(scope, keys, command).ok();
        }
        added.push(command);
    }
    added
}

async fn start_daemon() -> bool {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let ready = bouncer::daemon::launch().is_ok()
            && bouncer::daemon::wait_until_running(DAEMON_PATIENCE);
        let _ = sender.send(ready);
    });
    receiver.await.unwrap_or(false)
}

impl Applet for Irc {
    fn name(&self) -> &'static str {
        "irc"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn opt_in(&self) -> bool {
        true
    }

    fn subcommands(&self) -> &'static [Subcommand] {
        SUBCOMMANDS
    }

    fn install(&self, editor: &mut Editor) {
        let chats = editor.add_scope("IRC buffers");
        let setup = editor.add_scope("*IRC Setup*");
        editor.set_applet_state(IrcState::new(chats, setup));
        let global = register(editor, Scope::Global.into(), GLOBAL);
        register(editor, chats, CHATS);
        register(editor, setup, SETUP);
        if let Some(switch) = GLOBAL
            .iter()
            .position(|spec| spec.name == "irc-switch")
            .and_then(|index| global.get(index))
        {
            editor.bind(chats, "C-c C-b", *switch).ok();
        }
        editor.bind(chats, "M-p", Command::ComintPreviousInput).ok();
        editor.bind(chats, "M-n", Command::ComintNextInput).ok();
        editor.add_config_table::<settings::IrcSettings>(settings::TABLE);
        editor.on_display(|editor| {
            chat(editor, |chat| {
                chat.read_visible_chats();
                chat.refill_chats();
            });
        });
        editor.on_quit(|editor| {
            chat(editor, Chat::irc_farewell);
        });
        editor.on_event(|editor, ready: &DaemonReady| {
            chat(editor, |chat| chat.irc_daemon_ready(ready));
        });
        chat(editor, Chat::autostart);
    }
}

impl GuiApplet for Irc {
    fn perform(&mut self, effect: &Payload) -> Option<Task<Payload>> {
        effect.get::<LaunchDaemon>()?;
        Some(Task::perform(start_daemon(), |ready| {
            Payload::new(DaemonReady(ready))
        }))
    }

    fn beside<'a>(
        &'a self,
        editor: &'a Editor,
        pane: Pane<'a>,
        context: &Context,
    ) -> Option<Element<'a, Payload>> {
        let view = pane.applet_buffer::<ChatView>()?;
        let (members, own) = chat::members(editor, view)?;
        Some(
            iced::widget::container(nick_list::view(
                &members,
                Some(&own),
                context.faces,
                context.size,
            ))
            .height(Length::Fill)
            .into(),
        )
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use jiff::tz::TimeZone;
    use oxmacs_core::editor::Editor;
    use oxmacs_core::input::KeySequence;

    use crate::Irc;
    use crate::chat::IrcState;

    pub(crate) fn editor() -> Editor {
        let mut editor = Editor::new();
        editor.install(&Irc);
        state(&mut editor).zone = TimeZone::UTC;
        editor
    }

    pub(crate) fn state(editor: &mut Editor) -> &mut IrcState {
        editor.applet_state_mut::<IrcState>().unwrap()
    }

    pub(crate) fn press(editor: &mut Editor, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
    }

    pub(crate) fn type_text(editor: &mut Editor, text: &str) {
        for character in text.chars() {
            let notation = match character {
                ' ' => "SPC".to_owned(),
                other => other.to_string(),
            };
            press(editor, &notation);
        }
    }

    pub(crate) fn lines(editor: &Editor) -> Vec<String> {
        editor
            .buffer()
            .contents()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    pub(crate) fn labels(editor: &Editor) -> Vec<String> {
        editor
            .completions()
            .into_iter()
            .map(|completion| completion.label)
            .collect()
    }

    pub(crate) fn question(editor: &Editor) -> Option<String> {
        editor
            .question()
            .map(|question| question.prompt().to_owned())
    }
}
