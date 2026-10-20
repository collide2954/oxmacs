use std::collections::HashMap;

use crate::editor::Editor;
use crate::input::{Key, KeyCode, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switch {
    pub key: char,
    pub flag: &'static str,
    pub summary: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct Action {
    pub key: char,
    pub summary: &'static str,
    pub run: fn(&mut Editor, &[&'static str]),
}

#[derive(Debug, Clone, Copy)]
pub struct Group {
    pub title: &'static str,
    pub switches: &'static [Switch],
    pub actions: &'static [Action],
}

#[derive(Debug, Clone, Copy)]
pub struct Transient {
    pub title: &'static str,
    pub groups: &'static [Group],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryView {
    pub key: String,
    pub summary: &'static str,
    pub flag: Option<&'static str>,
    pub on: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupView {
    pub title: &'static str,
    pub entries: Vec<EntryView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransientView {
    pub title: &'static str,
    pub groups: Vec<GroupView>,
    pub switching: bool,
}

#[derive(Debug, Default)]
pub(in crate::editor) struct Transients {
    open: Option<Open>,
    remembered: HashMap<&'static str, Vec<&'static str>>,
}

#[derive(Debug)]
struct Open {
    transient: &'static Transient,
    on: Vec<&'static str>,
    switching: bool,
}

fn plain(key: Key) -> Option<char> {
    match key.code() {
        KeyCode::Char(character) if (key.modifiers() - Modifiers::SHIFT).is_empty() => {
            Some(character)
        }
        _ => None,
    }
}

impl Editor {
    pub fn open_transient(&mut self, transient: &'static Transient) {
        let on = self
            .transients
            .remembered
            .get(transient.title)
            .cloned()
            .unwrap_or_default();
        self.transients.open = Some(Open {
            transient,
            on,
            switching: false,
        });
    }

    pub fn transient(&self) -> Option<TransientView> {
        let open = self.transients.open.as_ref()?;
        let groups = open
            .transient
            .groups
            .iter()
            .map(|group| GroupView {
                title: group.title,
                entries: group
                    .switches
                    .iter()
                    .map(|switch| EntryView {
                        key: format!("-{}", switch.key),
                        summary: switch.summary,
                        flag: Some(switch.flag),
                        on: open.on.contains(&switch.flag),
                    })
                    .chain(group.actions.iter().map(|action| EntryView {
                        key: action.key.to_string(),
                        summary: action.summary,
                        flag: None,
                        on: false,
                    }))
                    .collect(),
            })
            .collect();
        Some(TransientView {
            title: open.transient.title,
            groups,
            switching: open.switching,
        })
    }

    pub(in crate::editor) fn transient_key(&mut self, key: Key) -> bool {
        let Some(open) = self.transients.open.as_mut() else {
            return false;
        };
        let quits = key == Key::new(KeyCode::Char('g'), Modifiers::CONTROL)
            || (plain(key) == Some('q') && !open.switching);
        if quits {
            self.transients.open = None;
            return true;
        }
        let Some(character) = plain(key) else {
            self.show_message(format!("{key} is undefined here"));
            return true;
        };
        let groups = open.transient.groups;
        if open.switching {
            open.switching = false;
            let found = groups
                .iter()
                .flat_map(|group| group.switches)
                .find(|switch| switch.key == character);
            match found {
                Some(switch) => {
                    if let Some(at) = open.on.iter().position(|flag| *flag == switch.flag) {
                        open.on.remove(at);
                    } else {
                        open.on.push(switch.flag);
                    }
                }
                None => self.show_message(format!("-{character} is undefined here")),
            }
            return true;
        }
        if character == '-' {
            open.switching = true;
            return true;
        }
        let found = groups
            .iter()
            .flat_map(|group| group.actions)
            .find(|action| action.key == character)
            .copied();
        let Some(action) = found else {
            self.show_message(format!("{character} is undefined here"));
            return true;
        };
        let Some(open) = self.transients.open.take() else {
            return true;
        };
        self.transients
            .remembered
            .insert(open.transient.title, open.on.clone());
        (action.run)(self, &open.on);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_support::press;

    #[derive(Debug, Default)]
    struct Ran(Vec<String>);

    fn note(editor: &mut Editor, what: &str, flags: &[&'static str]) {
        let line = format!("{what} {}", flags.join(" "));
        if let Some(ran) = editor.applet_state_mut::<Ran>() {
            ran.0.push(line.trim().to_owned());
        }
    }

    static COMMIT: Transient = Transient {
        title: "Commit",
        groups: &[
            Group {
                title: "Arguments",
                switches: &[
                    Switch {
                        key: 'a',
                        flag: "--all",
                        summary: "Stage all modified and deleted files",
                    },
                    Switch {
                        key: 'n',
                        flag: "--no-verify",
                        summary: "Skip the commit hooks",
                    },
                ],
                actions: &[],
            },
            Group {
                title: "Create",
                switches: &[],
                actions: &[Action {
                    key: 'c',
                    summary: "Commit",
                    run: |editor, flags| note(editor, "commit", flags),
                }],
            },
            Group {
                title: "Edit HEAD",
                switches: &[],
                actions: &[Action {
                    key: 'a',
                    summary: "Amend",
                    run: |editor, flags| note(editor, "amend", flags),
                }],
            },
        ],
    };

    fn ran(editor: &Editor) -> Vec<String> {
        editor
            .applet_state::<Ran>()
            .map(|ran| ran.0.clone())
            .unwrap_or_default()
    }

    fn opened() -> Editor {
        let mut editor = Editor::new();
        editor.set_applet_state(Ran::default());
        editor.open_transient(&COMMIT);
        editor
    }

    #[test]
    fn a_transient_lists_its_switches_and_actions_by_group() {
        let editor = opened();

        let view = editor.transient().unwrap();

        assert_eq!(view.title, "Commit");
        let keys: Vec<Vec<String>> = view
            .groups
            .iter()
            .map(|group| {
                group
                    .entries
                    .iter()
                    .map(|entry| entry.key.clone())
                    .collect()
            })
            .collect();
        assert_eq!(keys, [vec!["-a", "-n"], vec!["c"], vec!["a"]]);
        assert_eq!(view.groups[0].entries[0].flag, Some("--all"));
        assert!(!view.groups[0].entries[0].on);
    }

    #[test]
    fn switches_toggle_and_actions_run_with_them_then_close() {
        let mut editor = opened();

        press(&mut editor, "- a - n - n c");

        assert_eq!(ran(&editor), ["commit --all"]);
        assert!(editor.transient().is_none());
    }

    #[test]
    fn a_dash_waits_for_the_switch_key() {
        let mut editor = opened();

        press(&mut editor, "-");

        assert!(editor.transient().unwrap().switching);
        press(&mut editor, "a");
        let view = editor.transient().unwrap();
        assert!(!view.switching);
        assert!(view.groups[0].entries[0].on);
        assert_eq!(ran(&editor), Vec::<String>::new());
    }

    #[test]
    fn switches_are_remembered_for_next_time() {
        let mut editor = opened();
        press(&mut editor, "- n a");

        editor.open_transient(&COMMIT);

        assert!(editor.transient().unwrap().groups[0].entries[1].on);
        press(&mut editor, "c");
        assert_eq!(ran(&editor), ["amend --no-verify", "commit --no-verify"]);
    }

    #[test]
    fn q_and_c_g_close_without_running_anything() {
        for keys in ["q", "C-g"] {
            let mut editor = opened();

            press(&mut editor, keys);

            assert!(editor.transient().is_none(), "{keys}");
            assert_eq!(ran(&editor), Vec::<String>::new(), "{keys}");
        }
    }

    #[test]
    fn unknown_keys_are_reported_and_keep_it_open() {
        let mut editor = opened();

        press(&mut editor, "z");

        assert_eq!(editor.message(), Some("z is undefined here"));
        assert!(editor.transient().is_some());
        press(&mut editor, "- z");
        assert_eq!(editor.message(), Some("-z is undefined here"));
    }

    #[test]
    fn keys_reach_the_buffer_again_once_it_closes() {
        let mut editor = opened();
        press(&mut editor, "q x");

        assert_eq!(editor.buffer().contents(), "x");
    }
}
