use std::fmt::Write;

use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::languages::r_environment::VIEW_ROWS;
use crate::editor::page::{Page, clip};
use crate::external::r;
use crate::text::{Buffer, BufferId};
use crate::theme::Highlight;

const CELL_WIDTH: usize = 24;
const GAP: usize = 2;
const ROOM: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Table {
    rows: usize,
    columns: usize,
    names: Vec<String>,
    types: Vec<String>,
    cells: Vec<Vec<String>>,
}

#[derive(Debug, Default)]
pub(in crate::editor) struct TableView {
    session: Option<BufferId>,
    name: String,
    table: Option<Table>,
    first: usize,
    width: usize,
    header: Vec<(String, LineStyle)>,
    styles: Vec<LineStyle>,
}

impl TableView {
    pub(in crate::editor) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }

    pub(in crate::editor) fn session(&self) -> Option<BufferId> {
        self.session
    }

    pub(in crate::editor) fn header(&self) -> &[(String, LineStyle)] {
        &self.header
    }
}

fn short_type(name: &str) -> String {
    let short = match name {
        "numeric" => "dbl",
        "integer" => "int",
        "character" => "chr",
        "factor" => "fct",
        "logical" => "lgl",
        "Date" => "date",
        "POSIXct" => "dttm",
        "complex" => "cpl",
        other => return format!("<{}>", other.chars().take(4).collect::<String>()),
    };
    format!("<{short}>")
}

fn table(payload: &str) -> Option<Table> {
    let mut lines = payload.lines();
    let mut title = lines.next()?.split('\t').skip(1);
    let rows = title.next()?.parse().ok()?;
    let columns = title.next()?.parse().ok()?;
    let names = lines
        .next()?
        .split('\t')
        .skip(1)
        .map(str::to_owned)
        .collect();
    let types = lines.next()?.split('\t').skip(1).map(short_type).collect();
    let cells = lines
        .map(|line| line.split('\t').map(str::to_owned).collect())
        .collect();
    Some(Table {
        rows,
        columns,
        names,
        types,
        cells,
    })
}

fn width_of(cells: impl Iterator<Item = usize>) -> usize {
    cells.max().unwrap_or(0).min(CELL_WIDTH)
}

fn styled(text: &str, highlight: Option<Highlight>) -> (String, LineStyle) {
    let mut page = Page::default();
    page.line(&[(text, highlight)]);
    (text.to_owned(), page.styles.pop().unwrap_or_default())
}

fn lay_out(
    name: &str,
    table: &Table,
    first: usize,
    room: usize,
) -> (Vec<(String, LineStyle)>, Page) {
    let label_width = width_of(
        table
            .cells
            .iter()
            .map(|row| row.first().map_or(0, |label| label.chars().count())),
    );
    let column_width = |column: usize| {
        width_of(
            std::iter::once(
                table
                    .names
                    .get(column)
                    .map_or(0, |name| name.chars().count()),
            )
            .chain(std::iter::once(
                table
                    .types
                    .get(column)
                    .map_or(0, |kind| kind.chars().count()),
            ))
            .chain(
                table
                    .cells
                    .iter()
                    .map(|row| row.get(column + 1).map_or(0, |cell| cell.chars().count())),
            ),
        )
    };
    let mut shown = Vec::new();
    let mut used = label_width;
    for column in first..table.names.len() {
        let width = column_width(column);
        if !shown.is_empty() && used + GAP + width > room {
            break;
        }
        used += GAP + width;
        shown.push((column, width));
    }
    let numeric = |column: usize| {
        matches!(
            table.types.get(column).map(String::as_str),
            Some("<dbl>" | "<int>")
        )
    };
    let render = |label: &str, cell: &dyn Fn(usize) -> String| -> String {
        let mut line = format!("{:<label_width$}", clip(label, label_width));
        for (column, width) in &shown {
            let text = clip(&cell(*column), *width);
            if numeric(*column) {
                let _ = write!(line, "  {text:>width$}");
            } else {
                let _ = write!(line, "  {text:<width$}");
            }
        }
        line.trim_end().to_owned()
    };
    let last = shown.last().map_or(first, |(column, _)| *column);
    let info = format!(
        "{name}  {} × {}{}  columns {}–{}  ← →",
        table.rows,
        table.columns,
        if table.rows > table.cells.len() {
            format!(" (first {})", table.cells.len())
        } else {
            String::new()
        },
        first + 1,
        last + 1
    );
    let header = vec![
        styled(&info, Some(Highlight::Comment)),
        styled(
            &render("", &|column| table.names[column].clone()),
            Some(Highlight::Heading),
        ),
        styled(
            &render("", &|column| table.types[column].clone()),
            Some(Highlight::Comment),
        ),
    ];
    let mut page = Page::default();
    for row in &table.cells {
        let label = row.first().map_or("", String::as_str);
        let line = render(label, &|column| {
            row.get(column + 1).cloned().unwrap_or_default()
        });
        let split = line
            .char_indices()
            .nth(label_width)
            .map_or(line.len(), |(at, _)| at);
        page.line(&[
            (&line[..split], Some(Highlight::Comment)),
            (&line[split..], None),
        ]);
    }
    (header, page)
}

