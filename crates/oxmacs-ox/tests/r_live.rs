use std::io::Write;
use std::process::{Command, Stdio};

use oxmacs_core::editor::Editor;
use oxmacs_ox::Ox;

fn unquoted(shown: &str) -> String {
    shown
        .trim_matches('"')
        .replace("\\\\", "\\")
        .replace("\\\"", "\"")
}

fn session(input: &str) -> (String, tempfile::TempDir) {
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, None);
    let mut eval = |source: &str| {
        ox.eval(
            &mut editor,
            &format!("(module (import (r session) (r helper))) {source}"),
        )
        .unwrap()
    };
    let program = eval("(session/find-r)");
    assert_ne!(program, "nil", "R is not installed");
    let arguments: Vec<String> =
        eval("(apply str (map (fn [argument] (str argument \"\\n\")) (session/arguments)))")
            .trim_matches('"')
            .split("\\n")
            .filter(|argument| !argument.is_empty())
            .map(str::to_owned)
            .collect();
    let helper = unquoted(&eval("(await (helper/write))"));
    let directory = tempfile::tempdir().unwrap();
    let mut child = Command::new(unquoted(&program))
        .args(arguments)
        .env("R_PROFILE_USER", &helper)
        .env("OXMACS_R_PLOTS", directory.path().join("plots"))
        .current_dir(directory.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let _ = std::fs::remove_file(helper);
    (
        String::from_utf8_lossy(&output.stdout).replace('\r', ""),
        directory,
    )
}

#[test]
#[ignore = "runs the R installed on this machine"]
fn help_comes_back_as_markers_and_leaves_the_last_value_alone() {
    let (output, _directory) = session(
        "42\ngetOption(\"oxmacs\")$help(\"mean\")\n.Last.value\ngetOption(\"oxmacs\")$help(\"nosuchtopicxyz\")\n?median\nq()\n",
    );

    assert!(output.contains("\u{1}help\u{2}mean "), "{output:?}");
    assert_eq!(output.matches("[1] 42\n").count(), 2, "{output:?}");
    assert!(
        output.contains("\u{1}message\u{2}No R help for nosuchtopicxyz\u{3}"),
        "{output:?}"
    );
    assert!(output.contains("\u{1}help\u{2}median "), "{output:?}");
}

#[test]
#[ignore = "runs the R installed on this machine"]
fn plots_are_rendered_when_made_changed_or_asked_for() {
    let (output, _directory) = session(
        "plot(1:10)\nabline(h = 5)\n1 + 1\nhist(c(1, 2, 2, 3))\ngetOption(\"oxmacs\")$plot(1L, 400L, 300L)\nq()\n",
    );

    let plots: Vec<Vec<&str>> = output
        .split("\u{1}plot\u{2}")
        .skip(1)
        .filter_map(|rest| rest.split_once('\u{3}'))
        .map(|(plot, _)| plot.split('\t').collect())
        .collect();
    let summary: Vec<(&str, &str, &str, &str, &str)> = plots
        .iter()
        .map(|fields| (fields[0], fields[1], fields[2], fields[4], fields[5]))
        .collect();
    assert_eq!(
        summary,
        [
            ("new", "1", "1", "672", "480"),
            ("new", "1", "1", "672", "480"),
            ("new", "2", "2", "672", "480"),
            ("asked", "1", "2", "400", "300"),
        ],
        "{output:?}"
    );
    for fields in &plots {
        let png = std::fs::metadata(fields[6]).unwrap();
        assert!(png.len() > 1000);
    }
    assert!(!output.contains("oxmacs-plots"), "{output:?}");
}

