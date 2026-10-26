use std::fs::File;

use keepass::db::fields;
use keepass::{Database, DatabaseKey};
use ox_lang::Isolate;
use ox_lang::value::print;
use oxmacs_core::editor::Editor;
use serde_json::{Value as Json, json};

fn field(value: Option<&str>) -> Json {
    value.map_or(Json::Null, |text| Json::String(text.to_owned()))
}

fn read(path: &str, password: &str) -> Result<Json, String> {
    let mut file = File::open(path).map_err(|error| format!("Couldn't open {path}: {error}"))?;
    let key = DatabaseKey::new().with_password(password);
    let database = Database::open(&mut file, key).map_err(|error| {
        format!("Couldn't open the KeePass database (wrong password?): {error}")
    })?;
    let binned = database.recycle_bin().map(|bin| bin.id());
    let entries: Vec<Json> = database
        .iter_all_entries()
        .filter(|entry| binned.is_none_or(|bin| entry.parent().id() != bin))
        .map(|entry| {
            let group = entry.parent();
            json!({
                "title": field(entry.get_title()),
                "username": field(entry.get_username()),
                "password": field(entry.get_password()),
                "url": field(entry.get_url()),
                "notes": field(entry.get(fields::NOTES)),
                "totp": field(entry.get_raw_otp_value()),
                "group": group.name.clone(),
            })
        })
        .collect();
    Ok(Json::Array(entries))
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "vault-read-kdbx")
        .arity(2, Some(2))
        .host::<Editor>(|heap, _, arguments| {
            let path = print::display(heap, arguments[0]);
            let password = print::display(heap, arguments[1]);
            heap.background_json("vault-read-kdbx", move || read(&path, &password))
        });
}

#[cfg(test)]
mod tests {
    use keepass::db::{GroupMut, fields};
    use keepass::{Database, DatabaseKey};
    use oxmacs_core::editor::Editor;

    use crate::Ox;
    use crate::testing::settle;

    #[test]
    fn keepass_databases_are_read_without_their_recycle_bin() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.kdbx");
        let mut database = Database::new();
        {
            let mut root: GroupMut<'_> = database.root_mut();
            let mut entry = root.add_entry();
            entry.set_unprotected(fields::TITLE, "mail");
            entry.set_unprotected(fields::USERNAME, "ana");
            entry.set_protected(fields::PASSWORD, "hunter2");
            entry.set_unprotected(fields::URL, "https://mail.example.com");
        }
        database
            .save(
                &mut std::fs::File::create(&path).unwrap(),
                DatabaseKey::new().with_password("open sesame"),
            )
            .unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        ox.eval(
            &mut editor,
            &format!(
                "(def found (atom nil)) (spawn (reset! found (await (vault-read-kdbx {:?} \"open sesame\"))))",
                path.display().to_string()
            ),
        )
        .unwrap();
        settle(&mut editor, &mut ox);

        assert_eq!(
            ox.eval(
                &mut editor,
                "(let [entry (first @found)] [(count @found) entry.title entry.username entry.password entry.url])"
            )
            .unwrap(),
            r#"[1 "mail" "ana" "hunter2" "https://mail.example.com"]"#
        );
        ox.eval(
            &mut editor,
            &format!(
                "(spawn (reset! found (try (await (vault-read-kdbx {:?} \"wrong\")) (catch error e e.message))))",
                path.display().to_string()
            ),
        )
        .unwrap();
        settle(&mut editor, &mut ox);
        let failed = ox.eval(&mut editor, "@found").unwrap();
        assert!(
            failed.starts_with("\"Couldn't open the KeePass database"),
            "{failed}"
        );
    }
}
