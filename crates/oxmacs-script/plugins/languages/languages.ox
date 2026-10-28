(module
  (import (languages lexer :as lexer)
          (languages specs :as specs)
          (markdown style :as style)))

(def (highlighter spec)
  (fn [_line text state] (lexer/lex spec text state)))

(def braces {after: #"[{(\[]\s*(//.*)?$" dedent: #"^\s*[}\])]"})

(defmode python "Python"
  :highlight (highlighter specs/python)
  :indent {width: 4 after: #":\s*(#.*)?$" dedent: #"^\s*(else|elif|except|finally)\b"})

(defmode go "Go"
  :highlight (highlighter specs/go)
  :indent (assoc braces :width 4 :tabs true))

(defmode c "C"
  :highlight (highlighter specs/c)
  :indent (assoc braces :width 4))

(defmode c++ "C++"
  :highlight (highlighter specs/cpp)
  :indent (assoc braces :width 4))

(defmode typescript "TypeScript"
  :highlight (highlighter specs/typescript)
  :indent (assoc braces :width 2))

(defmode tsx "TSX"
  :highlight (highlighter specs/typescript)
  :indent (assoc braces :width 2))

(defmode javascript "JavaScript"
  :highlight (highlighter specs/javascript)
  :indent (assoc braces :width 2))

(defmode json "JSON"
  :highlight (highlighter specs/json)
  :indent (assoc braces :width 2))

(defmode markdown "Markdown"
  :highlight style/highlight)

(define-language-server "pyright"
  :languages {"Python" "python"}
  :programs [["basedpyright-langserver" "--stdio"] ["pyright-langserver" "--stdio"] ["pylsp"]]
  :install "npm install -g pyright"
  :runs true)

(define-language-server "gopls"
  :languages {"Go" "go"}
  :program "gopls"
  :install "go install golang.org/x/tools/gopls@latest"
  :runs true)

(define-language-server "clangd"
  :languages {"C" "c" "C++" "cpp"}
  :program "clangd"
  :install "install clangd from your system's packages or https://clangd.llvm.org")

(define-language-server "typescript-language-server"
  :languages {"TypeScript" "typescript" "JavaScript" "javascript" "TSX" "typescriptreact"}
  :program "typescript-language-server"
  :args ["--stdio"]
  :install "npm install -g typescript typescript-language-server"
  :runs true)

(define-language-server "marksman"
  :languages {"Markdown" "markdown"}
  :program "marksman"
  :args ["server"]
  :install "download marksman from https://github.com/artempyanykh/marksman/releases")
