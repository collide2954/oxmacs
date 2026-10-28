(plugin languages
  :version "0.1.1"
  :summary "Python, Go, C, C++, TypeScript, JavaScript, JSON and Markdown modes with their language servers"
  :requires {markdown: "^0.2"}
  :modes [{title: "Python" extensions: ["py" "pyi" "pyw"] comment: "#"}
          {title: "Go" extensions: ["go"] comment: "//"}
          {title: "C" extensions: ["c" "h"] comment: "//"}
          {title: "C++" extensions: ["cc" "cpp" "cxx" "hh" "hpp" "hxx"] comment: "//"}
          {title: "TypeScript" extensions: ["ts" "mts" "cts"] comment: "//"}
          {title: "TSX" extensions: ["tsx"] comment: "//"}
          {title: "JavaScript" extensions: ["js" "mjs" "cjs" "jsx"] comment: "//"}
          {title: "JSON" extensions: ["json" "jsonc"] comment: "//"}
          {title: "Markdown" extensions: ["md" "markdown"]}]
  :entry "languages.ox")
