(define-language-server "taplo"
  :languages {"TOML" "toml"}
  :program "taplo"
  :args ["lsp" "stdio"]
  :install "cargo install taplo-cli --locked --features lsp"
  :runs true)
