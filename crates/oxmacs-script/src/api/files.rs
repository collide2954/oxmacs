use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ox_lang::Isolate;
use ox_lang::native::text;
use ox_lang::portable::{Failure, Portable};
use oxmacs_core::editor::Editor;
use oxmacs_core::files::paths;

use crate::api::prompts::{Answer, Inbox, reply};

fn moved(editor: &mut Editor, from: &Path, to: &Path) -> Answer {
    match fs::rename(from, to) {
        Ok(()) => {
            editor.file_moved(from, to);
            Answer::Choice(true)
        }
        Err(error) => Answer::Failed(format!("Couldn't move {}: {error}", from.display())),
    }
}

pub fn install(isolate: &mut Isolate, inbox: &Inbox) {
    isolate
        .native("ox", "delete-path")
        .suspends()
        .arity(1, Some(1))
        .outside()
        .host::<Editor>(|heap, editor, arguments| {
            let path = PathBuf::from(text(heap, "delete-path", arguments[0])?);
            let trash = editor.deletes_to_trash();
            heap.background("ox delete-path", move || {
                let done = if trash {
                    ox_library::to_trash(&path)
                } else {
                    ox_library::remove(&path)
                };
                done.map(|()| Portable::Nil).map_err(|error| {
                    Failure::from(format!("Couldn't delete {}: {error}", path.display()))
                })
            })
        });
    let asking = Arc::clone(inbox);
    isolate
        .native("ox", "move-path")
        .suspends()
        .arity(2, Some(2))
        .outside()
        .host::<Editor>(move |heap, editor, arguments| {
            let from = PathBuf::from(text(heap, "move-path", arguments[0])?);
            let to = PathBuf::from(text(heap, "move-path", arguments[1])?);
            let (promise, reply) = reply(heap, &asking)?;
            if fs::symlink_metadata(&to).is_err() {
                reply.send(moved(editor, &from, &to));
                return Ok(promise);
            }
            let shown = paths::abbreviate(&to, std::env::home_dir().as_deref());
            editor.ask(
                &format!("{shown} exists; replace it? "),
                move |editor, yes| {
                    reply.send(if yes {
                        moved(editor, &from, &to)
                    } else {
                        Answer::Choice(false)
                    });
                },
            );
            Ok(promise)
        });
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oxmacs_core::editor::Editor;

    use crate::Ox;
    use crate::testing::{question, settle};

    fn literal(path: &std::path::Path) -> String {
        format!("{:?}", path.display().to_string())
    }

    #[test]
    fn moving_carries_visited_files_along_and_asks_before_replacing() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to, taken) = (
            dir.path().join("a.txt"),
            dir.path().join("b.txt"),
            dir.path().join("c.txt"),
        );
        fs::write(&from, "a").unwrap();
        fs::write(&taken, "c").unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        editor.open_file(&from);

        ox.eval(
            &mut editor,
            &format!(
                "(def first (spawn (move-path {} {})))",
                literal(&from),
                literal(&to)
            ),
        )
        .unwrap();
        settle(&mut editor, &mut ox);

        assert_eq!(ox.eval(&mut editor, "(await first)").unwrap(), "true");
        assert_eq!(editor.buffer().file().unwrap().path, to);
        ox.eval(
            &mut editor,
            &format!(
                "(def second (spawn (move-path {} {})))",
                literal(&to),
                literal(&taken)
            ),
        )
        .unwrap();
        settle(&mut editor, &mut ox);
        assert!(
            question(&editor)
                .unwrap()
                .contains("c.txt exists; replace it?")
        );
        crate::testing::press(&mut editor, &mut ox, "n");
        assert_eq!(ox.eval(&mut editor, "(await second)").unwrap(), "false");
        assert_eq!(fs::read_to_string(&taken).unwrap(), "c");
    }

    #[test]
    fn deleting_follows_the_delete_to_trash_setting() {
        let dir = tempfile::tempdir().unwrap();
        let doomed = dir.path().join("gone.txt");
        fs::write(&doomed, "x").unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        ox.eval(
            &mut editor,
            &format!(
                "(settings {{delete-to-trash: false}}) (await (delete-path {}))",
                literal(&doomed)
            ),
        )
        .unwrap();

        assert!(!doomed.exists());
    }
}
