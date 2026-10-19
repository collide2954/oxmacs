use std::ops::Range;
use std::sync::LazyLock;

use jiff::civil::Date;
use regex::Regex;

use crate::atlas::date::{self, Every};
use crate::atlas::note::{self, Note, State, Style};
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::text::{Buffer, BufferId};
use crate::theme::Highlight;

pub(in crate::editor) mod links;
mod vaults;

pub(in crate::editor) const EXTENSION: &str = "atlas";

pub(in crate::editor) fn today() -> Date {
    jiff::Zoned::now().date()
}

pub(in crate::editor) fn time_now() -> jiff::civil::Time {
    jiff::Zoned::now().time()
}

fn face(style: Style) -> Highlight {
    match style {
        Style::Key | Style::Bullet => Highlight::Keyword,
        Style::Separator
        | Style::Finished
        | Style::Anchor
        | Style::Comment
        | Style::Box(State::Dropped) => Highlight::Comment,
        Style::Heading(_) => Highlight::Heading,
        Style::Box(State::Todo) => Highlight::Function,
        Style::Box(State::Doing) | Style::Loose => Highlight::Warning,
        Style::Box(State::Waiting) | Style::Field => Highlight::Constant,
        Style::Box(State::Done) => Highlight::Success,
        Style::Priority => Highlight::Error,
        Style::Tag => Highlight::Type,
        Style::Link => Highlight::Link,
        Style::When => Highlight::Attribute,
    }
}

fn runs(faces: &[Option<Highlight>]) -> Vec<(Range<usize>, Highlight)> {
    let mut found: Vec<(Range<usize>, Highlight)> = Vec::new();
    for (column, face) in faces.iter().enumerate() {
        let Some(face) = face else {
            continue;
        };
        match found.last_mut() {
            Some((range, last)) if range.end == column && last == face => range.end += 1,
            _ => found.push((column..column + 1, *face)),
        }
    }
    found
}

static DONE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s+done:\S+").unwrap_or_else(|_| unreachable!("a valid pattern"))
});
static DATED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(on|due):(\d{4}-\d{2}-\d{2})")
        .unwrap_or_else(|_| unreachable!("a valid pattern"))
});

fn marked(text: &str, depth: usize, state: State) -> String {
    text.chars()
        .enumerate()
        .map(|(column, c)| if column == depth + 1 { state.mark() } else { c })
        .collect()
}

fn exact(text: &str, today: Date) -> Option<String> {
    let parsed = note::parse(text, today);
    let spans = parsed.spans.first()?;
    let chars: Vec<char> = text.chars().collect();
    let mut changed = chars.clone();
    let mut any = false;
    for span in spans.iter().rev().filter(|span| span.style == Style::Loose) {
        let word: String = chars[span.columns.clone()].iter().collect();
        let Some((key, value)) = word.split_once(':') else {
            continue;
        };
        if key.eq_ignore_ascii_case("every") {
            continue;
        }
        if let Some(date) = date::resolve(value, today) {
            let replacement: Vec<char> = format!("{key}:{date}").chars().collect();
            changed.splice(span.columns.clone(), replacement);
            any = true;
        }
    }
    any.then(|| changed.into_iter().collect())
}

impl Editor {
    fn replace_line(&mut self, index: usize, line: usize, text: &str) {
        let Some(buffer) = self.buffers.get_mut(index) else {
            return;
        };
        let start = buffer.line_start(line);
        let end = buffer.line_end(line);
        let point = buffer.point();
        buffer.delete_range(start..end);
        buffer.insert_at(start, text);
        if (start..=end).contains(&point) {
            let column = point - start;
            buffer.set_point(start + column.min(text.chars().count()));
        }
    }

    fn exact_dates_on(&mut self, index: usize, line: usize) {
        let today = (self.today)();
        let Some(buffer) = self.buffers.get(index) else {
            return;
        };
        if line >= buffer.len_lines() {
            return;
        }
        let text = buffer.line_text(line).to_string();
        if let Some(text) = exact(&text, today) {
            self.replace_line(index, line, &text);
        }
    }

    pub(in crate::editor) fn exact_atlas_dates(&mut self, index: usize) {
        if !self.is_atlas(index) {
            return;
        }
        let lines = self.buffers.get(index).map_or(0, Buffer::len_lines);
        for line in 0..lines {
            self.exact_dates_on(index, line);
        }
    }

