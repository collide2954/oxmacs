use std::path::{Path, PathBuf};

use oxmacs_core::editor::{Action, Editor, Group, Offer, Picked, Transient};

use crate::history;
use crate::run::{self, Plan};
use crate::view;

pub(crate) static BRANCH: Transient = Transient {
    title: "Branch",
    groups: &[
        Group {
            title: "Checkout",
            switches: &[],
            actions: &[
                Action {
                    key: 'b',
                    summary: "Branch or revision",
                    run: |editor, _| checkout(editor),
                },
                Action {
                    key: 'c',
                    summary: "New branch",
                    run: |editor, _| create(editor, true),
                },
            ],
        },
        Group {
            title: "Create",
            switches: &[],
            actions: &[Action {
                key: 'n',
                summary: "New branch",
                run: |editor, _| create(editor, false),
            }],
        },
        Group {
            title: "Do",
            switches: &[],
            actions: &[
                Action {
                    key: 'm',
                    summary: "Rename",
                    run: |editor, _| rename(editor),
                },
                Action {
                    key: 'x',
                    summary: "Delete",
                    run: |editor, _| delete(editor),
                },
            ],
        },
    ],
};

pub(crate) fn dispatch(editor: &mut Editor) {
    if view::current(editor).is_some() {
        editor.open_transient(&BRANCH);
    }
}

fn here(editor: &Editor) -> Option<(PathBuf, Option<String>)> {
    let (_, view) = view::current(editor)?;
    Some((view.root.clone(), view.status.snapshot.branch.clone()))
}

fn pick(
    editor: &mut Editor,
    prompt: &str,
    names: Vec<String>,
    then: impl FnOnce(&mut Editor, String) + Send + 'static,
) {
    let offered = names.clone();
    editor.prompt_pick(
        prompt,
        move |_| {
            offered
                .iter()
                .map(|name| Offer::from(name.as_str()))
                .collect()
        },
        move |editor, picked| {
            let name = match picked {
                Picked::Offered(index) => names.get(index).cloned().unwrap_or_default(),
                Picked::Typed(text) => text.trim().to_owned(),
            };
            if name.is_empty() {
                editor.show_message("No branch given");
                return;
            }
            then(editor, name);
        },
    );
}

fn checkout(editor: &mut Editor) {
    let Some((root, current)) = here(editor) else {
        return;
    };
    let (mut names, remote) = history::branch_names(&root);
    names.extend(remote);
    names.retain(|name| Some(name) != current.as_ref());
    pick(editor, "Checkout: ", names, move |editor, name| {
        run::run(
            editor,
            root,
            Plan::default().step(["checkout", name.as_str()]),
        );
    });
}

fn create(editor: &mut Editor, switch: bool) {
    let Some((root, _)) = here(editor) else {
        return;
    };
    let prompt = if switch {
        "Create and checkout branch: "
    } else {
        "Create branch: "
    };
    editor.prompt(prompt, move |editor, name| {
        let name = name.trim();
        if name.is_empty() {
            editor.show_message("No branch given");
            return;
        }
        let plan = if switch {
            Plan::default().step(["checkout", "-b", name])
        } else {
            Plan::default().step(["branch", name])
        };
        run::run(editor, root, plan);
    });
}

fn local_names(root: &Path, current: Option<&String>, first: bool) -> Vec<String> {
    let (mut names, _) = history::branch_names(root);
    names.retain(|name| Some(name) != current);
    if first && let Some(current) = current {
        names.insert(0, current.clone());
    }
    names
}

fn rename(editor: &mut Editor) {
    let Some((root, current)) = here(editor) else {
        return;
    };
    let names = local_names(&root, current.as_ref(), true);
    pick(editor, "Rename branch: ", names, move |editor, old| {
        let prompt = format!("Rename {old} to: ");
        editor.prompt(&prompt, move |editor, new| {
            let new = new.trim();
            if new.is_empty() {
                editor.show_message("No branch given");
                return;
            }
            run::run(
                editor,
                root,
                Plan::default().step(["branch", "-m", old.as_str(), new]),
            );
        });
    });
}

