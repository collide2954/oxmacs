use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::command::{Scope, ScopeId};
use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::files::paths;
use crate::special_buffer::SpecialBuffer;
use crate::text::{Buffer, BufferId};
use crate::theme::Highlight;

const XREF: &str = "*xref*";
const KEYS: &str = "RET visits   SPC shows   n p step   q quits";
const REFRESHING: &str = "RET visits   SPC shows   n p step   g refreshes   q quits";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub label: Option<(String, Highlight)>,
    pub text: Option<String>,
    pub source: Option<Arc<str>>,
}

impl Place {
    pub fn at(path: impl Into<PathBuf>, line: usize, column: usize) -> Self {
        Self {
            path: path.into(),
            line,
            column,
            label: None,
            text: None,
            source: None,
        }
    }
}

pub type OnVisit = Arc<dyn Fn(&mut Editor, usize) + Send + Sync>;
pub type OnRefresh = Arc<dyn Fn(&mut Editor) + Send + Sync>;

#[derive(Clone, Default)]
pub struct Places {
    pub title: String,
    pub header: Vec<String>,
    pub places: Vec<Place>,
    pub ordered: bool,
    pub root: Option<PathBuf>,
    pub empty: Option<String>,
    pub on_visit: Option<OnVisit>,
    pub on_refresh: Option<OnRefresh>,
}

impl fmt::Debug for Places {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Places")
            .field("title", &self.title)
            .field("places", &self.places.len())
            .field("ordered", &self.ordered)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
pub(in crate::editor) struct PlacesView {
    name: String,
    list: Places,
    rows: Vec<(usize, usize)>,
    styles: Vec<LineStyle>,
}

impl PlacesView {
    fn row(&self, line: usize) -> Option<usize> {
        self.rows
            .iter()
            .find(|(row, _)| *row == line)
            .map(|(_, place)| *place)
    }

    pub(in crate::editor) fn line_of(&self, place: usize) -> Option<usize> {
        self.rows
            .iter()
            .find(|(_, shown)| *shown == place)
            .map(|(line, _)| *line)
    }

