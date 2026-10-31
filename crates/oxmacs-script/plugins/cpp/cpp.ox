(module
  (import (ox lexer)
          (cpp spec :as spec)))

(def (highlighter rules)
  (let [lexer (lexer/spec rules)]
    (fn [_line text state] (lexer/lex lexer text state))))

(def braces {after: #"[{(\[]\s*(//.*)?$" dedent: #"^\s*[}\])]"})

(defmode c++ "C++"
  :highlight (highlighter spec/cpp)
  :indent (assoc braces :width 4))

(define-language-server "clangd"
  :languages {"C" "c" "C++" "cpp"}
  :program "clangd"
  :install "install clangd from your system's packages or https://clangd.llvm.org"
  :packages {fedora: "clang-tools-extra" debian: "clangd" arch: "clang" suse: "clang-tools" alpine: "clang-extra-tools"})
