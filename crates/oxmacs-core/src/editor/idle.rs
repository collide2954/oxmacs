use std::time::{Duration, Instant};

use crate::editor::Editor;

pub const SIGNATURE_DELAY: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Chore {
    Reparse,
    AutoComplete,
    Signature,
    AutoSave,
}

const CHORES: [Chore; 4] = [
    Chore::Reparse,
    Chore::AutoComplete,
    Chore::Signature,
    Chore::AutoSave,
];

impl Chore {
    fn delay(self) -> Duration {
        match self {
            Self::Reparse | Self::AutoComplete => Duration::from_millis(250),
            Self::Signature => SIGNATURE_DELAY,
            Self::AutoSave => Duration::from_millis(1500),
        }
    }

    fn wanted(self, editor: &Editor) -> bool {
        match self {
            Self::Reparse => editor.wants_reparse(),
            Self::AutoComplete => editor.wants_auto_complete(),
            Self::Signature => editor.wants_signature(),
            Self::AutoSave => editor.wants_auto_save(),
        }
    }

    fn run(self, editor: &mut Editor) {
        match self {
            Self::Reparse => editor.reparse(),
            Self::AutoComplete => editor.auto_complete(),
            Self::Signature => editor.show_signature(),
            Self::AutoSave => editor.auto_save(),
        }
    }
}

#[derive(Debug, Default)]
pub(in crate::editor) struct Rest {
    since: Option<Instant>,
    done: Vec<Chore>,
}

impl Editor {
    fn done_in(&self, since: Instant, chore: Chore) -> bool {
        self.rest.since == Some(since) && self.rest.done.contains(&chore)
    }

    pub fn idle_wait(&self, since: Instant) -> Option<Duration> {
        CHORES
            .into_iter()
            .filter(|chore| !self.done_in(since, *chore) && chore.wanted(self))
            .map(Chore::delay)
            .min()
    }

    pub fn idle(&mut self, since: Instant, now: Instant) {
        if self.rest.since != Some(since) {
            self.rest = Rest {
                since: Some(since),
                done: Vec::new(),
            };
        }
        let rested = now.saturating_duration_since(since);
        for chore in CHORES {
            if rested >= chore.delay() && !self.rest.done.contains(&chore) && chore.wanted(self) {
                self.rest.done.push(chore);
                chore.run(self);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use crate::editor::Editor;
    use crate::testing::type_text;

    #[test]
    fn each_idle_chore_runs_once_after_its_own_pause() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, "hello\n").unwrap();
        let mut editor = Editor::new();
        editor.set_recovery_folder(Some(dir.path().join("recovery")));
        editor.open_file(&path);
        type_text(&mut editor, "x");
        let since = Instant::now();
        let saved = || std::fs::read_dir(dir.path().join("recovery")).map_or(0, Iterator::count);

        assert_eq!(editor.idle_wait(since), Some(Duration::from_millis(1500)));
        editor.idle(since, since + Duration::from_secs(1));
        assert_eq!(saved(), 0);
        editor.idle(since, since + Duration::from_secs(2));
        assert_eq!(saved(), 1);
        assert_eq!(editor.idle_wait(since), None);

        type_text(&mut editor, "y");
        let later = since + Duration::from_secs(3);
        assert_eq!(editor.idle_wait(later), Some(Duration::from_millis(1500)));
    }
}
