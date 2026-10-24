use std::time::{Duration, Instant};

use oxmacs_core::editor::JobId;

use super::{PROMPT, R, ox_text, script};

fn offered(r: &R) -> Option<Vec<String>> {
    r.editor.pane().popup().map(|view| view.items)
}

fn console() -> (R, JobId) {
    let mut r = script("x\n", 0);
    let job = r.running();
    r.press("C-c C-z");
    r.editor.take_effects();
    (r, job)
}

fn answer(r: &mut R, job: JobId, payload: &str) {
    r.output(job, &format!("\u{1}complete\u{2}{payload}\u{3}{PROMPT}"));
}

#[test]
fn tab_in_the_console_asks_r_and_offers_its_answers() {
    let (mut r, job) = console();
    r.typed("pr");
    r.editor.take_effects();

    r.press("TAB");
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$complete(\"pr\", 2L)\n"]);
    answer(&mut r, job, "0\tprint\tprod");

    assert_eq!(
        offered(&r),
        Some(vec!["print".to_owned(), "prod".to_owned()])
    );
    assert_eq!(r.contents(), "> pr");
    r.press("TAB");
    assert_eq!(r.contents(), "> print");
}

#[test]
fn tab_in_a_script_indents_before_it_completes() {
    let mut r = script("f <- function() {\npr\n}\n", 20);
    let job = r.running();

    r.press("TAB");
    assert_eq!(r.contents(), "f <- function() {\n  pr\n}\n");
    assert_eq!(r.sent(), Vec::<String>::new());

    r.press("TAB");
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$complete(\"  pr\", 4L)\n"]);
    answer(&mut r, job, "2\tprint\tprod");
    assert_eq!(
        offered(&r),
        Some(vec!["print".to_owned(), "prod".to_owned()])
    );
}

#[test]
fn a_sole_candidate_is_inserted_at_once() {
    let (mut r, job) = console();
    r.typed("mydata$a");
    r.editor.take_effects();

    r.press("TAB");
    answer(&mut r, job, "0\tmydata$alpha");

    assert_eq!(r.contents(), "> mydata$alpha");
    assert_eq!(offered(&r), None);
}

#[test]
fn nothing_to_offer_is_reported() {
    let (mut r, job) = console();
    r.typed("zz");

    r.press("TAB");
    answer(&mut r, job, "0");

    assert_eq!(r.editor.message(), Some("No completions"));
}

#[test]
fn completion_needs_r_to_be_running() {
    let mut r = script("pr", 2);

    r.press("C-M-i");

    assert_eq!(
        r.editor.message(),
        Some("Start R with C-c C-z to complete from its session")
    );
    assert_eq!(r.started(), Vec::new());
}

#[test]
fn busy_r_is_not_asked() {
    let mut r = script("Sys.sleep(5)\npr", 15);
    r.running();
    r.press("C-p C-RET");
    r.editor.take_effects();
    r.press("C-e");

    r.press("C-M-i");

    assert_eq!(r.editor.message(), Some("R is busy"));
    assert_eq!(r.sent(), Vec::<String>::new());
}

#[test]
fn three_typed_characters_ask_for_completion_quietly_after_a_pause() {
    let (mut r, job) = console();
    r.typed("me");
    rested(&mut r);
    assert_eq!(r.sent(), Vec::<String>::new());

    r.typed("a");
    assert_eq!(r.sent(), Vec::<String>::new());
    rested(&mut r);
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$complete(\"mea\", 3L)\n"]);
    answer(&mut r, job, "0\tmean");

    assert_eq!(offered(&r), Some(vec!["mean".to_owned()]));
    assert_eq!(r.contents(), "> mea");
    assert_eq!(r.editor.message(), None);
}

#[test]
fn automatic_answers_stay_quiet_and_skip_what_is_already_typed() {
    let (mut r, job) = console();
    r.typed("mean");
    rested(&mut r);
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$complete(\"mean\", 4L)\n"]);
    answer(&mut r, job, "0\tmean");
    assert_eq!(offered(&r), None);
    assert_eq!(r.editor.message(), None);

    r.typed("x");
    rested(&mut r);
    assert_eq!(
        r.sent(),
        ["getOption(\"oxmacs\")$complete(\"meanx\", 5L)\n"]
    );
    answer(&mut r, job, "0");
    assert_eq!(r.editor.message(), None);
}

