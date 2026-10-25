use oxmacs_core::editor::Editor;

use crate::Ox;
use crate::tabbar::{self, TabBar, TabEvent};
use crate::testing::{press, settle, typed};

fn started(init: &str) -> (tempfile::TempDir, Editor, Ox) {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("init.ox"), init).unwrap();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn tabbed() -> (tempfile::TempDir, Editor, Ox) {
    started("(use-plugin tabs)")
}

fn bar(editor: &Editor) -> Option<TabBar> {
    tabbar::shown(editor).cloned()
}

fn tabs(names: &[&str], selected: usize) -> Option<TabBar> {
    Some(TabBar {
        tabs: names.iter().map(|name| (*name).to_owned()).collect(),
        selected: Some(selected),
    })
}

fn visit(editor: &mut Editor, ox: &mut Ox, name: &str) {
    press(editor, ox, "C-x b");
    typed(editor, ox, name);
    press(editor, ox, "RET");
}

fn shown(editor: &Editor) -> Vec<String> {
    editor
        .tiling()
        .windows()
        .into_iter()
        .map(|window| editor.pane_of(window).unwrap().buffer().name().to_owned())
        .collect()
}

#[test]
fn the_bar_waits_for_a_second_tab_unless_always_shown() {
    let (_home, editor, _ox) = tabbed();
    assert_eq!(bar(&editor), None);

    let (_home, editor, _ox) = started("(use-plugin tabs :config {show: :always})");
    assert_eq!(bar(&editor), tabs(&["*scratch*"], 0));
}

#[test]
fn each_tab_keeps_its_own_windows() {
    let (_home, mut editor, mut ox) = tabbed();
    visit(&mut editor, &mut ox, "one");
    press(&mut editor, &mut ox, "C-x 3");

    press(&mut editor, &mut ox, "C-x t 2");
    assert_eq!(shown(&editor), ["one"]);
    visit(&mut editor, &mut ox, "two");
    assert_eq!(bar(&editor), tabs(&["one", "two"], 1));

    press(&mut editor, &mut ox, "C-x t o");
    assert_eq!(shown(&editor), ["one", "one"]);
    assert_eq!(bar(&editor), tabs(&["one", "two"], 0));

    press(&mut editor, &mut ox, "C-TAB");
    assert_eq!(shown(&editor), ["two"]);
    assert_eq!(bar(&editor), tabs(&["one", "two"], 1));
}

#[test]
fn closed_tabs_come_back_with_their_windows() {
    let (_home, mut editor, mut ox) = tabbed();
    visit(&mut editor, &mut ox, "one");
    press(&mut editor, &mut ox, "C-x t 2 C-x 2");
    visit(&mut editor, &mut ox, "two");

    press(&mut editor, &mut ox, "C-x t 0");
    assert_eq!(bar(&editor), None);
    assert_eq!(shown(&editor), ["one"]);

    press(&mut editor, &mut ox, "C-x t u");
    assert_eq!(bar(&editor), tabs(&["one", "two"], 1));
    assert_eq!(shown(&editor), ["two", "one"]);

    press(&mut editor, &mut ox, "C-x t 0 C-x t 0");
    assert_eq!(editor.message(), Some("This is the only tab"));
}

#[test]
fn tabs_can_be_named_switched_to_and_moved() {
    let (_home, mut editor, mut ox) = tabbed();
    visit(&mut editor, &mut ox, "one");
    press(&mut editor, &mut ox, "C-x t 2");
    visit(&mut editor, &mut ox, "two");

    press(&mut editor, &mut ox, "C-x t r C-a C-k");
    typed(&mut editor, &mut ox, "work");
    press(&mut editor, &mut ox, "RET");
    assert_eq!(bar(&editor), tabs(&["one", "work"], 1));

    press(&mut editor, &mut ox, "C-x t RET");
    typed(&mut editor, &mut ox, "one");
    press(&mut editor, &mut ox, "RET");
    assert_eq!(bar(&editor), tabs(&["one", "work"], 0));

    press(&mut editor, &mut ox, "C-x t m");
    assert_eq!(bar(&editor), tabs(&["work", "one"], 1));
    assert_eq!(shown(&editor), ["one"]);
}

#[test]
fn the_bar_selects_closes_adds_and_moves_tabs() {
    let (_home, mut editor, mut ox) = tabbed();
    visit(&mut editor, &mut ox, "one");
    press(&mut editor, &mut ox, "C-x t 2");
    visit(&mut editor, &mut ox, "two");

    ox.tab_event(&mut editor, TabEvent::Select(0));
    settle(&mut editor, &mut ox);
    assert_eq!(shown(&editor), ["one"]);

    ox.tab_event(&mut editor, TabEvent::New);
    settle(&mut editor, &mut ox);
    assert_eq!(bar(&editor), tabs(&["one", "one", "two"], 1));

    ox.tab_event(&mut editor, TabEvent::Move { from: 2, to: 0 });
    settle(&mut editor, &mut ox);
    assert_eq!(bar(&editor), tabs(&["two", "one", "one"], 2));

    ox.tab_event(&mut editor, TabEvent::Close(0));
    settle(&mut editor, &mut ox);
    assert_eq!(bar(&editor), tabs(&["one", "one"], 1));
}
