use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;

use crate::editor::Editor;

pub trait Applet {
    fn name(&self) -> &'static str;

    fn version(&self) -> &'static str;

    fn opt_in(&self) -> bool {
        false
    }

    fn install(&self, editor: &mut Editor);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Installed {
    pub name: &'static str,
    pub version: &'static str,
    pub opt_in: bool,
}

#[derive(Default)]
pub struct States(HashMap<TypeId, Box<dyn Any + Send>>);

impl fmt::Debug for States {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("States")
            .field("count", &self.0.len())
            .finish()
    }
}

impl States {
    pub fn get<T: Any + Send>(&self) -> Option<&T> {
        self.0.get(&TypeId::of::<T>())?.downcast_ref()
    }

    pub fn get_mut<T: Any + Send>(&mut self) -> Option<&mut T> {
        self.0.get_mut(&TypeId::of::<T>())?.downcast_mut()
    }

    pub fn set<T: Any + Send>(&mut self, state: T) {
        self.0.insert(TypeId::of::<T>(), Box::new(state));
    }
}

#[cfg(test)]
mod tests {
    use crate::command::{CommandSpec, Scope};
    use crate::editor::Editor;
    use crate::input::KeySequence;

    use super::*;

    #[derive(Debug, Default)]
    struct Count(usize);

    fn count(editor: &mut Editor) {
        if let Some(count) = editor.applet_state_mut::<Count>() {
            count.0 += 1;
        }
    }

    struct Counter;

    impl Applet for Counter {
        fn name(&self) -> &'static str {
            "counter"
        }

        fn version(&self) -> &'static str {
            "0.2.0"
        }

        fn opt_in(&self) -> bool {
            true
        }

        fn install(&self, editor: &mut Editor) {
            let command = editor.add_command(CommandSpec {
                name: "count-up",
                summary: "Count one more",
                help: None,
                scope: Scope::Global.into(),
                run: count,
            });
            editor.set_applet_state(Count::default());
            editor.bind(Scope::Global, "C-c +", command).ok();
        }
    }

    #[test]
    fn installed_applets_add_commands_keys_and_state() {
        let mut editor = Editor::new();

        editor.install(&Counter);
        let keys: KeySequence = "C-c + C-c +".parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }

        assert_eq!(editor.applet_state::<Count>().map(|count| count.0), Some(2));
        assert_eq!(
            editor.installed(),
            [Installed {
                name: "counter",
                version: "0.2.0",
                opt_in: true
            }]
        );
    }

    #[test]
    fn state_is_kept_per_type() {
        let mut states = States::default();
        states.set(Count(3));
        states.set(String::from("text"));

        assert_eq!(states.get::<Count>().map(|count| count.0), Some(3));
        assert_eq!(states.get::<String>().map(String::as_str), Some("text"));
        assert!(states.get::<u8>().is_none());
    }
}
