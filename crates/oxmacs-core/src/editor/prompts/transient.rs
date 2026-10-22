use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::editor::Editor;
use crate::input::{Key, KeyCode, Modifiers};

pub type Run = Arc<dyn Fn(&mut Editor, &[String]) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Switch {
    pub key: char,
    pub flag: String,
    pub summary: String,
}

#[derive(Clone)]
pub struct Action {
    pub key: char,
    pub summary: String,
    pub run: Run,
}

impl fmt::Debug for Action {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Action")
            .field("key", &self.key)
            .field("summary", &self.summary)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct Group {
    pub title: String,
    pub switches: Vec<Switch>,
    pub actions: Vec<Action>,
}

impl Group {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_owned(),
            switches: Vec::new(),
            actions: Vec::new(),
        }
    }

    #[must_use]
    pub fn switch(mut self, key: char, flag: &str, summary: &str) -> Self {
        self.switches.push(Switch {
            key,
            flag: flag.to_owned(),
            summary: summary.to_owned(),
        });
        self
    }

    #[must_use]
    pub fn action(
        mut self,
        key: char,
        summary: &str,
        run: impl Fn(&mut Editor, &[String]) + Send + Sync + 'static,
    ) -> Self {
        self.actions.push(Action {
            key,
            summary: summary.to_owned(),
            run: Arc::new(run),
        });
        self
    }
}

#[derive(Debug, Clone)]
pub struct Transient {
    pub title: String,
    pub groups: Vec<Group>,
}

impl Transient {
    pub fn new(title: &str, groups: Vec<Group>) -> Self {
        Self {
            title: title.to_owned(),
            groups,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryView {
    pub key: String,
    pub summary: String,
    pub flag: Option<String>,
    pub on: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupView {
    pub title: String,
    pub entries: Vec<EntryView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransientView {
    pub title: String,
    pub groups: Vec<GroupView>,
    pub switching: bool,
}

#[derive(Debug, Default)]
pub(in crate::editor) struct Transients {
    open: Option<Open>,
    remembered: HashMap<String, Vec<String>>,
}

#[derive(Debug)]
struct Open {
    transient: Transient,
    on: Vec<String>,
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
    pub fn open_transient(&mut self, transient: Transient) {
        let on = self
            .transients
            .remembered
            .get(&transient.title)
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
                title: group.title.clone(),
                entries: group
                    .switches
                    .iter()
                    .map(|switch| EntryView {
                        key: format!("-{}", switch.key),
                        summary: switch.summary.clone(),
                        flag: Some(switch.flag.clone()),
                        on: open.on.contains(&switch.flag),
                    })
                    .chain(group.actions.iter().map(|action| EntryView {
                        key: action.key.to_string(),
                        summary: action.summary.clone(),
                        flag: None,
                        on: false,
                    }))
                    .collect(),
            })
            .collect();
        Some(TransientView {
            title: open.transient.title.clone(),
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
        if open.switching {
            open.switching = false;
            let found = open
                .transient
                .groups
                .iter()
                .flat_map(|group| &group.switches)
                .find(|switch| switch.key == character)
                .map(|switch| switch.flag.clone());
            match found {
                Some(flag) => {
                    if let Some(at) = open.on.iter().position(|on| *on == flag) {
                        open.on.remove(at);
                    } else {
                        open.on.push(flag);
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
        let found = open
            .transient
            .groups
            .iter()
            .flat_map(|group| &group.actions)
            .find(|action| action.key == character)
            .map(|action| action.run.clone());
        let Some(run) = found else {
            self.show_message(format!("{character} is undefined here"));
            return true;
        };
        let Some(open) = self.transients.open.take() else {
            return true;
        };
        self.transients
            .remembered
            .insert(open.transient.title, open.on.clone());
        run(self, &open.on);
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_support::press;

    #[derive(Debug, Default)]
    struct Ran(Vec<String>);

    fn note(editor: &mut Editor, what: &str, flags: &[String]) {
        let line = format!("{what} {}", flags.join(" "));
        if let Some(ran) = editor.plugin_state_mut::<Ran>() {
            ran.0.push(line.trim().to_owned());
        }
    }

    fn commit() -> Transient {
        Transient::new(
            "Commit",
            vec![
                Group::new("Arguments")
                    .switch('a', "--all", "Stage all modified and deleted files")
                    .switch('n', "--no-verify", "Skip the commit hooks"),
                Group::new("Create").action('c', "Commit", |editor, flags| {
                    note(editor, "commit", flags);
                }),
                Group::new("Edit HEAD").action('a', "Amend", |editor, flags| {
                    note(editor, "amend", flags);
                }),
            ],
        )
    }
    fn ran(editor: &Editor) -> Vec<String> {
        editor
            .plugin_state::<Ran>()
            .map(|ran| ran.0.clone())
            .unwrap_or_default()
    }

    fn opened() -> Editor {
        let mut editor = Editor::new();
        editor.set_plugin_state(Ran::default());
        editor.open_transient(commit());
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
        assert_eq!(view.groups[0].entries[0].flag.as_deref(), Some("--all"));
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

        editor.open_transient(commit());

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
    fn transients_can_be_built_at_runtime_with_closures() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let noted = seen.clone();
        let mut editor = Editor::new();
        editor.open_transient(Transient::new(
            "Push",
            vec![
                Group::new("Arguments").switch('f', "--force-with-lease", "Force safely"),
                Group::new("Push to").action('p', "pushRemote", move |_, flags| {
                    noted.lock().unwrap().push(flags.join(" "));
                }),
            ],
        ));

        assert_eq!(editor.transient().unwrap().title, "Push");
        press(&mut editor, "- f p");

        assert_eq!(*seen.lock().unwrap(), ["--force-with-lease"]);
        assert!(editor.transient().is_none());
    }

    #[test]
    fn keys_reach_the_buffer_again_once_it_closes() {
        let mut editor = opened();
        press(&mut editor, "q x");

        assert_eq!(editor.buffer().contents(), "x");
    }
}
