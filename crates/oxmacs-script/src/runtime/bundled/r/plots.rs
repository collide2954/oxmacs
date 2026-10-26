use oxmacs_core::editor::{Axis, Effect, JobId, Tiling, WindowId};

use super::{CONSOLE, PLOTS, PROMPT, R, script};
use oxmacs_core::view::{Node, ViewBuffer};

fn plot(r: &mut R, job: JobId, fields: &str) {
    r.output(job, &format!("\u{1}plot\u{2}{fields}\u{3}{PROMPT}"));
}

fn view(r: &mut R, name: &str) -> String {
    r.eval(&format!(
        "(module (import (r plots))) (plots/view {name:?})"
    ))
}

fn shown(index: usize, count: usize, path: &str) -> String {
    format!("[{index} {count} {path:?}]")
}

fn gallery(r: &mut R, name: &str) -> String {
    r.eval(&format!(
        "(module (import (r plots))) (plots/gallery {name:?})"
    ))
}

fn texts(node: &Node<usize>, out: &mut Vec<String>) {
    match node {
        Node::Column { children, .. } | Node::Row { children, .. } => {
            for child in children {
                texts(child, out);
            }
        }
        Node::Text { text, .. } => out.push(text.clone()),
        Node::Scrollable(child) => texts(child, out),
        _ => {}
    }
}

fn written(r: &R, name: &str) -> Vec<String> {
    let id = r.editor.buffer_named(name).unwrap();
    let mut out = Vec::new();
    texts(
        &r.editor.plugin_buffer::<ViewBuffer>(id).unwrap().1.node,
        &mut out,
    );
    out
}

fn sized(r: &mut R, name: &str, width: u32, height: u32) {
    let id = r.editor.buffer_named(name).unwrap();
    r.ox.view_sized(&mut r.editor, id, width, height);
    crate::testing::settle(&mut r.editor, &mut r.ox);
}

fn started() -> (R, JobId) {
    let mut r = script("plot(1:10)\n", 0);
    let job = r.running();
    (r, job)
}

fn names(r: &R, part: &Tiling) -> Vec<String> {
    match part {
        Tiling::Window(window) => vec![name_in(r, *window)],
        Tiling::Split { parts, .. } => parts.iter().flat_map(|(_, part)| names(r, part)).collect(),
    }
}

fn name_in(r: &R, window: WindowId) -> String {
    r.editor
        .pane_of(window)
        .map(|pane| pane.buffer().name().to_owned())
        .unwrap_or_default()
}

#[test]
fn a_new_plot_opens_below_the_console() {
    let (mut r, job) = started();

    plot(&mut r, job, "new\t1\t1\t1\t672\t480\tC:/plots/plot-1.png");

    assert_eq!(view(&mut r, PLOTS), shown(1, 1, "C:/plots/plot-1.png"));
    assert_eq!(r.editor.buffer().name(), "t");
    let Tiling::Split { parts, .. } = r.editor.tiling() else {
        panic!("not split");
    };
    let Tiling::Split { axis, .. } = &parts[1].1 else {
        panic!("the console window was not split");
    };
    assert_eq!(*axis, Axis::Rows);
    assert_eq!(names(&r, &parts[1].1), [CONSOLE, PLOTS]);
}

#[test]
fn later_plots_reuse_the_plots_window() {
    let (mut r, job) = started();
    plot(&mut r, job, "new\t1\t1\t1\t672\t480\tC:/plots/plot-1.png");

    plot(&mut r, job, "new\t2\t2\t2\t672\t480\tC:/plots/plot-2.png");

    assert_eq!(view(&mut r, PLOTS), shown(2, 2, "C:/plots/plot-2.png"));
    assert_eq!(r.editor.tiling().windows().len(), 3);
}

