use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use serde_json::json;

use crate::command::{CommandSpec, Scope, ScopeId};
use crate::config::Config;
use crate::display::LineStyle;
use crate::editor::{Editor, Event, EventKind, InputLine, Offer, Page, Question};
use crate::plugin_state::States;
use crate::special_buffer::SpecialBuffer;
use crate::testing::press;
use crate::theme::Highlight;

#[derive(Debug, Default)]
struct Count(usize);

fn count(editor: &mut Editor) {
    if let Some(count) = editor.plugin_state_mut::<Count>() {
        count.0 += 1;
    }
}

fn install_counter(editor: &mut Editor) {
    let command = editor.add_command(CommandSpec {
        name: "count-up",
        summary: "Count one more",
        help: None,
        scope: Scope::Global.into(),
        run: count,
    });
    editor.set_plugin_state(Count::default());
    editor.bind(Scope::Global, "C-c +", command).ok();
}

#[test]
fn plugins_add_commands_keys_and_state() {
    let mut editor = Editor::new();

    install_counter(&mut editor);
    press(&mut editor, "C-c + C-c +");

    assert_eq!(editor.plugin_state::<Count>().map(|count| count.0), Some(2));
}

#[derive(Debug)]
struct Greeter {
    scope: ScopeId,
    count: usize,
    styles: Vec<LineStyle>,
}

impl SpecialBuffer for Greeter {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn line_style(&self, line: usize, _: &str) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }
}

fn greet_open(editor: &mut Editor) {
    let Some(scope) = editor.plugin_state::<GreeterScope>().map(|known| known.0) else {
        return;
    };
    let id = editor.open_special_buffer(
        "*Greeter*",
        Greeter {
            scope,
            count: 0,
            styles: Vec::new(),
        },
    );
    render_greeter(editor, id);
}

fn greet_more(editor: &mut Editor) {
    let Some(id) = editor.find_special_buffer::<Greeter>(|_| true) else {
        return;
    };
    if let Some((_, greeter)) = editor.special_buffer_mut::<Greeter>(id) {
        greeter.count += 1;
    }
    render_greeter(editor, id);
}

fn render_greeter(editor: &mut Editor, id: crate::text::BufferId) {
    let Some((_, greeter)) = editor.special_buffer_mut::<Greeter>(id) else {
        return;
    };
    let mut page = Page::default();
    page.heading(&format!("Hello {} times", greeter.count));
    let (text, styles) = page.into_parts();
    greeter.styles = styles;
    editor.fill_buffer(id, &text);
}

#[derive(Debug)]
struct GreeterScope(ScopeId);

fn install_greetings(editor: &mut Editor) {
    let scope = editor.add_scope("*Greeter*");
    editor.set_plugin_state(GreeterScope(scope));
    let open = editor.add_command(CommandSpec {
        name: "greet",
        summary: "Open the greeter",
        help: None,
        scope: Scope::Global.into(),
        run: greet_open,
    });
    let more = editor.add_command(CommandSpec {
        name: "greet-more",
        summary: "Greet once more",
        help: None,
        scope,
        run: greet_more,
    });
    editor.bind(Scope::Global, "C-c g", open).ok();
    editor.bind(scope, "+", more).ok();
}

#[test]
fn plugin_buffers_have_their_own_keys_text_and_styles() {
    let mut editor = Editor::new();
    install_greetings(&mut editor);

    press(&mut editor, "C-c g + +");

    assert_eq!(editor.buffer().name(), "*Greeter*");
    assert!(editor.buffer().is_read_only());
    assert_eq!(editor.buffer().contents(), "Hello 2 times\n");
    assert_eq!(editor.line_style(0).spans, [(0..13, Highlight::Heading)]);
    press(&mut editor, "C-x b RET +");
    assert_eq!(editor.buffer().name(), "*scratch*");
    assert_eq!(editor.buffer().contents(), "+");
}

#[derive(Debug, Default)]
struct Answers(Vec<String>);

fn note(editor: &mut Editor, answer: String) {
    if let Some(answers) = editor.plugin_state_mut::<Answers>() {
        answers.0.push(answer);
    }
}

