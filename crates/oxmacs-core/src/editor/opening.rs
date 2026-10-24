use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use strum::VariantArray;

use crate::command::{Command, Scope};
use crate::editor::Editor;
use crate::editor::test_support::editor_with;
use crate::syntax::Language;
use crate::text::{Buffer, BufferId};

const BUDGET: Duration = Duration::from_millis(50);
const TRIES: usize = 3;
const STARTS: [(Scope, Option<Language>, &str); 2] = [
    (Scope::Global, None, ""),
    (Scope::Rust, Some(Language::RUST), "fn main() {}\n"),
];

fn start(folder: &Path, language: Option<Language>, text: &str) -> Editor {
    let mut editor = editor_with(text, 0);
    editor.set_major_mode(language);
    editor.set_working_directory(folder.to_owned());
    editor.init_path = Some(folder.join("init.ox"));
    editor
}

fn buffer_ids(editor: &Editor) -> Vec<BufferId> {
    (0..editor.buffers.len())
        .filter_map(|index| editor.buffers.get(index))
        .map(Buffer::id)
        .collect()
}

fn opening(
    folder: &Path,
    language: Option<Language>,
    text: &str,
    command: Command,
) -> Option<(String, Duration)> {
    let mut fastest: Option<(String, Duration)> = None;
    for _ in 0..TRIES {
        let mut editor = start(folder, language, text);
        let before = buffer_ids(&editor);
        let begun = Instant::now();
        editor.execute(command);
        if editor.minibuffer().is_some() {
            editor.execute(Command::ExitMinibuffer);
        }
        let took = begun.elapsed();
        let opened = (0..editor.buffers.len())
            .filter_map(|index| editor.buffers.get(index))
            .find(|buffer| !before.contains(&buffer.id()))?
            .name()
            .to_owned();
        if fastest.as_ref().is_none_or(|(_, best)| took < *best) {
            fastest = Some((opened, took));
        }
    }
    fastest
}

#[test]
fn every_buffer_opens_quickly() {
    let folder = tempfile::tempdir().unwrap();
    fs::write(folder.path().join("notes.md"), "# Notes\n").unwrap();
    let mut opened = Vec::new();
    let mut slow = Vec::new();
    for (scope, language, text) in STARTS {
        for command in Command::VARIANTS
            .iter()
            .copied()
            .filter(|command| command.scope() == scope)
        {
            let Some((buffer, took)) = opening(folder.path(), language, text, command) else {
                continue;
            };
            if took > BUDGET {
                slow.push(format!("{} opened {buffer} in {took:?}", command.name()));
            }
            opened.push(buffer);
        }
    }

    assert!(opened.len() >= 14, "{opened:?}");
    assert!(slow.is_empty(), "slower than {BUDGET:?}: {slow:#?}");
}