#[test]
#[ignore = "runs the R installed on this machine"]
fn plots_are_saved_zoomed_and_deleted() {
    let out = tempfile::tempdir().unwrap();
    let at = |name: &str| {
        out.path()
            .join(name)
            .display()
            .to_string()
            .replace('\\', "/")
    };
    let call = |function: &str, arguments: &str| {
        format!("getOption(\"oxmacs\")${function}({arguments})\n")
    };
    let input = [
        "plot(1:3)\n".to_owned(),
        "hist(1:5)\n".to_owned(),
        call(
            "plot_save",
            &format!("1L, \"{}\", 400L, 300L", at("one.pdf")),
        ),
        call(
            "plot_save",
            &format!("2L, \"{}\", 400L, 300L", at("two.png")),
        ),
        call(
            "plot_save",
            &format!("1L, \"{}\", 400L, 300L", at("one.svg")),
        ),
        call("plot_zoom", "1L, 1200L, 900L"),
        call("plot_delete", "2L, 400L, 300L"),
        "abline(h = 2)\n".to_owned(),
        call("plot_pinned", "1L, 300L, 200L"),
        call("plot_pinned", "99L, 300L, 200L"),
        call("plot_delete", "3L, 400L, 300L"),
        call("plot_delete", "1L, 400L, 300L"),
        "q()\n".to_owned(),
    ]
    .concat();
    let (output, _directory) = session(&input);

    for name in ["one.pdf", "two.png", "one.svg"] {
        let saved = std::fs::metadata(out.path().join(name)).unwrap();
        assert!(saved.len() > 500, "{name}");
        assert!(
            output.contains(&format!("Saved plot to {}", at(name))),
            "{output:?}"
        );
    }
    let zoom = output
        .split_once("\u{1}zoom\u{2}")
        .and_then(|(_, rest)| rest.split_once('\u{3}'))
        .map(|(path, _)| path.to_owned())
        .unwrap();
    assert!(std::fs::metadata(zoom).unwrap().len() > 1000);
    let plots: Vec<String> = output
        .split("\u{1}plot\u{2}")
        .skip(1)
        .filter_map(|rest| rest.split_once('\u{3}'))
        .map(|(plot, _)| plot.split('\t').take(3).collect::<Vec<_>>().join(" "))
        .collect();
    assert_eq!(
        plots,
        [
            "new 1 1",
            "new 2 2",
            "deleted 1 1",
            "new 2 2",
            "pinned 1 2",
            "gone 99",
            "deleted 1 1",
            "none"
        ],
        "{output:?}"
    );
}

fn marked<'a>(output: &'a str, kind: &str) -> Vec<&'a str> {
    output
        .split(&format!("\u{1}{kind}\u{2}"))
        .skip(1)
        .filter_map(|rest| rest.split_once('\u{3}'))
        .map(|(payload, _)| payload)
        .collect()
}

#[test]
#[ignore = "runs the R installed on this machine"]
fn the_environment_tables_and_objects_are_described() {
    let (output, _directory) = session(
        "x <- 1:10\ns <- \"hello world\"\ncars <- mtcars\nadd <- function(a, b = 2) a + b\ngetOption(\"oxmacs\")$environment()\ngetOption(\"oxmacs\")$view(\"cars\", 2L)\ngetOption(\"oxmacs\")$view(\"x\", 2L)\ngetOption(\"oxmacs\")$inspect(\"add\")\ngetOption(\"oxmacs\")$peek(\"cars\")\nq()\n",
    );

    let environment = marked(&output, "environment");
    let rows: Vec<&str> = environment[0].lines().collect();
    assert_eq!(
        rows,
        [
            "functions\tadd\tfunction\t1\tfunction (a, b = 2) ",
            "data\tcars\tdata.frame\t32 x 11\t32 obs. of 11 variables",
            "values\ts\tcharacter\t1\t\"hello world\"",
            "values\tx\tinteger\t10\t1 2 3 4 5 6 7 8 9 10",
        ],
        "{output:?}"
    );
    let table: Vec<&str> = marked(&output, "table")[0].lines().collect();
    assert_eq!(table[0], "cars\t32\t11\t0\t32\t\tFALSE\t");
    assert!(table[1].starts_with("\tmpg\tcyl\tdisp"), "{table:?}");
    assert!(table[2].starts_with("\tnumeric\tnumeric"), "{table:?}");
    assert!(table[3].starts_with("Mazda RX4\t21\t6\t160"), "{table:?}");
    assert_eq!(table.len(), 5);
    assert_eq!(marked(&output, "message"), ["x is not a table"]);
    assert!(marked(&output, "object")[0].starts_with("add\nfunction (a, b = 2)"));
    let peek: Vec<&str> = marked(&output, "peek")[0].lines().collect();
    assert_eq!(peek[0], "cars");
    assert!(peek[1].starts_with("'data.frame':"), "{peek:?}");
    assert_eq!(peek.len(), 13, "{peek:?}");
}

