use std::collections::{HashMap, HashSet};

use super::Runtime;
use crate::symbol::Sym;
use crate::value::Value;

pub type ModuleId = usize;

pub const PRELUDE: ModuleId = 0;
pub const USER: ModuleId = 1;

const REFERENCE: &str = "#module:";
const SEPARATOR: char = '\u{1}';

#[derive(Debug, Default)]
pub struct Module {
    pub name: String,
    pub globals: HashMap<Sym, Value>,
    pub macros: HashMap<Sym, Value>,
    pub docs: HashMap<Sym, String>,
    pub exports: Option<HashSet<Sym>>,
    pub imports: Vec<Import>,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub module: ModuleId,
    pub alias: Sym,
    pub only: Vec<Sym>,
}

impl Module {
    fn exports(&self, name: Sym) -> bool {
        self.exports
            .as_ref()
            .is_none_or(|exported| exported.contains(&name))
    }

    fn table(&self, macros: bool) -> &HashMap<Sym, Value> {
        if macros { &self.macros } else { &self.globals }
    }
}

pub fn reference(module: &str, name: &str) -> Sym {
    Sym::intern(&format!("{REFERENCE}{module}{SEPARATOR}{name}"))
}

pub fn display(name: Sym) -> &'static str {
    let text = name.name();
    text.strip_prefix(REFERENCE)
        .and_then(|rest| rest.split_once(SEPARATOR))
        .map_or(text, |(_, short)| short)
}

fn qualified(name: Sym) -> Option<(Sym, Sym)> {
    let (alias, short) = name.name().split_once('/')?;
    (!alias.is_empty() && !short.is_empty()).then(|| (Sym::intern(alias), Sym::intern(short)))
}

impl Runtime {
    pub fn module_named(&mut self, name: &str) -> ModuleId {
        if let Some(id) = self.module_ids.get(name) {
            return *id;
        }
        let id = self.modules.len();
        self.modules.push(Module {
            name: name.to_owned(),
            ..Module::default()
        });
        self.module_ids.insert(name.to_owned(), id);
        id
    }

    fn referenced(&self, name: Sym) -> Option<(ModuleId, Sym)> {
        let rest = name.name().strip_prefix(REFERENCE)?;
        let (module, short) = rest.split_once(SEPARATOR)?;
        Some((*self.module_ids.get(module)?, Sym::intern(short)))
    }

    fn exported(&self, module: ModuleId, name: Sym, macros: bool) -> Option<(Value, ModuleId)> {
        let found = self.modules.get(module)?;
        if !found.exports(name) {
            return None;
        }
        found.table(macros).get(&name).map(|value| (*value, module))
    }

    fn find(&self, module: ModuleId, name: Sym, macros: bool) -> Option<(Value, ModuleId)> {
        if let Some((target, short)) = self.referenced(name) {
            let found = self.modules.get(target)?.table(macros).get(&short)?;
            return Some((*found, target));
        }
        let current = self.modules.get(module)?;
        if let Some(value) = current.table(macros).get(&name) {
            return Some((*value, module));
        }
        if let Some((alias, short)) = qualified(name)
            && let Some(found) = current
                .imports
                .iter()
                .filter(|import| import.alias == alias)
                .find_map(|import| self.exported(import.module, short, macros))
        {
            return Some(found);
        }
        if let Some(found) = current
            .imports
            .iter()
            .filter(|import| import.only.contains(&name))
            .find_map(|import| self.exported(import.module, name, macros))
        {
            return Some(found);
        }
        if module == PRELUDE {
            return None;
        }
        let found = self.modules.get(PRELUDE)?.table(macros).get(&name)?;
        Some((*found, PRELUDE))
    }

    pub fn lookup(&self, module: ModuleId, name: Sym) -> Option<Value> {
        self.find(module, name, false).map(|(value, _)| value)
    }

    pub fn lookup_macro(&self, module: ModuleId, name: Sym) -> Option<(Value, ModuleId)> {
        self.find(module, name, true)
    }

    pub fn reset_module(&mut self, module: ModuleId) {
        if let Some(found) = self.module_mut(module) {
            *found = Module {
                name: std::mem::take(&mut found.name),
                ..Module::default()
            };
        }
    }

    pub fn module_mut(&mut self, module: ModuleId) -> Option<&mut Module> {
        self.epoch += 1;
        self.modules.get_mut(module)
    }

    pub fn define(&mut self, module: ModuleId, name: Sym, value: Value) {
        if let Some(found) = self.module_mut(module) {
            found.globals.insert(name, value);
        }
    }

    pub fn define_macro(&mut self, module: ModuleId, name: Sym, value: Value) {
        if let Some(found) = self.module_mut(module) {
            found.macros.insert(name, value);
        }
    }

    pub fn document(&mut self, module: ModuleId, name: Sym, doc: Option<String>) {
        if let Some(found) = self.modules.get_mut(module) {
            match doc {
                Some(doc) => found.docs.insert(name, doc),
                None => found.docs.remove(&name),
            };
        }
    }

    pub fn doc(&self, module: ModuleId, name: Sym) -> Option<&str> {
        [module, PRELUDE]
            .into_iter()
            .filter_map(|module| self.modules.get(module))
            .find_map(|found| found.docs.get(&name))
            .map(String::as_str)
    }

    pub fn defined(&self, module: ModuleId, name: Sym) -> bool {
        self.modules
            .get(module)
            .is_some_and(|found| found.globals.contains_key(&name))
    }

    pub fn assignable(&mut self, module: ModuleId, name: Sym) -> Option<&mut Value> {
        let (target, short) = self.referenced(name).unwrap_or((module, name));
        self.module_mut(target)?.globals.get_mut(&short)
    }

    pub fn module_of_mark(&self, mark: u32) -> Option<ModuleId> {
        self.mark_modules.get(&mark).copied()
    }
}
