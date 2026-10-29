(plugin rust
  :version "0.1.0"
  :opt-in true
  :summary "Rust with highlighting, cargo builds and lints, and the rust-analyzer language server"
  :modes [{title: "Rust" grammar: "rust" extensions: ["rs"] comment: "//"
           roots: ["Cargo.toml"] locate: ["cargo" "locate-project" "--workspace" "--message-format" "plain"]}]
  :builds {check: "cargo check" build: "cargo build" test: "cargo test" run: "cargo run" clippy: "cargo clippy"}
  :mode-keys {"Rust" {"C-c C-c C-k" rust-check
                      "C-c C-c C-b" rust-build
                      "C-c C-c C-t" rust-test
                      "C-c C-c C-r" rust-run
                      "C-c C-c C-l" rust-clippy
                      "C-c C-f" format-buffer
                      "C-c !" list-diagnostics}}
  :settings {lint: {type: [:check :clippy :off] default: :check doc: "What cargo runs over the workspace each time a Rust file is saved"}}
  :lint {program: "cargo" args: [:lint "--workspace" "--all-targets" "--message-format=json"] format: :cargo}
  :error-patterns [#"^\s*--> (?P<file>.+):(?P<line>\d+):(?P<column>\d+):?\s*$"
                   #"panicked at (?P<file>.+):(?P<line>\d+):(?P<column>\d+):?\s*$"]
  :entry "rust.ox")
