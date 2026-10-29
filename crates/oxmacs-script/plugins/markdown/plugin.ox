(plugin markdown
  :version "0.2.9"
  :summary "Markdown with highlighting, a rendered preview and the marksman language server"
  :commands [markdown-preview]
  :modes [{title: "Markdown" extensions: ["md" "markdown"] roots: [".marksman.toml"]}]
  :entry "markdown.ox")
