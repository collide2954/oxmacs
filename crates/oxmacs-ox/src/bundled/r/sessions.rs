use oxmacs_core::editor::{Editor, JobId, Outcome};

use super::{CONSOLE, CONTINUE, PROMPT, R, arguments, script, started, with_program};

fn running_r() -> (R, JobId) {
    let mut r = started();
    r.run("run-r");
    let job = r.sessions().pop().unwrap().id;
    (r, job)
}

fn profile(job: &oxmacs_core::editor::Job) -> String {
    job.env
        .iter()
        .find(|(name, _)| name == "R_PROFILE_USER")
        .map(|(_, value)| value.clone())
        .unwrap()
}

#[test]
fn run_r_starts_r_with_the_helper_as_its_profile() {
    let mut r = started();

    r.run("run-r");

    let job = r.sessions().pop().unwrap();
    assert_eq!(job.program, r.program().display().to_string());
    assert_eq!(job.args, arguments());
    assert!(
        std::fs::read_to_string(profile(&job))
            .unwrap()
            .contains("oxmacs$help <- function(topic)")
    );
    assert_eq!(
        r.eval(&format!(
            "(module (import (r helper) (ox file))) (= helper/script (await (file/read {:?})))",
            profile(&job)
        )),
        "true"
    );
    assert!(
        job.env
            .iter()
            .any(|(name, value)| name == "OXMACS_R_PLOTS" && value.contains("oxmacs"))
    );
    assert_eq!(r.editor.buffer().name(), CONSOLE);
    assert_eq!(r.editor.tiling().windows().len(), 2);
}

#[test]
fn each_session_gets_its_own_helper() {
    let mut r = started();

    assert_eq!(
        r.eval(
            "(module (import (r helper) (ox file))) (let [first (await (helper/write)) second (await (helper/write))] [(not= first second) (= (await (file/read first)) helper/script) (= (await (file/read second)) helper/script)])"
        ),
        "[true true true]"
    );
}

#[test]
fn the_helper_is_removed_once_r_is_ready() {
    let (mut r, job) = running_r();
    let path = r
        .eval("(module (import (r session))) (session/helper-of \"*R:analysis*\")")
        .trim_matches('"')
        .replace("\\\\", "\\");
    assert!(std::path::Path::new(&path).exists(), "{path}");

    r.output(job, PROMPT);

    assert!(!std::path::Path::new(&path).exists());
}

fn r_home_in(root: &std::path::Path, name: &str) -> std::path::PathBuf {
    let home = root.join(name);
    let bin = if cfg!(windows) {
        home.join("bin").join("x64")
    } else {
        home.join("bin")
    };
    std::fs::create_dir_all(&bin).unwrap();
    let program = if cfg!(windows) { "Rterm.exe" } else { "R" };
    std::fs::write(bin.join(program), "").unwrap();
    home
}

fn binary(home: &std::path::Path) -> String {
    let path = if cfg!(windows) {
        home.join("bin").join("x64").join("Rterm.exe")
    } else {
        home.join("bin").join("R")
    };
    format!("{:?}", path.display().to_string())
}

fn located(r: &mut R, configured: &str, which: &str, home: &str, installs: &str) -> String {
    r.eval(&format!(
        "(module (import (r session))) (session/locate {configured} {{which: {which} home: (fn [] {home}) installs: (fn [] {installs})}})"
    ))
}

#[test]
fn r_comes_from_the_setting_then_the_path() {
    let mut r = started();
    let dir = tempfile::tempdir().unwrap();
    let bin = binary(&r_home_in(dir.path(), "path"));
    let configured = dir.path().join("my-r");
    std::fs::write(&configured, "").unwrap();
    let configured = format!("{:?}", configured.display().to_string());
    let program = if cfg!(windows) { "Rterm" } else { "R" };
    let which = format!("(fn [name] (when (= name {program:?}) {bin}))");

    assert_eq!(
        located(&mut r, &configured, &which, "nil", "[]"),
        configured
    );
    assert_eq!(located(&mut r, "nil", &which, "nil", "[]"), bin);
    assert_eq!(
        located(&mut r, "\"oxmacs-no-such-r\"", "(fn [_] nil)", "nil", "[]"),
        "nil"
    );
}

