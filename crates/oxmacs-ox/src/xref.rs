use std::fs;
use std::path::{Path, PathBuf};

use ox_lang::Isolate;
use ox_lang::describe::Kind;
use ox_lang::names;
use ox_lang::span::line_column;
use oxmacs_core::editor::Editor;

use crate::help;
use crate::stacktrace::{self, Place};

pub const USAGES: &str = "*ox usages*";
const FIRST_USAGE: usize = 2;
const MOST_FILES: usize = 2000;

pub(crate) fn find_definition(isolate: &Isolate, editor: &mut Editor, home: Option<&Path>) {
    let Some(name) = help::name_at_point(editor) else {
        editor.show_message("There is no Ox name at point");
        return;
    };
    let Some(description) = isolate.describe(&help::module(editor, home), &name) else {
        editor.show_message(format!("{name} is not defined"));
        return;
    };
    let Some(place) = description.place else {
        editor.show_message(match description.kind {
            Kind::Native | Kind::Macro => {
                format!("{name} is built into Ox; C-c C-d shows its docs")
            }
            Kind::Special => format!("{name} is a special form; C-c C-d shows its docs"),
            _ => format!("Ox doesn't know where {name} was defined"),
        });
        return;
    };
    let here = editor.current_location();
    let opened = stacktrace::open_place(
        editor,
        place.file(),
        place.line_column(),
        Some(&place.source.text),
    );
    if opened {
        editor.remember_location(here);
    }
}

fn usages_in(text: &str, wanted: &str) -> Vec<(usize, usize)> {
    names::occurrences(text, wanted)
        .into_iter()
        .map(|span| line_column(text, span.start))
        .collect()
}

fn ox_files(root: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if found.len() >= MOST_FILES {
            return;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if !name.starts_with('.') && name != "target" {
                ox_files(&path, found);
            }
        } else if path.extension().is_some_and(|extension| extension == "ox") {
            found.push(path);
        }
    }
}

struct Source {
    file: String,
    path: Option<PathBuf>,
    text: String,
}

