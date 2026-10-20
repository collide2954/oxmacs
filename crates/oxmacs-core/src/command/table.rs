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

#[derive(Debug, Default)]
pub struct Commands {
    added: Vec<CommandSpec>,
}

impl Commands {
    pub fn add(&mut self, spec: CommandSpec) -> CommandId {
        let id = Command::VARIANTS.len() + self.added.len();
        self.added.push(spec);
        CommandId(u16::try_from(id).unwrap_or(u16::MAX))
    }

    fn added(&self, id: CommandId) -> Option<&CommandSpec> {
        usize::from(id.0)
            .checked_sub(Command::VARIANTS.len())
            .and_then(|index| self.added.get(index))
    }

    pub fn all(&self) -> impl Iterator<Item = CommandId> + '_ {
        (0..Command::VARIANTS.len() + self.added.len())
            .filter_map(|index| u16::try_from(index).ok())
            .map(CommandId)
    }

    pub fn find(&self, name: &str) -> Option<CommandId> {
        self.all().find(|id| self.name(*id) == name)
    }

    pub fn name(&self, id: CommandId) -> &'static str {
        match id.core() {
            Some(command) => command.name(),
            None => self.added(id).map_or("", |spec| spec.name),
        }
    }

    pub fn summary(&self, id: CommandId) -> &'static str {
        match id.core() {
            Some(command) => command.summary(),
            None => self.added(id).map_or("", |spec| spec.summary),
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
            None => self
                .added(id)
                .map_or(Scope::Global.into(), |spec| spec.scope),
        }
    }

    pub fn handler(&self, id: CommandId) -> Option<fn(&mut Editor)> {
        self.added(id).map(|spec| spec.run)
    }
}

#[cfg(test)]
mod tests {
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
        let id = CommandId::from(Command::DiredMark);

        assert_eq!(id.core(), Some(Command::DiredMark));
        assert_eq!(commands.name(id), "dired-mark");
        assert_eq!(commands.scope(id), Scope::Dired.into());
        assert_eq!(commands.find("dired-mark"), Some(id));
        assert!(commands.handler(id).is_none());
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
