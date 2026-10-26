use oxmacs_core::editor::{Editor, Effect, Presentation};

use crate::Ox;
use crate::testing::{press, settle, typed};

fn started(config: &str) -> (tempfile::TempDir, Editor, Ox) {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join("init.ox"),
        format!("(use-plugin zen{config})"),
    )
    .unwrap();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn zen(editor: &mut Editor, ox: &mut Ox) {
    press(editor, ox, "M-x");
    typed(editor, ox, "zen");
    press(editor, ox, "RET");
}

fn fullscreen_asked(editor: &mut Editor) -> Vec<bool> {
    editor
        .take_effects()
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::Fullscreen(on) => Some(on),
            _ => None,
        })
        .collect()
}

#[test]
fn zen_shows_one_centered_window_and_brings_the_others_back() {
    let (_home, mut editor, mut ox) = started("");
    press(&mut editor, &mut ox, "C-x 3 C-x b");
    typed(&mut editor, &mut ox, "draft");
    press(&mut editor, &mut ox, "RET");

    zen(&mut editor, &mut ox);

    assert_eq!(
        editor.presentation(),
        Presentation {
            column: Some(80),
            hide_chrome: true,
            dim: true,
            typewriter: true,
            fullscreen: true,
        }
    );
    assert_eq!(editor.tiling().windows().len(), 1);
    assert_eq!(editor.buffer().name(), "draft");
    assert!(editor.buffer().word_wrap());
    assert_eq!(fullscreen_asked(&mut editor), [true]);

    zen(&mut editor, &mut ox);

    assert_eq!(editor.presentation(), Presentation::default());
    assert_eq!(editor.tiling().windows().len(), 2);
    assert_eq!(editor.buffer().name(), "draft");
    assert!(!editor.buffer().word_wrap());
    assert_eq!(fullscreen_asked(&mut editor), [false]);
}

#[test]
fn each_part_of_zen_follows_its_setting() {
    let (_home, mut editor, mut ox) = started(
        " :config {width: 60 hide-chrome: false dim: false typewriter: false fullscreen: false wrap-words: false}",
    );
    press(&mut editor, &mut ox, "C-x 3");

    zen(&mut editor, &mut ox);

    assert_eq!(
        editor.presentation(),
        Presentation {
            column: Some(60),
            ..Presentation::default()
        }
    );
    assert_eq!(editor.tiling().windows().len(), 2);
    assert!(!editor.buffer().word_wrap());
    assert_eq!(fullscreen_asked(&mut editor), Vec::<bool>::new());
}
