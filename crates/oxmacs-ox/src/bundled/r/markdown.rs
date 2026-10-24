use std::path::PathBuf;

use oxmacs_core::editor::{Editor, Effect};
use oxmacs_core::syntax::Language;
use oxmacs_core::theme::Highlight;

use super::{PROMPT, R, with_program};

const REPORT: &str = "# Report\n\nSome text.\n\n```{r setup}\nlibrary(stats)\nx <- 1:10\n```\n\n## Plots\n\n```{r}\nplot(x)\n```\n\nThe end.\n";

fn opened(name: &str, text: &str) -> (tempfile::TempDir, R) {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join(name);
    std::fs::write(&path, text).unwrap();
    let mut r = with_program(None, Editor::new());
    r.editor.open_file(&path);
    crate::testing::settle(&mut r.editor, &mut r.ox);
    (folder, r)
}

fn report() -> (tempfile::TempDir, R) {
    opened("report.Rmd", REPORT)
}

fn line(r: &R) -> usize {
    r.editor.buffer().line_of(r.editor.buffer().point())
}

fn render_call(file: &std::path::Path) -> String {
    format!(
        "getOption(\"oxmacs\")$render({:?})\n",
        file.display().to_string()
    )
}

#[test]
fn r_markdown_files_open_in_their_mode_with_headings_and_chunks_marked() {
    let (_folder, r) = opened(
        "report.Rmd",
        "---\ntitle: \"# Not a heading\"\n---\n# Report\n```{r}\n# a comment\n#| echo: false\nx\n```\n",
    );

    assert_eq!(r.editor.language().map(Language::name), Some("R Markdown"));
    assert_eq!(r.editor.line_style(1).spans, [(0..24, Highlight::String)]);
    assert_eq!(r.editor.line_style(3).spans, [(0..8, Highlight::Heading)]);
    assert_eq!(r.editor.line_style(4).spans, [(0..6, Highlight::Keyword)]);
    assert_eq!(r.editor.line_style(5).spans, [(0..11, Highlight::Comment)]);
    assert_eq!(
        r.editor.line_style(6).spans,
        [(0..14, Highlight::Attribute)]
    );
    assert_eq!(r.editor.line_style(7).spans, []);
    assert_eq!(r.editor.line_style(8).spans, [(0..3, Highlight::Keyword)]);
}

#[test]
fn quarto_files_open_in_the_same_mode() {
    let (_folder, r) = opened("slides.qmd", REPORT);

    assert_eq!(r.editor.language().map(Language::name), Some("R Markdown"));
}

#[test]
fn control_c_control_c_runs_the_chunk_at_point() {
    let (_folder, mut r) = report();
    r.editor.go_to_line(6, 1);

    r.press("C-c C-c");

    let effects = r.editor.take_effects();
    assert!(matches!(effects.first(), Some(Effect::StartSession(_))));
    assert!(
        effects.iter().any(
            |effect| matches!(effect, Effect::Send(_, code) if code == "library(stats)\nx <- 1:10\n")
        ),
        "{effects:?}"
    );
    r.editor.go_to_line(1, 1);
    r.press("C-c C-c");
    assert_eq!(r.editor.message(), Some("Not in an R chunk"));
}

#[test]
fn chunk_options_and_other_languages_are_not_sent() {
    let (_folder, mut r) = opened(
        "options.qmd",
        "```{r}\n#| label: load\nx <- 1\n```\n\n```{python}\nprint(1)\n```\n",
    );
    r.running();
    r.editor.go_to_line(3, 1);

    r.press("C-S-RET");
    assert_eq!(r.sent(), ["x <- 1\n"]);
    r.editor.go_to_line(7, 1);
    r.press("C-c C-c");
    assert_eq!(
        r.editor.message(),
        Some("This is a python chunk, so R does not run it")
    );
    assert_eq!(r.sent(), Vec::<String>::new());
}

