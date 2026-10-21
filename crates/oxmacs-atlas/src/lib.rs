mod agenda;
mod calendar;
mod links;
mod lists;
pub mod model;
mod notes;
pub mod settings;
mod vaults;

use oxmacs_core::applet::{Applet, DoctorLine, Finding};
use oxmacs_core::command::{Command, CommandId, CommandSpec, Scope, ScopeId};
use oxmacs_core::editor::Editor;
use oxmacs_core::syntax::{Feature, Indentation, Language, LanguageSpec, Outline};
use oxmacs_gui::applet::GuiApplet;

use notes::{AtlasState, Notebook, notebook};

static SPEC: LanguageSpec = LanguageSpec {
    name: "Atlas",
    extensions: &[model::vault::EXTENSION],
    file_names: &[],
    aliases: &[],
    line_comment: None,
    pairs: &[],
    indentation: Indentation::KEEP,
    grammar: None,
    grammars: &[],
    outline: Outline::NONE,
    features: &[Feature::Highlighting],
};

pub const LANGUAGE: Language = Language::new(&SPEC);

#[derive(Debug)]
pub struct Atlas;

struct Spec {
    name: &'static str,
    summary: &'static str,
    help: Option<&'static str>,
    keys: &'static [&'static str],
    run: fn(&mut Editor),
}

const GLOBAL: &[Spec] = &[
    Spec {
        name: "atlas-find-note",
        summary: "Open an Atlas note by title, or start a new one",
        help: Some(
            "Lists the notes of every Atlas vault by title. RET opens the selected note; a title nothing matches, or M-RET, starts a new note with that title in your first vault. Offers to create a vault the first time.",
        ),
        keys: &["C-c n f"],
        run: |editor| {
            notebook(editor, Notebook::find_note);
        },
    },
    Spec {
        name: "atlas-today",
        summary: "Open today's Atlas journal page",
        help: Some(
            "Opens journal/YYYY-MM-DD.atlas in your first vault, creating it with today's date as its heading.",
        ),
        keys: &["C-c n t"],
        run: |editor| {
            notebook(editor, Notebook::today_page);
        },
    },
    Spec {
        name: "atlas-capture",
        summary: "Capture a line into today's Atlas journal",
        help: Some(
            "Asks for a line and adds it to today's journal page without leaving what you are doing: tasks such as [ ] Buy seeds due:fri keep their form with exact dates, anything else is stamped with the time.",
        ),
        keys: &["C-c n c"],
        run: |editor| {
            notebook(editor, Notebook::capture);
        },
    },
    Spec {
        name: "atlas-backlinks",
        summary: "List the Atlas notes that link to this one",
        help: Some(
            "Opens *Backlinks* beside the note: each note that links here, with the lines that hold the link. RET visits a line, g refreshes and q quits.",
        ),
        keys: &["C-c n b"],
        run: |editor| {
            notebook(editor, Notebook::backlinks);
        },
    },
    Spec {
        name: "atlas-agenda",
        summary: "Show the Atlas agenda across vaults",
        help: Some(
            "Opens *Agenda*: overdue work, today's events and tasks, each of the next seven days, then tasks waiting on others, from every vault with its name in its color. RET visits a task, t cycles it, s plans it and d sets its due date (fri, tomorrow, +3d), each written back to its note; v filters by vault, g refreshes and q quits.",
        ),
        keys: &["C-c n a"],
        run: |editor| {
            notebook(editor, Notebook::agenda);
        },
    },
    Spec {
        name: "atlas-calendar",
        summary: "Show the Atlas calendar",
        help: Some(
            "Opens *Calendar* on this month: each day with its events and tasks from every vault, and the selected day listed below. The arrows move the day, n and p the month, w switches to the week by hour, . returns to today and v filters by vault; RET steps into the day's list and visits an entry.",
        ),
        keys: &["C-c n m"],
        run: |editor| {
            notebook(editor, Notebook::calendar);
        },
    },
];

