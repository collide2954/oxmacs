use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::dired::Mark;
use crate::editor::Editor;
use crate::editor::dired::operations::files;
use crate::editor::minibuffer::Purpose;
use crate::files::{ops, paths};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::editor) enum Kind {
    Move,
    Copy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::editor) struct Transfer {
    kind: Kind,
    pairs: VecDeque<(PathBuf, PathBuf)>,
    done: usize,
    overwrite_all: bool,
    last: Option<PathBuf>,
}

impl Editor {
    pub(in crate::editor) fn dired_do_transfer(&mut self, kind: Kind) {
        let targets = self.dired_targets(Mark::Marked, true);
        let prompt = match (kind, targets.as_slice()) {
            (_, []) => {
                self.show_message("No file on this line");
                return;
            }
            (Kind::Move, [(_, name)]) => format!("Rename {name} to: "),
            (Kind::Copy, [(_, name)]) => format!("Copy {name} to: "),
            (Kind::Move, many) => format!("Move {} to: ", files(many.len())),
            (Kind::Copy, many) => format!("Copy {} to: ", files(many.len())),
        };
        let sources: Vec<PathBuf> = targets.into_iter().map(|(path, _)| path).collect();
        if self.open_file_prompt(&prompt, Purpose::Transfer(kind))
            && let Some(minibuffer) = &mut self.minibuffer
        {
            minibuffer.sources = sources;
        }
    }

    pub(in crate::editor) fn transfer_to(
        &mut self,
        kind: Kind,
        sources: Vec<PathBuf>,
        target: &Path,
    ) {
        let target = self.absolute(target);
        let into = target.is_dir();
        if sources.len() > 1 && !into {
            let shown = paths::abbreviate(&target, self.home.as_deref());
            self.show_message(format!("Target must be a directory: {shown}"));
            return;
        }
        let pairs = sources
            .into_iter()
            .map(|from| {
                let to = match from.file_name() {
                    Some(name) if into => target.join(name),
                    _ => target.clone(),
                };
                (from, to)
            })
            .collect();
        self.continue_transfer(Transfer {
            kind,
            pairs,
            done: 0,
            overwrite_all: false,
            last: None,
        });
    }

    pub(in crate::editor) fn continue_transfer(&mut self, mut transfer: Transfer) {
        while let Some((from, to)) = transfer.pairs.front() {
            if from == to {
                transfer.pairs.pop_front();
                continue;
            }
            if !transfer.overwrite_all && fs::symlink_metadata(to).is_ok() {
                let to = to.clone();
                self.ask_to_overwrite_transfer(&to, transfer);
                return;
            }
            if !self.transfer_front(&mut transfer) {
                return;
            }
        }
        self.finish_transfer(&transfer);
    }

    pub(in crate::editor) fn answer_transfer(&mut self, mut transfer: Transfer, answer: char) {
        match answer {
            'y' => {
                if !self.transfer_front(&mut transfer) {
                    return;
                }
            }
            'n' => {
                transfer.pairs.pop_front();
            }
            '!' => transfer.overwrite_all = true,
            _ => {
                self.finish_transfer(&transfer);
                return;
            }
        }
        self.continue_transfer(transfer);
    }

    fn transfer_front(&mut self, transfer: &mut Transfer) -> bool {
        let Some((from, to)) = transfer.pairs.pop_front() else {
            return true;
        };
        match perform(transfer.kind, &from, &to) {
            Ok(()) => {
                if transfer.kind == Kind::Move {
                    self.relocate(&from, &to);
                }
                transfer.done += 1;
                transfer.last = Some(to);
                true
            }
            Err(error) => {
                self.finish_transfer(transfer);
                self.show_message(format!("{}: {error}", from.display()));
                false
            }
        }
    }

    fn finish_transfer(&mut self, transfer: &Transfer) {
        self.name_file_buffers();
        self.refresh_listings();
        if let Some(last) = &transfer.last
            && let Some(listing) = self.buffers.listing(0)
            && Some(listing.directory()) == last.parent()
            && let Some(name) = last.file_name()
        {
            let name = name.to_owned();
            self.dired_focus(&name);
        }
        let verb = match transfer.kind {
            Kind::Move => "Moved",
            Kind::Copy => "Copied",
        };
        self.show_message(format!("{verb} {}", files(transfer.done)));
    }

    fn relocate(&mut self, from: &Path, to: &Path) {
        let moved = |path: &Path| {
            path.strip_prefix(from).ok().map(|rest| {
                if rest.as_os_str().is_empty() {
                    to.to_owned()
                } else {
                    to.join(rest)
                }
            })
        };
        for index in 0..self.buffers.len() {
            if let Some(file) = self
                .buffers
                .get_mut(index)
                .and_then(|buffer| buffer.file_mut())
                && let Some(path) = moved(&file.path)
            {
                file.path = path;
            }
            if let Some((_, listing)) = self.buffers.dired_mut(index)
                && let Some(directory) = moved(listing.directory())
            {
                listing.set_directory(directory);
            }
        }
    }
}