    pub(in crate::editor) fn after_atlas_command(&mut self) {
        let here = self.is_atlas(0).then(|| {
            let buffer = self.buffers.current();
            (buffer.id(), buffer.line_of(buffer.point()))
        });
        let left = std::mem::replace(&mut self.atlas_line, here);
        if let Some((id, line)) = left
            && left != here
            && let Some(index) = self.buffers.position(|buffer| buffer.id() == id)
            && self.is_atlas(index)
        {
            self.exact_dates_on(index, line);
        }
    }

    pub(in crate::editor) fn atlas_cycle_task(&mut self) {
        let today = (self.today)();
        let buffer = self.buffers.current();
        let line = buffer.line_of(buffer.point());
        let text = buffer.line_text(line).to_string();
        let parsed = note::parse(&text, today);
        let Some(task) = parsed.tasks.first() else {
            self.show_message("No task on this line");
            return;
        };
        let next = task.state.cycled();
        if next == State::Done
            && let Some(every) = task.every
        {
            self.repeat_task(line, &text, task.depth, every, today);
            return;
        }
        let mut changed = marked(&text, task.depth, next);
        if task.state == State::Done {
            changed = DONE.replace_all(&changed, "").into_owned();
        }
        if next == State::Done && task.done.is_none() {
            changed = format!("{}  done:{today}", changed.trim_end());
        }
        self.replace_line(0, line, &changed);
    }

