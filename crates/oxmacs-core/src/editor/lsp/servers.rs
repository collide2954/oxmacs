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
        name: "pyright",
        languages: &[("Python", "python")],
        programs: &[
            ("basedpyright-langserver", &["--stdio"]),
            ("pyright-langserver", &["--stdio"]),
            ("pylsp", &[]),
        ],
        roots: &[
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "requirements.txt",
        ],
        install: "npm install -g pyright",
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
    ServerSpec {
        name: "marksman",
        languages: &[("Markdown", "markdown")],
        programs: &[("marksman", &["server"])],
        roots: &[".marksman.toml"],
        install: "download marksman from https://github.com/artempyanykh/marksman/releases",
        runs: false,
    },
    ServerSpec {
        name: "clangd",
        languages: &[("C", "c"), ("C++", "cpp")],
        programs: &[("clangd", &[])],
        roots: &["compile_commands.json", "CMakeLists.txt", ".clangd"],
        install: "install clangd from your system's packages or https://clangd.llvm.org",
        runs: false,
    },
    ServerSpec {
        name: "gopls",
        languages: &[("Go", "go")],
        programs: &[("gopls", &[])],
        roots: &["go.mod"],
        install: "go install golang.org/x/tools/gopls@latest",
        runs: true,
    },
    ServerSpec {
        name: "typescript-language-server",
        languages: &[
            ("TypeScript", "typescript"),
            ("JavaScript", "javascript"),
            ("TSX", "typescriptreact"),
        ],
        programs: &[("typescript-language-server", &["--stdio"])],
        roots: &["package.json", "tsconfig.json"],
        install: "npm install -g typescript typescript-language-server",
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
