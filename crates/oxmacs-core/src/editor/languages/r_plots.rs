use std::path::{Path, PathBuf};

use crate::editor::WindowId;
use crate::editor::buffers::Mode;
use crate::editor::prompts::minibuffer::Purpose;
use crate::editor::{Editor, Effect};
use crate::external::r;
use crate::text::{Buffer, BufferId};

const SIZE: (u32, u32) = (672, 480);
const ZOOM_WIDTH: u32 = 1200;
const NONE: &str = "none";
const EMPTY: &str = "No plots";

type Render = (usize, (u32, u32));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlotView {
    pub path: PathBuf,
    pub index: usize,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GalleryView {
    pub paths: Vec<PathBuf>,
    pub selected: usize,
    pub columns: usize,
}

fn columns(count: usize) -> usize {
    (1..=count.max(1))
        .find(|columns| columns * columns >= count)
        .unwrap_or(1)
}

#[derive(Debug, Default)]
pub(in crate::editor) struct Plots {
    session: Option<BufferId>,
    gallery: Option<GalleryView>,
    shown: Option<PlotView>,
    size: Option<(u32, u32)>,
    wanted: Option<usize>,
    rendered: Option<Render>,
    asked: Option<Render>,
}

impl Plots {
    pub(in crate::editor) fn view(&self) -> Option<PlotView> {
        self.shown.clone()
    }

    pub(in crate::editor) fn session(&self) -> Option<BufferId> {
        self.session
    }

    pub(in crate::editor) fn gallery(&self) -> Option<GalleryView> {
        self.gallery.clone()
    }

    fn move_selection(&mut self, step: isize) -> bool {
        let Some(gallery) = &mut self.gallery else {
            return false;
        };
        if let Some(next) = gallery
            .selected
            .checked_add_signed(step)
            .filter(|next| *next < gallery.paths.len())
        {
            gallery.selected = next;
        }
        true
    }

    fn want(&self) -> Option<Render> {
        let index = self
            .wanted
            .or_else(|| self.shown.as_ref().map(|shown| shown.index))?;
        Some((index, self.size.unwrap_or(SIZE)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why {
    New,
    Asked,
    Deleted,
}

struct Arrival {
    why: Why,
    index: usize,
    count: usize,
    size: (u32, u32),
    path: PathBuf,
}

fn arrival(payload: &str) -> Option<Arrival> {
    let mut fields = payload.splitn(6, '\t');
    let why = match fields.next()? {
        "new" => Why::New,
        "asked" => Why::Asked,
        "deleted" => Why::Deleted,
        _ => return None,
    };
    let index = fields.next()?.parse().ok()?;
    let count = fields.next()?.parse().ok()?;
    let width = fields.next()?.parse().ok()?;
    let height = fields.next()?.parse().ok()?;
    let path = PathBuf::from(fields.next()?);
    Some(Arrival {
        why,
        index,
        count,
        size: (width, height),
        path,
    })
}

impl Editor {
    fn plots_index(&self) -> Option<usize> {
        let in_plots = matches!(self.buffers.mode(0), Some(Mode::Plots(_)));
        if in_plots && !self.r_focused() {
            return Some(0);
        }
        let session = self.session_id();
        self.buffers
            .find_mode(|mode| matches!(mode, Mode::Plots(plots) if plots.session == session))
    }

    fn plots_in(&mut self, window: WindowId) -> Option<BufferId> {
        let pane = self.pane_of(window)?;
        let buffer = pane.buffer().id();
        pane.plot_session()?;
        Some(buffer)
    }

    fn in_plots_window(&mut self, window: WindowId, act: impl FnOnce(&mut Self)) {
        let Some(buffer) = self.plots_in(window) else {
            return;
        };
        let Some(index) = self.buffers.position(|known| known.id() == buffer) else {
            return;
        };
        let Some(Mode::Plots(plots)) = self.buffers.mode(index) else {
            return;
        };
        let Some(session) = plots.session else {
            return;
        };
        self.with_session(session, act);
    }

    fn plots_mut(&mut self) -> Option<&mut Plots> {
        let index = self.plots_index()?;
        match self.buffers.mode_mut(index)? {
            Mode::Plots(plots) => Some(plots),
            _ => None,
        }
    }

    fn show_plots(&mut self) {
        let showing = self
            .plots_index()
            .and_then(|index| self.buffers.get(index))
            .is_some_and(|buffer| self.window_showing(buffer.id()).is_some());
        if showing {
            return;
        }
        let place = |editor: &mut Self| {
            if let Some(index) = editor.plots_index() {
                editor.select_buffer(index);
                return;
            }
            let session = editor.session_id();
            let console = session
                .and_then(|id| editor.buffers.position(|known| known.id() == id))
                .and_then(|index| editor.buffers.get(index))
                .map_or("*R*", Buffer::name);
            let name = format!("{} plots*", console.trim_end_matches('*'));
            let mut buffer = Buffer::with_text(&name, "");
            buffer.set_read_only(true);
            let plots = Plots {
                session,
                ..Plots::default()
            };
            editor.add_buffer_with(buffer, Mode::Plots(plots));
        };
        let console = self
            .console_index()
            .and_then(|index| self.buffers.get(index))
            .and_then(|buffer| self.window_showing(buffer.id()));
        match console {
            Some(window) => self.below(window, place),
            None => self.beside(place),
        }
    }

    pub(in crate::editor) fn r_plot(&mut self, payload: &str) {
        if payload == NONE {
            self.r_plots_emptied();
            return;
        }
        let Some(arrival) = arrival(payload) else {
            return;
        };
        self.show_plots();
        let Some(plots) = self.plots_mut() else {
            return;
        };
        if arrival.why != Why::New {
            plots.asked = None;
        }
        if arrival.why != Why::Asked {
            plots.wanted = Some(arrival.index);
        }
        plots.rendered = Some((arrival.index, arrival.size));
        plots.shown = Some(PlotView {
            path: arrival.path,
            index: arrival.index,
            count: arrival.count,
        });
        self.r_plot_refresh();
    }

    fn r_plot_refresh(&mut self) {
        let Some(plots) = self.plots_mut() else {
            return;
        };
        let Some(want) = plots.want() else {
            return;
        };
        if plots.asked.is_some() || plots.rendered == Some(want) {
            return;
        }
        let (index, (width, height)) = want;
        let call = format!("getOption(\"oxmacs\")$plot({index}L, {width}L, {height}L)");
        if self.r_request(&call)
            && let Some(plots) = self.plots_mut()
        {
            plots.asked = Some(want);
        }
    }

    pub fn plots_sized(&mut self, window: WindowId, width: u32, height: u32) {
        self.in_plots_window(window, |editor| {
            if let Some(plots) = editor.plots_mut() {
                plots.size = Some((width, height));
            }
            editor.r_plot_refresh();
        });
    }

    pub(in crate::editor) fn r_plot_step(&mut self, forward: bool) {
        let Some(plots) = self.plots_mut() else {
            return;
        };
        if plots.move_selection(if forward { 1 } else { -1 }) {
            return;
        }
        let Some((current, count)) = plots
            .want()
            .zip(plots.shown.as_ref())
            .map(|((index, _), shown)| (index, shown.count))
        else {
            return;
        };
        let next = if forward {
            Some(current + 1).filter(|next| *next <= count)
        } else {
            current.checked_sub(1).filter(|next| *next >= 1)
        };
        let Some(next) = next else {
            let edge = if forward { "last" } else { "first" };
            self.show_message(format!("This is the {edge} plot"));
            return;
        };
        plots.wanted = Some(next);
        self.r_plot_refresh();
    }

    fn r_plots_emptied(&mut self) {
        let Some(index) = self.plots_index() else {
            return;
        };
        if let Some((buffer, Mode::Plots(plots))) = self.buffers.entry_mut(index) {
            *plots = Plots {
                size: plots.size,
                ..Plots::default()
            };
            buffer.set_text(EMPTY);
        }
    }

    fn shown_plot(&mut self, action: &str) -> Option<(usize, (u32, u32))> {
        let shown = self.plots_mut().and_then(|plots| {
            let index = plots.shown.as_ref()?.index;
            Some((index, plots.size.unwrap_or(SIZE)))
        });
        if shown.is_none() {
            self.show_message(format!("No plot to {action}"));
        }
        shown
    }

    pub(in crate::editor) fn r_plot_save(&mut self) {
        let Some((index, _)) = self.shown_plot("save") else {
            return;
        };
        if self.open_file_prompt("Save plot as: ", Purpose::SavePlot) {
            let input = self.minibuffer_input();
            self.set_minibuffer_input(&format!("{input}plot-{index}.png"));
        }
    }

    pub(in crate::editor) fn save_plot_to(&mut self, path: &Path) {
        let Some((index, (width, height))) = self.shown_plot("save") else {
            return;
        };
        let file = r::string(&path.display().to_string());
        self.r_request(&format!(
            "getOption(\"oxmacs\")$plot_save({index}L, {file}, {width}L, {height}L)"
        ));
    }

    pub(in crate::editor) fn r_plot_delete(&mut self) {
        let Some(shown) = self.shown_plot("delete") else {
            return;
        };
        let (index, (width, height)) = shown;
        let call = format!("getOption(\"oxmacs\")$plot_delete({index}L, {width}L, {height}L)");
        if self.r_request(&call)
            && let Some(plots) = self.plots_mut()
        {
            plots.asked = Some(shown);
            plots.wanted = None;
        }
    }

    pub(in crate::editor) fn r_plot_zoom(&mut self) {
        let Some((index, (width, height))) = self.shown_plot("zoom") else {
            return;
        };
        let wide = width.saturating_mul(2).max(ZOOM_WIDTH);
        let tall = u32::try_from(u64::from(wide) * u64::from(height) / u64::from(width.max(1)))
            .unwrap_or(u32::MAX);
        self.r_request(&format!(
            "getOption(\"oxmacs\")$plot_zoom({index}L, {wide}L, {tall}L)"
        ));
    }

    pub(in crate::editor) fn r_plot_row(&mut self, down: bool) {
        let Some(plots) = self.plots_mut() else {
            return;
        };
        let Some(columns) = plots.gallery.as_ref().map(|gallery| gallery.columns) else {
            self.r_plot_step(down);
            return;
        };
        let columns = isize::try_from(columns).unwrap_or(1);
        plots.move_selection(if down { columns } else { -columns });
    }

    pub(in crate::editor) fn r_plot_gallery(&mut self) {
        if let Some(plots) = self.plots_mut()
            && plots.gallery.take().is_some()
        {
            return;
        }
        let Some(count) = self
            .plots_mut()
            .and_then(|plots| plots.shown.as_ref().map(|shown| shown.count))
        else {
            self.show_message("No plots");
            return;
        };
        let Some((_, (width, height))) = self.shown_plot("show") else {
            return;
        };
        let across = columns(count);
        let down = count.div_ceil(across);
        let tile = |extent: u32, parts: usize| extent / u32::try_from(parts).unwrap_or(1).max(1);
        self.r_request(&format!(
            "getOption(\"oxmacs\")$plot_gallery({}L, {}L)",
            tile(width, across),
            tile(height, down)
        ));
    }

    pub(in crate::editor) fn r_plot_thumbnails(&mut self, payload: &str) {
        let paths: Vec<PathBuf> = payload
            .split('\t')
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .collect();
        let Some(plots) = self.plots_mut() else {
            return;
        };
        if paths.is_empty() {
            plots.gallery = None;
            return;
        }
        let shown = plots.shown.as_ref().map_or(1, |shown| shown.index);
        plots.gallery = Some(GalleryView {
            selected: shown.saturating_sub(1).min(paths.len() - 1),
            columns: columns(paths.len()),
            paths,
        });
    }

    pub(in crate::editor) fn r_plot_open(&mut self) {
        let selected = self
            .plots_mut()
            .and_then(|plots| plots.gallery.as_ref())
            .map(|gallery| gallery.selected);
        if let Some(selected) = selected {
            self.pick_plot(selected);
        }
    }

    pub fn pick_plot_in(&mut self, window: WindowId, position: usize) {
        self.in_plots_window(window, |editor| editor.pick_plot(position));
    }

    fn pick_plot(&mut self, position: usize) {
        let Some(plots) = self.plots_mut() else {
            return;
        };
        plots.gallery = None;
        plots.wanted = Some(position + 1);
        self.r_plot_refresh();
    }

    pub(in crate::editor) fn r_plot_zoomed(&mut self, path: &str) {
        self.effects
            .push(Effect::OpenExternally(PathBuf::from(path)));
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::test_support::{
        R_CONSOLE, R_PLOTS, R_PROMPT, press, r_running, r_script, r_sent, r_sessions, type_text,
    };
    use crate::editor::{Axis, Editor, Effect, JobId, Tiling, WindowId};
    use crate::external::r::PLOTS_VARIABLE;

    use std::path::PathBuf;

    use super::{GalleryView, PlotView};

    fn plot(editor: &mut Editor, job: JobId, fields: &str) {
        editor.session_output(job, &format!("\u{1}plot\u{2}{fields}\u{3}{R_PROMPT}"));
    }

    fn plots_window(editor: &Editor) -> WindowId {
        editor
            .tiling()
            .windows()
            .into_iter()
            .find(|window| {
                editor
                    .pane_of(*window)
                    .is_some_and(|pane| pane.plot().is_some())
            })
            .unwrap()
    }

    fn shown(editor: &Editor) -> Option<PlotView> {
        editor
            .tiling()
            .windows()
            .into_iter()
            .find_map(|window| editor.pane_of(window)?.plot())
    }

    fn view(index: usize, count: usize, path: &str) -> PlotView {
        PlotView {
            path: path.into(),
            index,
            count,
        }
    }

    fn started() -> (Editor, JobId) {
        let mut editor = r_script("plot(1:10)\n", 0);
        let job = r_running(&mut editor);
        (editor, job)
    }

    #[test]
    fn a_new_plot_opens_below_the_console() {
        let (mut editor, job) = started();

        plot(&mut editor, job, "new\t1\t1\t672\t480\tC:/plots/plot-1.png");

        assert_eq!(shown(&editor), Some(view(1, 1, "C:/plots/plot-1.png")));
        assert_eq!(editor.buffer().name(), "t");
        let Tiling::Split { parts, .. } = editor.tiling() else {
            panic!("not split");
        };
        let Tiling::Split { axis, parts: right } = &parts[1].1 else {
            panic!("the console window was not split");
        };
        assert_eq!(*axis, Axis::Rows);
        let names: Vec<String> = right
            .iter()
            .filter_map(|(_, part)| match part {
                Tiling::Window(window) => editor.pane_of(*window),
                Tiling::Split { .. } => None,
            })
            .map(|pane| pane.buffer().name().to_owned())
            .collect();
        assert_eq!(names, [R_CONSOLE, R_PLOTS]);
    }

    #[test]
    fn later_plots_reuse_the_plots_window() {
        let (mut editor, job) = started();
        plot(&mut editor, job, "new\t1\t1\t672\t480\tC:/plots/plot-1.png");

        plot(&mut editor, job, "new\t2\t2\t672\t480\tC:/plots/plot-2.png");

        assert_eq!(shown(&editor), Some(view(2, 2, "C:/plots/plot-2.png")));
        assert_eq!(editor.tiling().windows().len(), 3);
    }

    #[test]
    fn the_plots_window_has_r_redraw_at_its_size_one_request_at_a_time() {
        let (mut editor, job) = started();
        plot(&mut editor, job, "new\t1\t1\t672\t480\tC:/plots/plot-1.png");
        editor.take_effects();

        editor.plots_sized(plots_window(&editor), 800, 600);
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$plot(1L, 800L, 600L)\n"]
        );
        editor.plots_sized(plots_window(&editor), 900, 600);
        assert_eq!(r_sent(&mut editor), Vec::<String>::new());

        plot(
            &mut editor,
            job,
            "asked\t1\t1\t800\t600\tC:/plots/plot-2.png",
        );
        assert_eq!(shown(&editor), Some(view(1, 1, "C:/plots/plot-2.png")));
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$plot(1L, 900L, 600L)\n"]
        );
        plot(
            &mut editor,
            job,
            "asked\t1\t1\t900\t600\tC:/plots/plot-3.png",
        );
        editor.plots_sized(plots_window(&editor), 900, 600);
        assert_eq!(r_sent(&mut editor), Vec::<String>::new());
    }

    #[test]
    fn n_and_p_step_through_earlier_plots() {
        let (mut editor, job) = started();
        plot(&mut editor, job, "new\t1\t2\t672\t480\tC:/plots/plot-1.png");
        plot(&mut editor, job, "new\t2\t2\t672\t480\tC:/plots/plot-2.png");
        editor.take_effects();
        press(&mut editor, "C-x o C-x o");
        assert_eq!(editor.buffer().name(), R_PLOTS);

        press(&mut editor, "p");
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$plot(1L, 672L, 480L)\n"]
        );
        plot(
            &mut editor,
            job,
            "asked\t1\t2\t672\t480\tC:/plots/plot-3.png",
        );
        assert_eq!(shown(&editor), Some(view(1, 2, "C:/plots/plot-3.png")));

        press(&mut editor, "p");
        assert_eq!(editor.message(), Some("This is the first plot"));
        press(&mut editor, "n");
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$plot(2L, 672L, 480L)\n"]
        );
    }

    fn in_plots(editor: &mut Editor, job: JobId, count: usize) {
        for index in 1..=count {
            plot(
                editor,
                job,
                &format!("new\t{index}\t{count}\t672\t480\tC:/plots/plot-{index}.png"),
            );
        }
        editor.take_effects();
        press(editor, "C-x o C-x o");
        assert_eq!(editor.buffer().name(), R_PLOTS);
    }

    #[test]
    fn s_saves_the_shown_plot_under_a_chosen_name() {
        let (mut editor, job) = started();
        in_plots(&mut editor, job, 1);

        press(&mut editor, "s");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Save plot as: ");
        let input = editor.minibuffer().unwrap().input().contents();
        assert!(input.ends_with("plot-1.png"), "{input}");
        press(&mut editor, "RET");

        let sent = r_sent(&mut editor);
        assert_eq!(sent.len(), 1);
        assert!(sent[0].starts_with("getOption(\"oxmacs\")$plot_save(1L, \""));
        assert!(
            sent[0].ends_with("plot-1.png\", 672L, 480L)\n"),
            "{}",
            sent[0]
        );
    }

    #[test]
    fn d_deletes_the_shown_plot_and_shows_its_neighbour() {
        let (mut editor, job) = started();
        in_plots(&mut editor, job, 2);

        press(&mut editor, "d");
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$plot_delete(2L, 672L, 480L)\n"]
        );
        plot(
            &mut editor,
            job,
            "deleted\t1\t1\t672\t480\tC:/plots/plot-3.png",
        );
        assert_eq!(shown(&editor), Some(view(1, 1, "C:/plots/plot-3.png")));

        press(&mut editor, "d");
        plot(&mut editor, job, "none");
        assert_eq!(shown(&editor), None);
        assert_eq!(editor.buffer().contents(), "No plots");
        press(&mut editor, "d");
        assert_eq!(editor.message(), Some("No plot to delete"));
    }

    #[test]
    fn z_opens_a_large_copy_in_the_system_viewer() {
        let (mut editor, job) = started();
        in_plots(&mut editor, job, 1);

        press(&mut editor, "z");
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$plot_zoom(1L, 1344L, 960L)\n"]
        );
        editor.session_output(
            job,
            &format!("\u{1}zoom\u{2}C:/plots/zoom-1.png\u{3}{R_PROMPT}"),
        );

        assert_eq!(
            editor.take_effects(),
            [Effect::OpenExternally("C:/plots/zoom-1.png".into())]
        );
    }

    fn gallery(editor: &Editor) -> Option<GalleryView> {
        editor
            .tiling()
            .windows()
            .into_iter()
            .find_map(|window| editor.pane_of(window)?.plot_gallery())
    }

    fn thumbnails(editor: &mut Editor, job: JobId, count: usize) {
        let paths: Vec<String> = (1..=count)
            .map(|index| format!("C:/plots/thumb-{index}.png"))
            .collect();
        editor.session_output(
            job,
            &format!("\u{1}gallery\u{2}{}\u{3}{R_PROMPT}", paths.join("\t")),
        );
    }

    #[test]
    fn v_asks_r_for_a_grid_of_thumbnails() {
        let (mut editor, job) = started();
        in_plots(&mut editor, job, 4);

        press(&mut editor, "v");
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$plot_gallery(336L, 240L)\n"]
        );
        thumbnails(&mut editor, job, 4);

        let shown = gallery(&editor).unwrap();
        assert_eq!(shown.columns, 2);
        assert_eq!(shown.selected, 3);
        assert_eq!(shown.paths[0], PathBuf::from("C:/plots/thumb-1.png"));
        assert_eq!(shown.paths.len(), 4);
    }

    #[test]
    fn keys_move_through_the_gallery_and_return_opens_a_plot() {
        let (mut editor, job) = started();
        in_plots(&mut editor, job, 4);
        press(&mut editor, "v");
        thumbnails(&mut editor, job, 4);
        editor.take_effects();

        press(&mut editor, "<up>");
        assert_eq!(gallery(&editor).unwrap().selected, 1);
        press(&mut editor, "p");
        assert_eq!(gallery(&editor).unwrap().selected, 0);
        press(&mut editor, "<down> <right>");
        assert_eq!(gallery(&editor).unwrap().selected, 3);
        press(&mut editor, "<left>");
        assert_eq!(r_sent(&mut editor), Vec::<String>::new());

        press(&mut editor, "RET");
        assert_eq!(gallery(&editor), None);
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$plot(3L, 672L, 480L)\n"]
        );
    }

    #[test]
    fn a_click_opens_a_thumbnail_and_v_leaves_the_gallery() {
        let (mut editor, job) = started();
        in_plots(&mut editor, job, 2);
        press(&mut editor, "v");
        thumbnails(&mut editor, job, 2);
        editor.take_effects();

        editor.pick_plot_in(plots_window(&editor), 0);
        assert_eq!(gallery(&editor), None);
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$plot(1L, 672L, 480L)\n"]
        );

        press(&mut editor, "v");
        thumbnails(&mut editor, job, 2);
        press(&mut editor, "v");
        assert_eq!(gallery(&editor), None);
    }

    #[test]
    fn a_new_plot_keeps_the_console_prompts_in_step() {
        let mut editor = r_script("plot(1:3)\nabline(h = 2)\n", 0);
        press(&mut editor, "C-c C-z");
        editor.resize_pane(editor.selected_window(), 20, 61);
        let job = r_sessions(&mut editor)[0];
        editor.session_output(job, R_PROMPT);
        let width = r_sent(&mut editor);
        editor.session_output(job, R_PROMPT);
        press(&mut editor, "C-c C-z");
        assert_eq!(width, ["getOption(\"oxmacs\")$width(60L)\n"]);

        press(&mut editor, "C-RET");
        editor.take_effects();
        plot(&mut editor, job, "new\t1\t1\t672\t480\tC:/plots/plot-1.png");
        assert_eq!(r_sent(&mut editor), Vec::<String>::new());
        press(&mut editor, "C-RET");
        editor.session_output(job, R_PROMPT);

        press(&mut editor, "C-x b");
        type_text(&mut editor, R_CONSOLE);
        press(&mut editor, "RET");
        assert_eq!(
            editor.buffer().contents(),
            "> plot(1:3)\n> abline(h = 2)\n> "
        );
    }

    #[test]
    fn r_keeps_its_plots_where_oxmacs_can_read_them() {
        let mut editor = r_script("x\n", 0);
        press(&mut editor, "C-c C-z");

        let job = editor
            .take_effects()
            .into_iter()
            .find_map(|effect| match effect {
                Effect::StartSession(job) => Some(job),
                _ => None,
            })
            .unwrap();

        let (_, directory) = job
            .env
            .iter()
            .find(|(name, _)| name == PLOTS_VARIABLE)
            .unwrap();
        assert!(directory.contains("oxmacs"));
    }
}
