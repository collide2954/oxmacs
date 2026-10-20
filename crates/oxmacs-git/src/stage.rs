use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

use oxmacs_core::editor::Editor;
use oxmacs_core::text::BufferId;

use crate::diff;
use crate::run::{self, Plan, args};
use crate::status::{Row, Section};
use crate::view::{self, GitState, StatusView};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    Section(Section),
    Files(Section, Vec<String>),
    Hunk {
        section: Section,
        path: String,
        hunk: usize,
        lines: Option<Range<usize>>,
    },
}

pub(crate) fn target(editor: &Editor) -> Option<(BufferId, Result<Target, &'static str>)> {
    let (id, view) = view::current(editor)?;
    let buffer = editor.buffer();
    let rows = &view.rendered.rows;
    let path = |section, index| view.status.path(section, index).map(str::to_owned);
    let Some(selection) = buffer.selection().filter(|range| !range.is_empty()) else {
        let row = *rows.get(buffer.line_of(buffer.point()))?;
        let found = match row {
            Row::Section(section) => Ok(Target::Section(section)),
            Row::File(section, index) => Ok(Target::Files(section, vec![path(section, index)?])),
            Row::Hunk(section, index, hunk) | Row::Line(section, index, hunk, _) => {
                Ok(Target::Hunk {
                    section,
                    path: path(section, index)?,
                    hunk,
                    lines: None,
                })
            }
            Row::Info | Row::Blank | Row::Stash(_) | Row::Commit(_) => {
                Err("Nothing here to act on")
            }
        };
        return Some((id, found));
    };
    let first = buffer.line_of(selection.start);
    let last = buffer
        .line_of(selection.end - 1)
        .min(rows.len().saturating_sub(1));
    let picked = rows.get(first..=last)?;
    let found = match picked {
        [Row::Line(section, index, hunk, _), ..]
            if picked.iter().all(
                |row| matches!(row, Row::Line(s, i, h, _) if (s, i, h) == (section, index, hunk)),
            ) =>
        {
            let at = |row: &Row| match row {
                Row::Line(.., line) => *line,
                _ => 0,
            };
            Ok(Target::Hunk {
                section: *section,
                path: path(*section, *index)?,
                hunk: *hunk,
                lines: Some(at(&picked[0])..at(&picked[picked.len() - 1]) + 1),
            })
        }
        [Row::File(section, _), ..]
            if picked
                .iter()
                .all(|row| matches!(row, Row::File(s, _) if s == section)) =>
        {
            let paths = picked
                .iter()
                .filter_map(|row| match row {
                    Row::File(s, i) => path(*s, *i),
                    _ => None,
                })
                .collect();
            Ok(Target::Files(*section, paths))
        }
        _ => Err("Select lines of one hunk or files of one section"),
    };
    Some((id, found))
}

fn patch_file(editor: &mut Editor, metadata: &Path) -> PathBuf {
    let number = editor.applet_state_mut::<GitState>().map_or(0, |state| {
        state.patches += 1;
        state.patches
    });
    metadata.join(format!("oxmacs-{number}.patch"))
}

struct Prepared {
    root: PathBuf,
    patch: Option<PathBuf>,
    plan: Plan,
}

fn hunk_patch(
    editor: &mut Editor,
    id: BufferId,
    target: &Target,
    reverse: bool,
) -> Option<Prepared> {
    let Target::Hunk {
        section,
        path,
        hunk,
        lines,
    } = target
    else {
        return None;
    };
    let (_, view) = editor.applet_buffer::<StatusView>(id)?;
    let root = view.root.clone();
    let metadata = view.metadata.clone();
    let hunk = view.status.diff(*section, path)?.hunks.get(*hunk)?.clone();
    let text = diff::patch(path, &hunk, lines.as_ref(), reverse);
    let file = patch_file(editor, &metadata);
    if let Err(error) = fs::write(&file, text) {
        editor.show_message(format!("Could not write the patch: {error}"));
        return None;
    }
    Some(Prepared {
        root,
        patch: Some(file.clone()),
        plan: Plan::default().scratch(file),
    })
}

fn with_target(editor: &mut Editor, act: fn(&mut Editor, BufferId, Target)) {
    match target(editor) {
        Some((id, Ok(target))) => {
            editor.buffer_mut().deactivate_mark();
            act(editor, id, target);
        }
        Some((_, Err(reason))) => editor.show_message(reason),
        None => {}
    }
}

fn root_of(editor: &Editor, id: BufferId) -> Option<PathBuf> {
    editor
        .applet_buffer::<StatusView>(id)
        .map(|(_, view)| view.root.clone())
}

