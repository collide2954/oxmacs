use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::dired::Mark;
use crate::editor::Editor;
use crate::editor::dired::files;
use crate::editor::minibuffer::Purpose;
use crate::files::{ops, paths};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Move,
    Copy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Transfer {
    kind: Kind,
    pairs: VecDeque<(PathBuf, PathBuf)>,
    done: usize,
    overwrite_all: bool,
    last: Option<PathBuf>,
}

impl Editor {
    pub(super) fn dired_do_transfer(&mut self, kind: Kind) {
        let line = self.point_line();
        let Some(listing) = self.buffers.listing(0) else {
            return;
        };
        let mut targets = listing.marked(Mark::Marked);
        if targets.is_empty() {
            targets.extend(listing.entry_at(line));
        }
        let sources: Vec<PathBuf> = targets
            .iter()
            .map(|entry| listing.directory().join(&entry.name))
            .collect();
        let prompt = match (kind, targets.as_slice()) {
            (_, []) => {
                self.show_message("No file on this line");
                return;
            }
            (Kind::Move, [one]) => format!("Rename {} to: ", one.display_name()),
            (Kind::Copy, [one]) => format!("Copy {} to: ", one.display_name()),
            (Kind::Move, many) => format!("Move {} to: ", files(many.len())),
            (Kind::Copy, many) => format!("Copy {} to: ", files(many.len())),
        };
        if self.open_file_prompt(&prompt, Purpose::Transfer(kind))
            && let Some(minibuffer) = &mut self.minibuffer
        {
            minibuffer.sources = sources;
        }
    }

    pub(super) fn transfer_to(&mut self, kind: Kind, sources: Vec<PathBuf>, target: &Path) {
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

    pub(super) fn continue_transfer(&mut self, mut transfer: Transfer) {
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

    pub(super) fn answer_transfer(&mut self, mut transfer: Transfer, answer: char) {
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
