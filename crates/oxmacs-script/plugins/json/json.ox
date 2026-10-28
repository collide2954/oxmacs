(module
  (import (ox lexer)
          (json spec :as spec)))

(def (highlighter rules)
  (let [lexer (lexer/spec rules)]
    (fn [_line text state] (lexer/lex lexer text state))))

(def braces {after: #"[{(\[]\s*(//.*)?$" dedent: #"^\s*[}\])]"})

(defmode json "JSON"
  :highlight (highlighter spec/json)
  :indent (assoc braces :width 2))