#[test]
fn installs_are_only_listed_when_r_is_not_on_the_path() {
    let mut r = started();
    let dir = tempfile::tempdir().unwrap();
    let bin = binary(&r_home_in(dir.path(), "path"));

    let found = r.eval(&format!(
        "(module (import (r session))) (def listed (atom false)) [(session/locate nil {{which: (fn [_] {bin}) home: (fn [] nil) installs: (fn [] (reset! listed true) [])}}) @listed]"
    ));

    assert_eq!(found, format!("[{bin} false]"));
}

#[test]
fn r_home_and_the_newest_install_are_fallbacks() {
    let mut r = started();
    let dir = tempfile::tempdir().unwrap();
    let home = r_home_in(dir.path(), "home");
    let old = r_home_in(dir.path(), "R-4.9.2");
    let new = r_home_in(dir.path(), "R-4.10.0");
    let quoted = |path: &std::path::Path| format!("{:?}", path.display().to_string());

    assert_eq!(
        located(&mut r, "nil", "(fn [_] nil)", &quoted(&home), "[]"),
        binary(&home)
    );
    assert_eq!(
        located(
            &mut r,
            "nil",
            "(fn [_] nil)",
            "nil",
            &format!("[{} {}]", quoted(&old), quoted(&new))
        ),
        binary(&new)
    );
    assert_eq!(located(&mut r, "nil", "(fn [_] nil)", "nil", "[]"), "nil");
}

#[test]
fn missing_r_is_reported() {
    let mut r = with_program(Some("oxmacs-no-such-r"), Editor::new());

    r.run("run-r");

    assert_eq!(
        r.editor.message(),
        Some(
            "R not found; install it from https://cran.r-project.org or set the r plugin's program"
        )
    );
    assert_eq!(r.sessions(), []);
}

#[test]
fn typed_input_is_sent_and_answered_after_it() {
    let (mut r, job) = running_r();
    r.output(job, PROMPT);
    assert_eq!(r.contents(), "> ");

    r.typed("1 + 1");
    r.press("RET");
    assert_eq!(r.sent(), ["1 + 1\n"]);

    r.output(job, &format!("[1] 2\n{PROMPT}"));
    assert_eq!(r.contents(), "> 1 + 1\n[1] 2\n> ");
    assert_eq!(r.editor.buffer().point(), r.editor.buffer().len_chars());
}

#[test]
fn history_recalls_earlier_input() {
    let (mut r, job) = running_r();
    r.output(job, PROMPT);
    for input in ["a", "b"] {
        r.typed(input);
        r.press("RET");
        r.output(job, PROMPT);
    }

    r.press("M-p");
    assert_eq!(r.lines().last().unwrap(), "> b");
    r.press("M-p");
    assert_eq!(r.lines().last().unwrap(), "> a");
    r.press("M-n");
    assert_eq!(r.lines().last().unwrap(), "> b");
}

#[test]
fn return_on_old_input_copies_it_to_the_prompt() {
    let (mut r, job) = running_r();
    r.output(job, PROMPT);
    r.typed("x");
    r.press("RET");
    r.output(job, &format!("[1] 1\n{PROMPT}"));
    r.editor.take_effects();

    r.press("M-< RET");

    assert_eq!(r.contents(), "> x\n[1] 1\n> x");
    assert_eq!(r.sent(), Vec::<String>::new());
}

#[test]
fn prompts_for_lines_already_shown_are_hidden() {
    let (mut r, job) = running_r();
    r.output(job, PROMPT);
    r.typed("f <- function() {");
    r.press("C-j");
    r.typed("}");
    r.press("RET");

    r.output(job, &format!("{CONTINUE}{PROMPT}"));

    assert_eq!(r.contents(), "> f <- function() {\n}\n> ");
}