#[test]
fn plugins_prompt_with_their_own_completions_and_ask_questions() {
    let mut editor = Editor::new();
    editor.set_plugin_state(Answers::default());

    editor.prompt_with(
        "Fruit: ",
        |_| vec!["apple".into(), "banana".into(), "cherry".into()],
        note,
    );
    assert_eq!(editor.minibuffer().unwrap().prompt(), "Fruit: ");
    press(&mut editor, "a n");
    let offered: Vec<String> = editor
        .completions()
        .into_iter()
        .map(|completion| completion.label)
        .collect();
    assert_eq!(offered, ["banana"]);
    press(&mut editor, "RET");

    editor.prompt("Name: ", note);
    assert!(!editor.minibuffer().unwrap().completes());
    press(&mut editor, "z o e RET");

    editor.prompt_with_text("Port: ", "66", note);
    press(&mut editor, "9 7 RET");

    editor.ask("Keep it?", |editor, keep| note(editor, keep.to_string()));
    assert_eq!(editor.question().unwrap().prompt(), "Keep it? (y or n) ");
    press(&mut editor, "y");

    editor.ask("Again?", |editor, keep| note(editor, keep.to_string()));
    press(&mut editor, "C-g");

    assert_eq!(
        editor
            .plugin_state::<Answers>()
            .map(|answers| answers.0.clone()),
        Some(vec![
            "banana".to_owned(),
            "zoe".to_owned(),
            "6697".to_owned(),
            "true".to_owned()
        ])
    );
    assert!(editor.question().is_none() && editor.minibuffer().is_none());
}

#[test]
fn offers_carry_notes_and_secret_prompts_hide_what_is_typed() {
    let mut editor = Editor::new();
    editor.set_plugin_state(Answers::default());

    editor.prompt_with(
        "Server: ",
        |_| {
            vec![
                Offer::noted("libera", "irc.libera.chat:6697"),
                Offer::from("oftc"),
            ]
        },
        note,
    );
    let notes: Vec<Option<String>> = editor
        .completions()
        .into_iter()
        .map(|completion| completion.annotation)
        .collect();
    assert_eq!(notes, [Some("irc.libera.chat:6697".to_owned()), None]);
    press(&mut editor, "C-g");

    editor.prompt_secret("Password: ", note);
    assert!(editor.minibuffer().unwrap().is_secret());
    assert!(!editor.minibuffer().unwrap().completes());
    press(&mut editor, "s 3 c r 3 t RET");

    assert_eq!(
        editor
            .plugin_state::<Answers>()
            .map(|answers| answers.0.clone()),
        Some(vec!["s3cr3t".to_owned()])
    );
}

#[test]
fn buffers_can_keep_header_lines_above_their_text() {
    let mut editor = Editor::new();
    let id = editor.open_special_buffer("*table*", blank());
    let style = LineStyle {
        background: None,
        spans: vec![(0..4, Highlight::Heading)],
    };

    editor.set_header_lines(id, vec![("name".to_owned(), style.clone())]);
    assert_eq!(editor.pane().header_lines(), [("name".to_owned(), style)]);

    editor.set_header_lines(id, Vec::new());
    assert_eq!(editor.pane().header_lines(), []);
}

#[test]
fn windows_are_arranged_in_nested_splits_and_read_back() {
    use crate::editor::{Arrangement, Axis};

    let mut editor = Editor::new();
    let a = editor.open_special_buffer("*a*", blank());
    let b = editor.open_special_buffer("*b*", blank());
    let c = editor.open_special_buffer("*c*", blank());
    let layout = Arrangement::Split {
        axis: Axis::Columns,
        parts: vec![
            (0.25, Arrangement::Buffer(a)),
            (
                0.75,
                Arrangement::Split {
                    axis: Axis::Rows,
                    parts: vec![(0.5, Arrangement::Buffer(b)), (0.5, Arrangement::Buffer(c))],
                },
            ),
        ],
    };

    editor.arrange_windows(&layout, Some(c));

    assert_eq!(shown_in(&editor), ["*a*", "*b*", "*c*"]);
    assert_eq!(editor.buffer().name(), "*c*");
    assert_eq!(editor.window_arrangement(), layout);

    let scratch = editor.buffer_named("*scratch*").unwrap();
    editor.arrange_windows(&Arrangement::Buffer(scratch), None);
    assert_eq!(shown_in(&editor), ["*scratch*"]);
}

fn shown_in(editor: &Editor) -> Vec<String> {
    editor
        .tiling()
        .windows()
        .into_iter()
        .filter_map(|window| editor.pane_of(window))
        .map(|pane| pane.buffer().name().to_owned())
        .collect()
}

fn blank() -> Greeter {
    Greeter {
        scope: Scope::Global.into(),
        count: 0,
        styles: Vec::new(),
    }
}

