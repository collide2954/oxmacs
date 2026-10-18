use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
}

impl Language {
    pub fn for_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        extension.eq_ignore_ascii_case("rs").then_some(Self::Rust)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
        }
    }
}

#[derive(Debug)]
pub struct Syntax {
    language: Language,
}

impl Syntax {
    pub fn new(language: Language) -> Self {
        Self { language }
    }

    pub fn language(&self) -> Language {
        self.language
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_follow_file_extensions() {
        assert_eq!(
            Language::for_path(Path::new("src/main.rs")),
            Some(Language::Rust)
        );
        assert_eq!(
            Language::for_path(Path::new("LIB.RS")),
            Some(Language::Rust)
        );
        assert_eq!(Language::for_path(Path::new("notes.txt")), None);
        assert_eq!(Language::for_path(Path::new("Makefile")), None);
    }
}