#[test]
#[ignore = "runs the R installed on this machine"]
fn definitions_point_at_sourced_files_or_show_the_source() {
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("helpers.R");
    std::fs::write(&script, "\n\nhelper <- function(x) {\n  x * 2\n}\n").unwrap();
    let path = script.display().to_string().replace('\\', "/");
    let (output, _directory) = session(&format!(
        "source(\"{path}\", keep.source = TRUE)\ngetOption(\"oxmacs\")$definition(\"helper\")\ngetOption(\"oxmacs\")$definition(\"stats::sd\")\ngetOption(\"oxmacs\")$definition(\"mtcars\")\nq()\n"
    ));

    let definitions = marked(&output, "definition");
    let (name, place) = definitions[0].split_once("\tfile\t").unwrap();
    assert_eq!(name, "helper");
    let (file, line) = place.rsplit_once('\t').unwrap();
    assert_eq!(
        std::path::Path::new(file).canonicalize().unwrap(),
        script.canonicalize().unwrap()
    );
    assert_eq!(line, "3");
    assert!(
        definitions[1].starts_with("stats::sd\tsource\tfunction (x, na.rm = FALSE)"),
        "{definitions:?}"
    );
    assert_eq!(definitions[2], "mtcars\t");
}

#[test]
#[ignore = "runs the R installed on this machine"]
fn signatures_name_the_arguments_of_functions_only() {
    let (output, _directory) = session(
        "add <- function(a, b = 2) a + b\ngetOption(\"oxmacs\")$signature(\"add\")\ngetOption(\"oxmacs\")$signature(\"stats::sd\")\ngetOption(\"oxmacs\")$signature(\"sum\")\ngetOption(\"oxmacs\")$signature(\"mtcars\")\ngetOption(\"oxmacs\")$signature(\"nothing.here\")\n.Last.value\nq()\n",
    );

    assert_eq!(
        marked(&output, "signature"),
        [
            "add\tadd(a, b = 2)",
            "stats::sd\tstats::sd(x, na.rm = FALSE)",
            "sum\tsum(..., na.rm = FALSE)",
            "mtcars\t",
            "nothing.here\t",
        ],
        "{output:?}"
    );
}

#[test]
#[ignore = "runs the R installed on this machine"]
fn output_and_help_fit_the_width_r_is_given() {
    let (output, _directory) = session(
        "getOption(\"oxmacs\")$width(40L)\n1:30\ngetOption(\"oxmacs\")$help(\"mean\")\nq()\n",
    );

    assert!(
        output.contains("\n[13]") || output.contains("\n[12]"),
        "{output:?}"
    );
    let help = output
        .split_once("\u{1}help\u{2}")
        .and_then(|(_, rest)| rest.split_once('\u{3}'))
        .map(|(help, _)| help.replace("_\u{8}", ""))
        .unwrap();
    let widest = help.lines().map(|line| line.chars().count()).max().unwrap();
    assert!(widest <= 42, "{widest}: {help}");
}

