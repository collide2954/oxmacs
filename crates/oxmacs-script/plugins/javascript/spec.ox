(module
  (import (ox lexer))
  (export javascript))

(def slash-comment {"/*" {close: #"\*/" face: :comment}})

(def javascript
  {token: #"(?P<comment>//.*)|(?P<block>/\*|`)|(?P<string>\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*')|(?P<attribute>@[A-Za-z_]\w*)|(?P<number>\b\d[\w.]*)|(?P<call>[A-Za-z_$][\w$]*)\s*\(|(?P<word>[A-Za-z_$][\w$]*)"
   blocks: (assoc slash-comment "`" {close: #"^(?:[^`\\]|\\.)*`" face: :string})
   keywords: #{"async" "await" "break" "case" "catch" "class" "const" "continue" "debugger" "default" "delete"
               "do" "else" "export" "extends" "finally" "for" "from" "function" "if" "import" "in" "instanceof"
               "let" "new" "of" "return" "static" "super" "switch" "this" "throw" "try" "typeof" "var" "void"
               "while" "with" "yield"}
   constants: #{"true" "false" "null" "undefined" "NaN" "Infinity"}
   types: #{}})

(def (faces spec text)
  (map (fn [span] [(nth span 2) (nth span 0) (nth span 1)]) (get (lexer/lex spec text nil) :spans)))

(test "JavaScript keeps template strings open across lines"
  (expect (= (faces javascript "interface A") []))
  (expect (= (lexer/lex javascript "const s = `a ${b}" nil) {spans: [[0 5 :keyword] [10 17 :string]] state: "`"})))