#[test]
fn buffers_are_displayed_beside_or_under_another_one() {
    use crate::editor::Axis;

    let mut editor = Editor::new();
    let console = editor.open_special_buffer("*console*", blank());
    editor.show_buffer(editor.buffer_named("*scratch*").unwrap());
    editor.display_buffer(console, None, false);
    assert_eq!(shown_in(&editor), ["*scratch*", "*console*"]);
    assert_eq!(editor.buffer().name(), "*scratch*");

    let plots = editor.open_special_buffer("*plots*", blank());
    editor.show_buffer(editor.buffer_named("*scratch*").unwrap());
    editor.display_buffer(plots, Some((console, Axis::Rows)), false);
    assert_eq!(shown_in(&editor), ["*scratch*", "*console*", "*plots*"]);
    let crate::editor::Tiling::Split { parts, .. } = editor.tiling() else {
        panic!("not split");
    };
    assert!(matches!(
        &parts[1].1,
        crate::editor::Tiling::Split {
            axis: Axis::Rows,
            ..
        }
    ));

    editor.display_buffer(plots, None, true);
    assert_eq!(editor.buffer().name(), "*plots*");
    assert_eq!(editor.tiling().windows().len(), 3);
}

fn fetch_effects(editor: &mut Editor) -> Vec<(crate::editor::FetchId, String)> {
    editor
        .take_effects()
        .into_iter()
        .filter_map(|effect| match effect {
            crate::editor::Effect::Fetch(id, url) => Some((id, url)),
            _ => None,
        })
        .collect()
}

#[test]
fn plugins_fetch_urls_and_get_the_answer_once() {
    let mut editor = Editor::new();
    editor.set_plugin_state(Answers::default());

    editor.fetch("https://example.org/a", |editor, result| {
        if let Some(answers) = editor.plugin_state_mut::<Answers>() {
            answers.0.push(result.unwrap_or_else(|error| error));
        }
    });
    let fetches = fetch_effects(&mut editor);
    assert_eq!(fetches.len(), 1);
    assert_eq!(fetches[0].1, "https://example.org/a");

    editor.fetched(fetches[0].0, Ok("body".to_owned()));
    editor.fetched(fetches[0].0, Err("again".to_owned()));
    editor.fetched(crate::editor::FetchId(999), Ok("stray".to_owned()));

    assert_eq!(
        editor
            .plugin_state::<Answers>()
            .map(|answers| answers.0.clone()),
        Some(vec!["body".to_owned()])
    );
}

#[test]
fn plugins_add_mode_line_segments_in_order() {
    let mut editor = Editor::new();

    let counted = |editor: &Editor| Some(format!("{} buffers", editor.buffer_names().len()));
    editor.add_mode_line_segment(counted);
    editor.add_mode_line_segment(|_| None);
    editor.add_mode_line_segment(counted);

    assert_eq!(editor.mode_line_segments(), ["1 buffers", "1 buffers"]);
}

#[test]
fn config_changes_come_back_once_per_drain_when_subscribed() {
    let mut editor = Editor::new();
    editor.apply_config(&Config::default().with("tab-width", json!(2)).unwrap());
    assert_eq!(editor.take_events(), []);
    editor.subscribe(EventKind::Configured);

    editor.apply_config(&Config::default().with("tab-width", json!(2)).unwrap());
    editor.apply_config(&Config::default().with("tab-width", json!(3)).unwrap());

    assert_eq!(editor.take_events(), [Event::Configured]);
    assert_eq!(editor.tab_width(), 3);
}

#[derive(Debug)]
struct Shell {
    scope: ScopeId,
    input: InputLine,
}

impl SpecialBuffer for Shell {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn input_line(&mut self) -> Option<&mut InputLine> {
        Some(&mut self.input)
    }
}

fn send(editor: &mut Editor) {
    let id = editor.buffer().id();
    let Some((buffer, shell)) = editor.special_buffer_mut::<Shell>(id) else {
        return;
    };
    let text = shell.input.text(buffer).unwrap_or_default();
    shell.input.remember(&text);
    shell.input.replace(buffer, "");
    note(editor, text);
}

