use std::fs;
use std::path::Path;

use regex::Regex;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Style {
    pub tabs: Option<bool>,
    pub indent: Option<usize>,
    pub tab_width: Option<usize>,
}

#[derive(Debug, Default)]
struct Raw {
    style: Option<String>,
    size: Option<String>,
    tab: Option<String>,
}

fn width(text: Option<&str>) -> Option<usize> {
    text?.parse().ok().filter(|width| *width > 0)
}

impl Raw {
    fn resolve(&self) -> Style {
        let tabs = match self.style.as_deref() {
            Some("tab") => Some(true),
            Some("space") => Some(false),
            _ => None,
        };
        let tab_width = width(self.tab.as_deref());
        let indent = match self.size.as_deref() {
            Some("tab") => tab_width,
            Some(size) => width(Some(size)),
            None if tabs == Some(true) => tab_width,
            None => None,
        };
        Style {
            tabs,
            indent,
            tab_width: tab_width.or_else(|| width(self.size.as_deref())),
        }
    }
}

fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with(['#', ';']))
}

fn pair(line: &str) -> Option<(String, String)> {
    let (key, value) = line.split_once('=')?;
    Some((key.trim().to_lowercase(), value.trim().to_lowercase()))
}

fn is_root(text: &str) -> bool {
    lines(text)
        .take_while(|line| !line.starts_with('['))
        .filter_map(pair)
        .any(|(key, value)| key == "root" && value == "true")
}

fn glob(pattern: &str) -> Option<Regex> {
    let mut out = String::from("^");
    let mut chars = pattern.chars().peekable();
    let mut braces = 0;
    while let Some(character) = chars.next() {
        match character {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                out.push_str(".*");
            }
            '*' => out.push_str("[^/]*"),
            '?' => out.push_str("[^/]"),
            '{' => {
                braces += 1;
                out.push_str("(?:");
            }
            '}' if braces > 0 => {
                braces -= 1;
                out.push(')');
            }
            ',' if braces > 0 => out.push('|'),
            '[' => {
                out.push('[');
                if chars.peek() == Some(&'!') {
                    chars.next();
                    out.push('^');
                }
                for inside in chars.by_ref() {
                    out.push(inside);
                    if inside == ']' {
                        break;
                    }
                }
            }
            '\\' => {
                if let Some(next) = chars.next() {
                    out.push_str(&regex::escape(&next.to_string()));
                }
            }
            other => out.push_str(&regex::escape(&other.to_string())),
        }
    }
    out.push('$');
    Regex::new(&out).ok()
}

fn matches(folder: &Path, pattern: &str, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(folder) else {
        return false;
    };
    let relative = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    let (pattern, subject) = if pattern.contains('/') {
        (pattern.trim_start_matches('/'), relative.as_str())
    } else {
        (pattern, relative.rsplit('/').next().unwrap_or_default())
    };
    glob(pattern).is_some_and(|glob| glob.is_match(subject))
}

fn apply(raw: &mut Raw, folder: &Path, text: &str, path: &Path) {
    let mut chosen = false;
    for line in lines(text) {
        if let Some(section) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            chosen = matches(folder, section, path);
            continue;
        }
        let Some((key, value)) = pair(line).filter(|_| chosen) else {
            continue;
        };
        let value = (value != "unset").then_some(value);
        match key.as_str() {
            "indent_style" => raw.style = value,
            "indent_size" => raw.size = value,
            "tab_width" => raw.tab = value,
            _ => {}
        }
    }
}

pub fn style_for(path: &Path) -> Style {
    let mut found = Vec::new();
    for folder in path.ancestors().skip(1) {
        let Ok(text) = fs::read_to_string(folder.join(".editorconfig")) else {
            continue;
        };
        let root = is_root(&text);
        found.push((folder, text));
        if root {
            break;
        }
    }
    let mut raw = Raw::default();
    for (folder, text) in found.iter().rev() {
        apply(&mut raw, folder, text, path);
    }
    raw.resolve()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{Style, style_for};

    fn folder(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            let path = dir.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        dir
    }

    #[test]
    fn sections_match_file_names_and_later_ones_win() {
        let dir = folder(&[(
            ".editorconfig",
            "root = true\n\n[*]\nindent_style = space\nindent_size = 4\n\n# Python\n[*.{py,pyi}]\nindent_size = 2\n\n[Makefile]\nindent_style = tab\n",
        )]);
        let style = |name: &str| style_for(&dir.path().join(name));

        assert_eq!(
            style("a.rs"),
            Style {
                tabs: Some(false),
                indent: Some(4),
                tab_width: Some(4)
            }
        );
        assert_eq!(
            style("deep/b.pyi"),
            Style {
                tabs: Some(false),
                indent: Some(2),
                tab_width: Some(2)
            }
        );
        assert_eq!(style("Makefile").tabs, Some(true));
    }

    #[test]
    fn nearer_files_win_until_one_is_the_root() {
        let dir = folder(&[
            (".editorconfig", "[*]\nindent_style = tab\n"),
            (
                "project/.editorconfig",
                "root = true\n[*]\nindent_size = 3\n",
            ),
            (
                "project/src/.editorconfig",
                "[*.c]\nindent_style = tab\ntab_width = 8\n",
            ),
        ]);

        assert_eq!(
            style_for(&dir.path().join("project/src/main.c")),
            Style {
                tabs: Some(true),
                indent: Some(3),
                tab_width: Some(8)
            }
        );
        assert_eq!(style_for(&dir.path().join("project/notes.txt")).tabs, None);
    }

    #[test]
    fn paths_with_slashes_start_at_the_files_folder() {
        let dir = folder(&[(
            ".editorconfig",
            "[lib/**.js]\nindent_size = 2\n[/top.js]\nindent_size = 6\n",
        )]);
        let indent = |name: &str| style_for(&dir.path().join(name)).indent;

        assert_eq!(indent("lib/a/b.js"), Some(2));
        assert_eq!(indent("other/lib/b.js"), None);
        assert_eq!(indent("top.js"), Some(6));
        assert_eq!(indent("deeper/top.js"), None);
    }

    #[test]
    fn tabs_indent_by_the_tab_width_and_unset_clears_a_key() {
        let dir = folder(&[(
            ".editorconfig",
            "[*]\nindent_style = TAB\ntab_width = 4\n[*.go]\nindent_size = tab\n[*.txt]\nindent_style = unset\n",
        )]);

        assert_eq!(
            style_for(&dir.path().join("a.go")),
            Style {
                tabs: Some(true),
                indent: Some(4),
                tab_width: Some(4)
            }
        );
        assert_eq!(style_for(&dir.path().join("a.rs")).indent, Some(4));
        assert_eq!(style_for(&dir.path().join("a.txt")).tabs, None);
    }
}
