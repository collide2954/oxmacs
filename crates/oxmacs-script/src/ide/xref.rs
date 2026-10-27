use std::fs;
use std::path::{Path, PathBuf};

use ox_lang::Isolate;
use ox_lang::check::Occurrence;
use ox_lang::describe::Kind;
use ox_lang::ide::{self, ox_files};
use ox_lang::names;
use ox_lang::span::line_column;
use oxmacs_api::{check_roots, editor_code_in};
use oxmacs_core::editor::{Editor, Place, Places};

use crate::ide::help;

pub const USAGES: &str = "*ox usages*";
const MOST_FILES: usize = 2000;

struct Source {
    file: String,
    path: Option<PathBuf>,
    text: String,
}

#[derive(Default)]
struct Checking {
    plain: Option<Isolate>,
    editor: Option<Isolate>,
}

impl Checking {
    fn bindings(
        &mut self,
        source: &Source,
        home: Option<&Path>,
        roots: &[PathBuf],
    ) -> Vec<Occurrence> {
        let path = source.path.as_deref();
        let isolate = if path.is_none_or(|path| editor_code_in(path, home)) {
            self.editor.get_or_insert_with(oxmacs_api::editor_isolate)
        } else {
            self.plain.get_or_insert_with(oxmacs_api::plain_isolate)
        };
        for root in path.map(check_roots).unwrap_or_default() {
            isolate.add_root(root);
        }
        for root in roots {
            isolate.add_root(root.clone());
        }
        isolate.bindings(&source.file, &source.text)
    }
}

fn module_of(source: &Source, roots: &[PathBuf]) -> Option<String> {
    let path = source.path.as_deref()?;
    let mut all = check_roots(path);
    all.extend(roots.iter().cloned());
    ide::module_name(&all, path)
}

fn byte_at(text: &str, point: usize) -> usize {
    text.char_indices()
        .nth(point)
        .map_or(text.len(), |(offset, _)| offset)
}

fn search_roots(editor: &Editor, home: Option<&Path>) -> Vec<PathBuf> {
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
    roots
}

fn same_binding(
    editor: &Editor,
    home: Option<&Path>,
    sources: &[Source],
    roots: &[PathBuf],
) -> Vec<(usize, Occurrence)> {
    let mut checking = Checking::default();
    let found: Vec<Vec<Occurrence>> = sources
        .iter()
        .map(|source| checking.bindings(source, home, roots))
        .collect();
    let modules: Vec<Option<String>> = sources
        .iter()
        .map(|source| module_of(source, roots))
        .collect();
    let views: Vec<ide::Source<'_>> = found
        .iter()
        .zip(&modules)
        .map(|(occurrences, module)| ide::Source {
            module: module.as_deref(),
            occurrences,
        })
        .collect();
    let Some(current) = sources.first() else {
        return Vec::new();
    };
    let at = byte_at(&current.text, editor.buffer().point());
    ide::same_binding(&views, 0, at)
}

pub(crate) fn find_definition(isolate: &Isolate, editor: &mut Editor, home: Option<&Path>) {
    let Some(name) = help::name_at_point(editor) else {
        editor.show_message("There is no Ox name at point");
        return;
    };
    let roots = search_roots(editor, home);
    let sources = open_sources(editor);
    let defined = same_binding(editor, home, &sources, &roots)
        .into_iter()
        .find(|(_, occurrence)| occurrence.defines);
    if let Some((index, occurrence)) = defined {
        let source = &sources[index];
        let (line, column) = line_column(&source.text, occurrence.span.start);
        let here = editor.current_location();
        let opened = editor.open_place(&Place {
            source: Some(source.text.as_str().into()),
            ..Place::at(&source.file, line, column)
        });
        if opened {
            editor.remember_location(here);
        }
        return;
    }
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
    let (line, column) = place.line_column();
    let here = editor.current_location();
    let opened = editor.open_place(&Place {
        source: Some(place.source.text.as_str().into()),
        ..Place::at(place.file(), line, column)
    });
    if opened {
        editor.remember_location(here);
    }
}

fn same(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn open_sources(editor: &Editor) -> Vec<Source> {
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
    found
}

fn sources(editor: &Editor, roots: &[PathBuf], wanted: &str) -> Vec<Source> {
    let mut found = open_sources(editor);
    let mut files = Vec::new();
    for root in roots {
        files.extend(ox_files(root, MOST_FILES.saturating_sub(files.len())));
    }
    for path in files {
        let known = found
            .iter()
            .any(|source| source.path.as_deref().is_some_and(|seen| same(seen, &path)));
        if known {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path)
            && text.contains(wanted)
        {
            found.push(Source {
                file: path.display().to_string(),
                path: Some(path),
                text,
            });
        }
    }
    found
}

pub(crate) fn show_usages(editor: &mut Editor, home: Option<&Path>) {
    let Some(name) = help::name_at_point(editor) else {
        editor.show_message("There is no Ox name at point");
        return;
    };
    let wanted = names::short(&name).to_owned();
    let roots = search_roots(editor, home);
    let sources = sources(editor, &roots, &wanted);
    let places: Vec<Place> = same_binding(editor, home, &sources, &roots)
        .into_iter()
        .map(|(index, occurrence)| {
            let source = &sources[index];
            let (line, column) = line_column(&source.text, occurrence.span.start);
            Place {
                source: source.path.is_none().then(|| source.text.as_str().into()),
                ..Place::at(&source.file, line, column)
            }
        })
        .collect();
    if places.is_empty() {
        editor.show_message(format!("No usages of {wanted}"));
        return;
    }
    let count = if places.len() == 1 {
        "1 usage".to_owned()
    } else {
        format!("{} usages", places.len())
    };
    editor.show_places(
        USAGES,
        Places {
            title: format!("{count} of {wanted}"),
            places,
            root: roots.first().cloned(),
            ..Places::default()
        },
    );
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
        assert_eq!(editor.buffer().point(), 6);
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
                "3 usages of area\nRET visits   SPC shows   n p step   q quits\n\n{}\n     2:2    (sh/area 3 4) ; area in a comment\n\nshapes.ox\n     1:7    (def (area w h) (* w h))\n     2:2    (area 1 2)\n",
                more.display()
            )
        );
        press(&mut editor, &mut ox, "n p RET");
        assert_eq!(editor.buffer().name(), "use.ox");
        let buffer = editor.buffer();
        assert_eq!(buffer.point(), buffer.line_start(1) + 1);
    }

    #[test]
    fn usages_and_definitions_follow_bindings_not_spellings() {
        let home = tempfile::tempdir().unwrap();
        let text = "(def (area w h) (* w h))\n(def (scaled area k) (* area k))\n(area 1 2)\n";
        let (mut editor, mut ox) = started(home.path(), &[("shapes.ox", text)], "shapes.ox");
        let local = text.find("(* area").unwrap() + 3;

        go(&mut editor, &mut ox, local);
        press(&mut editor, &mut ox, "M-?");
        assert!(
            editor.buffer().contents().starts_with("2 usages of area\n"),
            "{}",
            editor.buffer().contents()
        );
        press(&mut editor, &mut ox, "q");
        go(&mut editor, &mut ox, local);
        press(&mut editor, &mut ox, "M-.");
        assert_eq!(
            editor.buffer().point(),
            text.find("scaled area").unwrap() + 7
        );

        go(&mut editor, &mut ox, text.rfind("(area").unwrap() + 1);
        press(&mut editor, &mut ox, "M-?");
        assert!(editor.buffer().contents().starts_with("2 usages of area\n"));
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
