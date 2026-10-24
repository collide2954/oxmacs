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