#[test]
fn moves_comments_and_busy_r_do_not_ask() {
    let (mut r, _) = console();
    r.typed("mea");
    r.editor.take_effects();
    r.press("C-b");
    rested(&mut r);
    assert_eq!(r.sent(), Vec::<String>::new());

    let mut r = script("x <- 1 # mea", 12);
    r.running();
    r.typed("n");
    rested(&mut r);
    assert_eq!(r.sent(), Vec::<String>::new());

    let mut r = script("Sys.sleep(5)\nmea", 16);
    r.running();
    r.press("M-< C-RET M->");
    r.editor.take_effects();
    r.typed("n");
    rested(&mut r);
    assert_eq!(r.sent(), Vec::<String>::new());
}

#[test]
fn answers_for_another_buffer_are_dropped() {
    let (mut r, job) = console();
    r.typed("pr");
    r.press("TAB C-c C-z");

    answer(&mut r, job, "0\tprint\tprod");

    assert_eq!(r.editor.buffer().name(), "t");
    assert_eq!(offered(&r), None);
}

fn call(r: &mut R, text: &str) -> String {
    r.eval(&format!(
        "(module (import (r complete))) (first (complete/call-name {}))",
        ox_text(text)
    ))
}

#[test]
fn the_innermost_open_call_is_found() {
    let (mut r, _) = console();

    for (text, name) in [
        ("lm(mpg ~ wt, ", "\"lm\""),
        ("fit <- stats::lm(", "\"stats::lm\""),
        ("plot(density(x), ", "\"plot\""),
        ("plot(density(", "\"density\""),
        ("{\n  summary (", "\"summary\""),
        (".hidden(", "\".hidden\""),
        ("paste(\"(\", ", "\"paste\""),
        ("paste(\"a, ", "nil"),
        ("# see plot(", "nil"),
        ("if (x > ", "nil"),
        ("function(a, ", "nil"),
        ("x[f(1), ", "nil"),
        ("plot(x)", "nil"),
        ("(1 + ", "nil"),
        ("1(x", "nil"),
    ] {
        assert_eq!(call(&mut r, text), name, "{text}");
    }
}

fn rested(r: &mut R) {
    let since = Instant::now();
    r.ox.idle(&mut r.editor, since, since + Duration::from_secs(2));
    crate::testing::settle(&mut r.editor, &mut r.ox);
}

fn inside_plot() -> (R, JobId) {
    let mut r = script("x <- 1:10\nplot(x, \n", 18);
    let job = r.running();
    (r, job)
}

fn signature(r: &mut R, job: JobId, payload: &str) {
    r.output(job, &format!("\u{1}signature\u{2}{payload}\u{3}{PROMPT}"));
}

#[test]
fn the_signature_of_the_call_at_point_is_asked_and_shown() {
    let (mut r, job) = inside_plot();

    rested(&mut r);
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$signature(\"plot\")\n"]);
    rested(&mut r);
    assert_eq!(r.sent(), Vec::<String>::new());

    signature(&mut r, job, "plot\tplot(x, y, ...)");
    assert_eq!(r.editor.message(), Some("plot(x, y, ...)"));
}

#[test]
fn known_signatures_show_without_asking_again() {
    let (mut r, job) = inside_plot();
    rested(&mut r);
    r.sent();
    signature(&mut r, job, "plot\tplot(x, y, ...)");

    r.press("C-a");
    rested(&mut r);
    r.editor.show_message("elsewhere");
    r.press("C-e");
    rested(&mut r);

    assert_eq!(r.editor.message(), Some("plot(x, y, ...)"));
    assert_eq!(r.sent(), Vec::<String>::new());
}

#[test]
fn running_code_forgets_known_signatures() {
    let (mut r, job) = inside_plot();
    rested(&mut r);
    r.sent();
    signature(&mut r, job, "plot\tplot(x, y, ...)");

    r.press("C-c C-z");
    r.typed("plot <- function(z) z");
    r.press("RET");
    r.output(job, PROMPT);
    r.sent();
    r.press("C-c C-z C-a");
    r.press("C-e");
    rested(&mut r);

    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$signature(\"plot\")\n"]);
}

#[test]
fn nothing_is_asked_outside_a_call_or_without_r() {
    let mut r = script("plot(x, ", 8);
    rested(&mut r);
    assert_eq!(r.sent(), Vec::<String>::new());
    assert_eq!(r.started(), Vec::new());

    let (mut r, _) = inside_plot();
    r.press("M-<");
    rested(&mut r);
    assert_eq!(r.sent(), Vec::<String>::new());
}
