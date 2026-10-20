mod browse;
#[cfg(test)]
mod fixtures;
mod format;
mod listing;
mod marks;
mod operations;
mod ops;
mod transfer;

use oxmacs_core::applet::Applet;
use oxmacs_core::command::{Command, CommandId, CommandSpec, Scope, ScopeId};
use oxmacs_core::config::schema::{Kind as SettingKind, Setting};
use oxmacs_core::editor::Editor;
use oxmacs_gui::applet::GuiApplet;
use serde::{Deserialize, Serialize};

pub use listing::{Listing, Mark};

use browse::DiredState;
use transfer::Kind;

pub const TABLE: &str = "dired";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DiredSettings {
    pub delete_to_trash: bool,
}

impl Default for DiredSettings {
    fn default() -> Self {
        Self {
            delete_to_trash: true,
        }
    }
}

const SETTINGS: &[Setting] = &[Setting {
    key: "delete-to-trash",
    group: "Dired",
    kind: SettingKind::Toggle,
    unset: None,
    hint: "Dired deletes to the trash instead of permanently",
}];

pub(crate) fn settings(editor: &Editor) -> DiredSettings {
    editor.applet_config(TABLE)
}

#[derive(Debug)]
pub struct Dired;

struct Spec {
    name: &'static str,
    summary: &'static str,
    help: Option<&'static str>,
    keys: &'static [&'static str],
    run: fn(&mut Editor),
}

const GLOBAL: &[Spec] = &[
    Spec {
        name: "dired",
        summary: "List a directory to manage its files",
        help: Some(
            "In the listing, RET opens a file or directory, ^ goes up and g refreshes. Mark files with m and flag them for deletion with d; x deletes the flagged files, D deletes the marked ones, R renames or moves, C copies, + creates a directory and W opens a file with its usual app. s changes the sort and ( hides the details. Deleted files go to the trash unless delete-to-trash is off.",
        ),
        keys: &["C-x d"],
        run: browse::prompt,
    },
    Spec {
        name: "dired-jump",
        summary: "List the directory of the current file",
        help: None,
        keys: &["C-x C-j"],
        run: browse::jump,
    },
];

