use std::path::Path;

use crate::editor::Editor;

#[derive(Debug)]
struct Claim {
    plugin: String,
    extensions: Vec<String>,
    file_names: Vec<String>,
}

impl Claim {
    fn covers(&self, path: &Path) -> bool {
        let named = path.file_name().and_then(|name| name.to_str());
        let extension = path.extension().and_then(|extension| extension.to_str());
        named.is_some_and(|name| {
            self.file_names
                .iter()
                .any(|known| name.eq_ignore_ascii_case(known))
        }) || extension.is_some_and(|extension| {
            self.extensions
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
    }
}

#[derive(Debug, Default)]
pub(in crate::editor) struct Dormant {
    claims: Vec<Claim>,
    told: Vec<String>,
}

impl Editor {
    pub fn add_dormant_mode(&mut self, plugin: &str, extensions: &[String], file_names: &[String]) {
        self.dormant.claims.push(Claim {
            plugin: plugin.to_owned(),
            extensions: extensions.to_vec(),
            file_names: file_names.to_vec(),
        });
    }

    pub fn wake_dormant_modes(&mut self, plugin: &str) {
        self.dormant.claims.retain(|claim| claim.plugin != plugin);
    }

    pub(in crate::editor) fn tell_dormant(&mut self, path: &Path) {
        let Some(plugin) = self
            .dormant
            .claims
            .iter()
            .find(|claim| claim.covers(path))
            .map(|claim| claim.plugin.clone())
        else {
            return;
        };
        if self.dormant.told.contains(&plugin) {
            return;
        }
        let name = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        self.show_message(format!(
            "{name} opens as plain text while the {plugin} plugin is off; (use-plugin {plugin}) in init.ox turns it on"
        ));
        self.dormant.told.push(plugin);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::editor::Editor;

    #[test]
    fn files_of_a_plugin_that_is_off_say_so_once() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.py", "b.PY", "SConstruct", "c.txt"] {
            fs::write(dir.path().join(name), "").unwrap();
        }
        let mut editor = Editor::new();
        editor.add_dormant_mode("python", &["py".to_owned()], &["SConstruct".to_owned()]);

        editor.open_file(&dir.path().join("a.py"));
        assert_eq!(editor.language(), None);
        assert_eq!(
            editor.message(),
            Some(
                "a.py opens as plain text while the python plugin is off; (use-plugin python) in init.ox turns it on"
            )
        );
        editor.show_message("");
        editor.open_file(&dir.path().join("b.PY"));
        editor.open_file(&dir.path().join("SConstruct"));
        assert_eq!(editor.message(), Some(""));

        editor.wake_dormant_modes("python");
        let mut fresh = Editor::new();
        fresh.add_dormant_mode("python", &["py".to_owned()], &[]);
        fresh.wake_dormant_modes("python");
        fresh.open_file(&dir.path().join("a.py"));
        assert_eq!(fresh.message(), None);
    }
}
