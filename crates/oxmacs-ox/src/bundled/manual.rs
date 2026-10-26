use std::fs;

use oxmacs_core::editor::{Editor, Effect};
use oxmacs_core::text::Buffer;
use oxmacs_core::theme::Highlight;

use crate::Ox;
use crate::log::LOG;
use crate::repl::REPL;
use crate::testing::{home_with, press, settle, typed};

const MANUAL: &str = "*Ox manual*";

fn started() -> (tempfile::TempDir, Editor, Ox) {
    let home = home_with(&[]);
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn lines(editor: &Editor) -> Vec<String> {
    editor
        .buffer()
        .contents()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn go_to(editor: &mut Editor, text: &str) {
    let found = lines(editor).iter().enumerate().find_map(|(line, shown)| {
        shown
            .find(text)
            .map(|at| (line, shown[..at].chars().count()))
    });
    let Some((line, column)) = found else {
        panic!("{text:?} is not shown: {:#?}", lines(editor));
    };
    let start = editor.buffer().line_start(line);
    editor.buffer_mut().set_point(start + column);
}

fn current_line(editor: &Editor) -> String {
    let buffer = editor.buffer();
    lines(editor)[buffer.line_of(buffer.point())].clone()
}

fn opened() -> (tempfile::TempDir, Editor, Ox) {
    let (home, mut editor, mut ox) = started();
    press(&mut editor, &mut ox, "C-h i");
    assert_eq!(editor.buffer().name(), MANUAL);
    (home, editor, ox)
}

#[test]
fn c_h_i_opens_the_contents_and_links_lead_through_the_guide() {
    let (_home, mut editor, mut ox) = opened();
    let shown = lines(&editor);
    assert_eq!(shown[0], "The Ox manual");
    assert_eq!(editor.line_style(0).spans, [(0..13, Highlight::Heading)]);
    assert!(shown.contains(&"  • The language".to_owned()), "{shown:#?}");
    assert!(
        shown.contains(&"  • Modules and libraries".to_owned()),
        "{shown:#?}"
    );
    assert!(
        shown.contains(&"  • Buffers and views".to_owned()),
        "{shown:#?}"
    );
    assert!(shown.contains(&"  • Editor API".to_owned()), "{shown:#?}");
    assert!(shown.contains(&"  • ox/string".to_owned()), "{shown:#?}");

    go_to(&mut editor, "The language");
    press(&mut editor, &mut ox, "RET");
    assert_eq!(lines(&editor)[0], "The language");

    press(&mut editor, &mut ox, "l");
    assert_eq!(lines(&editor)[0], "The Ox manual");
    assert_eq!(current_line(&editor), "  • The language");

    press(&mut editor, &mut ox, "r");
    assert_eq!(lines(&editor)[0], "The language");
    press(&mut editor, &mut ox, "n");
    assert_eq!(lines(&editor)[0], "Data");
    press(&mut editor, &mut ox, "p p");
    assert_eq!(lines(&editor)[0], "Getting started");
    press(&mut editor, &mut ox, "u");
    assert_eq!(lines(&editor)[0], "The Ox manual");
}

#[test]
fn tab_moves_between_links() {
    let (_home, mut editor, mut ox) = opened();

    press(&mut editor, &mut ox, "M-< TAB");
    assert_eq!(current_line(&editor), "  • Getting started");
    press(&mut editor, &mut ox, "TAB");
    assert_eq!(current_line(&editor), "  • The language");
    press(&mut editor, &mut ox, "S-TAB");
    assert_eq!(current_line(&editor), "  • Getting started");
}

#[test]
fn the_reference_comes_from_the_documentation_and_names_can_be_looked_up() {
    let (_home, mut editor, mut ox) = opened();

    go_to(&mut editor, "ox/string");
    press(&mut editor, &mut ox, "RET");
    let shown = lines(&editor);
    assert_eq!(shown[0], "ox/string");
    assert!(
        shown.iter().any(|line| line.starts_with("(split ")),
        "{shown:#?}"
    );

    press(&mut editor, &mut ox, "i");
    typed(&mut editor, &mut ox, "map");
    press(&mut editor, &mut ox, "RET");
    assert_eq!(lines(&editor)[0], "Core library");
    assert!(
        current_line(&editor).starts_with("(map "),
        "{}",
        current_line(&editor)
    );

    press(&mut editor, &mut ox, "i");
    typed(&mut editor, &mut ox, "open-buffer");
    press(&mut editor, &mut ox, "RET");
    assert_eq!(lines(&editor)[0], "Editor API");
    assert!(current_line(&editor).starts_with("(open-buffer "));
}

#[test]
fn search_lists_pages_and_names_that_mention_a_word() {
    let (_home, mut editor, mut ox) = opened();

    press(&mut editor, &mut ox, "s");
    typed(&mut editor, &mut ox, "destructuring");
    press(&mut editor, &mut ox, "RET");

    let shown = lines(&editor);
    assert_eq!(shown[0], "Search: destructuring");
    go_to(&mut editor, "Data:");
    press(&mut editor, &mut ox, "RET");
    assert_eq!(lines(&editor)[0], "Data");
    assert!(
        current_line(&editor).contains("Destructuring"),
        "{}",
        current_line(&editor)
    );

    press(&mut editor, &mut ox, "s");
    typed(&mut editor, &mut ox, "split");
    press(&mut editor, &mut ox, "RET");
    go_to(&mut editor, "string/split");
    press(&mut editor, &mut ox, "RET");
    assert_eq!(lines(&editor)[0], "ox/string");
    assert!(current_line(&editor).starts_with("(split "));
}

#[test]
fn x_runs_the_example_at_point_in_the_repl() {
    let (_home, mut editor, mut ox) = opened();
    go_to(&mut editor, "Getting started");
    press(&mut editor, &mut ox, "RET");

    go_to(&mut editor, "(str \"Hello, \" \"Ox\")");
    press(&mut editor, &mut ox, "x");

    let repl = editor.buffer_named(REPL).expect("a REPL");
    let text = editor.buffer_by_id(repl).unwrap().contents();
    assert!(text.contains("\"Hello, Ox\""), "{text}");
}

const GUIDE: &[(&str, &str)] = &[
    ("start", include_str!("../../plugins/manual/start.ox")),
    ("language", include_str!("../../plugins/manual/language.ox")),
    ("data", include_str!("../../plugins/manual/data.ox")),
    ("flow", include_str!("../../plugins/manual/flow.ox")),
    ("modules", include_str!("../../plugins/manual/modules.ox")),
    ("tasks", include_str!("../../plugins/manual/tasks.ox")),
    ("editor", include_str!("../../plugins/manual/editor.ox")),
    ("ui", include_str!("../../plugins/manual/ui.ox")),
    ("plugins", include_str!("../../plugins/manual/plugins.ox")),
    ("tools", include_str!("../../plugins/manual/tools.ox")),
];

fn examples(page: &str) -> Vec<String> {
    blocks(page, &["", "ox"])
}

fn blocks(page: &str, wanted: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    let mut fenced = false;
    let mut current: Option<Vec<&str>> = None;
    for line in page.lines() {
        match line.strip_prefix("```") {
            Some(_) if fenced => {
                found.extend(current.take().map(|lines| lines.join("\n")));
                fenced = false;
            }
            Some(fence) => {
                fenced = true;
                current = wanted.contains(&fence).then(Vec::new);
            }
            None => {
                if let Some(lines) = &mut current {
                    lines.push(line);
                }
            }
        }
    }
    found
}

fn logged(editor: &Editor) -> String {
    editor
        .buffer_named(LOG)
        .and_then(|id| editor.buffer_by_id(id))
        .map(Buffer::contents)
        .unwrap_or_default()
}

#[test]
fn every_example_in_the_guide_runs() {
    let (_home, mut editor, mut ox) = started();
    let mut failures = Vec::new();
    let mut count = 0;
    for (page, text) in GUIDE {
        for source in examples(text) {
            count += 1;
            if let Err(error) = ox.eval(&mut editor, &source) {
                failures.push(format!("{page}: {source}\n  {}", error.message));
            }
            settle(&mut editor, &mut ox);
            let log = logged(&editor);
            if !log.is_empty() {
                failures.push(format!("{page}: {source}\n  logged {log}"));
                break;
            }
        }
    }
    assert!(count > 30, "only {count} examples");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn links(page: &str) -> Vec<String> {
    page.split("](")
        .skip(1)
        .filter_map(|rest| rest.split_once(')'))
        .map(|(target, _)| target.to_owned())
        .collect()
}

#[test]
fn every_link_in_the_guide_leads_to_a_page() {
    let (_home, mut editor, mut ox) = opened();
    let mut broken = Vec::new();
    let mut count = 0;
    for (page, text) in GUIDE {
        for target in links(text) {
            count += 1;
            if target == "tutorial" {
                continue;
            }
            let check = format!(
                r#"(module (import (manual)))
                   (let [spot (manual/spot-of "{target}")
                         found (manual/page-of spot.id (manual/reference))]
                     (and (some? found)
                          (or (nil? spot.anchor) (some? (find (fn [entry] (= entry.name spot.anchor)) found.entries)))))"#
            );
            match ox.eval(&mut editor, &check) {
                Ok(shown) if shown == "true" => {}
                Ok(shown) => broken.push(format!("{page}: {target} ({shown})")),
                Err(error) => broken.push(format!("{page}: {target} ({})", error.message)),
            }
        }
    }
    assert!(count > 10, "only {count} links");
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}

#[test]
fn the_plugins_the_guide_builds_work() {
    let page = GUIDE
        .iter()
        .find(|(id, _)| *id == "plugins")
        .map(|(_, text)| *text)
        .unwrap();
    let [manifest, entry, init, library]: [String; 4] = blocks(page, &["text"]).try_into().unwrap();
    let init = format!("{init}\n(use-plugin strings)");
    let home = home_with(&[
        ("plugins/hello/plugin.ox", manifest.as_str()),
        ("plugins/hello/hello.ox", entry.as_str()),
        ("plugins/strings/plugin.ox", library.as_str()),
        ("plugins/strings/strings.ox", ""),
        (
            "plugins/strings/case.ox",
            "(def (shout text) (str text \"!\"))",
        ),
        ("init.ox", init.as_str()),
    ]);
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);

    press(&mut editor, &mut ox, "M-x");
    typed(&mut editor, &mut ox, "hello");
    press(&mut editor, &mut ox, "RET");
    assert_eq!(editor.message(), Some("Hello, Ana!"));

    let shouted = ox
        .eval(
            &mut editor,
            "(module (import (strings case))) (case/shout \"hi\")",
        )
        .unwrap();
    assert_eq!(shouted, "\"hi!\"");
    assert_eq!(logged(&editor), "");
}

#[test]
fn c_h_t_opens_your_own_copy_of_the_tutorial_where_you_left_it() {
    let (home, mut editor, mut ox) = started();

    press(&mut editor, &mut ox, "C-h t");

    let copy = home.path().join("tutorial.txt");
    assert!(copy.is_file());
    assert_eq!(
        editor.buffer().file().map(|file| file.path.clone()),
        Some(copy.clone())
    );
    assert!(lines(&editor)[0].starts_with("The oxmacs tutorial"));
    assert_eq!(editor.buffer().line_of(editor.buffer().point()), 0);

    fs::write(home.path().join("tutorial.place"), "12").unwrap();
    press(&mut editor, &mut ox, "C-x k RET");
    press(&mut editor, &mut ox, "C-h t");
    assert_eq!(editor.buffer().line_of(editor.buffer().point()), 12);
}

const TUTORIAL: &str = include_str!("../../plugins/manual/tutorial.ox");

fn tutorial_block(after: &str) -> String {
    let lines: Vec<&str> = TUTORIAL
        .lines()
        .skip_while(|line| !line.starts_with(after))
        .skip(1)
        .skip_while(|line| line.trim().is_empty())
        .take_while(|line| line.starts_with("  ") || line.trim().is_empty())
        .map(|line| line.strip_prefix("  ").unwrap_or(line))
        .collect();
    lines.join("\n").trim_end().to_owned()
}

#[test]
fn the_tutorials_exercises_do_what_it_says() {
    let (home, mut editor, mut ox) = started();
    editor.set_home_directory(Some(home.path().to_path_buf()));
    assert!(
        TUTORIAL
            .contains(">> Move to the next line, cut it with C-w, then paste it twice with C-y.")
    );
    press(&mut editor, &mut ox, "C-h t");

    go_to(&mut editor, "Copy me.");
    press(&mut editor, &mut ox, "C-w");
    for _ in 0..2 {
        press(&mut editor, &mut ox, "C-y");
        assert!(editor.take_effects().contains(&Effect::ReadClipboard));
        editor.paste(None);
    }
    assert_eq!(
        lines(&editor)
            .iter()
            .filter(|line| *line == "Copy me.")
            .count(),
        2
    );

    press(&mut editor, &mut ox, "C-x C-f");
    typed(&mut editor, &mut ox, "~/hello.ox");
    press(&mut editor, &mut ox, "RET");
    assert_eq!(
        editor.buffer().file().map(|file| file.path.clone()),
        Some(home.path().join("hello.ox"))
    );
    editor.paste(Some(tutorial_block(">> Now type the lines below")));
    press(&mut editor, &mut ox, "C-x C-e M-x");
    typed(&mut editor, &mut ox, "hello");
    press(&mut editor, &mut ox, "RET");
    assert_eq!(editor.message(), Some("Hello from Ox"));
}

#[test]
fn the_tutorials_configuration_works_in_a_fresh_init_ox() {
    let init = tutorial_block(">> Add these lines to init.ox");
    let home = home_with(&[("init.ox", init.as_str())]);
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);

    assert_eq!(logged(&editor), "");
    press(&mut editor, &mut ox, "C-c h");
    assert_eq!(editor.message(), Some("Hello from Ox"));
}

#[test]
fn deleting_your_copy_of_the_tutorial_starts_it_over() {
    let (home, mut editor, mut ox) = started();
    press(&mut editor, &mut ox, "C-h t");
    press(&mut editor, &mut ox, "C-x k RET");
    fs::write(home.path().join("tutorial.place"), "12").unwrap();
    fs::remove_file(home.path().join("tutorial.txt")).unwrap();

    press(&mut editor, &mut ox, "C-h t");

    assert!(lines(&editor)[0].starts_with("The oxmacs tutorial"));
    assert_eq!(editor.buffer().line_of(editor.buffer().point()), 0);
}
