use std::time::{Duration, Instant};

use oxmacs_core::editor::Editor;

use crate::Ox;
use crate::api::games;
use crate::testing::{press, settle, typed};

fn started() -> (tempfile::TempDir, Editor, Ox) {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("init.ox"), "(use-plugin rodeo)").unwrap();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn command(editor: &mut Editor, ox: &mut Ox, name: &str) {
    press(editor, ox, "M-x");
    typed(editor, ox, name);
    press(editor, ox, "RET");
}

fn status(editor: &Editor) -> Option<String> {
    let id = editor.buffer_named("*game*")?;
    games::screen(editor, id)?.status.clone()
}

#[test]
fn a_new_game_plays_beside_its_code_and_hears_keys() {
    let (home, mut editor, mut ox) = started();
    let path = home.path().join("jump.ox");
    let shown = path.display().to_string().replace('\\', "/");

    command(&mut editor, &mut ox, "rodeo-new");
    press(&mut editor, &mut ox, "C-a C-k");
    typed(&mut editor, &mut ox, &shown);
    press(&mut editor, &mut ox, "RET");
    settle(&mut editor, &mut ox);
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("(def (tick)")
    );

    command(&mut editor, &mut ox, "rodeo-play");
    let start = Instant::now();
    ox.games_frame(&mut editor, start);
    let id = editor.buffer_named("*game*").unwrap();
    let screen = games::screen(&editor, id).unwrap();
    assert_eq!(&screen.pixels[..4], &[0x1d, 0x2b, 0x53, 255]);
    assert_eq!(screen.captions[0].text, "Arrows move, space jumps");

    games::key(&mut editor, "right", true);
    ox.games_frame(&mut editor, start + Duration::from_millis(500));
    let moved = ox.eval(&mut editor, "(get @player :x)").unwrap();
    assert_eq!(moved, "62.49999910000003");

    command(&mut editor, &mut ox, "rodeo-pause");
    ox.games_frame(&mut editor, start + Duration::from_millis(520));
    assert_eq!(status(&editor).as_deref(), Some("Paused"));
    command(&mut editor, &mut ox, "rodeo-stop");
    assert!(editor.buffer_named("*game*").is_none());
}
