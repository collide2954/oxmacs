(module
  (import (languages lexer :as lexer)
          (languages specs :as specs)
          (markdown style :as style)))

(def (highlighter spec)
  (fn [_line text state] (lexer/lex spec text state)))

(def braces {after: #"[{(\[]\s*(//.*)?$" dedent: #"^\s*[}\])]"})

(defmode python "Python"
  :extensions ["py" "pyi" "pyw"]
  :comment "#"
  :highlight (highlighter specs/python)
  :indent {width: 4 after: #":\s*(#.*)?$" dedent: #"^\s*(else|elif|except|finally)\b"})

(defmode go "Go"
  :extensions ["go"]
  :comment "//"
  :highlight (highlighter specs/go)
  :indent (assoc braces :width 4 :tabs true))

(defmode c "C"
  :extensions ["c" "h"]
  :comment "//"
  :highlight (highlighter specs/c)
  :indent (assoc braces :width 4))

(defmode c++ "C++"
  :extensions ["cc" "cpp" "cxx" "hh" "hpp" "hxx"]
  :comment "//"
  :highlight (highlighter specs/cpp)
  :indent (assoc braces :width 4))

(defmode typescript "TypeScript"
  :extensions ["ts" "mts" "cts"]
  :comment "//"
  :highlight (highlighter specs/typescript)
  :indent (assoc braces :width 2))

(defmode tsx "TSX"
  :extensions ["tsx"]
  :comment "//"
  :highlight (highlighter specs/typescript)
  :indent (assoc braces :width 2))

(defmode javascript "JavaScript"
  :extensions ["js" "mjs" "cjs" "jsx"]
  :comment "//"
  :highlight (highlighter specs/javascript)
  :indent (assoc braces :width 2))

(defmode json "JSON"
  :extensions ["json" "jsonc"]
  :comment "//"
  :highlight (highlighter specs/json)
  :indent (assoc braces :width 2))

(defmode markdown "Markdown"
  :extensions ["md" "markdown"]
  :highlight style/highlight)

(define-language-server "pyright"
  :languages {"Python" "python"}
  :programs [["basedpyright-langserver" "--stdio"] ["pyright-langserver" "--stdio"] ["pylsp"]]
  :roots ["pyproject.toml" "setup.py" "setup.cfg" "requirements.txt"]
  :install "npm install -g pyright"
  :runs true)

(define-language-server "gopls"
  :languages {"Go" "go"}
  :program "gopls"
  :roots ["go.mod"]
  :install "go install golang.org/x/tools/gopls@latest"
  :runs true)

(define-language-server "clangd"
  :languages {"C" "c" "C++" "cpp"}
  :program "clangd"
  :roots ["compile_commands.json" "CMakeLists.txt" ".clangd"]
  :install "install clangd from your system's packages or https://clangd.llvm.org")

(define-language-server "typescript-language-server"
  :languages {"TypeScript" "typescript" "JavaScript" "javascript" "TSX" "typescriptreact"}
  :program "typescript-language-server"
  :args ["--stdio"]
  :roots ["package.json" "tsconfig.json"]
  :install "npm install -g typescript typescript-language-server"
  :runs true)

(define-language-server "marksman"
  :languages {"Markdown" "markdown"}
  :program "marksman"
  :args ["server"]
  :roots [".marksman.toml"]
  :install "download marksman from https://github.com/artempyanykh/marksman/releases")
