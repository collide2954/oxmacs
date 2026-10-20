use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const TABLE: &str = "atlas";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Vault {
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl Vault {
    pub fn directory(&self, home: Option<&Path>) -> PathBuf {
        match (self.path.strip_prefix("~"), home) {
            (Some(rest), Some(home)) => home.join(rest.trim_start_matches(['/', '\\'])),
            _ => PathBuf::from(&self.path),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct Unchecked {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    vault: Vec<Vault>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "Unchecked", into = "Unchecked")]
pub struct AtlasSettings {
    pub vault: Vec<Vault>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem(String);

impl fmt::Display for Problem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<Unchecked> for AtlasSettings {
    type Error = Problem;

    fn try_from(unchecked: Unchecked) -> Result<Self, Problem> {
        for (index, vault) in unchecked.vault.iter().enumerate() {
            if vault.path.trim().is_empty() {
                return Err(Problem(format!("{} needs a path", vault.name)));
            }
            if unchecked.vault[..index]
                .iter()
                .any(|earlier| earlier.name == vault.name)
            {
                return Err(Problem(format!("two vaults are named {}", vault.name)));
            }
        }
        Ok(Self {
            vault: unchecked.vault,
        })
    }
}

impl From<AtlasSettings> for Unchecked {
    fn from(settings: AtlasSettings) -> Self {
        Self {
            vault: settings.vault,
        }
    }
}

pub fn add_vault(document: &mut toml_edit::DocumentMut, vault: &Vault) -> Result<(), String> {
    let fields = toml::to_string(vault).map_err(|error| error.to_string())?;
    let parsed: toml_edit::DocumentMut = fields
        .parse()
        .map_err(|error: toml_edit::TomlError| error.message().to_owned())?;
    let mut table = parsed.as_table().clone();
    table.set_implicit(false);
    let atlas = document
        .entry(TABLE)
        .or_insert_with(|| {
            let mut table = toml_edit::Table::new();
            table.set_implicit(true);
            toml_edit::Item::Table(table)
        })
        .as_table_mut()
        .ok_or("[atlas] is not a table")?;
    match atlas.get_mut("vault") {
        Some(toml_edit::Item::ArrayOfTables(vaults)) => vaults.push(table),
        Some(_) => return Err("[atlas] vault is not a list of vaults".to_owned()),
        None => {
            let mut vaults = toml_edit::ArrayOfTables::new();
            vaults.push(table);
            atlas.insert("vault", toml_edit::Item::ArrayOfTables(vaults));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxmacs_core::config::Config;

    fn atlas(text: &str) -> Result<AtlasSettings, String> {
        Config::parse(text)
            .map_err(|error| error.to_string())?
            .applet::<AtlasSettings>(TABLE)
    }

    #[test]
    fn vaults_are_named_folders() {
        let settings = atlas(
            "[[atlas.vault]]\nname = \"personal\"\npath = \"~/Atlas\"\n\n[[atlas.vault]]\nname = \"work\"\npath = \"C:/Work/notes\"\ncolor = \"blue\"\n",
        )
        .unwrap();

        let [personal, work] = &settings.vault[..] else {
            panic!("{:?}", settings.vault);
        };
        assert_eq!(personal.name, "personal");
        assert_eq!(personal.color, None);
        assert_eq!(work.color.as_deref(), Some("blue"));
        let home = Path::new("/home/brett");
        assert_eq!(personal.directory(Some(home)), home.join("Atlas"));
        assert_eq!(work.directory(Some(home)), PathBuf::from("C:/Work/notes"));
    }

    #[test]
    fn the_old_vault_list_still_works() {
        let settings = atlas("[[atlas-vault]]\nname = \"personal\"\npath = \"~/Atlas\"\n").unwrap();

        assert_eq!(settings.vault[0].name, "personal");
    }

    #[test]
    fn vaults_need_unique_names_and_a_path() {
        let twice = "[[atlas.vault]]\nname = \"a\"\npath = \"x\"\n[[atlas.vault]]\nname = \"a\"\npath = \"y\"\n";
        assert_eq!(atlas(twice).unwrap_err(), "[atlas] two vaults are named a");
        assert_eq!(
            atlas("[[atlas.vault]]\nname = \"a\"\npath = \"\"\n").unwrap_err(),
            "[atlas] a needs a path"
        );
    }

    #[test]
    fn vaults_are_added_in_place() {
        let mut document: toml_edit::DocumentMut = "theme = \"adwaita\"\n".parse().unwrap();
        let vault = Vault {
            name: "personal".to_owned(),
            path: "~/Atlas".to_owned(),
            color: None,
        };

        add_vault(&mut document, &vault).unwrap();

        let added = document.to_string();
        assert_eq!(
            added,
            "theme = \"adwaita\"\n\n[[atlas.vault]]\nname = \"personal\"\npath = \"~/Atlas\"\n"
        );
        assert_eq!(atlas(&added).unwrap().vault, [vault]);
    }
}
