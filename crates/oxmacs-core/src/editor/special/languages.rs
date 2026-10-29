use std::fmt::Write;

use crate::command::{Scope, ScopeId};
use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::page::Page;
use crate::special_buffer::SpecialBuffer;
use crate::syntax::{Feature, Language};
use crate::theme::Highlight;

const NAME: &str = "*Languages*";
const NAME_WIDTH: usize = 16;

#[derive(Debug)]
struct LanguageList {
    styles: Vec<LineStyle>,
}

impl SpecialBuffer for LanguageList {
    fn scope(&self) -> ScopeId {
        Scope::Help.into()
    }

    fn line_style(&self, line: usize, _: &str) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }

    fn fixed_name(&self) -> bool {
        true
    }
}

impl Editor {
    fn features_of(&self, language: &Language) -> Vec<Feature> {
        let registered = self.registered(language);
        let grammar = language.grammar().is_some();
        let served = self
            .server_specs()
            .iter()
            .any(|spec| spec.language_id(language.name()).is_some());
        Feature::ALL
            .into_iter()
            .filter(|feature| match feature {
                Feature::Highlighting => {
                    grammar || registered.is_some_and(|known| known.highlight.is_some())
                }
                Feature::Indentation => {
                    registered.is_some_and(|known| known.indent.is_some())
                        || (grammar && !language.indentation().keeps())
                }
                Feature::Comments => language.line_comment().is_some(),
                Feature::Pairs => !language.pairs().is_empty(),
                Feature::Formatting => self.formatters.iter().any(|(known, _)| known == language),
                Feature::Lint => self.builds.has_linter(language),
                Feature::Motion => grammar || !language.pairs().is_empty(),
                Feature::Items => grammar,
                Feature::Completion => served || language.supports(*feature),
                Feature::Lsp => served,
                Feature::Preview | Feature::Repl => language.supports(*feature),
            })
            .collect()
    }

    pub(in crate::editor) fn list_languages(&mut self) {
        let rows: Vec<(Language, Vec<Feature>)> = self
            .languages()
            .into_iter()
            .map(|language| {
                let found = self.features_of(&language);
                (language, found)
            })
            .collect();
        let previewed = rows
            .iter()
            .any(|(_, found)| found.contains(&Feature::Preview));
        let columns: Vec<Feature> = Feature::ALL
            .into_iter()
            .filter(|feature| *feature != Feature::Preview || previewed)
            .collect();
        let mut page = Page::default();
        table(&mut page, &rows, &columns);
        let id = match self.find_special_buffer::<LanguageList>(|_| true) {
            Some(id) => id,
            None => self.add_special_buffer_with(NAME, |buffer| {
                buffer.set_read_only(true);
                LanguageList { styles: Vec::new() }
            }),
        };
        if let Some((buffer, list)) = self.special_buffer_mut::<LanguageList>(id) {
            page.fill(buffer);
            buffer.set_point(0);
            list.styles = page.into_parts().1;
        }
        self.show_buffer(id);
    }
}

fn table(page: &mut Page, rows: &[(Language, Vec<Feature>)], columns: &[Feature]) {
    let names: Vec<String> = rows
        .iter()
        .map(|(language, _)| {
            let extensions: Vec<String> = language
                .extensions()
                .iter()
                .map(|extension| format!(".{extension}"))
                .collect();
            format!("{} ({})", language.name(), extensions.join(" "))
        })
        .collect();
    let name_width = names
        .iter()
        .map(|name| name.chars().count() + 2)
        .max()
        .unwrap_or(0)
        .max(NAME_WIDTH);
    let mut header = format!("{:<width$}", "Languages", width = name_width + 2);
    for feature in columns {
        let _ = write!(
            header,
            "{:<width$}",
            feature.label(),
            width = column_width(*feature)
        );
    }
    page.line(&[(header.trim_end(), Some(Highlight::Heading))]);
    for ((_, found), name) in rows.iter().zip(names) {
        let name = format!("  {name:<name_width$}");
        let mut parts: Vec<(String, Option<Highlight>)> = vec![(name, None)];
        for feature in columns {
            let width = column_width(*feature);
            parts.push(if found.contains(feature) {
                (format!("{:<width$}", "✓"), Some(Highlight::Success))
            } else {
                (format!("{:<width$}", "–"), None)
            });
        }
        let borrowed: Vec<(&str, Option<Highlight>)> = parts
            .iter()
            .map(|(text, highlight)| (text.as_str(), *highlight))
            .collect();
        page.line(&borrowed);
    }
}

