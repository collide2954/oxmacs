use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::external::r;
use crate::text::{Buffer, BufferId};
use crate::theme::Highlight;

const GROUPS: [(&str, &str); 3] = [
    ("data", "Data"),
    ("values", "Values"),
    ("functions", "Functions"),
];
const HELP: &str = "RET opens   g refreshes   q quits";
const VALUE_WIDTH: usize = 60;
pub(in crate::editor) const VIEW_ROWS: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    group: String,
    name: String,
    class: String,
    size: String,
    value: String,
}

impl Entry {
    fn tabular(&self) -> bool {
        matches!(
            self.class.as_str(),
            "data.frame" | "matrix" | "tbl_df" | "data.table"
        )
    }
}

#[derive(Debug, Default)]
pub(in crate::editor) struct EnvironmentView {
    session: Option<BufferId>,
    entries: Vec<Entry>,
    rows: Vec<(usize, usize)>,
    styles: Vec<LineStyle>,
}

impl EnvironmentView {
    pub(in crate::editor) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }

    pub(in crate::editor) fn session(&self) -> Option<BufferId> {
        self.session
    }
}

fn entries(payload: &str) -> Vec<Entry> {
    payload
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(5, '\t');
            Some(Entry {
                group: fields.next()?.to_owned(),
                name: fields.next()?.to_owned(),
                class: fields.next()?.to_owned(),
                size: fields.next()?.to_owned(),
                value: fields.next().unwrap_or_default().trim_end().to_owned(),
            })
        })
        .collect()
}

fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let mut clipped: String = text.chars().take(width.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}

fn page(entries: &[Entry]) -> (Page, Vec<(usize, usize)>) {
    let mut page = Page::default();
    page.line(&[(HELP, Some(Highlight::Comment))]);
    let mut rows = Vec::new();
    if entries.is_empty() {
        page.line(&[]);
        page.line(&[("The environment is empty", Some(Highlight::Comment))]);
        return (page, rows);
    }
    let width = |pick: fn(&Entry) -> &str| {
        entries
            .iter()
            .map(|entry| pick(entry).chars().count())
            .max()
            .unwrap_or(0)
    };
    let (names, classes, sizes) = (
        width(|entry| &entry.name),
        width(|entry| &entry.class),
        width(|entry| &entry.size),
    );
    for (group, title) in GROUPS {
        let members: Vec<(usize, &Entry)> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.group == group)
            .collect();
        if members.is_empty() {
            continue;
        }
        page.line(&[]);
        page.heading(title);
        for (index, entry) in members {
            rows.push((page.styles.len(), index));
            let name = format!("  {:<names$}  ", entry.name);
            let class = format!("{:<classes$}  ", entry.class);
            let size = format!("{:>sizes$}  ", entry.size);
            let value = clip(&entry.value, VALUE_WIDTH);
            page.line(&[
                (&name, Some(Highlight::Link)),
                (&class, Some(Highlight::Comment)),
                (&size, None),
                (&value, Some(Highlight::Comment)),
            ]);
        }
    }
    (page, rows)
}

impl Editor {
    fn environment_index(&self) -> Option<usize> {
        let session = self.session_id();
        self.buffers
            .find_mode(|mode| matches!(mode, Mode::Environment(view) if view.session == session))
    }

    pub(in crate::editor) fn r_environment(&mut self) {
        if !self.r_running() && !self.start_r() {
            return;
        }
        self.link_script();
        let Some(session) = self.session_id() else {
            return;
        };
        self.with_session(session, |editor| {
            editor.show_environment();
            editor.r_request("getOption(\"oxmacs\")$environment()");
        });
        if let Some(window) = self
            .environment_index()
            .and_then(|index| self.buffers.get(index))
            .and_then(|buffer| self.window_showing(buffer.id()))
        {
            self.select_window(window);
        }
    }

    fn show_environment(&mut self) {
        let showing = self
            .environment_index()
            .and_then(|index| self.buffers.get(index))
            .is_some_and(|buffer| self.window_showing(buffer.id()).is_some());
        if showing {
            return;
        }
        let place = |editor: &mut Self| {
            if let Some(index) = editor.environment_index() {
                editor.select_buffer(index);
                return;
            }
            let session = editor.session_id();
            let console = session
                .and_then(|id| editor.buffers.position(|known| known.id() == id))
                .and_then(|index| editor.buffers.get(index))
                .map_or("*R*", Buffer::name);
            let name = format!("{} environment*", console.trim_end_matches('*'));
            let mut buffer = Buffer::with_text(&name, HELP);
            buffer.set_read_only(true);
            let view = EnvironmentView {
                session,
                ..EnvironmentView::default()
            };
            editor.add_buffer_with(buffer, Mode::Environment(view));
        };
        let console = self
            .console_index()
            .and_then(|index| self.buffers.get(index))
            .and_then(|buffer| self.window_showing(buffer.id()));
        match console {
            Some(window) => self.split_off(window, crate::editor::Axis::Rows, place),
            None => self.beside(place),
        }
    }

