use std::collections::HashSet;
use std::fmt;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use strum::VariantArray;

use crate::command::{Command, Scope};
use crate::editor::Editor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandId(u16);

impl From<Command> for CommandId {
    fn from(command: Command) -> Self {
        Self(command as u16)
    }
}

impl CommandId {
    pub fn core(self) -> Option<Command> {
        Command::VARIANTS.get(usize::from(self.0)).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(u16);

impl From<Scope> for ScopeId {
    fn from(scope: Scope) -> Self {
        Self(scope as u16)
    }
}

impl ScopeId {
    pub fn core(self) -> Option<Scope> {
        Scope::VARIANTS.get(usize::from(self.0)).copied()
    }

    pub(crate) fn at(index: usize) -> Self {
        Self(u16::try_from(index).unwrap_or(u16::MAX))
    }

    pub(crate) fn index(self) -> usize {
        usize::from(self.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    pub name: &'static str,
    pub summary: &'static str,
    pub help: Option<&'static str>,
    pub scope: ScopeId,
    pub run: fn(&mut Editor),
}

pub type Shared = Arc<dyn Fn(&mut Editor) + Send + Sync>;

pub struct DynamicSpec {
    pub name: String,
    pub summary: String,
    pub scope: ScopeId,
    pub run: Shared,
}

impl fmt::Debug for DynamicSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DynamicSpec")
            .field("name", &self.name)
            .field("summary", &self.summary)
            .field("scope", &self.scope)
            .finish_non_exhaustive()
    }
}

const DYNAMIC: usize = 0x4000;

fn intern(text: &str) -> &'static str {
    static KNOWN: LazyLock<Mutex<HashSet<&'static str>>> = LazyLock::new(Mutex::default);
    let mut known = KNOWN.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(found) = known.get(text) {
        return found;
    }
    let kept: &'static str = Box::leak(text.to_owned().into_boxed_str());
    known.insert(kept);
    kept
}

#[derive(Clone)]
struct Dynamic {
    summary: &'static str,
    scope: ScopeId,
    run: Shared,
}

#[derive(Default)]
pub struct Commands {
    added: Vec<CommandSpec>,
    dynamic: Vec<(&'static str, Option<Dynamic>)>,
}

impl fmt::Debug for Commands {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Commands")
            .field("added", &self.added.len())
            .field("dynamic", &self.dynamic.len())
            .finish()
    }
}

fn id_at(index: usize) -> CommandId {
    CommandId(u16::try_from(index).unwrap_or(u16::MAX))
}

impl Commands {
    pub fn add(&mut self, spec: CommandSpec) -> CommandId {
        let id = Command::VARIANTS.len() + self.added.len();
        self.added.push(spec);
        id_at(id)
    }

    fn added(&self, id: CommandId) -> Option<&CommandSpec> {
        usize::from(id.0)
            .checked_sub(Command::VARIANTS.len())
            .and_then(|index| self.added.get(index))
    }

    fn dynamic(&self, id: CommandId) -> Option<(&'static str, &Dynamic)> {
        let index = usize::from(id.0).checked_sub(DYNAMIC)?;
        let (name, slot) = self.dynamic.get(index)?;
        Some((name, slot.as_ref()?))
    }

    pub fn all(&self) -> impl Iterator<Item = CommandId> + '_ {
        let fixed = (0..Command::VARIANTS.len() + self.added.len()).map(id_at);
        let dynamic = self
            .dynamic
            .iter()
            .enumerate()
            .filter(|(_, (_, slot))| slot.is_some())
            .map(|(index, _)| id_at(DYNAMIC + index));
        fixed.chain(dynamic)
    }

    pub fn find(&self, name: &str) -> Option<CommandId> {
        self.all().find(|id| self.name(*id) == name)
    }

