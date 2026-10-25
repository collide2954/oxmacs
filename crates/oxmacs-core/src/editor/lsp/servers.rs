#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerSpec {
    pub name: &'static str,
    pub languages: &'static [(&'static str, &'static str)],
    pub programs: &'static [(&'static str, &'static [&'static str])],
    pub roots: &'static [&'static str],
    pub install: &'static str,
    pub runs: bool,
}

pub const SERVERS: &[ServerSpec] = &[
    ServerSpec {
        name: "rust-analyzer",
        languages: &[("Rust", "rust")],
        programs: &[("rust-analyzer", &[])],
        roots: &["Cargo.toml"],
        install: "rustup component add rust-analyzer",
        runs: true,
    },
    ServerSpec {
        name: "taplo",
        languages: &[("TOML", "toml")],
        programs: &[("taplo", &["lsp", "stdio"])],
        roots: &[],
        install: "cargo install taplo-cli --locked --features lsp",
        runs: true,
    },
];

impl ServerSpec {
    pub fn language_id(&self, language: &str) -> Option<&'static str> {
        self.languages
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(language))
            .map(|(_, id)| *id)
    }
}