#[test]
fn plugin_buffers_can_take_input_after_a_prompt_with_history() {
    let mut editor = Editor::new();
    editor.set_plugin_state(Answers::default());
    let scope = editor.add_scope("*Shell*");
    let command = editor.add_command(CommandSpec {
        name: "shell-send",
        summary: "Send the input",
        help: None,
        scope,
        run: send,
    });
    editor.bind(scope, "RET", command).unwrap();
    editor
        .bind(scope, "M-p", crate::command::Command::ComintPreviousInput)
        .unwrap();
    let id = editor.add_special_buffer_with("*Shell*", |buffer| {
        buffer.insert_at(0, "> ");
        Shell {
            scope,
            input: InputLine::new(buffer),
        }
    });
    assert_eq!(editor.buffer().name(), "*scratch*");
    editor.show_buffer(id);

    press(&mut editor, "h i RET");
    assert_eq!(editor.buffer().contents(), "> ");
    press(&mut editor, "C-a x");
    assert_eq!(editor.buffer().contents(), "> x");
    press(&mut editor, "DEL DEL");
    assert_eq!(editor.buffer().contents(), "> ");
    press(&mut editor, "M-p");
    assert_eq!(editor.buffer().contents(), "> hi");

    assert_eq!(
        editor
            .plugin_state::<Answers>()
            .map(|answers| answers.0.clone()),
        Some(vec!["hi".to_owned()])
    );
}

fn count_repeats(editor: &mut Editor) {
    let repeated = editor.repeated();
    note(editor, repeated.to_string());
}

#[test]
fn plugins_add_commands_made_at_run_time_and_find_commands_by_name() {
    let mut editor = Editor::new();
    editor.set_plugin_state(Answers::default());
    let id = editor.add_dynamic_command(crate::command::DynamicSpec {
        name: "note-hi".to_owned(),
        summary: "Note hi".to_owned(),
        scope: Scope::Global.into(),
        run: Arc::new(|editor: &mut Editor| note(editor, "hi".to_owned())),
    });
    editor.bind(Scope::Global, "C-c h", id).unwrap();

    press(&mut editor, "C-c h M-x n o t e - h i RET");
    assert_eq!(
        editor.command_named("next-line"),
        Some(crate::command::Command::NextLine.into())
    );
    assert_eq!(editor.command_named("note-hi"), Some(id));
    assert!(editor.remove_dynamic_command("note-hi"));
    press(&mut editor, "C-c h");

    assert_eq!(editor.command_named("note-hi"), None);
    assert_eq!(
        editor
            .plugin_state::<Answers>()
            .map(|answers| answers.0.clone()),
        Some(vec!["hi".to_owned(), "hi".to_owned()])
    );
}

#[test]
fn plugin_commands_know_when_they_run_again_right_away() {
    let mut editor = Editor::new();
    editor.set_plugin_state(Answers::default());
    let command = editor.add_command(CommandSpec {
        name: "count-repeats",
        summary: "Note whether this repeats",
        help: None,
        scope: Scope::Global.into(),
        run: count_repeats,
    });
    editor.bind(Scope::Global, "C-c r", command).unwrap();

    press(&mut editor, "C-c r C-c r C-f C-c r");

    assert_eq!(
        editor
            .plugin_state::<Answers>()
            .map(|answers| answers.0.clone()),
        Some(vec![
            "false".to_owned(),
            "true".to_owned(),
            "false".to_owned()
        ])
    );
}

#[derive(Debug)]
struct Board(ScopeId);

impl SpecialBuffer for Board {
    fn scope(&self) -> ScopeId {
        self.0
    }
}

#[test]
fn plugins_see_where_their_buffers_show() {
    let mut editor = Editor::new();
    let scope = editor.add_scope("*Board*");
    let id = editor.add_special_buffer_with("*Board*", |buffer| {
        buffer.insert_at(0, "one\ntwo\nthree");
        Board(scope)
    });
    assert!(!editor.buffer_shown(id));
    assert_eq!(editor.buffer_width(id), None);

    editor.show_buffer_beside(id);

    assert!(editor.buffer_shown(id));
    assert_eq!(editor.buffer().id(), id);
    assert_eq!(editor.tiling().windows().len(), 2);
    let window = editor.selected_window();
    editor.resize_pane(window, 10, 33);
    assert_eq!(editor.buffer_width(id), Some(32));
    editor.move_points(id, |_, end| Some(end));
    assert_eq!(editor.buffer().point(), 13);
}

