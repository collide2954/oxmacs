use std::fmt;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use tree_sitter::{Node, Query};

use crate::syntax::Syntax;
use crate::syntax::highlight::Highlighter;
use crate::text::Buffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    Highlighting,
    Indentation,
    Comments,
    Pairs,
    Formatting,
    Lint,
    Motion,
    Items,
    Folding,
    Preview,
    Completion,
    Repl,
    Lsp,
}

impl Feature {
    pub const ALL: [Self; 13] = [
        Self::Highlighting,
        Self::Indentation,
        Self::Comments,
        Self::Pairs,
        Self::Formatting,
        Self::Lint,
        Self::Motion,
        Self::Items,
        Self::Folding,
        Self::Preview,
        Self::Completion,
        Self::Repl,
        Self::Lsp,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Highlighting => "Highlight",
            Self::Indentation => "Indent",
            Self::Comments => "Comment",
            Self::Pairs => "Pairs",
            Self::Formatting => "Format",
            Self::Lint => "Lint",
            Self::Motion => "Motion",
            Self::Items => "Items",
            Self::Folding => "Fold",
            Self::Preview => "Preview",
            Self::Completion => "Complete",
            Self::Repl => "REPL",
            Self::Lsp => "LSP",
        }
    }
}

pub struct Grammar {
    name: &'static str,
    language: fn() -> tree_sitter::Language,
    highlights: &'static str,
    injections: &'static str,
    highlighter: OnceLock<Option<Highlighter>>,
    injector: OnceLock<Option<Query>>,
}

impl Grammar {
    pub const fn new(
        name: &'static str,
        language: fn() -> tree_sitter::Language,
        highlights: &'static str,
        injections: &'static str,
    ) -> Self {
        Self {
            name,
            language,
            highlights,
            injections,
            highlighter: OnceLock::new(),
            injector: OnceLock::new(),
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub(super) fn language(&self) -> tree_sitter::Language {
        (self.language)()
    }

    pub(super) fn highlighter(&self) -> Option<&Highlighter> {
        self.highlighter
            .get_or_init(|| Highlighter::new(&self.language(), self.highlights))
            .as_ref()
    }

    pub(super) fn injections(&self) -> Option<&Query> {
        self.injector
            .get_or_init(|| {
                (!self.injections.is_empty())
                    .then(|| Query::new(&self.language(), self.injections).ok())
                    .flatten()
            })
            .as_ref()
    }
}

impl fmt::Debug for Grammar {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Grammar").field(&self.name).finish()
    }
}

pub type IndentGuess = fn(&Syntax, &Buffer, usize, usize) -> Option<usize>;

#[derive(Debug, Clone, Copy)]
pub struct Indentation {
    pub unit: usize,
    pub continues: bool,
    pub fresh_words: &'static [&'static str],
    pub from_owner: bool,
    pub hanging: bool,
    pub guess: Option<IndentGuess>,
}

impl Indentation {
    pub const KEEP: Self = Self {
        unit: 4,
        continues: false,
        fresh_words: &[],
        from_owner: false,
        hanging: false,
        guess: None,
    };
}

#[derive(Clone, Copy)]
pub struct Outline {
    pub item: fn(Node<'_>, &Buffer) -> Option<String>,
    pub context: fn(Node<'_>) -> bool,
    pub defun: fn(Node<'_>) -> bool,
    pub sections: bool,
    pub statements: &'static [&'static str],
}

impl Outline {
    pub const NONE: Self = Self {
        item: |_, _| None,
        context: |_| false,
        defun: |_| false,
        sections: false,
        statements: &[],
    };
}

impl fmt::Debug for Outline {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Outline")
            .field("sections", &self.sections)
            .field("statements", &self.statements)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Roots {
    pub markers: Vec<String>,
    pub locate: Option<Vec<String>>,
}

impl Roots {
    pub fn marked(markers: &[&str]) -> Self {
        Self {
            markers: owned(markers),
            locate: None,
        }
    }
}

pub fn owned(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[derive(Debug, Clone)]
pub struct LanguageSpec {
    pub name: String,
    pub extensions: Vec<String>,
    pub file_names: Vec<String>,
    pub aliases: Vec<String>,
    pub line_comment: Option<String>,
    pub pairs: &'static [(char, char)],
    pub indentation: Indentation,
    pub grammar: Option<&'static Grammar>,
    pub grammars: &'static [&'static Grammar],
    pub outline: Outline,
    pub features: &'static [Feature],
    pub roots: Roots,
}

impl LanguageSpec {
    pub fn named(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            extensions: Vec::new(),
            file_names: Vec::new(),
            aliases: Vec::new(),
            line_comment: None,
            pairs: &[],
            indentation: Indentation::KEEP,
            grammar: None,
            grammars: &[],
            outline: Outline::NONE,
            features: &[],
            roots: Roots::default(),
        }
    }
}

#[derive(Clone)]
pub struct Language(Arc<LanguageSpec>);

impl Language {
    pub fn new(spec: LanguageSpec) -> Self {
        Self(Arc::new(spec))
    }

    pub fn spec(&self) -> &LanguageSpec {
        &self.0
    }

    pub fn name(&self) -> &str {
        &self.0.name
    }

    pub fn extensions(&self) -> &[String] {
        &self.0.extensions
    }

    pub fn aliases(&self) -> &[String] {
        &self.0.aliases
    }

    pub fn line_comment(&self) -> Option<&str> {
        self.0.line_comment.as_deref()
    }

    pub fn pairs(&self) -> &'static [(char, char)] {
        self.0.pairs
    }

    pub fn supports(&self, feature: Feature) -> bool {
        self.0.features.contains(&feature)
    }

    pub fn indentation(&self) -> Indentation {
        self.0.indentation
    }

    pub fn grammar(&self) -> Option<&'static Grammar> {
        self.0.grammar
    }

