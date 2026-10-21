use std::collections::HashMap;
use std::sync::{LazyLock, PoisonError, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sym(u32);

#[derive(Default)]
struct Table {
    names: Vec<&'static str>,
    symbols: HashMap<&'static str, Sym>,
}

static TABLE: LazyLock<RwLock<Table>> = LazyLock::new(RwLock::default);

impl Sym {
    pub fn intern(name: &str) -> Self {
        if let Some(found) = TABLE
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .symbols
            .get(name)
        {
            return *found;
        }
        let mut table = TABLE.write().unwrap_or_else(PoisonError::into_inner);
        if let Some(found) = table.symbols.get(name) {
            return *found;
        }
        let symbol = Self(u32::try_from(table.names.len()).unwrap_or(u32::MAX));
        let name: &'static str = Box::leak(name.into());
        table.names.push(name);
        table.symbols.insert(name, symbol);
        symbol
    }

    pub fn name(self) -> &'static str {
        TABLE
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .names
            .get(self.0 as usize)
            .copied()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::Sym;

    #[test]
    fn the_same_name_is_the_same_symbol() {
        assert_eq!(Sym::intern("weather"), Sym::intern("weather"));
        assert_ne!(Sym::intern("weather"), Sym::intern("forecast"));
    }

    #[test]
    fn symbols_remember_their_names() {
        assert_eq!(Sym::intern("café-λ").name(), "café-λ");
        assert_eq!(Sym::intern("").name(), "");
    }

    #[test]
    fn every_thread_sees_the_same_symbols() {
        let here = Sym::intern("shared-across-threads");

        let there = thread::spawn(|| Sym::intern("shared-across-threads"))
            .join()
            .unwrap();

        assert_eq!(here, there);
    }
}