const NOTES: &[Spec] = &[
    Spec {
        name: "atlas-cycle-task",
        summary: "Cycle the task on this line: todo, doing, done",
        help: Some(
            "Turns [ ] into [/] and [/] into [x], stamping done: with today's date; done, waiting and dropped tasks go back to [ ]. A task that repeats with every: is moved to its next date instead, with a done line logged under it.",
        ),
        keys: &["C-c C-c"],
        run: |editor| {
            notebook(editor, Notebook::cycle_task);
        },
    },
    Spec {
        name: "atlas-follow-link",
        summary: "Open the Atlas note named by the link at point",
        help: Some(
            "Follows a [Title] link to the note with that title in any vault, starting the note in the same vault when it does not exist yet. M-, comes back.",
        ),
        keys: &["C-c C-o"],
        run: |editor| {
            notebook(editor, Notebook::follow_link);
        },
    },
];

const LISTS: &[Spec] = &[
    Spec {
        name: "atlas-visit",
        summary: "Visit the note line on this row",
        help: None,
        keys: &["RET"],
        run: |editor| {
            notebook(editor, Notebook::visit);
        },
    },
    Spec {
        name: "atlas-agenda-cycle",
        summary: "Cycle the task on this row in its note",
        help: None,
        keys: &["t", "C-c C-c"],
        run: |editor| {
            notebook(editor, Notebook::agenda_cycle);
        },
    },
    Spec {
        name: "atlas-agenda-plan",
        summary: "Plan the task on this row for a day",
        help: None,
        keys: &["s"],
        run: |editor| {
            notebook(editor, |book| book.agenda_date(false));
        },
    },
    Spec {
        name: "atlas-agenda-due",
        summary: "Set the due date of the task on this row",
        help: None,
        keys: &["d"],
        run: |editor| {
            notebook(editor, |book| book.agenda_date(true));
        },
    },
    Spec {
        name: "atlas-filter-vault",
        summary: "Show the next vault, or all of them again",
        help: None,
        keys: &["v"],
        run: |editor| {
            notebook(editor, Notebook::filter_vault);
        },
    },
    Spec {
        name: "atlas-refresh",
        summary: "Draw this Atlas list again from the notes",
        help: None,
        keys: &["g"],
        run: |editor| {
            notebook(editor, Notebook::refresh_list);
        },
    },
];

const CALENDAR: &[Spec] = &[
    Spec {
        name: "atlas-calendar-back-day",
        summary: "Select the day before",
        help: None,
        keys: &["<left>"],
        run: |editor| {
            notebook(editor, |book| book.calendar_move(-1));
        },
    },
    Spec {
        name: "atlas-calendar-forward-day",
        summary: "Select the day after",
        help: None,
        keys: &["<right>"],
        run: |editor| {
            notebook(editor, |book| book.calendar_move(1));
        },
    },
    Spec {
        name: "atlas-calendar-back-week",
        summary: "Select the same day a week earlier",
        help: None,
        keys: &["<up>"],
        run: |editor| {
            notebook(editor, |book| book.calendar_move(-7));
        },
    },
    Spec {
        name: "atlas-calendar-forward-week",
        summary: "Select the same day a week later",
        help: None,
        keys: &["<down>"],
        run: |editor| {
            notebook(editor, |book| book.calendar_move(7));
        },
    },
    Spec {
        name: "atlas-calendar-next",
        summary: "Show the next month, or week",
        help: None,
        keys: &["n"],
        run: |editor| {
            notebook(editor, |book| book.calendar_month(true));
        },
    },
    Spec {
        name: "atlas-calendar-previous",
        summary: "Show the previous month, or week",
        help: None,
        keys: &["p"],
        run: |editor| {
            notebook(editor, |book| book.calendar_month(false));
        },
    },
    Spec {
        name: "atlas-calendar-week-view",
        summary: "Switch between the month and the week by hour",
        help: None,
        keys: &["w"],
        run: |editor| {
            notebook(editor, Notebook::calendar_week);
        },
    },
    Spec {
        name: "atlas-calendar-today",
        summary: "Select today",
        help: None,
        keys: &["."],
        run: |editor| {
            notebook(editor, Notebook::calendar_today);
        },
    },
    Spec {
        name: "atlas-calendar-select",
        summary: "Step into the selected day or visit the entry at point",
        help: None,
        keys: &["RET"],
        run: |editor| {
            notebook(editor, Notebook::calendar_select);
        },
    },
];