#[test]
fn code_from_a_script_is_echoed_line_by_line_as_r_reads_it() {
    let (mut r, job) = running_r();
    r.output(job, PROMPT);

    r.eval(r#"(module (import (r session))) (session/evaluate "*R:analysis*" "x <- 1\nf <- function() {{\n1\n}}")"#);

    assert_eq!(r.sent(), ["x <- 1\nf <- function() {\n1\n}\n"]);
    assert_eq!(r.contents(), "> x <- 1\n");
    r.output(job, &format!("{PROMPT}{CONTINUE}{CONTINUE}{PROMPT}"));
    assert_eq!(r.contents(), "> x <- 1\n> f <- function() {\n+ 1\n+ }\n> ");
}

#[test]
fn code_sent_while_r_is_busy_waits_for_its_prompt() {
    let (mut r, job) = running_r();

    r.eval(r#"(module (import (r session))) (session/evaluate "*R:analysis*" "1 + 1")"#);
    r.output(job, "R version 4.6.1\n");
    r.output(job, PROMPT);
    r.output(job, &format!("[1] 2\n{PROMPT}"));

    assert_eq!(r.contents(), "R version 4.6.1\n> 1 + 1\n[1] 2\n> ");
}

#[test]
fn r_learns_the_console_width_when_ready_and_when_it_changes() {
    let (mut r, job) = running_r();
    let window = r.editor.selected_window();
    r.editor.resize_pane(window, 20, 61);
    r.press("C-g");
    r.editor.take_effects();

    r.output(job, PROMPT);
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$width(60L)\n"]);
    r.output(job, PROMPT);
    assert_eq!(r.contents(), "> ");

    r.editor.resize_pane(window, 20, 71);
    r.press("C-g");
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$width(70L)\n"]);
    r.editor.resize_pane(window, 30, 71);
    r.press("C-g");
    assert_eq!(r.sent(), Vec::<String>::new());
}

fn new_session(r: &mut R) -> JobId {
    r.run("r-new-session");
    r.started()[0]
}

#[test]
fn sessions_are_named_after_their_folder_and_numbered_when_they_clash() {
    let mut r = script("x\n", 0);
    r.press("C-c C-z");
    assert_eq!(r.editor.buffer().name(), CONSOLE);
    r.press("C-c C-z");

    new_session(&mut r);

    assert_eq!(r.editor.buffer().name(), "*R:analysis:2*");
}

#[test]
fn each_script_sends_to_the_session_it_last_used() {
    let mut r = script("a <- 1\n", 0);
    r.press("C-RET");
    let first = r.started()[0];
    r.visit("b");
    r.run("r-mode");
    r.typed("b");
    r.press("C-RET");
    assert_eq!(r.sends(), [(first, "b\n".to_owned())]);

    let second = new_session(&mut r);
    r.visit("b");
    r.press("M-< C-RET");
    assert_eq!(r.sends(), [(second, "b\n".to_owned())]);

    r.visit("t");
    r.press("M-< C-RET");
    assert_eq!(r.sends(), [(first, "a <- 1\n".to_owned())]);
}

#[test]
fn output_goes_to_the_session_that_made_it() {
    let mut r = script("x\n", 0);
    r.press("C-c C-z");
    let first = r.started()[0];
    r.press("C-c C-z");
    let second = new_session(&mut r);

    r.output(first, PROMPT);
    r.output(second, "two\n");
    r.output(second, PROMPT);

    assert_eq!(r.contents(), "two\n> ");
    r.visit(CONSOLE);
    assert_eq!(r.contents(), "> ");
}

#[test]
fn the_end_of_r_is_reported() {
    let (mut r, job) = running_r();

    r.editor.job_finished(job, &Outcome::Exited(Some(0)));

    assert!(r.contents().ends_with("Process R finished\n"));
    assert_eq!(r.editor.message(), Some("Process R finished"));
}

#[test]
fn a_finished_session_starts_again_in_its_own_buffer() {
    let (mut r, job) = running_r();
    r.editor.job_finished(job, &Outcome::Exited(Some(0)));
    r.press("C-x o");

    r.run("run-r");

    assert_eq!(r.started().len(), 1);
    assert_eq!(
        r.editor
            .buffer_names()
            .iter()
            .filter(|name| name.starts_with("*R:"))
            .count(),
        1
    );
}

#[test]
fn input_history_carries_over_to_later_sessions() {
    let (mut r, job) = running_r();
    r.output(job, PROMPT);
    r.typed("x <- 1");
    r.press("RET");
    r.output(job, PROMPT);
    let saved = std::fs::read_to_string(r.home.path().join("r-history")).unwrap();
    assert_eq!(saved, "\"x <- 1\"\n");

    let mut later = r.again();
    later.run("run-r");
    let job = later.started()[0];
    later.output(job, PROMPT);
    later.press("M-p");

    assert_eq!(later.contents(), "> x <- 1");
}
