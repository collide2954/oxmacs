use std::fs;
use std::path::PathBuf;

use crate::command::{Scope, ScopeId};
use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::files::paths;
use crate::special_buffer::SpecialBuffer;
use crate::text::Buffer;
use crate::theme::Highlight;

const NAME: &str = "*xref*";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Default)]
pub(in crate::editor) struct Locations {
    rows: Vec<(usize, Place)>,
    styles: Vec<LineStyle>,
}

impl Locations {
    fn style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }
}

fn preview(cache: &mut Vec<(PathBuf, Vec<String>)>, editor: &Editor, place: &Place) -> String {
    let open = editor
        .index_visiting(&place.path)
        .and_then(|index| editor.buffers.get(index));
    if let Some(buffer) = open {
        let line = place.line.saturating_sub(1);
        return if line < buffer.len_lines() {
            buffer.line_text(line).trim().to_owned()
        } else {
            String::new()
        };
    }
    if !cache.iter().any(|(path, _)| *path == place.path) {
        let lines = fs::read_to_string(&place.path)
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default();
        cache.push((place.path.clone(), lines));
    }
    cache
        .iter()
        .find(|(path, _)| *path == place.path)
        .and_then(|(_, lines)| lines.get(place.line.saturating_sub(1)))
        .map(|line| line.trim().to_owned())
        .unwrap_or_default()
}

impl Editor {
    pub fn show_locations(&mut self, title: &str, mut places: Vec<Place>) {
        places.sort_by(|a, b| (&a.path, a.line, a.column).cmp(&(&b.path, b.line, b.column)));
        places.dedup();
        let mut page = Page::default();
        page.line(&[
            (title, Some(Highlight::Heading)),
            (
                "   RET visits, n and p step, q quits",
                Some(Highlight::Comment),
            ),
        ]);
        let mut rows = Vec::new();
        let mut cache = Vec::new();
        let mut file = None;
        for place in places {
            if file.as_ref() != Some(&place.path) {
                page.line(&[]);
                let shown = paths::abbreviate(&place.path, self.home.as_deref());
                page.line(&[(&shown, Some(Highlight::Link))]);
                file = Some(place.path.clone());
            }
            let at = format!("{:>6}:{:<4}", place.line, place.column);
            let text = preview(&mut cache, self, &place);
            rows.push((page.line_count(), place));
            page.line(&[(&at, Some(Highlight::Comment)), (" ", None), (&text, None)]);
        }
        if self.locations_index().is_none() {
            let mut buffer = Buffer::with_text(NAME, "");
            buffer.set_read_only(true);
            self.beside(|editor| {
                editor.add_buffer_with(buffer, Mode::special(Locations::default()));
            });
        }
        let Some(index) = self.locations_index() else {
            return;
        };
        let first = rows.first().map(|(line, _)| *line);
        if let Some((buffer, list)) = self.buffers.entry_special_mut::<Locations>(index) {
            page.fill(buffer);
            list.rows = rows;
            list.styles = page.styles;
        }
        self.show_beside(Self::locations_index);
        let buffer = self.buffers.current_mut();
        buffer.set_point(first.map_or(0, |line| buffer.line_start(line)));
        self.reveal_point();
    }

    fn locations_index(&self) -> Option<usize> {
        self.buffers.find_special::<Locations>(|_| true)
    }

    fn location_here(&self) -> Option<Place> {
        let list = self.buffers.special::<Locations>(0)?;
        let line = self.point_line();
        list.rows
            .iter()
            .find(|(row, _)| *row == line)
            .map(|(_, place)| place.clone())
    }

    pub(in crate::editor) fn visit_location(&mut self) {
        let Some(place) = self.location_here() else {
            self.show_message("No location on this line");
            return;
        };
        let list = self.buffers.current().id();
        self.leave_window_showing(list);
        if self.open_file(&place.path) {
            self.go_to_line(place.line, place.column);
        }
    }

    pub(in crate::editor) fn step_location(&mut self, forward: bool) {
        let Some(list) = self.buffers.special::<Locations>(0) else {
            return;
        };
        let line = self.point_line();
        let next = if forward {
            list.rows
                .iter()
                .map(|(row, _)| *row)
                .find(|row| *row > line)
        } else {
            list.rows
                .iter()
                .rev()
                .map(|(row, _)| *row)
                .find(|row| *row < line)
        };
        match next {
            Some(row) => {
                let buffer = self.buffers.current_mut();
                buffer.set_point(buffer.line_start(row));
                self.reveal_point();
            }
            None => self.show_message(if forward {
                "No more locations"
            } else {
                "No earlier locations"
            }),
        }
    }
}

impl SpecialBuffer for Locations {
    fn scope(&self) -> ScopeId {
        Scope::Locations.into()
    }

    fn line_style(&self, line: usize, _: &str) -> LineStyle {
        self.style(line)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::Place;
    use crate::editor::Editor;
    use crate::testing::{lines, press};

    fn listed() -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.rs"), "fn one() {}\nfn two() { one() }\n").unwrap();
        fs::write(dir.path().join("b.rs"), "use a::one;\n").unwrap();
        let mut editor = Editor::new();
        editor.show_locations(
            "3 references to one",
            vec![
                Place {
                    path: dir.path().join("b.rs"),
                    line: 1,
                    column: 8,
                },
                Place {
                    path: dir.path().join("a.rs"),
                    line: 2,
                    column: 12,
                },
                Place {
                    path: dir.path().join("a.rs"),
                    line: 1,
                    column: 4,
                },
            ],
        );
        (dir, editor)
    }

    #[test]
    fn locations_are_listed_by_file_with_their_lines() {
        let (_dir, editor) = listed();

        assert_eq!(editor.buffer().name(), "*xref*");
        let shown = lines(&editor);
        assert!(shown[0].starts_with("3 references to one"));
        assert!(shown[2].ends_with("a.rs"), "{shown:?}");
        assert_eq!(shown[3], "     1:4    fn one() {}");
        assert_eq!(shown[4], "     2:12   fn two() { one() }");
        assert!(shown[6].ends_with("b.rs"));
        assert_eq!(shown[7], "     1:8    use a::one;");
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 3);
    }

    #[test]
    fn n_and_p_step_and_return_visits_beside_the_list() {
        let (dir, mut editor) = listed();

        press(&mut editor, "n n");
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 7);
        press(&mut editor, "n");
        assert_eq!(editor.message(), Some("No more locations"));
        press(&mut editor, "p RET");

        assert_eq!(
            editor.buffer().file().map(|file| file.path.clone()),
            Some(dir.path().join("a.rs"))
        );
        let buffer = editor.buffer();
        assert_eq!(buffer.line_of(buffer.point()), 1);
        assert_eq!(buffer.point() - buffer.line_start(1), 11);
        assert_eq!(editor.tiling().windows().len(), 2);
    }
}
