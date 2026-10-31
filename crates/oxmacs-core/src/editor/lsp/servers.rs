#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSpec {
    pub name: String,
    pub languages: Vec<(String, String)>,
    pub programs: Vec<(String, Vec<String>)>,
    pub install: String,
    pub runs: bool,
    pub packages: Vec<(String, String)>,
}

impl ServerSpec {
    pub fn package_for<'a>(&self, family: &[&'a str]) -> Option<(&'a str, &str)> {
        family.iter().find_map(|kind| {
            self.packages
                .iter()
                .find(|(known, _)| known.eq_ignore_ascii_case(kind))
                .map(|(_, package)| (*kind, package.as_str()))
        })
    }

    pub fn language_id(&self, language: &str) -> Option<&str> {
        self.languages
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(language))
            .map(|(_, id)| id.as_str())
    }
}