#[test]
fn the_plots_window_has_r_redraw_at_its_size_one_request_at_a_time() {
    let (mut r, job) = started();
    plot(&mut r, job, "new\t1\t1\t1\t672\t480\tC:/plots/plot-1.png");
    r.editor.take_effects();

    sized(&mut r, PLOTS, 800, 600);
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$plot(1L, 800L, 600L)\n"]);
    sized(&mut r, PLOTS, 900, 600);
    assert_eq!(r.sent(), Vec::<String>::new());

    plot(&mut r, job, "asked\t1\t1\t1\t800\t600\tC:/plots/plot-2.png");
    assert_eq!(view(&mut r, PLOTS), shown(1, 1, "C:/plots/plot-2.png"));
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$plot(1L, 900L, 600L)\n"]);
    plot(&mut r, job, "asked\t1\t1\t1\t900\t600\tC:/plots/plot-3.png");
    sized(&mut r, PLOTS, 900, 600);
    assert_eq!(r.sent(), Vec::<String>::new());
}

fn in_plots(r: &mut R, job: JobId, count: usize) {
    for index in 1..=count {
        plot(
            r,
            job,
            &format!("new\t{index}\t{count}\t{index}\t672\t480\tC:/plots/plot-{index}.png"),
        );
    }
    r.editor.take_effects();
    r.press("C-x o C-x o");
    assert_eq!(r.editor.buffer().name(), PLOTS);
}

#[test]
fn n_and_p_step_through_earlier_plots() {
    let (mut r, job) = started();
    in_plots(&mut r, job, 2);

    r.press("p");
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$plot(1L, 672L, 480L)\n"]);
    plot(&mut r, job, "asked\t1\t2\t1\t672\t480\tC:/plots/plot-3.png");
    assert_eq!(view(&mut r, PLOTS), shown(1, 2, "C:/plots/plot-3.png"));

    r.press("p");
    assert_eq!(r.editor.message(), Some("This is the first plot"));
    r.press("n");
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$plot(2L, 672L, 480L)\n"]);
}

#[test]
fn s_saves_the_shown_plot_under_a_chosen_name() {
    let (mut r, job) = started();
    in_plots(&mut r, job, 1);

    r.press("s");
    assert_eq!(r.editor.minibuffer().unwrap().prompt(), "Save plot as: ");
    let input = r.editor.minibuffer().unwrap().input().contents();
    assert!(input.ends_with("plot-1.png"), "{input}");
    r.press("RET");

    let sent = r.sent();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].starts_with("getOption(\"oxmacs\")$plot_save(1L, \""));
    assert!(
        sent[0].ends_with("plot-1.png\", 672L, 480L)\n"),
        "{}",
        sent[0]
    );
}

#[test]
fn d_deletes_the_shown_plot_and_shows_its_neighbor() {
    let (mut r, job) = started();
    in_plots(&mut r, job, 2);

    r.press("d");
    assert_eq!(
        r.sent(),
        ["getOption(\"oxmacs\")$plot_delete(2L, 672L, 480L)\n"]
    );
    plot(
        &mut r,
        job,
        "deleted\t1\t1\t1\t672\t480\tC:/plots/plot-3.png",
    );
    assert_eq!(view(&mut r, PLOTS), shown(1, 1, "C:/plots/plot-3.png"));

    r.press("d");
    plot(&mut r, job, "none");
    assert_eq!(view(&mut r, PLOTS), "nil");
    assert_eq!(written(&r, PLOTS), ["No plots"]);
    r.press("d");
    assert_eq!(r.editor.message(), Some("No plot to delete"));
}

#[test]
fn z_opens_a_large_copy_in_the_system_viewer() {
    let (mut r, job) = started();
    in_plots(&mut r, job, 1);

    r.press("z");
    assert_eq!(
        r.sent(),
        ["getOption(\"oxmacs\")$plot_zoom(1L, 1344L, 960L)\n"]
    );
    r.output(
        job,
        &format!("\u{1}zoom\u{2}C:/plots/zoom-1.png\u{3}{PROMPT}"),
    );

    assert_eq!(
        r.editor.take_effects(),
        [Effect::OpenExternally("C:/plots/zoom-1.png".into())]
    );
}

