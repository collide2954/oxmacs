#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerSpec {
    pub name: &'static str,
    pub languages: &'static [(&'static str, &'static str)],
    pub programs: &'static [(&'static str, &'static [&'static str])],
    pub install: &'static str,
    pub runs: bool,
}

impl ServerSpec {
    pub fn language_id(&self, language: &str) -> Option<&'static str> {
        self.languages
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(language))
            .map(|(_, id)| *id)
    }
}
