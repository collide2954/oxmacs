use oxmacs_core::theme::Highlight;

use super::{CONSOLE, PROMPT, script};

const HELP: &str = "\u{1}help\u{2}mean   package:base\n\n_\u{8}U_\u{8}s_\u{8}a_\u{8}g_\u{8}e:\n\n     mean(x, ...)\n\u{3}";

#[test]
fn control_c_control_v_asks_with_the_symbol_at_point_as_default() {
    let mut r = script("mean(x)\n", 1);
    r.running();

    r.press("C-c C-v");
    assert_eq!(
        r.editor.minibuffer().unwrap().prompt(),
        "R help on (default mean): "
    );
    r.press("RET");

    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$help(\"mean\")\n"]);
}

#[test]
fn a_typed_topic_wins_over_the_default() {
    let mut r = script("mean(x)\n", 1);
    r.running();

    r.press("C-c C-v");
    r.typed("lm");
    r.press("RET");

    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$help(\"lm\")\n"]);
}

#[test]
fn without_a_symbol_the_prompt_has_no_default() {
    let mut r = script("  \n", 0);
    r.running();

    r.press("C-c C-v");
    assert_eq!(r.editor.minibuffer().unwrap().prompt(), "R help on: ");
    r.press("RET");

    assert_eq!(r.sent(), Vec::<String>::new());
}

#[test]
fn topics_are_quoted_for_r() {
    let mut r = script("x\n", 0);
    r.running();

    r.press("C-c C-v");
    r.typed("a\"b\\c");
    r.press("RET");

    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$help(\"a\\\"b\\\\c\")\n"]);
}

#[test]
fn help_from_r_opens_beside_with_its_headings() {
    let mut r = script("mean(x)\n", 1);
    let job = r.running();

    r.output(job, &format!("{HELP}{PROMPT}"));

    assert_eq!(r.logged(), "");
    assert_eq!(r.editor.buffer().name(), "*R help*");
    assert!(r.editor.buffer().is_read_only());
    assert_eq!(
        r.lines(),
        ["mean   package:base", "", "Usage:", "", "     mean(x, ...)"]
    );
    assert_eq!(r.editor.line_style(2).spans, [(0..5, Highlight::Heading)]);
    assert_eq!(r.editor.tiling().windows().len(), 2);
    assert_eq!(r.editor.buffer().point(), 0);
}

#[test]
fn help_lookups_leave_the_console_as_it_was() {
    let mut r = script("mean(x)\n", 1);
    let job = r.running();
    r.press("C-c C-v RET");

    r.output(job, &format!("{HELP}{PROMPT}"));
    r.visit(CONSOLE);

    assert_eq!(r.editor.buffer().name(), CONSOLE);
    assert_eq!(r.contents(), "> ");
}

#[test]
fn messages_from_r_show_in_the_echo_area() {
    let mut r = script("x\n", 0);
    let job = r.running();

    r.output(job, "\u{1}message\u{2}No R help for x\u{3}");

    assert_eq!(r.editor.message(), Some("No R help for x"));
}

fn heading(title: &str) -> String {
    title.chars().fold(String::new(), |mut struck, character| {
        struck.push_str("_\u{8}");
        struck.push(character);
        struck
    })
}

fn page(topic: &str, see_also: &str, examples: &str) -> String {
    format!(
        "\u{1}help\u{2}{topic}   package:base\n\n{}\n\n     Something about {topic}.\n\n{}\n\n     {see_also}\n\n{}\n\n{examples}\n\u{3}{PROMPT}",
        heading("Description:"),
        heading("See Also:"),
        heading("Examples:"),
    )
}

fn shown_mean(r: &mut super::R, job: oxmacs_core::editor::JobId) {
    r.press("C-c C-v RET");
    r.output(
        job,
        &page(
            "mean",
            "'weighted.mean', 'colMeans' for row means.",
            "     x <- c(0:10, 50)\n     mean(x)\n\n     mean(1:3)",
        ),
    );
    r.editor.take_effects();
}

#[test]
fn see_also_names_are_links_that_return_follows() {
    let mut r = script("mean(x)\n", 1);
    let job = r.running();
    shown_mean(&mut r, job);
    let see = r
        .lines()
        .iter()
        .position(|line| line.contains("'weighted.mean'"))
        .unwrap();
    assert!(
        r.editor
            .line_style(see)
            .spans
            .contains(&(6..19, Highlight::Link)),
        "{:?}",
        r.editor.line_style(see)
    );

    r.editor.go_to_line(see + 1, 8);
    r.press("RET");

    assert_eq!(
        r.sent(),
        ["getOption(\"oxmacs\")$help(\"weighted.mean\")\n"]
    );
}

#[test]
fn tab_moves_to_the_next_link_and_return_on_a_plain_line_says_so() {
    let mut r = script("mean(x)\n", 1);
    let job = r.running();
    shown_mean(&mut r, job);

    r.press("M-< RET");
    assert_eq!(r.editor.message(), Some("No link here"));
    r.press("TAB TAB RET");

    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$help(\"colMeans\")\n"]);
}

#[test]
fn l_and_r_go_back_and_forward_through_the_pages_seen() {
    let mut r = script("mean(x)\n", 1);
    let job = r.running();
    shown_mean(&mut r, job);
    r.press("TAB RET");
    r.output(
        job,
        &page("weighted.mean", "'mean'", "     weighted.mean(1:3, 3:1)"),
    );
    r.editor.take_effects();
    assert!(r.lines()[0].starts_with("weighted.mean"));

    r.press("l");
    assert!(r.lines()[0].starts_with("mean"));
    r.press("l");
    assert_eq!(r.editor.message(), Some("No earlier help page"));
    r.press("r");
    assert!(r.lines()[0].starts_with("weighted.mean"));
    r.press("r");
    assert_eq!(r.editor.message(), Some("No later help page"));
    assert_eq!(r.sent(), Vec::<String>::new());
}

#[test]
fn x_runs_the_example_at_point_and_e_runs_them_all() {
    let mut r = script("mean(x)\n", 1);
    let job = r.running();
    shown_mean(&mut r, job);
    let first = r
        .lines()
        .iter()
        .position(|line| line.contains("x <- c(0:10, 50)"))
        .unwrap();

    r.editor.go_to_line(first + 1, 0);
    r.press("x");
    assert_eq!(r.sent(), ["x <- c(0:10, 50)\nmean(x)\n"]);

    r.press("e");
    assert_eq!(r.sent(), ["x <- c(0:10, 50)\nmean(x)\n\nmean(1:3)\n"]);
    r.press("M-< x");
    assert_eq!(r.editor.message(), Some("No example here"));
}