#[test]
fn saves_come_back_as_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plan.txt");
    std::fs::write(&path, "one\n").unwrap();
    let mut editor = Editor::new();
    editor.subscribe(EventKind::Saved);
    let shout = editor.add_command(CommandSpec {
        name: "shout",
        summary: "Shout",
        help: None,
        scope: Scope::Global.into(),
        run: |editor| editor.show_message("hey"),
    });
    editor.bind(Scope::Global, "C-c s", shout).unwrap();
    editor.open_file(&path);
    let id = editor.buffer().id();

    press(&mut editor, "C-e x C-c s C-x C-s");

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "onex\n");
    assert_eq!(editor.take_events(), [Event::Saved(id)]);
}

#[test]
fn plugins_reach_files_by_path_and_save_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plan.txt");
    std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
    let mut editor = Editor::new();
    assert_eq!(editor.buffer_visiting(&path), None);

    editor.open_file(&path);
    let id = editor.buffer_visiting(&path).unwrap();
    assert!(editor.buffer_ids().contains(&id));
    let start = editor.current_location();
    editor.go_to_line(3, 2);
    editor.remember_location(start);
    if let Some(buffer) = editor.buffer_by_id_mut(id) {
        buffer.insert_at(0, "zero\n");
    }

    assert!(editor.save_buffer(id));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "zero\none\ntwo\nthree\n"
    );
    assert_eq!(editor.buffer().point(), 14);
    press(&mut editor, "M-,");
    assert_eq!(editor.buffer().point(), 0);
}

#[derive(Debug)]
struct Folder(ScopeId, std::path::PathBuf);

impl SpecialBuffer for Folder {
    fn scope(&self) -> ScopeId {
        self.0
    }

    fn directory(&self) -> Option<&Path> {
        Some(&self.1)
    }
}

#[test]
fn plugin_buffers_can_show_a_directory() {
    let dir = tempfile::tempdir().unwrap();
    let docs = dir.path().join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    let mut editor = Editor::new();
    let scope = editor.add_scope("*Folder*");

    let id = editor.add_special_buffer_with("*Folder*", |_| Folder(scope, docs.clone()));
    editor.show_buffer(id);

    assert_eq!(editor.default_directory(), docs);
    assert_eq!(
        editor.buffer().name(),
        format!("docs{}", crate::files::paths::SEPARATOR)
    );
    assert!(editor.watched_directories().contains(&docs));
}

#[derive(Debug)]
struct Repository(ScopeId, std::path::PathBuf, std::path::PathBuf);

impl SpecialBuffer for Repository {
    fn scope(&self) -> ScopeId {
        self.0
    }

    fn directory(&self) -> Option<&Path> {
        Some(&self.1)
    }

    fn fixed_name(&self) -> bool {
        true
    }

    fn watched(&self) -> Option<&Path> {
        Some(&self.2)
    }
}

#[test]
fn plugin_buffers_can_keep_their_name_and_watch_another_directory() {
    let dir = tempfile::tempdir().unwrap();
    let metadata = dir.path().join(".git");
    std::fs::create_dir_all(&metadata).unwrap();
    let mut editor = Editor::new();
    let scope = editor.add_scope("*Repository*");

    let id = editor.add_special_buffer_with("git: project", |_| {
        Repository(scope, dir.path().to_owned(), metadata.clone())
    });
    editor.show_buffer(id);

    assert_eq!(editor.buffer().name(), "git: project");
    assert_eq!(editor.default_directory(), dir.path());
    assert_eq!(
        editor.watched_directories(),
        [dir.path().to_owned(), metadata]
    );
}

#[test]
fn disk_changes_and_claimed_directories_come_back_as_events() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = Editor::new();
    editor.subscribe(EventKind::DiskChanged);
    editor.claim_directories("explorer");

    editor.check_disk_changes();
    editor.check_disk_changes();
    assert!(editor.open_file(dir.path()));

    assert_eq!(
        editor.take_events(),
        [
            Event::DiskChanged,
            Event::OpenDirectory {
                path: dir.path().to_owned(),
                owner: "explorer".to_owned()
            }
        ]
    );
}

#[test]
fn files_of_a_claimed_type_come_back_instead_of_opening() {
    let dir = tempfile::tempdir().unwrap();
    let book = dir.path().join("book.EPUB");
    std::fs::write(&book, [0x50, 0x4b, 3, 4]).unwrap();
    std::fs::write(dir.path().join("notes.txt"), "hello").unwrap();
    let mut editor = Editor::new();
    editor.claim_files("epub", "epub");

    assert!(editor.open_file(&book));
    assert_eq!(editor.buffer().name(), "*scratch*");
    assert_eq!(
        editor.take_events(),
        [Event::OpenFile {
            path: book,
            owner: "epub".to_owned()
        }]
    );

    assert!(editor.open_file(&dir.path().join("notes.txt")));
    assert_eq!(editor.buffer().contents(), "hello");
}