fn register(editor: &mut Editor, scope: ScopeId, specs: &[Spec]) -> Vec<CommandId> {
    let mut added = Vec::new();
    for spec in specs {
        let command = editor.add_command(CommandSpec {
            name: spec.name,
            summary: spec.summary,
            help: spec.help,
            scope,
            run: spec.run,
        });
        for keys in spec.keys {
            editor.bind(scope, keys, command).ok();
        }
        added.push(command);
    }
    added
}

fn find(specs: &[Spec], ids: &[CommandId], name: &str) -> Option<CommandId> {
    specs
        .iter()
        .position(|spec| spec.name == name)
        .and_then(|index| ids.get(index).copied())
}

fn doctor(editor: &Editor) -> Vec<DoctorLine> {
    let vaults = notes::vault_list(editor);
    if vaults.is_empty() {
        return vec![DoctorLine {
            label: "no vaults".to_owned(),
            finding: Finding::Info,
            text: "C-c n f offers to make one".to_owned(),
        }];
    }
    let index = editor
        .applet_state::<AtlasState>()
        .and_then(|state| state.index.as_ref());
    vaults
        .into_iter()
        .map(|(name, directory)| {
            let shown = directory.display().to_string();
            if !directory.is_dir() {
                return DoctorLine {
                    label: name,
                    finding: Finding::Problem,
                    text: format!("folder missing  {shown}"),
                };
            }
            let pages =
                index.map(|index| index.pages.iter().filter(|page| page.vault == name).count());
            let text = match pages {
                Some(1) => format!("1 page  {shown}"),
                Some(count) => format!("{count} pages  {shown}"),
                None => shown,
            };
            DoctorLine {
                label: name,
                finding: Finding::Good,
                text,
            }
        })
        .collect()
}

impl Applet for Atlas {
    fn name(&self) -> &'static str {
        "atlas"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn opt_in(&self) -> bool {
        true
    }

    fn install(&self, editor: &mut Editor) {
        let note_scope = editor.add_scope("Atlas notes");
        let list_scope = editor.add_scope("Atlas lists");
        let calendar_scope = editor.add_scope("*Calendar*");
        editor.set_applet_state(AtlasState::new(list_scope, calendar_scope));
        register(editor, Scope::Global.into(), GLOBAL);
        register(editor, note_scope, NOTES);
        let lists = register(editor, list_scope, LISTS);
        register(editor, calendar_scope, CALENDAR);
        for scope in [list_scope, calendar_scope] {
            editor.bind(scope, "q", Command::QuitWindow).ok();
        }
        editor.bind(list_scope, "n", Command::NextLine).ok();
        editor.bind(list_scope, "p", Command::PreviousLine).ok();
        for (name, keys) in [("atlas-filter-vault", "v"), ("atlas-refresh", "g")] {
            if let Some(command) = find(LISTS, &lists, name) {
                editor.bind(calendar_scope, keys, command).ok();
            }
        }
        editor.add_language(LANGUAGE, Some(note_scope));
        editor.set_highlighter(LANGUAGE, notes::highlight);
        editor.add_config_table::<settings::AtlasSettings>(settings::TABLE);
        editor.add_doctor_section("Atlas", doctor);
        editor.on_display(|editor| {
            notebook(editor, Notebook::sync_shown);
        });
        editor.on_command(|editor| {
            notebook(editor, Notebook::after_command);
        });
        editor.on_save(|editor, id| {
            notebook(editor, |book| book.exact_dates(id));
        });
        editor.on_saved(|editor, id| {
            notebook(editor, |book| book.saved(id));
        });
        editor.on_configured(|editor| {
            notebook(editor, Notebook::configured);
        });
    }
}

impl GuiApplet for Atlas {}

#[cfg(test)]
pub(crate) mod testing {
    use std::fmt::Write;
    use std::fs;
    use std::path::{Path, PathBuf};

