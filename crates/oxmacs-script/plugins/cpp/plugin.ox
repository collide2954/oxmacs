(plugin cpp
  :version "0.1.0"
  :opt-in true
  :summary "C++ with highlighting, indentation and the clangd language server"
  :modes [{title: "C++" extensions: ["cc" "cpp" "cxx" "hh" "hpp" "hxx"] comment: "//"
           roots: ["compile_commands.json" "CMakeLists.txt" ".clangd"]}])