#[test]
fn path_prompts_complete_names_and_answer_with_a_path() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("notes.txt"), "").unwrap();
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    let mut editor = Editor::new();
    editor.set_working_directory(dir.path().to_owned());
    editor.set_plugin_state(Answers::default());

    editor.prompt_path("Copy to: ", false, |editor, path| {
        note(editor, path.display().to_string());
    });
    assert_eq!(editor.minibuffer().unwrap().prompt(), "Copy to: ");
    assert_eq!(editor.completions().len(), 2);
    press(&mut editor, "n o t RET");

    editor.prompt_path("Directory: ", true, |editor, path| {
        note(editor, path.display().to_string());
    });
    assert_eq!(editor.completions().len(), 1);
    press(&mut editor, "s u b / RET");

    assert_eq!(
        editor
            .plugin_state::<Answers>()
            .map(|answers| answers.0.clone()),
        Some(vec![
            dir.path().join("notes.txt").display().to_string(),
            dir.path().join("sub").display().to_string(),
        ])
    );
}

#[test]
fn plugins_ask_typed_questions() {
    let mut editor = Editor::new();
    editor.set_plugin_state(Answers::default());

    editor.ask_typed("Delete it?", |editor, yes| note(editor, yes.to_string()));
    assert_eq!(
        editor.question().map(Question::prompt),
        Some("Delete it? (yes or no) ")
    );
    press(&mut editor, "y RET");
    assert_eq!(editor.message(), Some("Please answer yes or no"));
    press(&mut editor, "y e s RET");

    assert_eq!(
        editor
            .plugin_state::<Answers>()
            .map(|answers| answers.0.clone()),
        Some(vec!["true".to_owned()])
    );
}

#[derive(Debug)]
struct Viewer {
    scope: ScopeId,
    file: std::path::PathBuf,
}

impl SpecialBuffer for Viewer {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn position(&self) -> Option<String> {
        Some("2/9".to_owned())
    }

    fn pills(&self, line: usize) -> Vec<(Range<usize>, Highlight)> {
        if line == 0 {
            vec![(2..8, Highlight::Link)]
        } else {
            Vec::new()
        }
    }

    fn file(&self) -> Option<&Path> {
        Some(&self.file)
    }
}

#[test]
fn plugin_buffers_show_their_own_position_and_watch_their_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("paper.pdf");
    let mut editor = Editor::new();
    let scope = editor.add_scope("viewer");
    let id = editor.add_special_buffer_with("paper.pdf", |_| Viewer {
        scope,
        file: file.clone(),
    });
    editor.show_buffer(id);

    assert_eq!(editor.pane().status().position.as_deref(), Some("2/9"));
    assert_eq!(editor.pane().line_pills(0), [(2..8, Highlight::Link)]);
    assert_eq!(editor.pane().line_pills(1), []);
    assert_eq!(editor.visited_files(), [file]);
}

#[test]
fn visiting_a_file_a_plugin_buffer_shows_goes_back_to_that_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("paper.pdf");
    std::fs::write(&file, "%PDF").unwrap();
    let mut editor = Editor::new();
    editor.claim_files("pdf", "pdf");
    let scope = editor.add_scope("viewer");
    let id = editor.add_special_buffer_with("paper.pdf", |_| Viewer {
        scope,
        file: file.clone(),
    });

    assert!(editor.open_file(&dir.path().join(".").join("paper.pdf")));

    assert_eq!(editor.buffer().id(), id);
    assert_eq!(editor.take_events(), []);
    assert_eq!(editor.buffer_showing(&file), Some(id));
}

#[test]
fn plugin_buffers_showing_files_of_the_same_name_are_told_apart() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = Editor::new();
    let scope = editor.add_scope("viewer");
    for sub in ["x", "y"] {
        std::fs::create_dir(dir.path().join(sub)).unwrap();
        let file = dir.path().join(sub).join("paper.pdf");
        let id = editor.add_special_buffer_with("paper.pdf", |_| Viewer { scope, file });
        editor.show_buffer(id);
    }

    assert_eq!(
        editor.buffer_names(),
        ["paper.pdf<y>", "paper.pdf<x>", "*scratch*"]
    );
}

