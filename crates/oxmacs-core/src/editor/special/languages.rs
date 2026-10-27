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

    fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }

    fn fixed_name(&self) -> bool {
        true
    }
}

impl Editor {
    pub(in crate::editor) fn list_languages(&mut self) {
        let mut page = Page::default();
        table(&mut page, &self.languages());
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

fn table(page: &mut Page, languages: &[Language]) {
    let names: Vec<String> = languages
        .iter()
        .map(|language| {
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
    for feature in Feature::ALL {
        let _ = write!(
            header,
            "{:<width$}",
            feature.label(),
            width = column_width(feature)
        );
    }
    page.line(&[(header.trim_end(), Some(Highlight::Heading))]);
    for (language, name) in languages.iter().zip(names) {
        let name = format!("  {name:<name_width$}");
        let mut parts: Vec<(String, Option<Highlight>)> = vec![(name, None)];
        for feature in Feature::ALL {
            let width = column_width(feature);
            parts.push(if language.supports(feature) {
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
    use crate::editor::Editor;
    use crate::syntax::{Feature, Indentation, Language, LanguageSpec, Outline};
    use crate::testing::{lines, press, type_text};

    static NOTES: LanguageSpec = LanguageSpec {
        name: "Notes",
        extensions: &["notes"],
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
        let mut editor = Editor::new();

        listed(&mut editor);

        assert_eq!(editor.buffer().name(), "*Languages*");
        assert!(editor.buffer().is_read_only());
        let header = line_with(&editor, "Languages");
        let column = |label: &str| header.find(label).unwrap();
        let mark =
            |row: &str, label: &str| line_with(&editor, row).chars().nth(column(label)).unwrap();
        assert_eq!(mark("Rust (.rs)", "Highlight"), '✓');
        assert_eq!(mark("Rust (.rs)", "LSP"), '–');
        assert_eq!(mark("Rust (.rs)", "Lint"), '✓');
        assert_eq!(mark("TOML (", "Lint"), '–');
        assert_eq!(mark("R (.R .r)", "Indent"), '✓');
        assert_eq!(mark("R (.R .r)", "Items"), '✓');
        assert_eq!(mark("R (.R .r)", "Format"), '✓');
        assert_eq!(mark("R (.R .r)", "Complete"), '✓');
        assert_eq!(mark("R (.R .r)", "REPL"), '✓');
        assert_eq!(mark("Rust (.rs)", "REPL"), '–');
        assert_eq!(mark("TOML (", "Items"), '✓');
        assert_eq!(mark("TOML (", "Preview"), '–');
    }

    #[test]
    fn languages_that_plugins_add_are_listed() {
        let mut editor = Editor::new();
        editor.add_language(Language::new(&NOTES), None);

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