    use jiff::civil::{date, time};
    use oxmacs_core::config::Config;
    use oxmacs_core::editor::Editor;
    use oxmacs_core::input::KeySequence;

    use crate::Atlas;
    use crate::notes::AtlasState;

    pub(crate) fn editor() -> Editor {
        let mut editor = Editor::new();
        editor.install(&Atlas);
        let state = editor.applet_state_mut::<AtlasState>().unwrap();
        state.today = || date(2026, 10, 19);
        state.time_now = || time(9, 30, 0, 0);
        editor
    }

    pub(crate) fn configured(text: &str) -> Editor {
        let mut editor = editor();
        editor.apply_config(&Config::parse(text).unwrap());
        editor
    }

    pub(crate) fn slashed(path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }

    pub(crate) fn vault_config(vaults: &[(&str, &Path)]) -> String {
        let mut text = String::new();
        for (name, path) in vaults {
            let _ = writeln!(
                text,
                "[[atlas.vault]]\nname = \"{name}\"\npath = \"{}\"",
                slashed(path)
            );
        }
        text
    }

    pub(crate) fn write(path: &Path, text: &str) -> PathBuf {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
        path.to_owned()
    }

    pub(crate) fn press(editor: &mut Editor, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
    }

    pub(crate) fn type_text(editor: &mut Editor, text: &str) {
        for character in text.chars() {
            let notation = match character {
                ' ' => "SPC".to_owned(),
                other => other.to_string(),
            };
            press(editor, &notation);
        }
    }

    pub(crate) fn lines(editor: &Editor) -> Vec<String> {
        editor
            .buffer()
            .contents()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    pub(crate) fn labels(editor: &Editor) -> Vec<String> {
        editor
            .completions()
            .into_iter()
            .map(|completion| completion.label)
            .collect()
    }

    pub(crate) fn question(editor: &Editor) -> Option<String> {
        editor
            .question()
            .map(|question| question.prompt().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oxmacs_core::config::Config;
    use oxmacs_core::editor::Editor;

    use crate::testing::{lines, press, slashed, type_text};

    fn line_with(editor: &Editor, text: &str) -> String {
        lines(editor)
            .into_iter()
            .find(|line| line.contains(text))
            .unwrap_or_else(|| panic!("no {text} in {:?}", lines(editor)))
    }

    fn doctor(editor: &mut Editor) {
        press(editor, "M-x");
        type_text(editor, "oxmacs-doctor");
        press(editor, "RET");
    }

    #[test]
    fn the_doctor_lists_vaults_with_pages_once_indexed() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = crate::testing::editor();
        doctor(&mut editor);
        assert!(line_with(&editor, "  no vaults").ends_with("C-c n f offers to make one"));

        let notes = dir.path().join("notes");
        fs::create_dir_all(&notes).unwrap();
        fs::write(notes.join("Garden plan.atlas"), "title: Garden plan\n---\n").unwrap();
        let text = format!(
            "[[atlas.vault]]\nname = \"personal\"\npath = \"{}\"\n[[atlas.vault]]\nname = \"work\"\npath = \"{}\"\n",
            slashed(&notes),
            slashed(&dir.path().join("gone"))
        );
        editor.apply_config(&Config::parse(&text).unwrap());
        press(&mut editor, "g");
        assert!(line_with(&editor, "  personal").ends_with(&format!("✓ {}", slashed(&notes))));
        assert!(line_with(&editor, "  work").contains("✗ folder missing"));

        press(&mut editor, "C-c n f C-g C-x b");
        type_text(&mut editor, "*oxmacs-doctor*");
        press(&mut editor, "RET g");
        assert!(line_with(&editor, "  personal").contains("✓ 1 page  "));
    }

    #[test]
    fn the_language_list_knows_the_atlas_language() {
        let mut editor = crate::testing::editor();

        press(&mut editor, "M-x");
        type_text(&mut editor, "list-languages");
        press(&mut editor, "RET");

        assert!(line_with(&editor, "Atlas (.atlas)").contains('✓'));
    }
}