    pub fn name(&self, id: CommandId) -> &'static str {
        match id.core() {
            Some(command) => command.name(),
            None => self.added(id).map_or_else(
                || self.dynamic(id).map_or("", |(name, _)| name),
                |spec| spec.name,
            ),
        }
    }

    pub fn summary(&self, id: CommandId) -> &'static str {
        match id.core() {
            Some(command) => command.summary(),
            None => self.added(id).map_or_else(
                || self.dynamic(id).map_or("", |(_, found)| found.summary),
                |spec| spec.summary,
            ),
        }
    }

    pub fn help(&self, id: CommandId) -> Option<&'static str> {
        match id.core() {
            Some(command) => command.help(),
            None => self.added(id).and_then(|spec| spec.help),
        }
    }

    pub fn scope(&self, id: CommandId) -> ScopeId {
        match id.core() {
            Some(command) => command.scope().into(),
            None => self.added(id).map_or_else(
                || {
                    self.dynamic(id)
                        .map_or(Scope::Global.into(), |(_, found)| found.scope)
                },
                |spec| spec.scope,
            ),
        }
    }

    pub fn handler(&self, id: CommandId) -> Option<fn(&mut Editor)> {
        self.added(id).map(|spec| spec.run)
    }

    pub fn add_dynamic(&mut self, spec: DynamicSpec) -> CommandId {
        let name = intern(&spec.name);
        let found = Dynamic {
            summary: intern(&spec.summary),
            scope: spec.scope,
            run: spec.run,
        };
        let index = if let Some(index) = self.dynamic.iter().position(|(known, _)| *known == name) {
            self.dynamic[index].1 = Some(found);
            index
        } else {
            self.dynamic.push((name, Some(found)));
            self.dynamic.len() - 1
        };
        id_at(DYNAMIC + index)
    }

    pub fn remove_dynamic(&mut self, name: &str) -> bool {
        self.dynamic
            .iter_mut()
            .find(|(known, slot)| *known == name && slot.is_some())
            .is_some_and(|(_, slot)| slot.take().is_some())
    }

    pub fn shared(&self, id: CommandId) -> Option<Shared> {
        self.dynamic(id).map(|(_, found)| Arc::clone(&found.run))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn hello(editor: &mut Editor) {
        editor.show_message("hello");
    }

    const HELLO: CommandSpec = CommandSpec {
        name: "say-hello",
        summary: "Say hello in the echo area",
        help: Some("Shows hello."),
        scope: ScopeId(0),
        run: hello,
    };

    #[test]
    fn core_commands_keep_their_names_and_scopes() {
        let commands = Commands::default();
        let id = CommandId::from(Command::IsearchExit);

        assert_eq!(id.core(), Some(Command::IsearchExit));
        assert_eq!(commands.name(id), "isearch-exit");
        assert_eq!(commands.scope(id), Scope::Search.into());
        assert_eq!(commands.find("isearch-exit"), Some(id));
        assert!(commands.handler(id).is_none());
    }

    fn spec(name: &str, summary: &str) -> DynamicSpec {
        DynamicSpec {
            name: name.to_owned(),
            summary: summary.to_owned(),
            scope: Scope::Global.into(),
            run: Arc::new(|editor: &mut Editor| editor.show_message("hi")),
        }
    }

    #[test]
    fn dynamic_commands_take_names_made_at_run_time() {
        let mut commands = Commands::default();

        let id = commands.add_dynamic(spec(&format!("say-{}", "hi"), "Say hi"));

        assert_eq!(id.core(), None);
        assert_eq!(commands.find("say-hi"), Some(id));
        assert_eq!(commands.name(id), "say-hi");
        assert_eq!(commands.summary(id), "Say hi");
        assert_eq!(commands.help(id), None);
        assert_eq!(commands.scope(id), Scope::Global.into());
        assert!(commands.handler(id).is_none());
        assert!(commands.shared(id).is_some());
        assert!(commands.all().any(|known| known == id));
    }

    #[test]
    fn adding_a_dynamic_command_again_replaces_it_under_the_same_id() {
        let mut commands = Commands::default();
        let first = commands.add_dynamic(spec("say-hi", "Say hi"));

        let again = commands.add_dynamic(spec("say-hi", "Say hello"));

        assert_eq!(again, first);
        assert_eq!(commands.summary(first), "Say hello");
        assert_eq!(commands.all().filter(|known| *known == first).count(), 1);
    }

    #[test]
    fn removed_dynamic_commands_vanish_and_come_back_with_their_id() {
        let mut commands = Commands::default();
        let id = commands.add_dynamic(spec("say-hi", "Say hi"));

        assert!(commands.remove_dynamic("say-hi"));
        assert!(!commands.remove_dynamic("say-hi"));
        assert_eq!(commands.find("say-hi"), None);
        assert!(!commands.all().any(|known| known == id));
        assert!(commands.shared(id).is_none());
        assert_eq!(commands.add_dynamic(spec("say-hi", "Say hi")), id);
    }

    #[test]
    fn dynamic_and_added_commands_never_share_an_id() {
        let mut commands = Commands::default();
        let dynamic = commands.add_dynamic(spec("say-hi", "Say hi"));

        let added = commands.add(HELLO);

        assert_ne!(dynamic, added);
        assert_eq!(commands.find("say-hello"), Some(added));
        assert_eq!(commands.find("say-hi"), Some(dynamic));
    }

    #[test]
    fn added_commands_follow_the_core_ones() {
        let mut commands = Commands::default();

        let id = commands.add(HELLO);

        assert_eq!(id.core(), None);
        assert_eq!(commands.find("say-hello"), Some(id));
        assert_eq!(commands.summary(id), "Say hello in the echo area");
        assert_eq!(commands.help(id), Some("Shows hello."));
        assert_eq!(commands.all().count(), Command::VARIANTS.len() + 1);
        assert!(commands.handler(id).is_some());
    }
}
