(plugin c
  :version "0.1.1"
  :opt-in true
  :summary "C with highlighting, indentation and the clangd language server"
  :modes [{title: "C" extensions: ["c" "h"] comment: "//" roots: ["compile_commands.json" "CMakeLists.txt" ".clangd"]}])
