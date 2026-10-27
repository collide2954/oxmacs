use std::path::PathBuf;

use oxmacs_core::editor::{Editor, Effect, Job, Outcome, ProcessStatus};

use super::{PROMPT, R, ox_text, started, with_program};
use crate::testing::settle;

fn contents(r: &R, name: &str) -> String {
    r.editor
        .buffer_named(name)
        .and_then(|id| r.editor.buffer_by_id(id))
        .map(oxmacs_core::text::Buffer::contents)
        .unwrap_or_default()
}

fn spawned(r: &mut R) -> Job {
    r.editor
        .take_effects()
        .into_iter()
        .find_map(|effect| match effect {
            Effect::Spawn(job) => Some(job),
            _ => None,
        })
        .unwrap()
}

fn start(r: &mut R, name: &str, options: &str) -> Job {
    let folder = ox_text(&std::env::temp_dir().display().to_string());
    r.eval(&format!(
        "(module (import (r jobs :as jobs))) (jobs/start {} \"Rscript\" [\"job.R\"] {folder} {options})",
        ox_text(name)
    ));
    spawned(r)
}

#[test]
fn jobs_run_in_their_own_buffer_and_say_how_they_ended() {
    let mut r = started();

    let job = start(&mut r, "*R job: two*", ":title \"Rscript job.R\"");
    assert_eq!(
        (job.program.as_str(), job.args.as_slice()),
        ("Rscript", &["job.R".to_owned()][..])
    );
    r.editor.job_output(job.id, "one");
    r.editor.job_finished(job.id, &Outcome::Exited(Some(3)));
    settle(&mut r.editor, &mut r.ox);

    let shown = contents(&r, "*R job: two*");
    let lines: Vec<&str> = shown.lines().collect();
    assert_eq!(
        lines[1..],
        [
            "Rscript job.R",
            "",
            "one",
            "",
            "Exited abnormally with code 3"
        ]
    );
}

#[test]
fn c_c_c_k_stops_a_job_and_g_runs_it_again() {
    let mut r = started();
    let first = start(&mut r, "*R job: long*", "");
    let id = r.editor.buffer_named("*R job: long*").unwrap();
    r.editor.show_buffer(id);

    r.press("C-c C-k");
    assert_eq!(r.editor.job_status(first.id), Some(ProcessStatus::Stopping));
    r.editor.job_finished(first.id, &Outcome::Exited(None));
    assert!(contents(&r, "*R job: long*").ends_with("Killed\n"));

    r.press("g");
    let second = spawned(&mut r);
    assert_eq!(second.args, first.args);
    r.editor.job_finished(second.id, &Outcome::Exited(Some(0)));
    r.press("C-c C-k");
    assert_eq!(r.editor.message(), Some("Nothing is running here"));
}
#[test]
fn shiny_apps_open_once_they_listen() {
    let mut r = started();

    r.eval(r#"(module (import (r jobs :as jobs))) (jobs/heard-app "Listening on http://127.0.0.1:4321")"#);

    assert_eq!(
        r.editor.take_effects(),
        [Effect::OpenExternally(PathBuf::from(
            "http://127.0.0.1:4321"
        ))]
    );
}

#[test]
fn running_needs_rscript_beside_r_and_an_app_folder() {
    let folder = tempfile::tempdir().unwrap();
    let script = folder.path().join("job.R");
    std::fs::write(&script, "print(1)\n").unwrap();
    let mut r = with_program(None, Editor::new());
    r.editor.open_file(&script);
    settle(&mut r.editor, &mut r.ox);

    r.run("r-run-job");
    assert_eq!(
        r.editor.message(),
        Some(format!("Rscript was not found beside {}", r.program().display()).as_str())
    );
    r.run("r-run-app");
    assert_eq!(
        r.editor.message(),
        Some("No Shiny app here: it needs app.R, or ui.R and server.R")
    );
}

#[test]
fn widgets_r_shows_open_in_the_system_viewer() {
    let mut r = started();
    r.run("run-r");
    let job = r.started()[0];
    r.output(job, PROMPT);
    r.editor.take_effects();

    r.output(
        job,
        &format!("\u{1}viewer\u{2}C:/tmp/widget/index.html\u{3}{PROMPT}"),
    );

    assert!(
        r.editor
            .take_effects()
            .contains(&Effect::OpenExternally(PathBuf::from(
                "C:/tmp/widget/index.html"
            )))
    );
}
