(plugin languages
  :version "0.1.0"
  :summary "Python, Go, C, C++, TypeScript, JavaScript, JSON and Markdown modes with their language servers"
  :requires {markdown: "^0.2"}
  :modes [{name: "Python" extensions: ["py" "pyi" "pyw"] comment: "#"}
          {name: "Go" extensions: ["go"] comment: "//"}
          {name: "C" extensions: ["c" "h"] comment: "//"}
          {name: "C++" extensions: ["cc" "cpp" "cxx" "hh" "hpp" "hxx"] comment: "//"}
          {name: "TypeScript" extensions: ["ts" "mts" "cts"] comment: "//"}
          {name: "TSX" extensions: ["tsx"] comment: "//"}
          {name: "JavaScript" extensions: ["js" "mjs" "cjs" "jsx"] comment: "//"}
          {name: "JSON" extensions: ["json" "jsonc"] comment: "//"}
          {name: "Markdown" extensions: ["md" "markdown"]}]
  :entry "languages.ox")