impl Editor {
    fn table_index(&self, name: &str) -> Option<usize> {
        let session = self.session_id();
        (0..self.buffers.len()).find(|index| {
            matches!(self.buffers.mode(*index), Some(Mode::Table(view)) if view.session == session && view.name == name)
        })
    }

    fn open_table(&mut self, name: &str) -> Option<usize> {
        if self.table_index(name).is_none() {
            let session = self.session_id();
            let console = session
                .and_then(|id| self.buffers.position(|known| known.id() == id))
                .and_then(|index| self.buffers.get(index))
                .map_or_else(|| "*R*".to_owned(), |buffer| buffer.name().to_owned());
            let title = format!("{} {name}*", console.trim_end_matches('*'));
            let view = TableView {
                session,
                name: name.to_owned(),
                ..TableView::default()
            };
            self.beside(|editor| {
                let mut buffer = Buffer::with_text(&title, "");
                buffer.set_read_only(true);
                editor.add_buffer_with(buffer, Mode::Table(view));
            });
        }
        let name = name.to_owned();
        let session = self.session_id();
        self.show_beside(move |editor| {
            (0..editor.buffers.len()).find(|index| {
                matches!(editor.buffers.mode(*index), Some(Mode::Table(view)) if view.session == session && view.name == name)
            })
        });
        self.table_index_here()
    }

    fn table_index_here(&self) -> Option<usize> {
        matches!(self.buffers.mode(0), Some(Mode::Table(_))).then_some(0)
    }

    pub(in crate::editor) fn r_table_arrived(&mut self, payload: &str) {
        let Some(found) = table(payload) else {
            return;
        };
        let name = payload
            .lines()
            .next()
            .and_then(|line| line.split('\t').next())
            .unwrap_or_default()
            .to_owned();
        let Some(index) = self.open_table(&name) else {
            return;
        };
        self.show_message(format!(
            "{name}: {} rows, {} columns",
            found.rows, found.columns
        ));
        if let Some(Mode::Table(view)) = self.buffers.mode_mut(index) {
            view.table = Some(found);
            view.first = 0;
            view.width = 0;
        }
        self.render_tables();
        self.buffers.current_mut().set_point(0);
    }

    pub(in crate::editor) fn r_object_arrived(&mut self, payload: &str) {
        let (name, text) = payload.split_once('\n').unwrap_or((payload, ""));
        let name = name.to_owned();
        let Some(index) = self.open_table(&name) else {
            return;
        };
        let header = vec![styled(&format!("str({name})"), Some(Highlight::Comment))];
        if let Some((buffer, Mode::Table(view))) = self.buffers.entry_mut(index) {
            view.table = None;
            view.header = header;
            view.styles = Vec::new();
            buffer.set_text(text);
            buffer.set_point(0);
        }
    }

    pub(in crate::editor) fn render_tables(&mut self) {
        for index in 0..self.buffers.len() {
            let room = self
                .buffers
                .get(index)
                .and_then(|buffer| self.width_showing(buffer.id()))
                .filter(|width| *width > 0)
                .unwrap_or(ROOM);
            let Some((buffer, Mode::Table(view))) = self.buffers.entry_mut(index) else {
                continue;
            };
            let Some(table) = &view.table else {
                continue;
            };
            if view.width == room {
                continue;
            }
            let (header, page) = lay_out(&view.name, table, view.first, room);
            page.fill(buffer);
            view.header = header;
            view.styles = page.styles;
            view.width = room;
        }
    }