fn thumbnails(r: &mut R, job: JobId, count: usize) {
    let paths: Vec<String> = (1..=count)
        .map(|index| format!("C:/plots/thumb-{index}.png"))
        .collect();
    r.output(
        job,
        &format!("\u{1}gallery\u{2}{}\u{3}{PROMPT}", paths.join("\t")),
    );
}

#[test]
fn v_asks_r_for_a_grid_of_thumbnails() {
    let (mut r, job) = started();
    in_plots(&mut r, job, 4);

    r.press("v");
    assert_eq!(
        r.sent(),
        ["getOption(\"oxmacs\")$plot_gallery(336L, 240L)\n"]
    );
    thumbnails(&mut r, job, 4);

    assert_eq!(
        gallery(&mut r, PLOTS),
        r#"[3 2 ["C:/plots/thumb-1.png" "C:/plots/thumb-2.png" "C:/plots/thumb-3.png" "C:/plots/thumb-4.png"]]"#
    );
}

fn selected(r: &mut R) -> String {
    r.eval(&format!(
        "(module (import (r plots))) (first (plots/gallery {PLOTS:?}))"
    ))
}

#[test]
fn keys_move_through_the_gallery_and_return_opens_a_plot() {
    let (mut r, job) = started();
    in_plots(&mut r, job, 4);
    r.press("v");
    thumbnails(&mut r, job, 4);
    r.editor.take_effects();

    r.press("<up>");
    assert_eq!(selected(&mut r), "1");
    r.press("p");
    assert_eq!(selected(&mut r), "0");
    r.press("<down> <right>");
    assert_eq!(selected(&mut r), "3");
    r.press("<left>");
    assert_eq!(r.sent(), Vec::<String>::new());

    r.press("RET");
    assert_eq!(gallery(&mut r, PLOTS), "nil");
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$plot(3L, 672L, 480L)\n"]);
    assert_eq!(r.logged(), "");
}

#[test]
fn a_click_opens_a_thumbnail_and_v_leaves_the_gallery() {
    let (mut r, job) = started();
    in_plots(&mut r, job, 2);
    r.press("v");
    thumbnails(&mut r, job, 2);
    r.editor.take_effects();

    r.eval("(module (import (r plots))) (plots/pick \"*R:analysis plots*\" 0)");
    crate::testing::settle(&mut r.editor, &mut r.ox);
    assert_eq!(gallery(&mut r, PLOTS), "nil");
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$plot(1L, 672L, 480L)\n"]);

    r.press("v");
    thumbnails(&mut r, job, 2);
    r.press("v");
    assert_eq!(gallery(&mut r, PLOTS), "nil");
}

#[test]
fn a_new_plot_keeps_the_console_prompts_in_step() {
    let mut r = script("plot(1:3)\nabline(h = 2)\n", 0);
    r.press("C-c C-z");
    let window = r.editor.selected_window();
    r.editor.resize_pane(window, 20, 61);
    let job = r.started()[0];
    r.output(job, PROMPT);
    let width = r.sent();
    r.output(job, PROMPT);
    r.press("C-c C-z");
    assert_eq!(width, ["getOption(\"oxmacs\")$width(60L)\n"]);

    r.press("C-RET");
    r.editor.take_effects();
    plot(&mut r, job, "new\t1\t1\t1\t672\t480\tC:/plots/plot-1.png");
    assert_eq!(r.sent(), Vec::<String>::new());
    r.press("C-RET");
    r.output(job, PROMPT);

    r.visit(CONSOLE);
    assert_eq!(r.contents(), "> plot(1:3)\n> abline(h = 2)\n> ");
}

fn window_named(r: &R, name: &str) -> bool {
    r.editor
        .tiling()
        .windows()
        .into_iter()
        .any(|window| name_in(r, window) == name)
}

