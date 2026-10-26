use std::fs;
use std::path::Path;

use ox_lang::value::print;
use oxmacs_core::editor::{Editor, Effect};

use crate::Ox;
use crate::testing::{press, settle, typed};

const PASSWORD: &str = "correct horse";

fn cheap_vault(home: &Path, entries: &str) {
    let mut isolate = ox_library::isolate();
    let made = isolate
        .eval(&format!(
            "(module (import (ox crypto) (ox json)))
(let [salt (crypto/random 16)
      key (await (crypto/derive-key {PASSWORD:?} salt :memory 64 :iterations 1 :parallelism 1))]
  (json/write {{version: 1 salt: salt memory: 64 iterations: 1 parallelism: 1
               sealed: (crypto/seal key (json/write {{entries: {entries}}}))}}))"
        ))
        .unwrap();
    fs::write(
        home.join("vault.json"),
        print::display(isolate.heap(), made),
    )
    .unwrap();
}

const ENTRIES: &str = r#"[{id: "1" title: "mail" username: "ana@example.com" password: "hunter2" url: "https://mail.example.com" totp: "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ" notes: "backup codes in the drawer"}
                          {id: "2" title: "bank" username: "ana" password: "s3cret" url: "" totp: "" notes: ""}]"#;

fn started(init: &str) -> (tempfile::TempDir, Editor, Ox) {
    let home = tempfile::tempdir().unwrap();
    fs::write(
        home.path().join("init.ox"),
        format!("(use-plugin vault)\n{init}"),
    )
    .unwrap();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn run(editor: &mut Editor, ox: &mut Ox, command: &str) {
    let id = editor.command_named(command).unwrap();
    editor.execute(id);
    settle(editor, ox);
}

fn answer(editor: &mut Editor, ox: &mut Ox, text: &str) {
    typed(editor, ox, text);
    press(editor, ox, "RET");
}

fn lines(editor: &Editor) -> Vec<String> {
    editor
        .buffer()
        .contents()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn unlocked(init: &str) -> (tempfile::TempDir, Editor, Ox) {
    let (home, mut editor, mut ox) = started(init);
    cheap_vault(home.path(), ENTRIES);
    run(&mut editor, &mut ox, "vault");
    answer(&mut editor, &mut ox, PASSWORD);
    (home, editor, ox)
}

fn clipboard(editor: &mut Editor) -> Vec<String> {
    editor
        .take_effects()
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::WriteClipboard(text) => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn the_first_use_makes_a_vault_sealed_with_the_password() {
    let (home, mut editor, mut ox) = started("");

    run(&mut editor, &mut ox, "vault");
    assert_eq!(
        editor.minibuffer().unwrap().prompt(),
        "New vault password: "
    );
    answer(&mut editor, &mut ox, "a long secret");
    assert_eq!(
        editor.minibuffer().unwrap().prompt(),
        "Same password again: "
    );
    answer(&mut editor, &mut ox, "a long secret");

    assert_eq!(editor.buffer().name(), "*Vault*");
    assert!(
        lines(&editor)[3].starts_with("No entries yet"),
        "{:?}",
        lines(&editor)
    );
    let stored = fs::read_to_string(home.path().join("vault.json")).unwrap();
    assert!(stored.contains("\"sealed\""), "{stored}");
    assert!(stored.contains("\"memory\":65536"), "{stored}");
    assert!(!stored.contains("entries"), "{stored}");
}

#[test]
fn different_passwords_make_no_vault() {
    let (home, mut editor, mut ox) = started("");

    run(&mut editor, &mut ox, "vault");
    answer(&mut editor, &mut ox, "one");
    answer(&mut editor, &mut ox, "two");

    assert_eq!(
        editor.message(),
        Some("The passwords differ; nothing was made")
    );
    assert!(!home.path().join("vault.json").exists());
}

#[test]
fn a_wrong_password_keeps_the_vault_locked() {
    let (home, mut editor, mut ox) = started("");
    cheap_vault(home.path(), ENTRIES);

    run(&mut editor, &mut ox, "vault");
    answer(&mut editor, &mut ox, "wrong");

    assert_eq!(editor.message(), Some("Wrong vault password"));
    assert_ne!(editor.buffer().name(), "*Vault*");
}

#[test]
fn the_list_shows_entries_by_title_without_passwords() {
    let (_home, editor, _ox) = unlocked("");

    let shown = lines(&editor);
    assert_eq!(editor.buffer().name(), "*Vault*");
    assert!(shown[0].starts_with("Vault  2 entries"), "{shown:?}");
    assert!(shown[3].starts_with("bank "), "{shown:?}");
    assert!(shown[4].starts_with("mail ") && shown[4].contains("ana@example.com"));
    assert!(!editor.buffer().contents().contains("hunter2"));
    assert_eq!(editor.buffer().line_of(editor.buffer().point()), 3);
}

#[test]
fn copying_a_password_skips_the_kill_ring_and_clears_later() {
    let (_home, mut editor, mut ox) = unlocked("(use-plugin vault :config {clear-clipboard: 1})");
    editor.take_effects();
    press(&mut editor, &mut ox, "n");

    press(&mut editor, &mut ox, "c");
    assert_eq!(clipboard(&mut editor), ["hunter2"]);
    assert_eq!(
        editor.message(),
        Some("Copied the password; the clipboard clears in 1 seconds")
    );
    assert_eq!(editor.clipboard_history(), Vec::<String>::new());

    std::thread::sleep(std::time::Duration::from_millis(1100));
    settle(&mut editor, &mut ox);
    assert_eq!(clipboard(&mut editor), [""]);
    assert_eq!(editor.message(), Some("Cleared the clipboard"));
}

#[test]
fn entries_show_a_hidden_password_and_a_one_time_code() {
    let (_home, mut editor, mut ox) = unlocked("");
    press(&mut editor, &mut ox, "n RET");

    assert_eq!(editor.buffer().name(), "*Vault entry*");
    let shown = editor.buffer().contents();
    assert!(shown.contains("Password   •••••••"), "{shown}");
    assert!(!shown.contains("hunter2"), "{shown}");
    assert!(shown.contains("backup codes in the drawer"), "{shown}");
    let code = lines(&editor)
        .into_iter()
        .find(|line| line.starts_with("Code"))
        .unwrap();
    assert!(code.ends_with(" s left"), "{code}");
    assert_eq!(
        code[11..17].chars().filter(char::is_ascii_digit).count(),
        6,
        "{code}"
    );

    press(&mut editor, &mut ox, "s");
    assert!(editor.buffer().contents().contains("Password   hunter2"));
    editor.take_effects();
    press(&mut editor, &mut ox, "u");
    assert_eq!(clipboard(&mut editor), ["ana@example.com"]);
}

#[test]
fn added_entries_are_saved_sealed_and_survive_locking() {
    let (home, mut editor, mut ox) = unlocked("");

    press(&mut editor, &mut ox, "a");
    answer(&mut editor, &mut ox, "forum");
    answer(&mut editor, &mut ox, "ana");
    answer(&mut editor, &mut ox, "");
    answer(&mut editor, &mut ox, "https://forum.example.org");
    answer(&mut editor, &mut ox, "");
    assert_eq!(editor.message(), Some("Added forum"));
    assert!(lines(&editor).iter().any(|line| line.starts_with("forum ")));
    assert!(home.path().join("vault.json.bak").exists());

    run(&mut editor, &mut ox, "vault-lock");
    assert_eq!(editor.buffer_named("*Vault*"), None);
    run(&mut editor, &mut ox, "vault");
    answer(&mut editor, &mut ox, PASSWORD);
    let shown = lines(&editor);
    assert!(shown[0].starts_with("Vault  3 entries"), "{shown:?}");
    let generated = ox
        .eval(
            &mut editor,
            "(module (import (vault store :as store))) (count (get (store/find-entry \"forum\") :password))",
        )
        .unwrap();
    assert_eq!(generated, "20");
}

#[test]
fn deleting_asks_first() {
    let (_home, mut editor, mut ox) = unlocked("");

    press(&mut editor, &mut ox, "d");
    assert!(editor.question().is_some());
    press(&mut editor, &mut ox, "y");

    assert_eq!(editor.message(), Some("Deleted bank"));
    assert!(lines(&editor)[0].starts_with("Vault  1 entries"));
}

#[test]
fn other_plugins_ask_the_vault_for_secrets() {
    let (_home, mut editor, mut ox) = unlocked("");

    let secret = ox
        .eval(
            &mut editor,
            "(module (import (vault store :as vault))) [(vault/secret \"bank\") (vault/secret \"nothing\")]",
        )
        .unwrap();

    assert_eq!(secret, r#"["s3cret" nil]"#);
}

#[test]
fn passwords_and_passphrases_are_made_from_random_choices() {
    let (_home, mut editor, mut ox) = started("");
    run(&mut editor, &mut ox, "vault-lock");

    let made = ox
        .eval(
            &mut editor,
            "(module (import (vault store :as store) (ox string))) [(count (store/generate)) (count (string/split (store/passphrase) \"-\")) (not= (store/generate) (store/generate)) (count (store/generate :length 8 :symbols? false))]",
        )
        .unwrap();

    assert_eq!(made, "[20 7 true 8]");
}

#[test]
fn bitwarden_and_one_password_exports_are_imported_once() {
    let (home, mut editor, mut ox) = unlocked("");
    let bitwarden = home.path().join("bitwarden.csv");
    fs::write(
        &bitwarden,
        "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n,,login,shop,,,0,https://shop.example,ana,pw1,\n,,note,secret note,text,,0,,,,\n,,login,mail,,,0,https://mail.example.com,ana@example.com,hunter2,\n",
    )
    .unwrap();
    let one = home.path().join("1password.csv");
    fs::write(
        &one,
        "\"Title\",\"Url\",\"Username\",\"Password\",\"OTPAuth\",\"Favorite\",\"Archived\",\"Tags\",\"Notes\"\n\"git\",\"https://git.example\",\"ana\",\"pw2\",\"\",\"false\",\"false\",\"\",\"\"\n",
    )
    .unwrap();

    run(&mut editor, &mut ox, "vault-import");
    answer(&mut editor, &mut ox, "bitwarden");
    answer(&mut editor, &mut ox, &bitwarden.display().to_string());
    assert_eq!(
        editor.message(),
        Some("Imported 1 of 2 entries; the others were already there")
    );

    run(&mut editor, &mut ox, "vault-import");
    answer(&mut editor, &mut ox, "1password");
    answer(&mut editor, &mut ox, &one.display().to_string());
    assert_eq!(
        editor.message(),
        Some("Imported 1 of 1 entries; the others were already there")
    );
    assert!(
        lines(&editor)[0].starts_with("Vault  4 entries"),
        "{:?}",
        lines(&editor)
    );
}