fn delete(editor: &mut Editor) {
    let Some((root, current)) = here(editor) else {
        return;
    };
    let names = local_names(&root, current.as_ref(), false);
    pick(editor, "Delete branch: ", names, move |editor, name| {
        let merged = history::merged(&root, &name).unwrap_or(false);
        let (prompt, flag) = if merged {
            (format!("Delete branch {name}?"), "-d")
        } else {
            (
                format!("Branch {name} is not fully merged. Delete it anyway?"),
                "-D",
            )
        };
        editor.ask(&prompt, move |editor, yes| {
            if yes {
                run::run(
                    editor,
                    root,
                    Plan::default().step(["branch", flag, name.as_str()]),
                );
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use oxmacs_core::editor::Editor;

    use crate::repo::fixtures::{git, repository};
    use crate::testing::{installed, lines, press, run_jobs, type_text};

    fn branched() -> (tempfile::TempDir, Editor) {
        let dir = repository();
        git(dir.path(), &["checkout", "-q", "-b", "feature"]);
        fs::write(dir.path().join("f.txt"), "f\n").unwrap();
        git(dir.path(), &["add", "f.txt"]);
        git(dir.path(), &["commit", "-q", "-m", "Feature work"]);
        git(dir.path(), &["checkout", "-q", "main"]);
        git(dir.path(), &["branch", "-q", "merged"]);
        let mut editor = installed(dir.path());
        press(&mut editor, "C-x g");
        (dir, editor)
    }

    fn current(dir: &Path) -> String {
        git(dir, &["branch", "--show-current"]).trim().to_owned()
    }

    fn branches(dir: &Path) -> Vec<String> {
        git(dir, &["branch", "--format=%(refname:short)"])
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn question(editor: &Editor) -> Option<String> {
        editor
            .question()
            .map(|question| question.prompt().to_owned())
    }

    #[test]
    fn b_opens_the_branch_menu() {
        let (_dir, mut editor) = branched();

        press(&mut editor, "b");

        let menu = editor.transient().unwrap();
        assert_eq!(menu.title, "Branch");
        let keys: Vec<String> = menu
            .groups
            .iter()
            .flat_map(|group| group.entries.iter().map(|entry| entry.key.clone()))
            .collect();
        assert_eq!(keys, ["b", "c", "n", "m", "x"]);
    }

    #[test]
    fn b_b_checks_out_a_branch_and_refreshes_the_status() {
        let (dir, mut editor) = branched();

        press(&mut editor, "b b");
        type_text(&mut editor, "feature");
        press(&mut editor, "RET");
        run_jobs(&mut editor);

        assert_eq!(current(dir.path()), "feature");
        assert!(lines(&editor)[0].starts_with("Head:     feature  "));
    }

    #[test]
    fn b_c_creates_a_branch_from_head_and_checks_it_out() {
        let (dir, mut editor) = branched();

        press(&mut editor, "b c");
        type_text(&mut editor, "topic");
        press(&mut editor, "RET");
        run_jobs(&mut editor);

        assert_eq!(current(dir.path()), "topic");
        assert_eq!(
            git(dir.path(), &["rev-parse", "topic"]),
            git(dir.path(), &["rev-parse", "main"])
        );
    }

    #[test]
    fn b_n_creates_a_branch_without_checking_it_out() {
        let (dir, mut editor) = branched();

        press(&mut editor, "b n");
        type_text(&mut editor, "topic");
        press(&mut editor, "RET");
        run_jobs(&mut editor);

        assert_eq!(current(dir.path()), "main");
        assert!(branches(dir.path()).contains(&"topic".to_owned()));
    }

    #[test]
    fn b_m_renames_the_current_branch_first() {
        let (dir, mut editor) = branched();

        press(&mut editor, "b m RET");
        type_text(&mut editor, "trunk");
        press(&mut editor, "RET");
        run_jobs(&mut editor);

        assert_eq!(current(dir.path()), "trunk");
        assert!(!branches(dir.path()).contains(&"main".to_owned()));
    }

    #[test]
    fn b_x_deletes_a_merged_branch_after_asking() {
        let (dir, mut editor) = branched();

        press(&mut editor, "b x");
        type_text(&mut editor, "merged");
        press(&mut editor, "RET");
        assert_eq!(
            question(&editor),
            Some("Delete branch merged? (y or n) ".to_owned())
        );
        press(&mut editor, "y");
        run_jobs(&mut editor);

        assert!(!branches(dir.path()).contains(&"merged".to_owned()));
    }

    #[test]
    fn b_x_warns_before_forcing_an_unmerged_delete() {
        let (dir, mut editor) = branched();

        press(&mut editor, "b x");
        type_text(&mut editor, "feature");
        press(&mut editor, "RET");
        assert_eq!(
            question(&editor),
            Some("Branch feature is not fully merged. Delete it anyway? (y or n) ".to_owned())
        );
        press(&mut editor, "y");
        run_jobs(&mut editor);

        assert!(!branches(dir.path()).contains(&"feature".to_owned()));
    }

    #[test]
    fn answering_no_keeps_the_branch() {
        let (dir, mut editor) = branched();

        press(&mut editor, "b x");
        type_text(&mut editor, "feature");
        press(&mut editor, "RET n");
        run_jobs(&mut editor);

        assert!(branches(dir.path()).contains(&"feature".to_owned()));
    }
}