#[test]
fn k_pins_the_plot_and_new_plots_open_beside_it() {
    let (mut r, job) = started();
    in_plots(&mut r, job, 1);

    r.press("k");
    assert_eq!(r.editor.buffer().name(), "*R:analysis plot 1*");
    assert_eq!(
        r.editor.message(),
        Some("Pinned; new plots will open beside it")
    );
    plot(&mut r, job, "new\t2\t2\t2\t672\t480\tC:/plots/plot-2.png");

    assert!(window_named(&r, "*R:analysis plot 1*"));
    assert!(window_named(&r, PLOTS));
    assert_eq!(
        view(&mut r, "*R:analysis plot 1*"),
        shown(1, 1, "C:/plots/plot-1.png")
    );
    assert_eq!(view(&mut r, PLOTS), shown(2, 2, "C:/plots/plot-2.png"));
    assert_eq!(r.editor.tiling().windows().len(), 4);
}

#[test]
fn a_pinned_plot_redraws_by_id_and_cannot_be_browsed() {
    let (mut r, job) = started();
    in_plots(&mut r, job, 2);
    r.press("k");

    r.press("p");
    assert_eq!(
        r.editor.message(),
        Some("This plot is pinned; s saves it, z zooms and q closes it")
    );
    sized(&mut r, "*R:analysis plot 2*", 500, 400);
    assert_eq!(
        r.sent(),
        ["getOption(\"oxmacs\")$plot_pinned(2L, 500L, 400L)\n"]
    );
    plot(
        &mut r,
        job,
        "pinned\t2\t2\t2\t500\t400\tC:/plots/plot-9.png",
    );
    assert_eq!(
        view(&mut r, "*R:analysis plot 2*"),
        shown(2, 2, "C:/plots/plot-9.png")
    );

    r.output(job, &format!("\u{1}plot\u{2}gone\t2\u{3}{PROMPT}"));
    assert_eq!(view(&mut r, "*R:analysis plot 2*"), "nil");
    assert_eq!(
        written(&r, "*R:analysis plot 2*"),
        ["This plot is no longer in R's history"]
    );
}

#[test]
fn plots_go_to_the_session_that_made_them() {
    let mut r = script("x\n", 0);
    r.press("C-c C-z");
    r.started();
    r.press("C-c C-z");
    r.run("r-new-session");
    let second = r.started()[0];

    plot(&mut r, second, "new\t1\t1\t1\t672\t480\tC:/p.png");

    assert!(r.editor.buffer_names().contains(&"*R:analysis:2 plots*"));
    assert!(!r.editor.buffer_names().contains(&PLOTS));
}

#[test]
fn plots_show_their_image_once_it_is_read() {
    let (mut r, job) = started();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("plot-1.png");
    r.eval(&format!(
        "(module (import (ox image))) (await (image/save (image/new 8 6) {:?}))",
        path.display().to_string()
    ));

    plot(
        &mut r,
        job,
        &format!("new\t1\t1\t1\t4\t3\t{}", path.display()),
    );
    crate::testing::settle(&mut r.editor, &mut r.ox);

    let id = r.editor.buffer_named(PLOTS).unwrap();
    let node = &r.editor.plugin_buffer::<ViewBuffer>(id).unwrap().1.node;
    let mut found = Vec::new();
    images(node, &mut found);
    assert_eq!(found, [(8, 6, Some(4.0), Some(3.0))]);
    assert_eq!(written(&r, PLOTS), ["1 / 1"]);
    assert_eq!(r.logged(), "");
}

fn images(node: &Node<usize>, out: &mut Vec<(u32, u32, Option<f32>, Option<f32>)>) {
    match node {
        Node::Column { children, .. } | Node::Row { children, .. } => {
            for child in children {
                images(child, out);
            }
        }
        Node::Image {
            picture,
            width,
            height,
            ..
        } => out.push((picture.size().0, picture.size().1, *width, *height)),
        Node::Scrollable(child) => images(child, out),
        _ => {}
    }
}