    pub(in crate::editor) fn beyond(&self, line: usize, forward: bool) -> Option<usize> {
        let found = if forward {
            self.rows.iter().find(|(row, _)| *row > line)
        } else {
            self.rows.iter().rev().find(|(row, _)| *row < line)
        };
        found.map(|(_, place)| *place)
    }
}

#[derive(Debug)]
struct SourceView;

impl SpecialBuffer for SourceView {
    fn scope(&self) -> ScopeId {
        Scope::Help.into()
    }
}

fn preview(cache: &mut Vec<(PathBuf, Vec<String>)>, editor: &Editor, place: &Place) -> String {
    let wanted = place.line.saturating_sub(1);
    if let Some(source) = &place.source {
        return source
            .lines()
            .nth(wanted)
            .unwrap_or_default()
            .trim()
            .to_owned();
    }
    let open = editor
        .index_visiting(&place.path)
        .and_then(|index| editor.buffers.get(index));
    if let Some(buffer) = open {
        return if wanted < buffer.len_lines() {
            buffer.line_text(wanted).trim().to_owned()
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
        .and_then(|(_, lines)| lines.get(wanted))
        .map(|line| line.trim().to_owned())
        .unwrap_or_default()
}

fn shown_path(path: &Path, root: Option<&Path>, home: Option<&Path>) -> String {
    match root.and_then(|root| path.strip_prefix(root).ok()) {
        Some(relative) => relative
            .components()
            .collect::<PathBuf>()
            .display()
            .to_string(),
        None => paths::abbreviate(path, home),
    }
}

fn page(editor: &Editor, list: &Places) -> (Page, Vec<(usize, usize)>) {
    let mut page = Page::default();
    let root = list
        .root
        .as_deref()
        .map(|root| format!("  {}", paths::abbreviate(root, editor.home.as_deref())))
        .unwrap_or_default();
    page.line(&[(&list.title, Some(Highlight::Heading)), (&root, None)]);
    for line in &list.header {
        page.line(&[(line, None)]);
    }
    let keys = if list.on_refresh.is_some() {
        REFRESHING
    } else {
        KEYS
    };
    page.line(&[(keys, Some(Highlight::Comment))]);
    let mut order: Vec<usize> = (0..list.places.len()).collect();
    if !list.ordered {
        let at = |index: usize| {
            let place = &list.places[index];
            (&place.path, place.line, place.column)
        };
        order.sort_by(|a, b| at(*a).cmp(&at(*b)));
        order.dedup_by(|a, b| list.places[*a] == list.places[*b]);
    }
    let mut rows = Vec::new();
    if order.is_empty()
        && let Some(empty) = &list.empty
    {
        page.line(&[]);
        page.line(&[(empty, Some(Highlight::Success))]);
        return (page, rows);
    }
    let width = list
        .places
        .iter()
        .filter_map(|place| place.label.as_ref())
        .map(|(label, _)| label.chars().count())
        .max()
        .unwrap_or(0);
    let mut cache = Vec::new();
    let mut file = None;
    let home = editor.home.as_deref();
    if list.ordered {
        page.line(&[]);
    }
    for index in order {
        let place = &list.places[index];
        let shown = shown_path(&place.path, list.root.as_deref(), home);
        let text = place
            .text
            .clone()
            .unwrap_or_else(|| preview(&mut cache, editor, place));
        let label = place
            .label
            .as_ref()
            .map(|(label, face)| (format!("{label:<width$}"), Some(*face)));
        if list.ordered {
            rows.push((page.line_count(), index));
            let at = format!("{shown}:{}:{}", place.line, place.column);
            let mut pieces = vec![("  ".to_owned(), None)];
            pieces.extend(label.map(|(label, face)| (label + "  ", face)));
            pieces.push((at, Some(Highlight::Link)));
            if !text.is_empty() {
                pieces.push((format!("  {text}"), None));
            }
            let pieces: Vec<(&str, Option<Highlight>)> = pieces
                .iter()
                .map(|(text, face)| (text.as_str(), *face))
                .collect();
            page.line(&pieces);
            continue;
        }
        if file.as_ref() != Some(&place.path) {
            page.line(&[]);
            page.line(&[(&shown, Some(Highlight::Link))]);
            file = Some(place.path.clone());
        }
        rows.push((page.line_count(), index));
        let at = format!("{:>6}:{:<4}", place.line, place.column);
        let mut pieces = vec![(at, Some(Highlight::Comment)), (" ".to_owned(), None)];
        pieces.extend(label.map(|(label, face)| (label + "  ", face)));
        pieces.push((text, None));
        let pieces: Vec<(&str, Option<Highlight>)> = pieces
            .iter()
            .map(|(text, face)| (text.as_str(), *face))
            .collect();
        page.line(&pieces);
    }
    (page, rows)
}

impl Editor {
    fn places_named(&self, name: &str) -> Option<usize> {
        self.buffers
            .find_special::<PlacesView>(|view| view.name == name)
    }

    pub fn show_places(&mut self, name: &str, list: Places) -> Option<BufferId> {
        if self.places_named(name).is_none() {
            let mut buffer = Buffer::with_text(name, "");
            buffer.set_read_only(true);
            let view = PlacesView {
                name: name.to_owned(),
                ..PlacesView::default()
            };
            self.beside(|editor| {
                editor.add_buffer_with(buffer, Mode::special(view));
            });
        }
        let index = self.places_named(name)?;
        let first = self.fill_places(index, list);
        let id = self.buffers.get(index).map(Buffer::id)?;
        self.show_beside(|editor| editor.places_named(name));
        let buffer = self.buffers.current_mut();
        buffer.set_point(first.map_or(0, |line| buffer.line_start(line)));
        self.reveal_point();
        Some(id)
    }

    pub fn refresh_places(&mut self, name: &str, list: Places) -> bool {
        let Some(index) = self.places_named(name) else {
            return false;
        };
        let line = self
            .buffers
            .get(index)
            .map(|buffer| buffer.line_of(buffer.point()));
        self.fill_places(index, list);
        if let (Some(line), Some(buffer)) = (line, self.buffers.get_mut(index)) {
            let line = line.min(buffer.len_lines().saturating_sub(1));
            buffer.set_point(buffer.line_start(line));
        }
        true
    }

    fn fill_places(&mut self, index: usize, list: Places) -> Option<usize> {
        let (page, rows) = page(self, &list);
        let first = rows.first().map(|(line, _)| *line);
        if let Some((buffer, view)) = self.buffers.entry_special_mut::<PlacesView>(index) {
            page.fill(buffer);
            view.rows = rows;
            view.styles = page.styles;
            view.list = list;
        }
        first
    }

    pub(in crate::editor) fn places_view(&self, name: &str) -> Option<(usize, &PlacesView)> {
        let index = self.places_named(name)?;
        Some((index, self.buffers.special::<PlacesView>(index)?))
    }

    pub(in crate::editor) fn place_here(&self) -> Option<usize> {
        self.place_on_line(self.point_line())
    }

    pub(in crate::editor) fn place_on_line(&self, line: usize) -> Option<usize> {
        self.buffers.special::<PlacesView>(0)?.row(line)
    }

    pub(in crate::editor) fn show_xref(&mut self, title: String, places: Vec<Place>) {
        self.show_places(
            XREF,
            Places {
                title,
                places,
                ..Places::default()
            },
        );
    }

    pub(in crate::editor) fn visit_place(&mut self) {
        let Some(index) = self.place_here() else {
            self.show_message("No place on this line");
            return;
        };
        let Some(view) = self.buffers.special::<PlacesView>(0) else {
            return;
        };
        if let Some(visit) = view.list.on_visit.clone() {
            visit(self, index);
            return;
        }
        let Some(place) = view.list.places.get(index).cloned() else {
            return;
        };
        let list = self.buffers.current().id();
        self.leave_window_showing(list);
        self.open_place(&place);
    }

    pub fn open_place(&mut self, place: &Place) -> bool {
        if place.path.is_file() {
            if self.open_file(&place.path) {
                self.go_to_line(place.line, place.column);
                return true;
            }
            return false;
        }
        let name = place.path.display().to_string();
        let target = match (self.buffer_named(&name), place.source.as_deref()) {
            (Some(id), _) => id,
            (None, Some(source)) => {
                let mut buffer = Buffer::with_text(&name, source);
                buffer.set_read_only(true);
                self.add_buffer_with(buffer, Mode::special(SourceView));
                self.buffers.current().id()
            }
            (None, None) => {
                self.show_message(format!("Can't find {name}"));
                return false;
            }
        };
        self.show_buffer(target);
        self.go_to_line(place.line, place.column);
        true
    }

    pub(in crate::editor) fn show_place(&mut self) {
        let origin = self.selected_window();
        self.visit_place();
        self.select_window(origin);
    }

    pub(in crate::editor) fn refresh_this_list(&mut self) {
        let refresh = self
            .buffers
            .special::<PlacesView>(0)
            .and_then(|view| view.list.on_refresh.clone());
        match refresh {
            Some(refresh) => refresh(self),
            None => self.show_message("This list doesn't refresh"),
        }
    }

    pub(in crate::editor) fn step_place(&mut self, forward: bool) {
        let Some(view) = self.buffers.special::<PlacesView>(0) else {
            return;
        };
        let line = self.point_line();
        let next = view
            .beyond(line, forward)
            .and_then(|place| view.line_of(place));
        match next {
            Some(row) => {
                let buffer = self.buffers.current_mut();
                buffer.set_point(buffer.line_start(row));
                self.reveal_point();
            }
            None => self.show_message(if forward {
                "No more places"
            } else {
                "No earlier places"
            }),
        }
    }
}

impl SpecialBuffer for PlacesView {
    fn scope(&self) -> ScopeId {
        Scope::Places.into()
    }

    fn line_style(&self, line: usize, _: &str) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::{Arc, Mutex};

    use super::{Place, Places};
    use crate::editor::Editor;
    use crate::testing::{lines, press};
    use crate::theme::Highlight;

    fn listed() -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.rs"), "fn one() {}\nfn two() { one() }\n").unwrap();
        fs::write(dir.path().join("b.rs"), "use a::one;\n").unwrap();
        let mut editor = Editor::new();
        editor.show_xref(
            "3 references to one".to_owned(),
            vec![
                Place::at(dir.path().join("b.rs"), 1, 8),
                Place::at(dir.path().join("a.rs"), 2, 12),
                Place::at(dir.path().join("a.rs"), 1, 4),
            ],
        );
        (dir, editor)
    }

    #[test]
    fn places_are_listed_by_file_with_their_lines() {
        let (_dir, editor) = listed();

        assert_eq!(editor.buffer().name(), "*xref*");
        let shown = lines(&editor);
        assert!(shown[0].starts_with("3 references to one"));
        assert!(shown[3].ends_with("a.rs"), "{shown:?}");
        assert_eq!(shown[4], "     1:4    fn one() {}");
        assert_eq!(shown[5], "     2:12   fn two() { one() }");
        assert!(shown[7].ends_with("b.rs"));
        assert_eq!(shown[8], "     1:8    use a::one;");
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 4);
    }

    #[test]
    fn n_and_p_step_and_return_visits_beside_the_list() {
        let (dir, mut editor) = listed();

        press(&mut editor, "n n");
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 8);
        press(&mut editor, "n");
        assert_eq!(editor.message(), Some("No more places"));
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

    #[test]
    fn ordered_places_keep_their_order_labels_and_in_memory_sources() {
        let mut editor = Editor::new();
        let source: Arc<str> = "(def (f) (g))\n(def (g) (oops))\n".into();
        let frame = |label: &str, line| Place {
            label: Some((label.to_owned(), Highlight::Function)),
            source: Some(Arc::clone(&source)),
            ..Place::at("<repl>", line, 10)
        };
        editor.show_places(
            "*trace*",
            Places {
                title: "oops is not defined".to_owned(),
                header: vec!["undefined".to_owned()],
                places: vec![frame("g", 2), frame("f", 1)],
                ordered: true,
                ..Places::default()
            },
        );

        let shown = lines(&editor);
        assert_eq!(shown[1], "undefined");
        assert_eq!(shown[4], "  g  <repl>:2:10  (def (g) (oops))");
        assert_eq!(shown[5], "  f  <repl>:1:10  (def (f) (g))");
        press(&mut editor, "RET");
        assert_eq!(editor.buffer().name(), "<repl>");
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 1);
        assert!(editor.buffer().is_read_only());
    }

