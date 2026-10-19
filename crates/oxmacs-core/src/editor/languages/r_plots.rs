use std::path::{Path, PathBuf};

use crate::editor::buffers::Mode;
use crate::editor::prompts::minibuffer::Purpose;
use crate::editor::{Editor, Effect};
use crate::external::r;
use crate::text::Buffer;

const NAME: &str = "*R plots*";
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

#[derive(Debug, Default)]
pub(in crate::editor) struct Plots {
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
        self.buffers
            .find_mode(|mode| matches!(mode, Mode::Plots(_)))
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
            let mut buffer = Buffer::with_text(NAME, "");
            buffer.set_read_only(true);
            editor.add_buffer_with(buffer, Mode::Plots(Plots::default()));
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

    pub fn plots_sized(&mut self, width: u32, height: u32) {
        if let Some(plots) = self.plots_mut() {
            plots.size = Some((width, height));
        }
        self.r_plot_refresh();
    }

    pub(in crate::editor) fn r_plot_step(&mut self, forward: bool) {
        let Some(plots) = self.plots_mut() else {
            return;
        };
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

    pub(in crate::editor) fn r_plot_zoomed(&mut self, path: &str) {
        self.effects
            .push(Effect::OpenExternally(PathBuf::from(path)));
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::test_support::{R_PROMPT, press, r_running, r_script, r_sent};
    use crate::editor::{Axis, Editor, Effect, JobId, Tiling};
    use crate::external::r::PLOTS_VARIABLE;

    use super::PlotView;

    fn plot(editor: &mut Editor, job: JobId, fields: &str) {
        editor.session_output(job, &format!("\u{1}plot\u{2}{fields}\u{3}{R_PROMPT}"));
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
        assert_eq!(names, ["*R*", "*R plots*"]);
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

        editor.plots_sized(800, 600);
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$plot(1L, 800L, 600L)\n"]
        );
        editor.plots_sized(900, 600);
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
        editor.plots_sized(900, 600);
        assert_eq!(r_sent(&mut editor), Vec::<String>::new());
    }

    #[test]
    fn n_and_p_step_through_earlier_plots() {
        let (mut editor, job) = started();
        plot(&mut editor, job, "new\t1\t2\t672\t480\tC:/plots/plot-1.png");
        plot(&mut editor, job, "new\t2\t2\t672\t480\tC:/plots/plot-2.png");
        editor.take_effects();
        press(&mut editor, "C-x o C-x o");
        assert_eq!(editor.buffer().name(), "*R plots*");

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
        assert_eq!(editor.buffer().name(), "*R plots*");
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