#[test]
fn moved_files_keep_their_buffers() {
    let dir = tempfile::tempdir().unwrap();
    let from = dir.path().join("a.txt");
    let to = dir.path().join("c.txt");
    std::fs::write(&from, "hello").unwrap();
    let mut editor = Editor::new();
    editor.open_file(&from);
    std::fs::rename(&from, &to).unwrap();

    editor.file_moved(&from, &to);

    assert_eq!(editor.buffer().name(), "c.txt");
    assert_eq!(editor.buffer_visiting(&to), Some(editor.buffer().id()));
    assert_eq!(
        editor.absolute(Path::new("x")),
        editor.default_directory().join("x")
    );
}

#[test]
fn g_bound_to_revert_buffer_in_a_plugin_buffer_does_not_loop() {
    let mut editor = Editor::new();
    let scope = editor.add_scope("*Board*");
    editor
        .bind(scope, "g", crate::command::Command::RevertBuffer)
        .unwrap();
    let id = editor.add_special_buffer_with("*Board*", |_| Board(scope));
    editor.show_buffer(id);

    editor.execute(crate::command::Command::RevertBuffer);

    assert_eq!(editor.buffer().name(), "*Board*");
}
#[test]
fn reverting_a_plugin_buffer_runs_what_g_runs() {
    let mut editor = Editor::new();
    editor.set_plugin_state(Answers::default());
    let scope = editor.add_scope("*Board*");
    let refresh = editor.add_command(CommandSpec {
        name: "board-refresh",
        summary: "Refresh",
        help: None,
        scope,
        run: |editor| note(editor, "refreshed".to_owned()),
    });
    editor.bind(scope, "g", refresh).unwrap();
    let id = editor.add_special_buffer_with("*Board*", |_| Board(scope));
    editor.show_buffer(id);

    editor.execute(crate::command::Command::RevertBuffer);

    assert_eq!(
        editor
            .plugin_state::<Answers>()
            .map(|answers| answers.0.clone()),
        Some(vec!["refreshed".to_owned()])
    );
}

#[test]
fn double_clicks_in_plugin_buffers_run_what_return_runs() {
    let mut editor = Editor::new();
    editor.set_plugin_state(Answers::default());
    let scope = editor.add_scope("*Board*");
    let pick = editor.add_command(CommandSpec {
        name: "board-pick",
        summary: "Pick",
        help: None,
        scope,
        run: |editor| {
            let line = editor.buffer().line_of(editor.buffer().point());
            note(editor, format!("line {line}"));
        },
    });
    editor.bind(scope, "RET", pick).unwrap();
    let id = editor.add_special_buffer_with("*Board*", |buffer| {
        buffer.insert_at(0, "one\ntwo\nthree");
        Board(scope)
    });
    editor.show_buffer(id);
    let window = editor.selected_window();
    editor.resize_pane(window, 10, 40);

    editor.click(
        crate::editor::Cell { row: 1, column: 0 },
        crate::editor::Clicks::Double,
        false,
    );

    assert_eq!(
        editor
            .plugin_state::<Answers>()
            .map(|answers| answers.0.clone()),
        Some(vec!["line 1".to_owned()])
    );
}

#[test]
fn plugins_open_links_outside() {
    let mut editor = Editor::new();
    editor.open_externally("https://example.org");
    assert_eq!(
        editor.take_effects(),
        [crate::editor::Effect::OpenExternally(
            "https://example.org".into()
        )]
    );
}

#[test]
fn pick_prompts_say_which_offer_was_chosen_or_what_was_typed() {
    fn ask(editor: &mut Editor) {
        editor.prompt_pick(
            "Note: ",
            |_| {
                vec![
                    Offer::noted("Inbox", "personal"),
                    Offer::noted("Inbox", "work"),
                ]
            },
            |editor, picked| note(editor, format!("{picked:?}")),
        );
    }
    let mut editor = Editor::new();
    editor.set_plugin_state(Answers::default());

    ask(&mut editor);
    press(&mut editor, "C-n RET");
    ask(&mut editor);
    press(&mut editor, "G a r d e n RET");
    ask(&mut editor);
    press(&mut editor, "I n b M-RET");

    assert_eq!(
        editor
            .plugin_state::<Answers>()
            .map(|answers| answers.0.clone()),
        Some(vec![
            "Offered(1)".to_owned(),
            "Typed(\"Garden\")".to_owned(),
            "Typed(\"Inb\")".to_owned(),
        ])
    );
}