    #[test]
    fn lists_can_visit_and_refresh_their_own_way() {
        let mut editor = Editor::new();
        let heard = Arc::new(Mutex::new(Vec::new()));
        let visits = Arc::clone(&heard);
        let refreshes = Arc::clone(&heard);
        editor.show_places(
            "*mine*",
            Places {
                title: "Mine".to_owned(),
                places: vec![Place::at("nowhere.txt", 3, 1)],
                on_visit: Some(Arc::new(move |_, index| {
                    visits.lock().unwrap().push(format!("visit {index}"));
                })),
                on_refresh: Some(Arc::new(move |_| {
                    refreshes.lock().unwrap().push("refresh".to_owned());
                })),
                ..Places::default()
            },
        );

        press(&mut editor, "RET g");

        assert_eq!(*heard.lock().unwrap(), ["visit 0", "refresh"]);
        assert!(lines(&editor)[1].contains("g refreshes"));
    }

    #[test]
    fn empty_lists_say_so() {
        let mut editor = Editor::new();
        editor.show_places(
            "*mine*",
            Places {
                title: "Mine".to_owned(),
                empty: Some("Nothing here".to_owned()),
                ..Places::default()
            },
        );

        assert_eq!(lines(&editor)[3], "Nothing here");
        press(&mut editor, "RET");
        assert_eq!(editor.message(), Some("No place on this line"));
    }
}
