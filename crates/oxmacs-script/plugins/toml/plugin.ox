(plugin toml
  :version "0.1.0"
  :opt-in true
  :summary "TOML with highlighting, indentation and the taplo language server"
  :modes [{title: "TOML" grammar: "toml" extensions: ["toml"] file-names: ["Cargo.lock"] comment: "#"}]
  :entry "toml.ox")
