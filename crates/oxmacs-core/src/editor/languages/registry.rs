use std::ops::Range;
use std::path::Path;

use crate::command::{Scope, ScopeId};
use crate::editor::Editor;
use crate::syntax::{Language, language_for};
use crate::text::BufferId;
use crate::theme::Highlight;

pub type LineHighlighter = fn(&Editor, BufferId, usize) -> Vec<(Range<usize>, Highlight)>;

#[derive(Debug, Clone, Copy)]
pub(in crate::editor) struct Registered {
    pub(in crate::editor) language: Language,
    pub(in crate::editor) scope: Option<ScopeId>,
    pub(in crate::editor) highlight: Option<LineHighlighter>,
}

pub(in crate::editor) fn built_in() -> Vec<Registered> {
    [
        (Language::RUST, Some(Scope::Rust)),
        (Language::TOML, None),
        (Language::MARKDOWN, Some(Scope::Markdown)),
        (Language::R, Some(Scope::R)),
    ]
    .into_iter()
    .map(|(language, scope)| Registered {
        language,
        scope: scope.map(ScopeId::from),
        highlight: None,
    })
    .collect()
}

impl Editor {
    pub fn add_language(&mut self, language: Language, scope: Option<ScopeId>) {
        match self
            .languages
            .iter_mut()
            .find(|known| known.language == language)
        {
            Some(known) => known.scope = scope,
            None => self.languages.push(Registered {
                language,
                scope,
                highlight: None,
            }),
        }
    }

    pub fn set_highlighter(&mut self, language: Language, highlight: LineHighlighter) {
        if let Some(known) = self
            .languages
            .iter_mut()
            .find(|known| known.language == language)
        {
            known.highlight = Some(highlight);
        }
    }

    pub fn languages(&self) -> Vec<Language> {
        self.languages.iter().map(|known| known.language).collect()
    }

    pub fn language_for(&self, path: &Path) -> Option<Language> {
        language_for(&self.languages(), path)
    }

    pub fn buffer_language(&self, id: BufferId) -> Option<Language> {
        let index = self.buffers.position(|buffer| buffer.id() == id)?;
        self.buffers.language(index)
    }

    pub(in crate::editor) fn registered(&self, language: Language) -> Option<&Registered> {
        self.languages
            .iter()
            .find(|known| known.language == language)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::command::CommandSpec;
    use crate::editor::Editor;
    use crate::editor::test_support::{lines, press, type_text};
    use crate::syntax::{Feature, Grammar, Indentation, Language, LanguageSpec, Outline};
    use crate::theme::Highlight;

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

    static CONF_GRAMMAR: Grammar = Grammar::new(
        "conf",
        || tree_sitter_toml_ng::LANGUAGE.into(),
        include_str!("../../../queries/toml/highlights.scm"),
        "",
    );

    static CONF: LanguageSpec = LanguageSpec {
        name: "Conf",
        extensions: &["conf"],
        file_names: &[],
        aliases: &["conf"],
        line_comment: Some("#"),
        pairs: &[],
        indentation: Indentation::KEEP,
        grammar: Some(&CONF_GRAMMAR),
        grammars: &[],
        outline: Outline::NONE,
        features: &[Feature::Highlighting],
    };

    fn notes() -> Language {
        Language::new(&NOTES)
    }

    fn opened(editor: &mut Editor, name: &str, text: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        fs::write(&path, text).unwrap();
        editor.open_file(&path);
        dir
    }

    #[test]
    fn files_open_in_added_languages_with_their_keys() {
        let mut editor = Editor::new();
        let scope = editor.add_scope("Notes");
        let shout = editor.add_command(CommandSpec {
            name: "notes-shout",
            summary: "Shout",
            help: None,
            scope,
            run: |editor| editor.show_message("hey"),
        });
        editor.bind(scope, "C-c C-c", shout).unwrap();
        editor.add_language(notes(), Some(scope));

        let _dir = opened(&mut editor, "plan.notes", "hello\n");

        assert_eq!(editor.language(), Some(notes()));
        assert_eq!(editor.buffer_language(editor.buffer().id()), Some(notes()));
        assert!(
            editor.mode_line().ends_with("(Notes)"),
            "{}",
            editor.mode_line()
        );
        press(&mut editor, "C-c C-c");
        assert_eq!(editor.message(), Some("hey"));
        assert!(editor.languages().contains(&notes()));
        assert_eq!(editor.language_for(Path::new("x.NOTES")), Some(notes()));
    }

    #[test]
    fn languages_nobody_added_stay_plain_text() {
        let mut editor = Editor::new();

        let _dir = opened(&mut editor, "plan.notes", "hello\n");

        assert_eq!(editor.language(), None);
        assert_eq!(editor.language_for(Path::new("x.notes")), None);
    }

    #[test]
    fn highlighters_style_the_lines_of_their_language() {
        let mut editor = Editor::new();
        editor.add_language(notes(), None);
        editor.set_highlighter(notes(), |_, _, line| {
            vec![(0..line + 1, Highlight::Keyword)]
        });

        let _dir = opened(&mut editor, "plan.notes", "one\ntwo\n");

        assert_eq!(editor.line_style(0).spans, [(0..1, Highlight::Keyword)]);
        assert_eq!(editor.line_style(1).spans, [(0..2, Highlight::Keyword)]);
    }

    #[test]
    fn languages_without_a_grammar_edit_like_plain_text() {
        let mut editor = Editor::new();
        editor.add_language(notes(), None);
        let _dir = opened(&mut editor, "plan.notes", "");

        type_text(&mut editor, "{");
        press(&mut editor, "RET TAB");

        assert_eq!(editor.buffer().contents(), "{\n\t");
    }

    #[test]
    fn markdown_fences_highlight_added_languages() {
        let mut editor = Editor::new();
        editor.add_language(Language::new(&CONF), None);

        let _dir = opened(&mut editor, "notes.md", "```conf\nname = 1\n```\n");
        editor.prepare_display();

        assert_eq!(
            editor.line_style(1).spans,
            [(0..4, Highlight::Function), (7..8, Highlight::Constant)]
        );
    }

    #[test]
    fn the_doctor_lists_added_languages() {
        let mut editor = Editor::new();
        editor.add_language(notes(), None);

        press(&mut editor, "M-x");
        type_text(&mut editor, "oxmacs-doctor");
        press(&mut editor, "RET");

        let shown = lines(&editor);
        assert!(
            shown
                .iter()
                .any(|line| line.trim_start().starts_with("Notes (.notes)")),
            "{shown:?}"
        );
    }
}
