(module
  (import (ox lexer)
          (typescript spec :as spec)))

(def (highlighter rules)
  (let [lexer (lexer/spec rules)]
    (fn [_line text state] (lexer/lex lexer text state))))

(def braces {after: #"[{(\[]\s*(//.*)?$" dedent: #"^\s*[}\])]"})

(defmode typescript "TypeScript"
  :highlight (highlighter spec/typescript)
  :indent (assoc braces :width 2))

(defmode tsx "TSX"
  :highlight (highlighter spec/typescript)
  :indent (assoc braces :width 2))

(define-language-server "typescript-language-server"
  :languages {"TypeScript" "typescript" "JavaScript" "javascript" "TSX" "typescriptreact"}
  :program "typescript-language-server"
  :args ["--stdio"]
  :install "npm install -g typescript typescript-language-server"
  :runs true)
