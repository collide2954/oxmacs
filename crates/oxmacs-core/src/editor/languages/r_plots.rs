use std::path::PathBuf;

use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::text::Buffer;

const NAME: &str = "*R plots*";
const SIZE: (u32, u32) = (672, 480);

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

struct Arrival {
    asked: bool,
    index: usize,
    count: usize,
    size: (u32, u32),
    path: PathBuf,
}

fn arrival(payload: &str) -> Option<Arrival> {
    let mut fields = payload.splitn(6, '\t');
    let why = fields.next()?;
    let index = fields.next()?.parse().ok()?;
    let count = fields.next()?.parse().ok()?;
    let width = fields.next()?.parse().ok()?;
    let height = fields.next()?.parse().ok()?;
    let path = PathBuf::from(fields.next()?);
    Some(Arrival {
        asked: why == "asked",
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
        let Some(arrival) = arrival(payload) else {
            return;
        };
        self.show_plots();
        let Some(plots) = self.plots_mut() else {
            return;
        };
        if arrival.asked {
            plots.asked = None;
        } else {
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