fn patch_path(prepared: &Prepared) -> String {
    prepared
        .patch
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_default()
}

pub(crate) fn stage(editor: &mut Editor) {
    with_target(editor, |editor, id, target| {
        stage_target(editor, id, &target);
    });
}

fn stage_target(editor: &mut Editor, id: BufferId, target: &Target) {
    let Some(root) = root_of(editor, id) else {
        return;
    };
    let plan = match target {
        Target::Section(Section::Untracked) => {
            let Some((_, view)) = editor.applet_buffer::<StatusView>(id) else {
                return;
            };
            Plan::default().step(args(&["add", "--"], &view.status.snapshot.untracked))
        }
        Target::Section(Section::Unstaged) => Plan::default().step(["add", "--update", "--", "."]),
        Target::Files(Section::Untracked | Section::Unstaged, paths) => {
            Plan::default().step(args(&["add", "--"], paths))
        }
        Target::Hunk {
            section: section @ (Section::Untracked | Section::Unstaged),
            path,
            ..
        } => {
            let Some(prepared) = hunk_patch(editor, id, target, false) else {
                return;
            };
            let patch = patch_path(&prepared);
            let mut plan = prepared.plan;
            if *section == Section::Untracked {
                plan = plan.step(args(&["add", "--intent-to-add", "--"], [path]));
            }
            plan.step(["apply", "--cached", "--", patch.as_str()])
        }
        Target::Section(Section::Staged)
        | Target::Files(Section::Staged, _)
        | Target::Hunk {
            section: Section::Staged,
            ..
        } => {
            editor.show_message("Already staged");
            return;
        }
        _ => {
            editor.show_message("Nothing to stage here");
            return;
        }
    };
    run::run(editor, root, plan);
}

pub(crate) fn unstage(editor: &mut Editor) {
    with_target(editor, |editor, id, target| {
        unstage_target(editor, id, &target);
    });
}

fn unstage_target(editor: &mut Editor, id: BufferId, target: &Target) {
    let Some((_, view)) = editor.applet_buffer::<StatusView>(id) else {
        return;
    };
    let root = view.root.clone();
    let born = view.status.snapshot.head.is_some();
    let renamed_from = |paths: &[String]| -> Vec<String> {
        view.status
            .snapshot
            .staged
            .iter()
            .filter(|change| paths.contains(&change.path))
            .filter_map(|change| change.from.clone())
            .collect()
    };
    let forget = |paths: Vec<String>| {
        if born {
            Plan::default().step(args(&["reset", "-q", "--"], &paths))
        } else {
            Plan::default().step(args(&["rm", "--cached", "-r", "-q", "--"], &paths))
        }
    };
    let plan = match target {
        Target::Section(Section::Staged) => forget(vec![".".to_owned()]),
        Target::Files(Section::Staged, paths) => {
            let mut all = paths.clone();
            all.extend(renamed_from(paths));
            forget(all)
        }
        Target::Hunk {
            section: Section::Staged,
            ..
        } => {
            let Some(prepared) = hunk_patch(editor, id, target, true) else {
                return;
            };
            let patch = patch_path(&prepared);
            prepared
                .plan
                .step(["apply", "--cached", "--reverse", "--", patch.as_str()])
        }
        _ => {
            editor.show_message("Nothing staged here");
            return;
        }
    };
    run::run(editor, root, plan);
}

pub(crate) fn discard(editor: &mut Editor) {
    with_target(editor, discard_target);
}

fn describe(paths: &[String]) -> String {
    match paths {
        [one] => one.clone(),
        many => format!("{} files", many.len()),
    }
}

fn discard_target(editor: &mut Editor, id: BufferId, target: Target) {
    let Some((_, view)) = editor.applet_buffer::<StatusView>(id) else {
        return;
    };
    let root = view.root.clone();
    let untracked = view.status.snapshot.untracked.clone();
    match target {
        Target::Section(Section::Untracked) => trash_files(editor, root, untracked),
        Target::Files(Section::Untracked, paths) => trash_files(editor, root, paths),
        Target::Section(Section::Unstaged) => {
            editor.ask("Discard all unstaged changes?", move |editor, yes| {
                if yes {
                    run::run(editor, root, Plan::default().step(["restore", "--", "."]));
                }
            });
        }
        Target::Files(Section::Unstaged, paths) => {
            let prompt = format!("Discard changes to {}?", describe(&paths));
            editor.ask(&prompt, move |editor, yes| {
                if yes {
                    run::run(
                        editor,
                        root,
                        Plan::default().step(args(&["restore", "--"], &paths)),
                    );
                }
            });
        }
        Target::Hunk {
            section: Section::Unstaged,
            ref lines,
            ..
        } => {
            let prompt = if lines.is_some() {
                "Discard the selected lines?"
            } else {
                "Discard this hunk?"
            };
            let Some(prepared) = hunk_patch(editor, id, &target, true) else {
                return;
            };
            editor.ask(prompt, move |editor, yes| {
                let patch = patch_path(&prepared);
                let plan = if yes {
                    prepared
                        .plan
                        .step(["apply", "--reverse", "--", patch.as_str()])
                } else {
                    prepared.plan
                };
                run::run(editor, prepared.root, plan);
            });
        }
        Target::Hunk {
            section: Section::Untracked,
            ..
        } => editor.show_message("Discard the whole file, or edit it"),
        Target::Section(Section::Staged)
        | Target::Files(Section::Staged, _)
        | Target::Hunk {
            section: Section::Staged,
            ..
        } => editor.show_message("Unstage it first with u"),
        _ => editor.show_message("Nothing to discard here"),
    }
}

