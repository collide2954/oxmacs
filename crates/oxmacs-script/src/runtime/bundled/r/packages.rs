use std::path::PathBuf;

use oxmacs_core::editor::{Editor, JobId};

use super::{PROMPT, R, started, with_program};

fn project(package: bool) -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let marker = if package { "DESCRIPTION" } else { "renv.lock" };
    std::fs::write(root.path().join(marker), "Package: tidy\n").unwrap();
    std::fs::create_dir_all(root.path().join("R")).unwrap();
    std::fs::create_dir_all(root.path().join("tests").join("testthat")).unwrap();
    std::fs::write(
        root.path().join("R").join("tidy.R"),
        "tidy <- function(x) x\n",
    )
    .unwrap();
    std::fs::write(
        root.path()
            .join("tests")
            .join("testthat")
            .join("test-tidy.R"),
        "test_that(\"tidy works\", {\n  expect_equal(tidy(1), 1)\n})\ntest_that(\"tidy fails\", {\n  expect_equal(tidy(1), 2)\n})\n",
    )
    .unwrap();
    let script = root.path().join("R").join("tidy.R");
    (root, script)
}

fn in_package(package: bool, available: &str) -> (tempfile::TempDir, R, JobId) {
    let (root, script) = project(package);
    let mut r = with_program(None, Editor::new());
    r.editor.open_file(&script);
    let job = r.running();
    r.output(job, &format!("\u{1}packages\u{2}{available}\u{3}{PROMPT}"));
    r.editor.take_effects();
    (root, r, job)
}

fn quoted(root: &tempfile::TempDir) -> String {
    format!("{:?}", root.path().display().to_string())
}

#[test]
fn package_keys_run_devtools_on_the_project() {
    let (root, mut r, _) = in_package(true, "devtools\ttestthat");
    let at = quoted(&root);

    for (keys, call) in [
        ("C-c C-w l", "devtools::load_all"),
        ("C-S-l", "devtools::load_all"),
        ("C-c C-w d", "devtools::document"),
        ("C-c C-w c", "devtools::check"),
        ("C-c C-w i", "devtools::install"),
        ("C-c C-w b", "devtools::build"),
    ] {
        r.press(keys);
        assert_eq!(r.sent(), [format!("{call}({at})\n")], "{keys}");
    }
}

#[test]
fn without_devtools_its_parts_stand_in_or_say_what_is_missing() {
    let (root, mut r, _) = in_package(true, "pkgload\troxygen2\tpkgbuild");
    let at = quoted(&root);

    r.press("C-c C-w l");
    assert_eq!(r.sent(), [format!("pkgload::load_all({at})\n")]);
    r.press("C-c C-w d");
    assert_eq!(r.sent(), [format!("roxygen2::roxygenise({at})\n")]);
    r.press("C-c C-w b");
    assert_eq!(r.sent(), [format!("pkgbuild::build({at})\n")]);
    r.press("C-c C-w c");
    assert_eq!(
        r.editor.message(),
        Some("Checking a package needs devtools")
    );
    r.press("C-c C-w t");
    assert_eq!(r.editor.message(), Some("Running tests needs testthat"));
    assert_eq!(r.sent(), Vec::<String>::new());
}

#[test]
fn projects_without_a_description_are_not_packages() {
    let (_root, mut r, _) = in_package(false, "devtools");

    r.press("C-c C-w l");

    assert_eq!(
        r.editor.message(),
        Some("This project has no DESCRIPTION, so it is not an R package")
    );
    assert_eq!(r.sent(), Vec::<String>::new());
}

#[test]
fn renv_keeps_the_project_library_from_m_x() {
    let (root, mut r, _) = in_package(false, "renv");
    let at = quoted(&root);

    for (command, call) in [
        ("r-renv-status", "status"),
        ("r-renv-snapshot", "snapshot"),
        ("r-renv-restore", "restore"),
        ("r-renv-init", "init"),
    ] {
        r.run(command);
        assert_eq!(
            r.sent(),
            [format!("renv::{call}(project = {at})\n")],
            "{command}"
        );
    }
}

#[test]
fn renv_commands_say_when_renv_is_missing() {
    let (_root, mut r, _) = in_package(false, "devtools");

    r.run("r-renv-status");

    assert_eq!(
        r.editor.message(),
        Some("This needs renv; install it in R with install.packages(\"renv\")")
    );
    assert_eq!(r.sent(), Vec::<String>::new());
}

#[test]
fn test_results_are_listed_by_file_and_return_visits_a_failure() {
    let (root, mut r, job) = in_package(true, "devtools\ttestthat");

    r.press("C-c C-w t");
    assert_eq!(
        r.sent(),
        [format!("getOption(\"oxmacs\")$test({})\n", quoted(&root))]
    );
    r.output(
        job,
        &format!(
            "\u{1}tests\u{2}{}\ntest-tidy.R\ttidy works\tpassed\t\t\ntest-tidy.R\ttidy fails\tfailed\t5\ttidy(1) not equal to 2.\u{3}{PROMPT}",
            root.path().display()
        ),
    );

    assert_eq!(r.editor.buffer().name(), "*R tests*");
    assert_eq!(
        r.lines(),
        [
            "1 passed, 1 failed, 0 skipped",
            "RET visits   SPC shows   n p step   g refreshes   q quits",
            "",
            "test-tidy.R",
            "     1:1    ✓  tidy works",
            "     5:1    ✗  tidy fails  tidy(1) not equal to 2."
        ]
    );
    r.press("n RET");
    assert_eq!(r.editor.buffer().name(), "test-tidy.R");
    assert_eq!(r.editor.buffer().line_of(r.editor.buffer().point()), 4);
}

#[test]
fn the_packages_pane_lists_what_is_installed_and_attaches_with_return() {
    let mut r = started();
    r.run("run-r");
    let job = r.started()[0];
    r.output(job, PROMPT);
    r.editor.take_effects();

    r.run("r-packages");
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$packages()\n"]);
    r.output(
        job,
        &format!("\u{1}installed\u{2}MASS\t7.3-61\tFALSE\tC:/R/library\nstats\t4.6.1\tTRUE\tC:/R/library\u{3}{PROMPT}"),
    );

    assert_eq!(r.editor.buffer().name(), "*R packages*");
    assert_eq!(
        r.lines(),
        [
            "RET attaches or detaches   h shows its help   g refreshes   q quits",
            "",
            "  MASS   7.3-61",
            "* stats  4.6.1"
        ]
    );
    r.press("M-> C-p RET");
    assert_eq!(r.sent(), ["library(MASS)\n"]);
    r.press("C-n RET");
    assert_eq!(r.sent(), ["detach(\"package:stats\")\n"]);
    r.press("h");
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$help_index(\"stats\")\n"]);
}
