#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSpec {
    pub name: String,
    pub languages: Vec<(String, String)>,
    pub programs: Vec<(String, Vec<String>)>,
    pub install: String,
    pub runs: bool,
}

impl ServerSpec {
    pub fn language_id(&self, language: &str) -> Option<&str> {
        self.languages
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(language))
            .map(|(_, id)| id.as_str())
    }
}
