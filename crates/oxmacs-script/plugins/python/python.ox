(module
  (import (ox lexer)
          (python spec :as spec)))

(def (highlighter rules)
  (let [lexer (lexer/spec rules)]
    (fn [_line text state] (lexer/lex lexer text state))))

(defmode python "Python"
  :highlight (highlighter spec/python)
  :indent {width: 4 after: #":\s*(#.*)?$" dedent: #"^\s*(else|elif|except|finally)\b"})

(define-language-server "pyright"
  :languages {"Python" "python"}
  :programs [["basedpyright-langserver" "--stdio"] ["pyright-langserver" "--stdio"] ["pylsp"]]
  :install "npm install -g pyright"
  :runs true)