    pub(in crate::editor) fn r_environment_arrived(&mut self, payload: &str) {
        let Some(index) = self.environment_index() else {
            return;
        };
        let found = entries(payload);
        let (page, rows) = page(&found);
        if let Some(buffer) = self.buffers.get_mut(index) {
            page.fill(buffer);
        }
        if let Some(Mode::Environment(view)) = self.buffers.mode_mut(index) {
            view.entries = found;
            view.rows = rows;
            view.styles = page.styles;
        }
    }

    pub(in crate::editor) fn r_environment_due(&mut self) {
        let shown = self
            .environment_index()
            .and_then(|index| self.buffers.get(index))
            .is_some_and(|buffer| self.window_showing(buffer.id()).is_some());
        if shown {
            self.r_request("getOption(\"oxmacs\")$environment()");
        }
    }

    pub(in crate::editor) fn r_environment_visit(&mut self) {
        let line = self.point_line();
        let Some(Mode::Environment(view)) = self.buffers.mode(0) else {
            return;
        };
        let Some(entry) = view
            .rows
            .iter()
            .find(|(row, _)| *row == line)
            .and_then(|(_, index)| view.entries.get(*index))
            .cloned()
        else {
            self.show_message("No object on this line");
            return;
        };
        let name = r::string(&entry.name);
        let call = if entry.tabular() {
            format!("getOption(\"oxmacs\")$view({name}, {VIEW_ROWS}L)")
        } else {
            format!("getOption(\"oxmacs\")$inspect({name})")
        };
        self.r_request(&call);
    }

    pub(in crate::editor) fn refresh_environment(&mut self) {
        self.r_request("getOption(\"oxmacs\")$environment()");
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::test_support::{R_PROMPT, lines, press, r_running, r_script, r_sent};
    use crate::editor::{Editor, JobId};
    use crate::theme::Highlight;

    const LISTING: &str = "functions\tadd\tfunction\t1\tfunction (a, b = 2) \ndata\tcars\tdata.frame\t32 x 11\t32 obs. of 11 variables\nvalues\ts\tcharacter\t1\t\"hello world\"\nvalues\tx\tinteger\t10\t1 2 3 4 5 6 7 8 9 10";

    fn opened() -> (Editor, JobId) {
        let mut editor = r_script("x <- 1:10\n", 0);
        let job = r_running(&mut editor);
        press(&mut editor, "C-c C-e");
        (editor, job)
    }

    fn listing(editor: &mut Editor, job: JobId) {
        editor.session_output(
            job,
            &format!("\u{1}environment\u{2}{LISTING}\u{3}{R_PROMPT}"),
        );
    }

    #[test]
    fn control_c_control_e_asks_r_and_lists_objects_by_group() {
        let (mut editor, job) = opened();
        assert_eq!(editor.buffer().name(), "*R:analysis environment*");
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$environment()\n"]
        );

        listing(&mut editor, job);

        let shown = lines(&editor);
        assert_eq!(shown[2], "Data");
        assert!(
            shown[3].starts_with("  cars  data.frame  32 x 11  32 obs. of 11 variables"),
            "{shown:?}"
        );
        assert_eq!(shown[5], "Values");
        assert_eq!(shown[6], "  s     character         1  \"hello world\"");
        assert_eq!(shown[9], "Functions");
        assert_eq!(editor.line_style(2).spans[0].1, Highlight::Heading);
        assert_eq!(editor.tiling().windows().len(), 3);
    }

    #[test]
    fn return_opens_tables_as_tables_and_other_objects_with_str() {
        let (mut editor, job) = opened();
        listing(&mut editor, job);
        editor.take_effects();

        press(&mut editor, "M-< C-n C-n C-n RET");
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$view(\"cars\", 1000L)\n"]
        );
        press(&mut editor, "C-n C-n C-n RET");
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$inspect(\"s\")\n"]
        );
    }

    #[test]
    fn the_listing_follows_commands_while_it_is_shown() {
        let (mut editor, job) = opened();
        listing(&mut editor, job);
        editor.take_effects();
        press(&mut editor, "C-x o C-x o");

        press(&mut editor, "C-c C-z");
        crate::editor::test_support::type_text(&mut editor, "y <- 2");
        press(&mut editor, "RET");
        editor.take_effects();
        editor.session_output(job, R_PROMPT);

        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$environment()\n"]
        );
        editor.session_output(job, R_PROMPT);
        assert_eq!(r_sent(&mut editor), Vec::<String>::new());
    }
}
