use std::fs;

use oxmacs_core::config::schema::SETTINGS;
use oxmacs_core::editor::{Editor, Effect};

use crate::Ox;
use crate::testing::{at_point, press, settle, typed};

fn started(files: &[(&str, &str)]) -> (tempfile::TempDir, Editor, Ox) {
    let home = tempfile::tempdir().unwrap();
    fs::write(home.path().join("init.ox"), "(use-plugin weather)\n").unwrap();
    for (name, text) in files {
        let text = if *name == "init.ox" {
            format!("(use-plugin weather)\n{text}")
        } else {
            (*text).to_owned()
        };
        fs::write(home.path().join(name), text).unwrap();
    }
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn customizing(files: &[(&str, &str)]) -> (tempfile::TempDir, Editor, Ox) {
    let (home, mut editor, mut ox) = started(files);
    press(&mut editor, &mut ox, "M-x");
    typed(&mut editor, &mut ox, "customize");
    press(&mut editor, &mut ox, "RET");
    (home, editor, ox)
}

fn lines(editor: &Editor) -> Vec<String> {
    editor
        .buffer()
        .contents()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn line_of(editor: &Editor, key: &str) -> usize {
    lines(editor)
        .iter()
        .position(|line| {
            line.get(2..)
                .is_some_and(|rest| rest.starts_with(&format!("{key} ")))
        })
        .unwrap_or_else(|| panic!("{key} missing: {:#?}", lines(editor)))
}

fn row(editor: &Editor, key: &str) -> String {
    lines(editor)[line_of(editor, key)].clone()
}

fn on(editor: &mut Editor, key: &str) {
    let line = line_of(editor, key);
    let buffer = editor.buffer_mut();
    buffer.set_point(buffer.line_start(line));
}

fn custom_ox(home: &tempfile::TempDir) -> String {
    fs::read_to_string(home.path().join("custom.ox")).unwrap()
}

#[test]
fn customize_lists_every_setting_by_group_with_plugins_after() {
    let (_home, editor, _ox) = customizing(&[]);

    assert_eq!(editor.buffer().name(), "*Settings*");
    assert!(editor.buffer().is_read_only());
    let shown = lines(&editor);
    let title = &editor.pane().header_lines()[0].0;
    assert!(title.starts_with("Settings  "), "{title}");
    assert!(title.ends_with("custom.ox"), "{title}");
    for group in ["Appearance", "Editing", "Files", "Rust", "weather plugin"] {
        assert!(shown.iter().any(|line| line == group), "{group}");
    }
    for setting in SETTINGS {
        line_of(&editor, setting.key);
    }
    assert!(row(&editor, "theme").ends_with(" [follow OS]"));
    assert_eq!(
        shown[line_of(&editor, "theme") + 1],
        "    follow OS | RET picks a theme with a live preview"
    );
    assert_eq!(
        shown[line_of(&editor, "tab-width") + 1],
        "    Columns per tab character"
    );
    assert!(row(&editor, "animations").ends_with(" [x]"));
    assert!(row(&editor, "tab-width").ends_with(" 8"));
    assert!(row(&editor, "units").ends_with(" [metric]"));
    assert_eq!(
        shown[line_of(&editor, "units") + 1],
        "    metric | imperial"
    );
    assert!(row(&editor, "location").ends_with(" (unset)"));
    assert!(editor.mode_line().starts_with("%% *Settings*"));
}

#[test]
fn n_and_p_move_between_settings_and_explain_them() {
    let (_home, mut editor, mut ox) = customizing(&[]);
    assert!(at_point(&editor).starts_with("[follow OS]"));

    press(&mut editor, &mut ox, "n");
    assert!(at_point(&editor).starts_with("(bundled)"));
    assert_eq!(
        editor.message(),
        Some("Monospaced font name; leave empty for the bundled JetBrains Mono")
    );

    press(&mut editor, &mut ox, "p p");
    let buffer = editor.buffer();
    let last = buffer.line_of(buffer.point());
    assert_eq!(last, lines(&editor).len() - 2);

    press(&mut editor, &mut ox, "TAB S-TAB");
    let buffer = editor.buffer();
    assert_eq!(buffer.line_of(buffer.point()), last);
}

#[test]
fn return_toggles_switches_cycles_choices_and_marks_them_unsaved() {
    let (_home, mut editor, mut ox) = customizing(&[]);
    editor.take_effects();

    on(&mut editor, "animations");
    press(&mut editor, &mut ox, "RET");
    assert!(!editor.config().animations);
    assert!(row(&editor, "animations").starts_with("* animations"));
    assert!(row(&editor, "animations").contains("[ ]"));
    assert!(editor.take_effects().contains(&Effect::Reconfigure));
    assert!(editor.mode_line().starts_with("%* *Settings*"));

    on(&mut editor, "line-numbers");
    press(&mut editor, &mut ox, "RET");
    assert!(row(&editor, "line-numbers").contains("[relative]"));
    press(&mut editor, &mut ox, "C-n RET");
    assert!(row(&editor, "line-numbers").contains("[off]"));
}

#[test]
fn the_theme_is_picked_with_a_live_preview() {
    let (_home, mut editor, mut ox) = customizing(&[]);
    on(&mut editor, "theme");

    press(&mut editor, &mut ox, "RET");
    assert_eq!(editor.minibuffer().unwrap().prompt(), "Load theme: ");
    typed(&mut editor, &mut ox, "dark");
    press(&mut editor, &mut ox, "RET");

    assert_eq!(editor.config().theme.as_deref(), Some("adwaita-dark"));
    assert!(row(&editor, "theme").starts_with("* theme"));
    assert!(row(&editor, "theme").contains("[adwaita-dark]"));
}

#[test]
fn numbers_are_read_and_checked_in_the_minibuffer() {
    let (_home, mut editor, mut ox) = customizing(&[]);
    on(&mut editor, "tab-width");

    press(&mut editor, &mut ox, "RET");
    assert_eq!(editor.minibuffer().unwrap().prompt(), "tab-width: ");
    press(&mut editor, &mut ox, "C-a C-k 4 RET");
    assert_eq!(editor.tab_width(), 4);

    press(&mut editor, &mut ox, "RET C-a C-k 0 RET");
    assert_eq!(editor.message(), Some("tab-width must be at least 1"));
    assert_eq!(editor.tab_width(), 4);

    press(&mut editor, &mut ox, "RET C-a C-k");
    typed(&mut editor, &mut ox, "wide");
    press(&mut editor, &mut ox, "RET");
    assert_eq!(editor.message(), Some("tab-width needs a whole number"));
}

#[test]
fn text_settings_can_be_set_and_unset() {
    let (_home, mut editor, mut ox) = customizing(&[]);
    on(&mut editor, "font-family");

    press(&mut editor, &mut ox, "RET");
    typed(&mut editor, &mut ox, "Iosevka");
    press(&mut editor, &mut ox, "RET");
    assert_eq!(editor.config().font_family.as_deref(), Some("Iosevka"));
    assert!(row(&editor, "font-family").contains("Iosevka"));

    press(&mut editor, &mut ox, "RET C-a C-k RET");
    assert_eq!(editor.config().font_family, None);
    assert!(row(&editor, "font-family").contains("(bundled)"));
}

#[test]
fn return_on_a_hint_changes_the_setting_above_it() {
    let (_home, mut editor, mut ox) = customizing(&[]);
    on(&mut editor, "clock");

    press(&mut editor, &mut ox, "C-n RET");

    assert!(!editor.config().clock);
}

#[test]
fn saving_writes_custom_ox_and_runs_the_init_files_again() {
    let (home, mut editor, mut ox) = customizing(&[("init.ox", "(settings {tab-width: 4})")]);
    on(&mut editor, "clock");
    press(&mut editor, &mut ox, "RET");
    on(&mut editor, "font-family");
    press(&mut editor, &mut ox, "RET");
    typed(&mut editor, &mut ox, "Iosevka \"Term\"");
    press(&mut editor, &mut ox, "RET");

    press(&mut editor, &mut ox, "C-x C-s");

    assert_eq!(
        custom_ox(&home),
        ";; Written by M-x customize. Your own settings belong in init.ox,\n;; which runs after this file and wins.\n\n(settings {font-family: \"Iosevka \\\"Term\\\"\"\n           clock: false})\n"
    );
    let message = editor.message().unwrap();
    assert!(
        message.starts_with("Wrote ") && message.ends_with("custom.ox"),
        "{message}"
    );
    assert!(editor.mode_line().starts_with("%% *Settings*"));
    assert!(row(&editor, "clock").starts_with("  clock"));
    assert!(!editor.config().clock);
    assert_eq!(editor.tab_width(), 4);
}

#[test]
fn settings_init_ox_overrides_say_so_and_survive_saving() {
    let (home, mut editor, mut ox) = customizing(&[
        ("custom.ox", "(settings {tab-width: 2 splash: false})"),
        ("init.ox", "(settings {tab-width: 4})"),
    ]);
    assert!(
        row(&editor, "tab-width").ends_with(" 4  set in init.ox"),
        "{}",
        row(&editor, "tab-width")
    );
    on(&mut editor, "clock");
    press(&mut editor, &mut ox, "RET");

    press(&mut editor, &mut ox, "C-x C-s");

    let written = custom_ox(&home);
    assert!(written.contains("tab-width: 2"), "{written}");
    assert!(written.contains("splash: false"), "{written}");
    assert!(written.contains("clock: false"), "{written}");
}

#[test]
fn saving_without_changes_says_so() {
    let (home, mut editor, mut ox) = customizing(&[]);

    press(&mut editor, &mut ox, "C-x C-s");

    assert_eq!(editor.message(), Some("(No changes need to be saved)"));
    assert!(!home.path().join("custom.ox").exists());
}

#[test]
fn reverting_returns_to_the_saved_settings() {
    let (_home, mut editor, mut ox) = customizing(&[("init.ox", "(settings {tab-width: 4})")]);
    on(&mut editor, "clock");
    press(&mut editor, &mut ox, "RET");

    press(&mut editor, &mut ox, "g");

    assert!(editor.config().clock);
    assert_eq!(editor.tab_width(), 4);
    assert!(editor.mode_line().starts_with("%% *Settings*"));
}

#[test]
fn settings_changed_elsewhere_show_as_unsaved() {
    let (_home, mut editor, mut ox) = started(&[]);
    let toggle = editor.command_named("toggle-theme").unwrap();
    editor.execute(toggle);

    press(&mut editor, &mut ox, "M-x");
    typed(&mut editor, &mut ox, "customize");
    press(&mut editor, &mut ox, "RET");

    assert!(row(&editor, "theme").starts_with("* theme"));
    assert!(row(&editor, "theme").contains("[adwaita-dark]"));
    assert!(editor.mode_line().starts_with("%* *Settings*"));
}

#[test]
fn plugin_settings_change_and_save_as_settings() {
    let (home, mut editor, mut ox) = customizing(&[]);
    on(&mut editor, "units");
    press(&mut editor, &mut ox, "RET");
    assert!(row(&editor, "units").starts_with("* units"));
    assert!(row(&editor, "units").contains("[imperial]"));
    on(&mut editor, "location");
    press(&mut editor, &mut ox, "RET");
    typed(&mut editor, &mut ox, "Oslo");
    press(&mut editor, &mut ox, "RET");

    press(&mut editor, &mut ox, "C-x C-s");

    assert!(
        custom_ox(&home).ends_with("\n(settings 'weather {location: \"Oslo\" units: :imperial})\n"),
        "{}",
        custom_ox(&home)
    );
    assert_eq!(
        ox.eval(&mut editor, "(plugin-setting 'weather :units)")
            .unwrap(),
        ":imperial"
    );
    assert!(row(&editor, "units").starts_with("  units"));
}
