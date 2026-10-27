use std::fs;
use std::path::{Path, PathBuf};

use crate::check::{Binding, Occurrence, binding_at};
use crate::span::Span;

#[derive(Debug, Clone, Copy)]
pub struct Source<'a> {
    pub module: Option<&'a str>,
    pub occurrences: &'a [Occurrence],
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    Local(usize, Span),
    Global(Option<String>, usize, String),
}

fn key(index: usize, source: &Source<'_>, binding: &Binding) -> Key {
    match binding {
        Binding::Local(span) => Key::Local(index, *span),
        Binding::Global { module, name } => {
            let module = module.clone().or_else(|| source.module.map(str::to_owned));
            let alone = if module.is_some() { 0 } else { index };
            Key::Global(module, alone, name.clone())
        }
    }
}

pub fn module_name(roots: &[PathBuf], path: &Path) -> Option<String> {
    roots.iter().find_map(|root| {
        let relative = path.strip_prefix(root).ok()?.with_extension("");
        let parts: Vec<String> = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect();
        (!parts.is_empty()).then(|| parts.join("/"))
    })
}

pub fn same_binding(
    sources: &[Source<'_>],
    index: usize,
    offset: usize,
) -> Vec<(usize, Occurrence)> {
    let Some(source) = sources.get(index) else {
        return Vec::new();
    };
    let Some(at) = binding_at(source.occurrences, offset) else {
        return Vec::new();
    };
    let wanted = &key(index, source, &at.binding);
    sources
        .iter()
        .enumerate()
        .flat_map(|(other, found)| {
            found
                .occurrences
                .iter()
                .filter(move |occurrence| key(other, found, &occurrence.binding) == *wanted)
                .map(move |occurrence| (other, occurrence.clone()))
        })
        .collect()
}

pub fn definition(
    sources: &[Source<'_>],
    index: usize,
    offset: usize,
) -> Option<(usize, Occurrence)> {
    same_binding(sources, index, offset)
        .into_iter()
        .find(|(_, occurrence)| occurrence.defines)
}

pub fn name_span(occurrence: &Occurrence) -> Span {
    match &occurrence.binding {
        Binding::Global { name, .. } => {
            let end = occurrence.span.end as usize;
            Span::new(end.saturating_sub(name.len()), end)
        }
        Binding::Local(_) => occurrence.span,
    }
}

pub fn ox_files(path: &Path, most: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if path.is_dir() {
        walk(path, most, &mut found);
    } else {
        found.push(path.to_path_buf());
    }
    found
}

fn walk(folder: &Path, most: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if found.len() >= most {
            return;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if !name.starts_with('.') && name != "target" {
                walk(&path, most, found);
            }
        } else if path.extension().is_some_and(|extension| extension == "ox") {
            found.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{Source, module_name, name_span, same_binding};
    use crate::Isolate;

    use super::ox_files;

    #[test]
    fn files_under_a_root_are_named_like_the_modules_they_hold() {
        let roots = [PathBuf::from("/home/plugins")];

        assert_eq!(
            module_name(&roots, Path::new("/home/plugins/git/status.ox")).as_deref(),
            Some("git/status")
        );
        assert_eq!(module_name(&roots, Path::new("/elsewhere/a.ox")), None);
    }

    #[test]
    fn bindings_are_followed_into_every_file_that_imports_them() {
        let dir = tempfile::tempdir().unwrap();
        let geometry = "(def (area w h) (* w h))\n(def (other area) area)\n";
        fs::write(dir.path().join("geometry.ox"), geometry).unwrap();
        let mut isolate = Isolate::new();
        isolate.add_root(dir.path());
        let user = "(module (import (geometry :as g)))\n(g/area 1 2)\n(def area 5)\n";
        let here = isolate.bindings("geometry.ox", geometry);
        let there = isolate.bindings("user.ox", user);
        let sources = [
            Source {
                module: Some("geometry"),
                occurrences: &here,
            },
            Source {
                module: None,
                occurrences: &there,
            },
        ];

        let found: Vec<(usize, u32)> = same_binding(&sources, 1, user.find("g/area").unwrap() + 3)
            .iter()
            .map(|(index, occurrence)| (*index, name_span(occurrence).start))
            .collect();

        assert_eq!(found, [(0, 6), (1, 38)]);
        let local = same_binding(&sources, 0, geometry.rfind("area").unwrap());
        assert_eq!(local.len(), 2);
        assert!(local.iter().all(|(index, _)| *index == 0));
    }

    #[test]
    fn folders_give_their_ox_files_in_order_without_hidden_or_target_folders() {
        let dir = tempfile::tempdir().unwrap();
        for file in [
            "b.ox",
            "a.ox",
            "notes.txt",
            "lib/c.ox",
            ".git/d.ox",
            "target/e.ox",
        ] {
            let path = dir.path().join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }

        let found: Vec<String> = ox_files(dir.path(), usize::MAX)
            .iter()
            .map(|path| {
                path.strip_prefix(dir.path())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();

        assert_eq!(found, ["a.ox", "b.ox", "lib/c.ox"]);
        assert_eq!(ox_files(dir.path(), 1).len(), 1);
        assert_eq!(
            ox_files(&dir.path().join("notes.txt"), usize::MAX),
            [dir.path().join("notes.txt")]
        );
    }
}