fn same(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn sources(editor: &Editor, roots: &[PathBuf]) -> Vec<Source> {
    let current = editor.buffer().id();
    let mut ids = vec![current];
    ids.extend(
        editor
            .buffer_names()
            .iter()
            .filter_map(|name| editor.buffer_named(name))
            .filter(|id| *id != current),
    );
    let mut found = Vec::new();
    for id in ids {
        let Some(buffer) = editor.buffer_by_id(id) else {
            continue;
        };
        let path = buffer.file().map(|file| file.path.clone());
        let ox = path
            .as_deref()
            .is_some_and(|path| path.extension().is_some_and(|extension| extension == "ox"));
        if !ox && (id != current || !help::in_ox(editor)) {
            continue;
        }
        found.push(Source {
            file: path.as_ref().map_or_else(
                || buffer.name().to_owned(),
                |path| path.display().to_string(),
            ),
            path,
            text: buffer.contents(),
        });
    }
    let mut files = Vec::new();
    for root in roots {
        ox_files(root, &mut files);
    }
    for path in files {
        let known = found
            .iter()
            .any(|source| source.path.as_deref().is_some_and(|seen| same(seen, &path)));
        if known {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            found.push(Source {
                file: path.display().to_string(),
                path: Some(path),
                text,
            });
        }
    }
    found
}

fn shown(source: &Source, roots: &[PathBuf]) -> String {
    let Some(path) = &source.path else {
        return source.file.clone();
    };
    roots
        .iter()
        .find_map(|root| path.strip_prefix(root).ok())
        .map_or_else(
            || path.display().to_string(),
            |rest| rest.display().to_string(),
        )
}

pub(crate) fn show_usages(editor: &mut Editor, home: Option<&Path>) {
    let Some(name) = help::name_at_point(editor) else {
        editor.show_message("There is no Ox name at point");
        return;
    };
    let wanted = names::short(&name).to_owned();
    let folder = editor
        .buffer()
        .file()
        .and_then(|file| file.path.parent().map(Path::to_path_buf));
    let mut roots: Vec<PathBuf> = folder.into_iter().collect();
    if let Some(home) = home
        && !roots.iter().any(|root| root.starts_with(home))
    {
        roots.push(home.to_path_buf());
    }
    let mut lines = Vec::new();
    let mut places = Vec::new();
    for source in sources(editor, &roots) {
        let file = shown(&source, &roots);
        let texts: Vec<&str> = source.text.lines().collect();
        for (line, column) in usages_in(&source.text, &wanted) {
            let text = texts.get(line - 1).copied().unwrap_or_default();
            lines.push(format!("{file}:{line}:{column}: {}", text.trim_end()));
            places.push(Place {
                file: source.file.clone(),
                line,
                column,
                text: None,
            });
        }
    }
    if places.is_empty() {
        editor.show_message(format!("No usages of {wanted}"));
        return;
    }
    let count = if places.len() == 1 {
        "1 usage".to_owned()
    } else {
        format!("{} usages", places.len())
    };
    let text = format!("{count} of {wanted}\n\n{}\n", lines.join("\n"));
    stacktrace::show_places(editor, USAGES, &text, FIRST_USAGE, places);
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use oxmacs_core::editor::Editor;

    use super::USAGES;
    use crate::Ox;
    use crate::testing::{press, settle};

    fn started(home: &Path, files: &[(&str, &str)], open: &str) -> (Editor, Ox) {
        for (name, text) in files {
            let path = home.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.start(&mut editor, Some(home));
        settle(&mut editor, &mut ox);
        editor.open_file(&home.join(open));
        (editor, ox)
    }

    fn go(editor: &mut Editor, ox: &mut Ox, point: usize) {
        editor.buffer_mut().set_point(point + 1);
        press(editor, ox, "C-b");
    }

    const SHAPES: &str = "(def (area w h) (* w h))\n(+ (area 2 3) 1)\n(when true (inc 1))\n";

    #[test]
    fn m_dot_visits_definitions_and_m_comma_comes_back() {
        let home = tempfile::tempdir().unwrap();
        let (mut editor, mut ox) = started(home.path(), &[("shapes.ox", SHAPES)], "shapes.ox");
        press(&mut editor, &mut ox, "C-c C-b");
        let call = SHAPES.find("(area 2").unwrap() + 2;

        go(&mut editor, &mut ox, call);
        press(&mut editor, &mut ox, "M-.");
        assert_eq!(editor.buffer().name(), "shapes.ox");
        assert_eq!(editor.buffer().point(), 0);
        press(&mut editor, &mut ox, "M-,");
        assert_eq!(editor.buffer().point(), call);

        go(&mut editor, &mut ox, SHAPES.find("when").unwrap());
        press(&mut editor, &mut ox, "M-.");
        assert_eq!(editor.buffer().name(), "shapes.ox");
        assert_eq!(
            editor.message(),
            Some("when is built into Ox; C-c C-d shows its docs")
        );

        go(&mut editor, &mut ox, SHAPES.find("inc").unwrap());
        press(&mut editor, &mut ox, "M-.");
        assert_eq!(
            editor.message(),
            Some("inc is built into Ox; C-c C-d shows its docs")
        );
    }

    #[test]
    fn m_question_lists_usages_across_ox_files_and_visits_them() {
        let home = tempfile::tempdir().unwrap();
        let (mut editor, mut ox) = started(
            home.path(),
            &[
                ("shapes.ox", "(def (area w h) (* w h))\n(area 1 2)\n"),
                (
                    "more/use.ox",
                    "(module (import (shapes :as sh)))\n(sh/area 3 4) ; area in a comment\n\"area in a string\"\n",
                ),
            ],
            "shapes.ox",
        );

        go(&mut editor, &mut ox, 7);
        press(&mut editor, &mut ox, "M-?");

        let more = Path::new("more").join("use.ox");
        assert_eq!(editor.buffer().name(), USAGES);
        assert_eq!(
            editor.buffer().contents(),
            format!(
                "3 usages of area\n\nshapes.ox:1:7: (def (area w h) (* w h))\nshapes.ox:2:2: (area 1 2)\n{}:2:2: (sh/area 3 4) ; area in a comment\n",
                more.display()
            )
        );
        press(&mut editor, &mut ox, "n n RET");
        assert_eq!(editor.buffer().name(), "use.ox");
        let buffer = editor.buffer();
        assert_eq!(buffer.point(), buffer.line_start(1) + 1);
    }

    #[test]
    fn unsaved_text_is_searched_instead_of_the_file() {
        let home = tempfile::tempdir().unwrap();
        let (mut editor, mut ox) =
            started(home.path(), &[("shapes.ox", "(def x 1)\n")], "shapes.ox");
        editor.buffer_mut().insert_at(0, "(frob 1)\n(frob 2)\n");

        go(&mut editor, &mut ox, 2);
        press(&mut editor, &mut ox, "M-?");
        assert!(editor.buffer().contents().starts_with("2 usages of frob\n"));

        press(&mut editor, &mut ox, "q");
        go(&mut editor, &mut ox, 23);
        press(&mut editor, &mut ox, "M-?");
        assert!(editor.buffer().contents().starts_with("1 usage of x\n"));
    }
}