const LISTINGS: &[Spec] = &[
    Spec {
        name: "dired-next-line",
        summary: "Move to the next file",
        help: None,
        keys: &["n", "SPC", "C-n", "<down>"],
        run: |editor| browse::next_line(editor, 1),
    },
    Spec {
        name: "dired-previous-line",
        summary: "Move to the previous file",
        help: None,
        keys: &["p", "C-p", "<up>"],
        run: |editor| browse::next_line(editor, -1),
    },
    Spec {
        name: "dired-find-file",
        summary: "Open the file or directory on this line",
        help: None,
        keys: &["RET", "f", "e"],
        run: browse::find_file,
    },
    Spec {
        name: "dired-up-directory",
        summary: "Go up to the parent directory",
        help: None,
        keys: &["^"],
        run: browse::up_directory,
    },
    Spec {
        name: "dired-revert",
        summary: "Read the directory again",
        help: None,
        keys: &["g"],
        run: browse::revert,
    },
    Spec {
        name: "dired-cycle-sort",
        summary: "Sort by name, newest or largest",
        help: None,
        keys: &["s"],
        run: browse::cycle_sort,
    },
    Spec {
        name: "dired-hide-details-mode",
        summary: "Show or hide sizes and dates",
        help: None,
        keys: &["("],
        run: browse::hide_details,
    },
    Spec {
        name: "dired-mark",
        summary: "Mark this file and move on",
        help: None,
        keys: &["m"],
        run: |editor| marks::mark(editor, Some(Mark::Marked)),
    },
    Spec {
        name: "dired-flag-file-deletion",
        summary: "Flag this file for deletion and move on",
        help: None,
        keys: &["d"],
        run: |editor| marks::mark(editor, Some(Mark::Flagged)),
    },
    Spec {
        name: "dired-unmark",
        summary: "Remove this file's mark and move on",
        help: None,
        keys: &["u"],
        run: |editor| marks::mark(editor, None),
    },
    Spec {
        name: "dired-unmark-backward",
        summary: "Move back and remove that file's mark",
        help: None,
        keys: &["DEL"],
        run: marks::unmark_backward,
    },
    Spec {
        name: "dired-unmark-all-marks",
        summary: "Remove every mark and deletion flag",
        help: None,
        keys: &["U"],
        run: marks::unmark_all,
    },
    Spec {
        name: "dired-toggle-marks",
        summary: "Mark the unmarked files and unmark the marked ones",
        help: None,
        keys: &["t"],
        run: marks::toggle,
    },
    Spec {
        name: "dired-do-flagged-delete",
        summary: "Delete the files flagged for deletion",
        help: None,
        keys: &["x"],
        run: |editor| operations::delete(editor, true),
    },
    Spec {
        name: "dired-do-delete",
        summary: "Delete the marked files, or this one",
        help: None,
        keys: &["D"],
        run: |editor| operations::delete(editor, false),
    },
    Spec {
        name: "dired-do-rename",
        summary: "Rename or move the marked files, or this one",
        help: None,
        keys: &["R"],
        run: |editor| transfer::start(editor, Kind::Move),
    },
    Spec {
        name: "dired-do-copy",
        summary: "Copy the marked files, or this one",
        help: None,
        keys: &["C"],
        run: |editor| transfer::start(editor, Kind::Copy),
    },
    Spec {
        name: "dired-create-directory",
        summary: "Create a new directory",
        help: None,
        keys: &["+"],
        run: operations::create_directory,
    },
    Spec {
        name: "dired-do-open",
        summary: "Open the file with its usual app",
        help: None,
        keys: &["W"],
        run: operations::open,
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

impl Applet for Dired {
    fn name(&self) -> &'static str {
        "dired"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn install(&self, editor: &mut Editor) {
        let scope = editor.add_scope("dired");
        editor.set_applet_state(DiredState { scope });
        let global = register(editor, Scope::Global.into(), GLOBAL);
        register(editor, scope, LISTINGS);
        editor.bind(scope, "q", Command::QuitWindow).ok();
        editor.add_config_table::<DiredSettings>(TABLE);
        editor.add_settings(TABLE, SETTINGS);
        editor.on_open_directory(browse::dired);
        editor.on_disk_change(browse::refresh_changed);
        if let Some(dired) = global.first() {
            editor.set_splash_entry(*dired, "Browse a directory");
        }
    }
}

impl GuiApplet for Dired {}

#[cfg(test)]
pub(crate) mod testing {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::input::KeySequence;

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

    pub(crate) fn at_point(editor: &Editor) -> String {
        let buffer = editor.buffer();
        let line = buffer.line_of(buffer.point());
        buffer.text_in(buffer.point()..buffer.line_end(line))
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
    use oxmacs_core::editor::Editor;
    use oxmacs_core::text::Buffer;

    use crate::Dired;
    use crate::testing::{lines, press, type_text};

    fn installed() -> Editor {
        let mut editor = Editor::new();
        editor.install(&Dired);
        editor
    }

    fn line_of(lines: &[String], key: &str) -> usize {
        lines
            .iter()
            .position(|line| line.trim_start().starts_with(key))
            .unwrap()
    }

    #[test]
    fn the_start_screen_offers_dired_before_save_and_quit() {
        let mut editor = installed();
        editor.about();

        let lines = lines(&editor);
        let browse = line_of(&lines, "C-x d");
        assert!(lines[browse].contains("dired"));
        assert!(lines[browse].ends_with("Browse a directory"));
        assert!(browse < line_of(&lines, "C-x C-s"));
    }

    #[test]
    fn help_names_the_keys_used_in_listings() {
        let mut editor = installed();

        press(&mut editor, "C-h f");
        type_text(&mut editor, "dired-next-line");
        press(&mut editor, "RET");

        let help = editor
            .buffer_ids()
            .into_iter()
            .filter_map(|id| editor.buffer_by_id(id))
            .find(|buffer| buffer.name() == "*Help*")
            .map(Buffer::contents)
            .unwrap();
        assert!(
            help.contains("Keys      n, SPC, C-n or <down> in dired"),
            "{help}"
        );
    }
}