#[test]
fn chunks_are_stepped_through_and_run_together() {
    let (_folder, mut r) = report();
    r.running();

    r.press("M-n");
    assert_eq!(line(&r), 5);
    r.press("M-n");
    assert_eq!(line(&r), 12);
    r.press("M-n");
    assert_eq!(r.editor.message(), Some("No more chunks"));
    r.press("M-p");
    assert_eq!(line(&r), 5);
    r.press("M-p");
    assert_eq!(r.editor.message(), Some("No chunk before this one"));

    r.press("C-c C-a");
    assert_eq!(r.sent(), ["library(stats)\nx <- 1:10\nplot(x)\n"]);
    r.editor.go_to_line(13, 1);
    r.press("C-c C-r");
    assert_eq!(r.sent(), ["library(stats)\nx <- 1:10\n"]);
}

#[test]
fn control_return_sends_the_statement_in_the_chunk_and_steps() {
    let (_folder, mut r) = opened(
        "steps.Rmd",
        "```{r}\nlibrary(stats)\n\ny <- df |>\n  head(\n    3)\nz <- \"(\" # )\n```\n",
    );
    r.running();
    r.editor.go_to_line(2, 1);

    r.press("C-RET");
    assert_eq!(r.sent(), ["library(stats)\n"]);
    assert_eq!(line(&r), 3);
    r.press("C-RET");
    assert_eq!(r.sent(), ["y <- df |>\n  head(\n    3)\n"]);
    assert_eq!(line(&r), 6);
    r.press("C-RET");
    assert_eq!(r.sent(), ["z <- \"(\" # )\n"]);
    assert_eq!(line(&r), 7);
    r.editor.go_to_line(1, 1);
    r.press("C-RET");
    assert_eq!(r.editor.message(), Some("Not in an R chunk"));
}

#[test]
fn control_c_control_i_inserts_a_chunk() {
    let (_folder, mut r) = report();
    r.editor.go_to_line(4, 1);

    r.press("C-c C-i");

    assert_eq!(r.lines()[3..7], ["```{r}", "", "```", ""]);
    assert_eq!(line(&r), 4);
}

#[test]
fn control_c_control_k_renders_and_opens_the_result() {
    let (folder, mut r) = report();
    let job = r.running();
    let file = folder.path().join("report.Rmd");

    r.press("C-c C-k");
    assert_eq!(r.sent(), [render_call(&file)]);
    r.output(
        job,
        &format!(
            "\u{1}rendered\u{2}{}\tC:/out/report.html\u{3}{PROMPT}",
            file.display()
        ),
    );

    assert!(
        r.editor
            .take_effects()
            .contains(&Effect::OpenExternally(PathBuf::from("C:/out/report.html")))
    );
    assert_eq!(r.editor.message(), Some("Rendered report.html"));
}

#[test]
fn saving_renders_when_the_setting_asks_for_it() {
    let (folder, mut r) = report();
    r.running();
    let file = folder.path().join("report.Rmd");

    r.press("x C-x C-s");
    assert_eq!(r.sent(), Vec::<String>::new());

    r.eval("(configure-plugin 'r {render-on-save: true})");
    r.press("x C-x C-s");
    assert_eq!(r.sent(), [render_call(&file)]);
}

#[test]
fn m_g_i_offers_the_headings_and_chunks_and_jumps_to_the_one_chosen() {
    let (_folder, mut r) = report();

    r.press("M-g i");

    let offered: Vec<(String, Option<String>)> = r
        .editor
        .completions()
        .into_iter()
        .map(|completion| (completion.label, completion.annotation))
        .collect();
    let item = |label: &str, note: &str| (label.to_owned(), Some(note.to_owned()));
    assert_eq!(
        offered,
        [
            item("Report", "L1"),
            item("chunk setup", "r  L5"),
            item("Plots", "L10"),
            item("chunk 2", "r  L12"),
        ]
    );
    r.typed("Plots");
    r.press("RET");
    assert_eq!(line(&r), 9);
}

#[test]
fn the_console_key_goes_between_the_document_and_its_session() {
    let (_folder, mut r) = report();
    r.running();

    r.press("C-c C-z");
    assert!(r.editor.buffer().name().starts_with("*R:"));
    r.press("C-c C-z");
    assert_eq!(r.editor.buffer().name(), "report.Rmd");
}