    pub(in crate::editor) fn r_table_shift(&mut self, forward: bool) {
        let Some(Mode::Table(view)) = self.buffers.mode_mut(0) else {
            return;
        };
        let Some(table) = &view.table else {
            return;
        };
        let last = table.names.len().saturating_sub(1);
        let next = if forward {
            (view.first + 1).min(last)
        } else {
            view.first.saturating_sub(1)
        };
        if next == view.first {
            let edge = if forward { "last" } else { "first" };
            self.show_message(format!("Already at the {edge} column"));
            return;
        }
        view.first = next;
        view.width = 0;
        self.render_tables();
    }

    pub(in crate::editor) fn r_table_reload(&mut self) {
        let Some(Mode::Table(view)) = self.buffers.mode(0) else {
            return;
        };
        let name = r::string(&view.name);
        let call = if view.table.is_some() {
            format!("getOption(\"oxmacs\")$view({name}, {VIEW_ROWS}L)")
        } else {
            format!("getOption(\"oxmacs\")$inspect({name})")
        };
        self.r_request(&call);
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::test_support::{R_PROMPT, lines, press, r_running, r_script, r_sent};
    use crate::editor::{Editor, JobId};

    const CARS: &str = "cars\t32\t11\n\tmpg\tcyl\tdisp\tname\nMazda RX4\t21\t6\t160\tfast\nDatsun 710\t22.8\t4\t108\tsmall";

    fn viewed(payload: &str) -> (Editor, JobId) {
        let mut editor = r_script("x\n", 0);
        let job = r_running(&mut editor);
        editor.session_output(job, &format!("\u{1}table\u{2}{payload}\u{3}{R_PROMPT}"));
        (editor, job)
    }

    fn header(editor: &Editor) -> Vec<String> {
        editor
            .pane()
            .header_lines()
            .into_iter()
            .map(|(text, _)| text)
            .collect()
    }

    #[test]
    fn tables_show_rows_under_a_header_of_names_and_types() {
        let (editor, _) = viewed(&CARS.replace(
            "\tcyl\tdisp\tname",
            "\tcyl\tdisp\tname\n\tnumeric\tnumeric\tnumeric\tcharacter",
        ));

        assert_eq!(editor.buffer().name(), "*R:analysis cars*");
        let header = header(&editor);
        assert!(
            header[0].starts_with("cars  32 × 11 (first 2)  columns 1–4"),
            "{header:?}"
        );
        assert_eq!(header[1], "              mpg    cyl   disp  name");
        assert_eq!(header[2], "            <dbl>  <dbl>  <dbl>  <chr>");
        assert_eq!(
            lines(&editor),
            [
                "Mazda RX4      21      6    160  fast",
                "Datsun 710   22.8      4    108  small"
            ]
        );
        assert_eq!(editor.message(), Some("cars: 32 rows, 11 columns"));
    }

    #[test]
    fn narrow_windows_scroll_through_columns_with_the_arrows() {
        let (mut editor, _) = viewed(&CARS.replace(
            "\tcyl\tdisp\tname",
            "\tcyl\tdisp\tname\n\tnumeric\tnumeric\tnumeric\tcharacter",
        ));
        editor.resize_pane(editor.selected_window(), 20, 26);

        assert_eq!(header(&editor)[1], "              mpg    cyl");
        press(&mut editor, "<right>");
        assert_eq!(header(&editor)[1], "              cyl   disp");
        press(&mut editor, "<left> <left>");
        assert_eq!(editor.message(), Some("Already at the first column"));
    }

    #[test]
    fn g_asks_r_for_the_table_again() {
        let (mut editor, _) = viewed(&CARS.replace(
            "\tcyl\tdisp\tname",
            "\tcyl\tdisp\tname\n\tnumeric\tnumeric\tnumeric\tcharacter",
        ));
        editor.take_effects();

        press(&mut editor, "g");

        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$view(\"cars\", 1000L)\n"]
        );
    }

    #[test]
    fn other_objects_show_their_structure() {
        let mut editor = r_script("x\n", 0);
        let job = r_running(&mut editor);

        editor.session_output(
            job,
            &format!("\u{1}object\u{2}add\nfunction (a, b = 2)  \u{3}{R_PROMPT}"),
        );

        assert_eq!(editor.buffer().name(), "*R:analysis add*");
        assert_eq!(header(&editor), ["str(add)"]);
        assert_eq!(lines(&editor), ["function (a, b = 2)  "]);
    }
}