fn trash_files(editor: &mut Editor, root: PathBuf, paths: Vec<String>) {
    let prompt = format!("Move {} to the trash?", describe(&paths));
    editor.ask(&prompt, move |editor, yes| {
        if !yes {
            return;
        }
        let full: Vec<PathBuf> = paths.iter().map(|path| root.join(path)).collect();
        if let Err(error) = trash(&full) {
            editor.show_message(format!("Could not move to the trash: {error}"));
        }
        view::refresh_root(editor, &root);
    });
}

#[cfg(not(test))]
fn trash(paths: &[PathBuf]) -> Result<(), String> {
    trash::delete_all(paths).map_err(|error| error.to_string())
}

#[cfg(test)]
thread_local! {
    pub(crate) static TRASHED: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
fn trash(paths: &[PathBuf]) -> Result<(), String> {
    for path in paths {
        fs::remove_file(path).map_err(|error| error.to_string())?;
        TRASHED.with_borrow_mut(|trashed| trashed.push(path.clone()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oxmacs_core::editor::Editor;

    use super::TRASHED;
    use crate::repo::fixtures::{git, repository};
    use crate::testing::{at_point, installed, lines, press, run_jobs};

    fn staged(dir: &std::path::Path, path: &str) -> String {
        git(dir, &["show", &format!(":{path}")])
    }

    fn changed() -> (tempfile::TempDir, Editor) {
        let dir = repository();
        fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\nfour\n").unwrap();
        fs::write(dir.path().join("c.txt"), "new\nlines\n").unwrap();
        let mut editor = installed(dir.path());
        press(&mut editor, "C-x g");
        (dir, editor)
    }

    fn shown(editor: &Editor, line: &str) -> bool {
        lines(editor).iter().any(|shown| shown == line)
    }

    #[test]
    fn s_stages_the_file_at_point() {
        let (dir, mut editor) = changed();
        press(&mut editor, "n n");
        assert_eq!(at_point(&editor), "  M  a.txt  +2 -1");

        press(&mut editor, "s");
        run_jobs(&mut editor);

        assert_eq!(staged(dir.path(), "a.txt"), "one\nTWO\nthree\nfour\n");
        assert!(shown(&editor, "Staged changes (1)"));
        assert!(!shown(&editor, "Unstaged changes (1)"));
    }

    #[test]
    fn s_on_untracked_files_adds_them() {
        let (dir, mut editor) = changed();

        press(&mut editor, "s");
        run_jobs(&mut editor);

        assert_eq!(staged(dir.path(), "c.txt"), "new\nlines\n");
        assert!(shown(&editor, "  A  c.txt  +2"));
    }

    #[test]
    fn s_on_a_section_stages_everything_in_it() {
        let (dir, mut editor) = changed();
        fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();
        press(&mut editor, "g n");
        assert_eq!(at_point(&editor), "Unstaged changes (2)");

        press(&mut editor, "s");
        run_jobs(&mut editor);

        assert!(shown(&editor, "Staged changes (2)"));
        assert!(shown(&editor, "Untracked files (1)"));
    }

    #[test]
    fn s_on_a_hunk_line_stages_the_whole_hunk() {
        let (dir, mut editor) = changed();
        press(&mut editor, "n n TAB n C-n");

        press(&mut editor, "s");
        run_jobs(&mut editor);

        assert_eq!(staged(dir.path(), "a.txt"), "one\nTWO\nthree\nfour\n");
    }

    #[test]
    fn a_region_stages_only_its_lines() {
        let (dir, mut editor) = changed();
        press(&mut editor, "n n TAB n C-n C-n C-n C-n C-n");
        assert_eq!(at_point(&editor), "+four");

        press(&mut editor, "C-SPC C-e s");
        run_jobs(&mut editor);

        assert_eq!(staged(dir.path(), "a.txt"), "one\ntwo\nthree\nfour\n");
        assert!(editor.buffer().selection().is_none());
        assert!(!dir.path().join(".git").read_dir().unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".patch")
        }));
    }

    #[test]
    fn a_region_in_an_untracked_file_stages_part_of_it() {
        let (dir, mut editor) = changed();
        press(&mut editor, "TAB n C-n");
        assert_eq!(at_point(&editor), "+new");

        press(&mut editor, "C-SPC C-e s");
        run_jobs(&mut editor);

        assert_eq!(staged(dir.path(), "c.txt"), "new\n");
    }

    fn go_to(editor: &mut Editor, text: &str) {
        let line = lines(editor).iter().position(|line| line == text).unwrap();
        let point = editor.buffer().line_start(line);
        editor.buffer_mut().set_point(point);
    }

    #[test]
    fn u_unstages_files_hunks_and_lines() {
        let (dir, mut editor) = changed();
        git(dir.path(), &["add", "a.txt"]);
        press(&mut editor, "g");
        go_to(&mut editor, "  M  a.txt  +2 -1");

        press(&mut editor, "TAB n C-n C-n C-SPC C-n C-n u");
        run_jobs(&mut editor);
        assert_eq!(staged(dir.path(), "a.txt"), "one\ntwo\nthree\nfour\n");

        go_to(&mut editor, "  M  a.txt  +1");
        press(&mut editor, "u");
        run_jobs(&mut editor);
        assert_eq!(staged(dir.path(), "a.txt"), "one\ntwo\nthree\n");
    }

    #[test]
    fn u_in_a_new_repository_unstages_without_a_head() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        git(dir.path(), &["add", "a.txt"]);
        let mut editor = installed(dir.path());
        press(&mut editor, "C-x g");
        assert_eq!(at_point(&editor), "  A  a.txt  +1");

        press(&mut editor, "u");
        run_jobs(&mut editor);

        assert!(shown(&editor, "  ?  a.txt  +1"));
    }

    #[test]
    fn k_discards_unstaged_changes_after_asking() {
        let (dir, mut editor) = changed();
        press(&mut editor, "n n k");
        assert_eq!(
            editor
                .question()
                .map(|question| question.prompt().to_owned()),
            Some("Discard changes to a.txt? (y or n) ".to_owned())
        );

        press(&mut editor, "y");
        run_jobs(&mut editor);

        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "one\ntwo\nthree\n"
        );
        assert!(!shown(&editor, "Unstaged changes (1)"));
    }

    #[test]
    fn k_on_selected_lines_discards_only_them() {
        let (dir, mut editor) = changed();
        press(&mut editor, "n n TAB n C-n C-n C-n C-n C-n C-SPC C-e k y");
        run_jobs(&mut editor);

        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "one\nTWO\nthree\n"
        );
    }

    #[test]
    fn k_moves_untracked_files_to_the_trash() {
        let (dir, mut editor) = changed();
        press(&mut editor, "k");
        assert_eq!(
            editor
                .question()
                .map(|question| question.prompt().to_owned()),
            Some("Move c.txt to the trash? (y or n) ".to_owned())
        );

        press(&mut editor, "y");

        assert!(!dir.path().join("c.txt").exists());
        assert!(TRASHED.with_borrow(|trashed| trashed.contains(&dir.path().join("c.txt"))));
        assert!(!shown(&editor, "Untracked files (1)"));
    }

    #[test]
    fn answering_no_keeps_everything() {
        let (dir, mut editor) = changed();
        press(&mut editor, "n n k n");
        run_jobs(&mut editor);

        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "one\nTWO\nthree\nfour\n"
        );
    }

    #[test]
    fn staged_changes_must_be_unstaged_before_discarding() {
        let (dir, mut editor) = changed();
        git(dir.path(), &["add", "a.txt"]);
        press(&mut editor, "g M-> p p");
        assert_eq!(at_point(&editor), "  M  a.txt  +2 -1");

        press(&mut editor, "k");

        assert_eq!(editor.message(), Some("Unstage it first with u"));
    }

    #[test]
    fn failures_show_what_git_said() {
        let (dir, mut editor) = changed();
        press(&mut editor, "n n");
        fs::write(dir.path().join(".git").join("index.lock"), "").unwrap();

        press(&mut editor, "s");
        run_jobs(&mut editor);

        let message = editor.message().unwrap_or_default().to_owned();
        assert!(message.starts_with("Git: "), "{message}");
    }
}
