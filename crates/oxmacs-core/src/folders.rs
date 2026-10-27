use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folders {
    pub home: PathBuf,
    system: PathBuf,
    cache: PathBuf,
}

impl Folders {
    pub fn under(home: &Path) -> Self {
        Self {
            home: home.to_path_buf(),
            system: home.to_path_buf(),
            cache: home.join("cache"),
        }
    }

    pub fn for_home(home: &Path) -> Self {
        let usual = std::env::var_os("OXMACS_HOME").is_none_or(|given| given.is_empty())
            && dirs::home_dir().is_some_and(|user| user.join(".oxmacs") == home);
        match (usual, dirs::data_dir(), dirs::cache_dir()) {
            (true, Some(data), Some(cache)) => Self {
                home: home.to_path_buf(),
                system: data.join("oxmacs"),
                cache: cache.join("oxmacs"),
            },
            _ => Self::under(home),
        }
    }

    pub fn tools(&self) -> PathBuf {
        self.system.join("tools")
    }

    pub fn recovery(&self) -> PathBuf {
        self.cache.join("recovery")
    }

    pub fn crashes(&self) -> PathBuf {
        self.cache.join("crashes")
    }

    pub fn plugin(&self, name: &str, cache: bool) -> PathBuf {
        let folder = if cache {
            self.cache.join("plugins").join(name)
        } else {
            self.home.join("data").join(name)
        };
        let old = self.home.join(name);
        if !folder.exists() && old.is_dir() && !["data", "cache", "plugins"].contains(&name) {
            let moved = folder
                .parent()
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|()| fs::rename(&old, &folder));
            if moved.is_err() {
                return old;
            }
        }
        folder
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::Folders;

    #[test]
    fn a_home_of_its_own_keeps_everything_under_it() {
        let home = tempfile::tempdir().unwrap();
        let folders = Folders::for_home(home.path());

        for folder in [
            folders.tools(),
            folders.recovery(),
            folders.crashes(),
            folders.plugin("vault", false),
            folders.plugin("remote", true),
        ] {
            assert!(folder.starts_with(home.path()), "{}", folder.display());
        }
        assert_eq!(
            folders.plugin("vault", false),
            home.path().join("data").join("vault")
        );
    }

    #[test]
    fn a_plugin_folder_from_before_moves_in_once() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir(home.path().join("calendar")).unwrap();
        fs::write(home.path().join("calendar").join("work.ics"), "events").unwrap();
        let folders = Folders::under(home.path());

        let folder = folders.plugin("calendar", false);

        assert_eq!(folder, home.path().join("data").join("calendar"));
        assert_eq!(
            fs::read_to_string(folder.join("work.ics")).unwrap(),
            "events"
        );
        assert!(!home.path().join("calendar").exists());
        fs::create_dir(home.path().join("plugins")).unwrap();
        assert_eq!(
            folders.plugin("plugins", false),
            home.path().join("data").join("plugins")
        );
        assert!(home.path().join("plugins").is_dir());
        assert_eq!(
            folders.plugin("remote", true),
            home.path().join("cache").join("plugins").join("remote")
        );
    }
}
