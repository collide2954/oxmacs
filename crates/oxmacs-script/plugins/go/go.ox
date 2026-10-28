(module
  (import (ox lexer)
          (go spec :as spec)))

(def (highlighter rules)
  (let [lexer (lexer/spec rules)]
    (fn [_line text state] (lexer/lex lexer text state))))

(def braces {after: #"[{(\[]\s*(//.*)?$" dedent: #"^\s*[}\])]"})

(defmode go "Go"
  :highlight (highlighter spec/go)
  :indent (assoc braces :width 4 :tabs true))

(define-language-server "gopls"
  :languages {"Go" "go"}
  :program "gopls"
  :install "go install golang.org/x/tools/gopls@latest"
  :runs true)