    pub fn outline(&self) -> Outline {
        self.0.outline
    }

    pub fn roots(&self) -> &Roots {
        &self.0.roots
    }

    fn named_file(&self, path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                self.0
                    .file_names
                    .iter()
                    .any(|known| name.eq_ignore_ascii_case(known))
            })
    }

    fn has_extension(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                self.0
                    .extensions
                    .iter()
                    .any(|known| extension.eq_ignore_ascii_case(known))
            })
    }

    pub(super) fn injected(&self, name: &str, known: &[Self]) -> Option<&'static Grammar> {
        self.0
            .grammars
            .iter()
            .copied()
            .find(|grammar| grammar.name.eq_ignore_ascii_case(name))
            .or_else(|| {
                known
                    .iter()
                    .find(|language| {
                        language
                            .0
                            .aliases
                            .iter()
                            .any(|alias| alias.eq_ignore_ascii_case(name))
                    })
                    .and_then(|language| language.0.grammar)
            })
    }
}

impl PartialEq for Language {
    fn eq(&self, other: &Self) -> bool {
        self.0.name == other.0.name
    }
}

impl Eq for Language {}

impl fmt::Debug for Language {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0.name)
    }
}

pub fn language_for(languages: &[Language], path: &Path) -> Option<Language> {
    languages
        .iter()
        .find(|language| language.named_file(path))
        .or_else(|| {
            languages
                .iter()
                .find(|language| language.has_extension(path))
        })
        .cloned()
}
#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::syntax::Syntax;
    use crate::theme::Highlight;

    static CONF_GRAMMAR: Grammar = Grammar::new(
        "conf",
        || tree_sitter_toml_ng::LANGUAGE.into(),
        include_str!("../../queries/toml/highlights.scm"),
        "",
    );

    fn conf() -> Language {
        Language::new(LanguageSpec {
            extensions: owned(&["conf"]),
            file_names: owned(&["Procfile"]),
            aliases: owned(&["conf"]),
            line_comment: Some("#".to_owned()),
            grammar: Some(&CONF_GRAMMAR),
            features: &[Feature::Highlighting],
            ..LanguageSpec::named("Conf")
        })
    }

    fn notes() -> Language {
        Language::new(LanguageSpec {
            extensions: owned(&["notes"]),
            features: &[Feature::Highlighting],
            ..LanguageSpec::named("Notes")
        })
    }

    fn synced(language: Language, known: &[Language], text: &str) -> (Buffer, Syntax) {
        let mut buffer = Buffer::with_text("t", text);
        let mut syntax = Syntax::knowing(language, known);
        syntax.sync(&mut buffer);
        (buffer, syntax)
    }

    #[test]
    fn languages_beyond_the_built_ins_are_found_and_highlighted() {
        let known = [Language::rust(), conf()];

        assert_eq!(language_for(&known, Path::new("app.CONF")), Some(conf()));
        assert_eq!(language_for(&known, Path::new("Procfile")), Some(conf()));
        assert_eq!(language_for(&known, Path::new("Cargo.toml")), None);
        let (buffer, syntax) = synced(conf(), &known, "name = 1\n");
        assert_eq!(
            syntax.highlights(&buffer, 0),
            [(0..4, Highlight::Function), (7..8, Highlight::Constant)]
        );
    }

    #[test]
    fn languages_without_a_grammar_parse_nothing() {
        let (buffer, syntax) = synced(notes(), &[notes()], "{\nx\n");

        assert!(!syntax.parses());
        assert!(syntax.tree().is_none());
        assert_eq!(syntax.highlights(&buffer, 0), []);
        assert_eq!(syntax.indentation(&buffer, 1, 8), None);
        assert_eq!(syntax.items(&buffer), []);
    }

    #[test]
    fn languages_are_the_same_when_their_names_are() {
        assert_eq!(conf(), conf());
        assert_ne!(conf(), Language::toml());
        assert_eq!(format!("{:?}", notes()), "Notes");
    }
}