fn perform(kind: Kind, from: &Path, to: &Path) -> io::Result<()> {
    if let Ok(existing) = fs::symlink_metadata(to) {
        let from_is_dir = fs::symlink_metadata(from)?.is_dir();
        if existing.is_dir() != from_is_dir || (existing.is_dir() && kind == Kind::Move) {
            ops::remove(to)?;
        }
    }
    match kind {
        Kind::Move => ops::rename(from, to),
        Kind::Copy => ops::copy(from, to),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::editor::dired::fixtures::{listed, workspace};
    use crate::editor::test_support::{at_point, press, question, type_text};

    #[test]
    fn r_renames_the_file_at_point() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "n R");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Rename a.txt to: ");
        type_text(&mut editor, "c.txt");
        press(&mut editor, "RET");

        assert!(!dir.path().join("a.txt").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("c.txt")).unwrap(),
            "hello"
        );
        assert_eq!(at_point(&editor), "c.txt");
        assert_eq!(editor.message(), Some("Moved 1 file"));
    }

    #[test]
    fn r_into_a_directory_moves_the_file_there() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "n R");
        type_text(&mut editor, "sub");
        press(&mut editor, "RET RET");

        assert!(dir.path().join("sub").join("a.txt").exists());
        assert!(!dir.path().join("a.txt").exists());
    }

    #[test]
    fn r_moves_marked_files_into_a_directory() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "n m m R");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Move 2 files to: ");
        type_text(&mut editor, "sub");
        press(&mut editor, "RET RET");

        assert!(dir.path().join("sub").join("a.txt").exists());
        assert!(dir.path().join("sub").join("b.txt").exists());
        assert_eq!(editor.message(), Some("Moved 2 files"));
    }

    #[test]
    fn moving_several_files_needs_a_directory() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "n m m R");
        type_text(&mut editor, "nowhere");
        press(&mut editor, "M-RET");

        assert!(
            editor
                .message()
                .unwrap()
                .starts_with("Target must be a directory")
        );
        assert!(dir.path().join("a.txt").exists());
    }

    #[test]
    fn renaming_onto_an_existing_file_asks_first() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "n R");
        type_text(&mut editor, "b.txt");
        press(&mut editor, "RET");
        assert_eq!(
            question(&editor),
            Some(format!(
                "Overwrite {}? (y, n, !, q) ",
                dir.path().join("b.txt").display()
            ))
        );
        press(&mut editor, "n");
        assert_eq!(fs::read_to_string(dir.path().join("b.txt")).unwrap(), "bee");

        press(&mut editor, "R");
        type_text(&mut editor, "b.txt");
        press(&mut editor, "RET y");
        assert_eq!(
            fs::read_to_string(dir.path().join("b.txt")).unwrap(),
            "hello"
        );
        assert!(!dir.path().join("a.txt").exists());
    }

    #[test]
    fn renaming_carries_buffers_visiting_the_file() {
        let (dir, mut editor) = workspace();
        editor.open_file(&dir.path().join("a.txt"));
        listed(&mut editor, &dir);

        press(&mut editor, "n R");
        type_text(&mut editor, "c.txt");
        press(&mut editor, "RET C-x b RET");

        assert_eq!(editor.buffer().name(), "c.txt");
        assert_eq!(
            editor.buffer().file().map(|file| file.path.clone()),
            Some(dir.path().join("c.txt"))
        );
    }

    #[test]
    fn c_copies_directories_recursively() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "C");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Copy sub to: ");
        type_text(&mut editor, "dup");
        press(&mut editor, "M-RET");

        assert_eq!(
            fs::read_to_string(dir.path().join("dup").join("c.txt")).unwrap(),
            "sea"
        );
        assert!(dir.path().join("sub").join("c.txt").exists());
        assert_eq!(editor.message(), Some("Copied 1 file"));
    }

    #[test]
    fn bang_overwrites_every_remaining_conflict() {
        let (dir, mut editor) = workspace();
        let target = dir.path().join("sub");
        fs::write(target.join("a.txt"), "old a").unwrap();
        fs::write(target.join("b.txt"), "old b").unwrap();
        listed(&mut editor, &dir);

        press(&mut editor, "n m m C");
        type_text(&mut editor, "sub");
        press(&mut editor, "RET RET !");

        assert_eq!(fs::read_to_string(target.join("a.txt")).unwrap(), "hello");
        assert_eq!(fs::read_to_string(target.join("b.txt")).unwrap(), "bee");
        assert_eq!(editor.message(), Some("Copied 2 files"));
    }
}