    fn repeat_task(&mut self, line: usize, text: &str, depth: usize, every: Every, today: Date) {
        let mut first = None;
        let moved = DATED.replace_all(text, |captures: &regex::Captures<'_>| {
            let next = captures[2]
                .parse::<Date>()
                .ok()
                .and_then(|date| every.next(date, today));
            match next {
                Some(next) => {
                    first.get_or_insert(next);
                    format!("{}:{next}", &captures[1])
                }
                None => captures[0].to_owned(),
            }
        });
        let changed = marked(&moved, depth, State::Todo);
        let log = format!("\n{}- done {today}", " ".repeat(depth + 4));
        self.replace_line(0, line, &format!("{changed}{log}"));
        match first {
            Some(next) => self.show_message(format!("Next: {next}")),
            None => self.show_message("Logged; the task has no date to move"),
        }
    }

    pub(in crate::editor) fn is_atlas(&self, index: usize) -> bool {
        matches!(self.buffers.mode(index), Some(Mode::Text(None)))
            && self
                .buffers
                .get(index)
                .and_then(|buffer| buffer.file())
                .and_then(|file| file.path.extension())
                .is_some_and(|extension| extension.eq_ignore_ascii_case(EXTENSION))
    }

    pub(in crate::editor) fn sync_atlas(&mut self, index: usize) {
        if !self.is_atlas(index) {
            return;
        }
        let Some(buffer) = self.buffers.get(index) else {
            return;
        };
        let (id, revision) = (buffer.id(), buffer.revision());
        let fresh = self
            .atlas_notes
            .iter()
            .any(|(known, seen, _)| *known == id && *seen == revision);
        if fresh {
            return;
        }
        let parsed = note::parse(&buffer.contents(), (self.today)());
        let buffers = &self.buffers;
        self.atlas_notes.retain(|(known, ..)| {
            *known != id && buffers.position(|buffer| buffer.id() == *known).is_some()
        });
        self.atlas_notes.push((id, revision, parsed));
    }

    pub(in crate::editor) fn atlas_note(&self, id: BufferId) -> Option<&Note> {
        self.atlas_notes
            .iter()
            .find(|(known, ..)| *known == id)
            .map(|(_, _, note)| note)
    }

    pub(in crate::editor) fn atlas_highlights(
        &self,
        index: usize,
        line: usize,
    ) -> Vec<(Range<usize>, Highlight)> {
        let Some(buffer) = self.buffers.get(index) else {
            return Vec::new();
        };
        let Some(spans) = self
            .atlas_note(buffer.id())
            .and_then(|note| note.spans.get(line))
        else {
            return Vec::new();
        };
        let width = buffer.line_end(line) - buffer.line_start(line);
        let mut faces = vec![None; width];
        let (finished, others): (Vec<_>, Vec<_>) =
            spans.iter().partition(|span| span.style == Style::Finished);
        for span in others.into_iter().chain(finished) {
            let end = span.columns.end.min(width);
            let start = span.columns.start.min(end);
            faces[start..end].fill(Some(face(span.style)));
        }
        runs(&faces)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use jiff::civil::date;

    use crate::editor::Editor;
    use crate::editor::test_support::press;
    use crate::theme::Highlight;

    fn note(text: &str) -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Garden plan.atlas");
        fs::write(&path, text).unwrap();
        let mut editor = Editor::new();
        editor.today = || date(2026, 10, 19);
        editor.open_file(&path);
        editor.prepare_display();
        (dir, editor)
    }

    #[test]
    fn atlas_notes_are_highlighted_from_their_parse() {
        let (_dir, editor) = note("title: Garden\n---\n= Plan\n[ ] Buy seeds  due:fri  #home\n");

        assert_eq!(editor.line_style(0).spans, [(0..6, Highlight::Keyword)]);
        assert_eq!(editor.line_style(2).spans, [(0..6, Highlight::Heading)]);
        assert_eq!(
            editor.line_style(3).spans,
            [
                (0..3, Highlight::Function),
                (15..22, Highlight::Warning),
                (24..29, Highlight::Type),
            ]
        );
    }

    #[test]
    fn finished_tasks_are_dimmed_after_their_box() {
        let (_dir, editor) = note("[x] Measure beds  done:2026-10-18\n");

        assert_eq!(
            editor.line_style(0).spans,
            [(0..3, Highlight::Success), (3..33, Highlight::Comment)]
        );
    }

    fn first_line(editor: &Editor) -> String {
        crate::editor::test_support::lines(editor)[0].clone()
    }

    #[test]
    fn c_c_c_c_cycles_a_task_and_stamps_when_it_was_done() {
        let (_dir, mut editor) = note("[ ] Buy seeds  #home\n");

        press(&mut editor, "C-c C-c");
        assert_eq!(first_line(&editor), "[/] Buy seeds  #home");
        press(&mut editor, "C-c C-c");
        assert_eq!(first_line(&editor), "[x] Buy seeds  #home  done:2026-10-19");
        press(&mut editor, "C-c C-c");
        assert_eq!(first_line(&editor), "[ ] Buy seeds  #home");
    }

    #[test]
    fn finishing_a_repeating_task_moves_it_on_and_logs_it() {
        let (_dir, mut editor) = note("  [/] Water plants  on:2026-10-21  every:3d\nnext\n");

        press(&mut editor, "C-c C-c");

        assert_eq!(
            crate::editor::test_support::lines(&editor)[..3],
            [
                "  [ ] Water plants  on:2026-10-24  every:3d",
                "      - done 2026-10-19",
                "next"
            ]
        );
        assert_eq!(editor.message(), Some("Next: 2026-10-24"));
    }

    #[test]
    fn c_c_c_c_off_a_task_says_so() {
        let (_dir, mut editor) = note("Just text\n");

        press(&mut editor, "C-c C-c");

        assert_eq!(editor.message(), Some("No task on this line"));
    }

    #[test]
    fn loose_dates_become_exact_when_point_leaves_the_line() {
        let (_dir, mut editor) = note("");
        crate::editor::test_support::type_text(&mut editor, "[ ] Call Ana  due:fri on:tomorrow");
        assert_eq!(first_line(&editor), "[ ] Call Ana  due:fri on:tomorrow");

        press(&mut editor, "RET");

        assert_eq!(
            first_line(&editor),
            "[ ] Call Ana  due:2026-10-23 on:2026-10-20"
        );
    }

    #[test]
    fn saving_makes_every_loose_date_exact() {
        let (dir, mut editor) = note("[ ] One  due:+1w\n[ ] Two  due:someday\n");
        press(&mut editor, "C-e SPC");

        press(&mut editor, "C-x C-s");

        assert_eq!(
            fs::read_to_string(dir.path().join("Garden plan.atlas")).unwrap(),
            "[ ] One  due:2026-10-26 \n[ ] Two  due:someday\n"
        );
    }

    #[test]
    fn the_mode_line_names_atlas_and_edits_reparse() {
        let (_dir, mut editor) = note("plain\n");
        assert!(
            editor.mode_line().contains("Atlas"),
            "{}",
            editor.mode_line()
        );

        crate::editor::test_support::type_text(&mut editor, "= ");
        editor.prepare_display();

        assert_eq!(editor.line_style(0).spans, [(0..7, Highlight::Heading)]);
    }
}