#[test]
#[ignore = "runs the R installed on this machine"]
fn completions_come_back_with_where_they_start() {
    let (output, _directory) = session(
        "mydata <- data.frame(alpha = 1, beta = 2)\ngetOption(\"oxmacs\")$complete(\"mydata$a\", 8L)\ngetOption(\"oxmacs\")$complete(\"x <- pri\", 8L)\ngetOption(\"oxmacs\")$complete(\"library(stat\", 12L)\nq()\n",
    );

    assert!(
        output.contains("\u{1}complete\u{2}0\tmydata$alpha\u{3}"),
        "{output:?}"
    );
    assert!(
        output.contains("\u{1}complete\u{2}5\tprincomp\tprint\t"),
        "{output:?}"
    );
    assert!(
        output.contains("\u{1}complete\u{2}8\tstats\tstats4"),
        "{output:?}"
    );
}

#[test]
#[ignore = "runs the R installed on this machine"]
fn tables_sort_filter_page_and_describe_their_columns() {
    let (output, _directory) = session(
        "cars <- mtcars\ngetOption(\"oxmacs\")$view(\"cars\", 3L, 1L, \"mpg\", TRUE, \"cyl == 4\")\ngetOption(\"oxmacs\")$view(\"cars\", 3L, 0L, \"\", FALSE, \"nope +\")\ngetOption(\"oxmacs\")$stats(\"cars\", \"mpg\")\nq()\n",
    );

    let table: Vec<&str> = marked(&output, "table")[0].lines().collect();
    assert_eq!(table[0], "cars\t11\t11\t1\t32\tmpg\tTRUE\tcyl == 4");
    assert!(table[3].starts_with("Fiat 128\t32.4"), "{table:?}");
    assert_eq!(table.len(), 6);
    assert!(
        marked(&output, "message")[0].starts_with("Cannot keep the rows where nope +"),
        "{output:?}"
    );
    assert_eq!(
        marked(&output, "stats"),
        ["mpg\tnumeric\tmean 20.09, sd 6.027, min 10.4, median 19.2, max 33.9, 0 missing"]
    );
}

#[test]
#[ignore = "runs the R and Quarto installed on this machine"]
fn documents_render_beside_themselves_or_say_why_not() {
    let folder = tempfile::tempdir().unwrap();
    let document = folder.path().join("notes.qmd");
    std::fs::write(
        &document,
        "---\ntitle: Notes\n---\n\n# Hello\n\nPlain text.\n",
    )
    .unwrap();
    let path = document.display().to_string().replace('\\', "/");
    let (output, _directory) = session(&format!(
        "getOption(\"oxmacs\")$render(\"{path}\")\ngetOption(\"oxmacs\")$render(\"{path}x.qmd\")\nq()\n"
    ));

    let rendered = marked(&output, "rendered");
    let (source, made) = rendered[0].split_once('\t').unwrap();
    assert_eq!(source, path);
    assert!(made.ends_with("/notes.html"), "{output:?}");
    assert!(std::fs::metadata(made).unwrap().len() > 1000);
    assert_eq!(rendered[1], format!("{path}x.qmd\t"));
    assert!(
        marked(&output, "message")[0].starts_with("Rendering failed:"),
        "{output:?}"
    );
}

#[test]
#[ignore = "runs the R installed on this machine"]
fn packages_are_listed_and_their_help_and_tests_answer() {
    let project = tempfile::tempdir().unwrap();
    let path = project.path().display().to_string().replace('\\', "/");
    let (output, _directory) = session(&format!(
        "getOption(\"oxmacs\")$packages()\ngetOption(\"oxmacs\")$help_index(\"stats\")\ngetOption(\"oxmacs\")$test(\"{path}\")\nq()\n"
    ));

    assert_eq!(marked(&output, "packages").len(), 1, "{output:?}");
    let installed = marked(&output, "installed");
    assert!(
        installed[0]
            .lines()
            .any(|line| line.starts_with("stats\t") && line.ends_with("\tTRUE")),
        "{installed:?}"
    );
    assert!(
        marked(&output, "help")[0].contains("package 'stats'")
            || marked(&output, "help")[0].contains("package ‘stats’"),
        "{output:?}"
    );
    assert!(
        !marked(&output, "tests").is_empty()
            || marked(&output, "message")
                .iter()
                .any(|message| message.contains("testthat") || message.contains("did not run")),
        "{output:?}"
    );
}