fn column_width(feature: Feature) -> usize {
    feature.label().len() + 2
}

#[cfg(test)]
mod tests {
    use crate::editor::{Editor, ServerSpec};
    use crate::syntax::{Feature, Indentation, Language, LanguageSpec, Outline};
    use crate::testing::{lines, press, type_text};

    fn notes_spec() -> LanguageSpec {
        LanguageSpec {
            name: "Notes".to_owned(),
            extensions: crate::syntax::owned(&["notes"]),
            file_names: Vec::new(),
            aliases: Vec::new(),
            line_comment: None,
            pairs: &[],
            indentation: Indentation::KEEP,
            grammar: None,
            grammars: &[],
            outline: Outline::NONE,
            features: &[Feature::Highlighting],
            roots: crate::syntax::Roots::default(),
            name_chars: "",
        }
    }

    fn listed(editor: &mut Editor) {
        press(editor, "M-x");
        type_text(editor, "list-languages");
        press(editor, "RET");
    }

    fn line_with(editor: &Editor, text: &str) -> String {
        lines(editor)
            .into_iter()
            .find(|line| line.contains(text))
            .unwrap_or_default()
    }

    #[test]
    fn list_languages_shows_what_works_in_each_language() {
        let mut editor = crate::testing::kits();
        editor.set_linter(
            Language::rust(),
            Some(crate::editor::build::lint::fixtures::cargo_check()),
        );
        editor.add_language_servers(&[ServerSpec {
            name: "rust-analyzer".to_owned(),
            languages: vec![("Rust".to_owned(), "rust".to_owned())],
            programs: vec![("rust-analyzer".to_owned(), Vec::new())],
            install: String::new(),
            runs: false,
        }]);

        listed(&mut editor);

        assert_eq!(editor.buffer().name(), "*Languages*");
        assert!(editor.buffer().is_read_only());
        let header = line_with(&editor, "Languages");
        let column = |label: &str| header.find(label).unwrap();
        let mark =
            |row: &str, label: &str| line_with(&editor, row).chars().nth(column(label)).unwrap();
        assert_eq!(mark("Rust (.rs)", "Highlight"), '✓');
        assert_eq!(mark("Rust (.rs)", "LSP"), '✓');
        assert_eq!(mark("Rust (.rs)", "Complete"), '✓');
        assert_eq!(mark("Rust (.rs)", "Lint"), '✓');
        assert_eq!(mark("Rust (.rs)", "REPL"), '–');
        assert_eq!(mark("TOML (", "Lint"), '–');
        assert_eq!(mark("TOML (", "Indent"), '–');
        assert_eq!(mark("TOML (", "LSP"), '–');
        assert_eq!(mark("TOML (", "Items"), '✓');
        assert_eq!(mark("R (.R .r)", "Indent"), '✓');
        assert_eq!(mark("R (.R .r)", "Items"), '✓');
        assert_eq!(mark("R (.R .r)", "Format"), '✓');
        assert_eq!(mark("R (.R .r)", "Complete"), '✓');
        assert_eq!(mark("R (.R .r)", "REPL"), '✓');
        assert!(
            !header.contains("Fold") && !header.contains("Preview"),
            "{header}"
        );
    }

    #[test]
    fn languages_that_plugins_add_are_listed() {
        let mut editor = Editor::new();
        editor.add_language(Language::new(notes_spec()), None);

        listed(&mut editor);

        assert!(line_with(&editor, "Notes (.notes)").starts_with("  Notes"));
    }

    #[test]
    fn q_closes_the_list() {
        let mut editor = Editor::new();
        listed(&mut editor);

        press(&mut editor, "q");

        assert_ne!(editor.buffer().name(), "*Languages*");
    }
}
