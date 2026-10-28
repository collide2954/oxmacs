(plugin languages
  :version "0.1.3"
  :summary "Python, Go, C, C++, TypeScript, JavaScript, JSON and Markdown modes with their language servers"
  :requires {markdown: "^0.2"}
  :modes [{title: "Python" extensions: ["py" "pyi" "pyw"] comment: "#" roots: ["pyproject.toml" "setup.py" "setup.cfg" "requirements.txt"]}
          {title: "Go" extensions: ["go"] comment: "//" roots: ["go.mod"]}
          {title: "C" extensions: ["c" "h"] comment: "//" roots: ["compile_commands.json" "CMakeLists.txt" ".clangd"]}
          {title: "C++" extensions: ["cc" "cpp" "cxx" "hh" "hpp" "hxx"] comment: "//" roots: ["compile_commands.json" "CMakeLists.txt" ".clangd"]}
          {title: "TypeScript" extensions: ["ts" "mts" "cts"] comment: "//" roots: ["package.json" "tsconfig.json"]}
          {title: "TSX" extensions: ["tsx"] comment: "//" roots: ["package.json" "tsconfig.json"]}
          {title: "JavaScript" extensions: ["js" "mjs" "cjs" "jsx"] comment: "//" roots: ["package.json" "tsconfig.json"]}
          {title: "JSON" extensions: ["json" "jsonc"] comment: "//"}
          {title: "Markdown" extensions: ["md" "markdown"] roots: [".marksman.toml"]}]
  :entry "languages.ox")