#[test]
fn m_ret_answers_a_prompt_with_what_was_typed() {
    let mut editor = Editor::new();
    editor.set_plugin_state(Answers::default());
    editor.prompt_with("Server: ", |_| vec![Offer::from("libera")], note);

    press(&mut editor, "l i M-RET");

    assert_eq!(
        editor
            .plugin_state::<Answers>()
            .map(|answers| answers.0.clone()),
        Some(vec!["li".to_owned()])
    );
}

#[test]
fn styled_prompts_color_what_is_typed() {
    let mut editor = Editor::new();
    assert_eq!(editor.minibuffer_highlights(), []);

    editor.prompt_styled(
        "Capture: ",
        |_, text| vec![(0..text.chars().count(), Highlight::Keyword)],
        |_, _| {},
    );
    press(&mut editor, "a b");

    assert_eq!(editor.minibuffer_highlights(), [(0..2, Highlight::Keyword)]);
    press(&mut editor, "C-g");
    assert_eq!(editor.minibuffer_highlights(), []);
}

#[test]
fn plugins_take_their_state_out_until_they_set_it_again() {
    let mut editor = Editor::new();
    editor.set_plugin_state(Count(1));

    let taken = editor.take_plugin_state::<Count>();

    assert_eq!(taken.map(|count| count.0), Some(1));
    assert!(editor.plugin_state::<Count>().is_none());
    assert!(editor.take_plugin_state::<Count>().is_none());
}

#[test]
fn plugins_find_list_and_close_their_buffers() {
    let mut editor = Editor::new();
    let scope = editor.add_scope("*Board*");
    let first = editor.add_special_buffer_with("*Board one*", |_| Board(scope));
    let second = editor.add_special_buffer_with("*Board two*", |_| Board(scope));

    assert_eq!(editor.special_buffers::<Board>(), [first, second]);
    assert_eq!(editor.buffer_named("*Board two*"), Some(second));
    assert_eq!(editor.buffer_named("*Board three*"), None);

    editor.show_buffer(first);
    editor.buffer_mut().insert_at(0, "hello");
    assert_eq!(editor.buffer().contents(), "hello");
    editor.close_buffer(first);

    assert_eq!(editor.special_buffers::<Board>(), [second]);
    assert_ne!(editor.buffer().id(), first);
}

#[test]
fn state_is_kept_per_type() {
    let mut states = States::default();
    states.set(Count(3));
    states.set(String::from("text"));

    assert_eq!(states.get::<Count>().map(|count| count.0), Some(3));
    assert_eq!(states.get::<String>().map(String::as_str), Some("text"));
    assert!(states.get::<u8>().is_none());
}

#[test]
fn buffers_are_found_and_diagnosed_through_any_spelling_of_their_file() {
    use crate::external::diagnostics::{Diagnostic, Severity};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, "one\ntwo\n").unwrap();
    let mut editor = Editor::new();
    editor.open_file(&path);
    let other = dir.path().join(".").join("notes.txt");

    assert_eq!(editor.buffer_visiting(&other), Some(editor.buffer().id()));
    editor.replace_diagnostics(
        &other,
        vec![Diagnostic {
            path: other.clone(),
            start: (2, 1),
            end: (2, 4),
            severity: Severity::Warning,
            message: "two".to_owned(),
        }],
    );
    let identity = editor.buffer().file().unwrap().identity().to_path_buf();
    assert_eq!(editor.lint_status(&identity).as_deref(), Some("W1"));
    assert_eq!(
        editor
            .pane()
            .line_decorations(1, std::time::Instant::now())
            .waves
            .len(),
        1
    );
    assert_eq!(editor.diagnostic_counts()[0].0, other);
}

#[test]
fn moving_a_visited_file_follows_it_into_its_new_language() {
    let dir = tempfile::tempdir().unwrap();
    let from = dir.path().join("notes.txt");
    std::fs::write(&from, "fn main() {}\n").unwrap();
    let to = dir.path().join("notes.rs");
    let mut editor = Editor::new();
    editor.open_file(&from);
    std::fs::rename(&from, &to).unwrap();

    editor.file_moved(&from, &to);

    let id = editor.buffer().id();
    assert_eq!(editor.buffer().file().unwrap().path, to);
    assert_eq!(
        editor
            .buffer_language(id)
            .map(crate::syntax::Language::name),
        Some(crate::syntax::Language::RUST.name())
    );
}
