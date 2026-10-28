(module
  (import (ox lexer))
  (export python))

(def python
  {token: #"(?P<comment>#.*)|[rRbBuUfF]{0,2}(?P<block>\"\"\"|''')|(?P<string>[rRbBuUfF]{0,2}(?:\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*'))|(?P<attribute>@[A-Za-z_][\w.]*)|(?P<number>\b\d[\w.]*)|(?P<call>[A-Za-z_]\w*)\s*\(|(?P<word>[A-Za-z_]\w*)"
   blocks: {"\"\"\"" {close: #"^(?:[^\\]|\\.)*?\"\"\"" face: :string}
            "'''" {close: #"^(?:[^\\]|\\.)*?'''" face: :string}}
   keywords: #{"and" "as" "assert" "async" "await" "break" "case" "class" "continue" "def" "del" "elif" "else"
               "except" "finally" "for" "from" "global" "if" "import" "in" "is" "lambda" "match" "nonlocal"
               "not" "or" "pass" "raise" "return" "try" "while" "with" "yield"}
   constants: #{"True" "False" "None" "self" "cls"}
   types: #{"bool" "bytes" "dict" "float" "int" "list" "object" "set" "str" "tuple" "type"}})

(def (faces spec text)
  (map (fn [span] [(nth span 2) (nth span 0) (nth span 1)]) (get (lexer/lex spec text nil) :spans)))

(test "Python shows keywords, decorators, prefixed strings and docstrings"
  (expect (= (faces python "@cache") [[:attribute 0 6]]))
  (expect (= (faces python "def f(x: int) -> None:  # done")
             [[:keyword 0 3] [:function 4 5] [:type 9 12] [:constant 17 21] [:comment 24 30]]))
  (expect (= (faces python "s = rb'\\x00' + f\"{{x}}\"") [[:string 4 12] [:string 15 21]]))
  (expect (= (lexer/lex python "    r\"\"\"Doc" nil) {spans: [[4 11 :string]] state: "\"\"\""}))
  (expect (= (lexer/lex python "ends \\\"\"\" here\"\"\" pass" "\"\"\"")
             {spans: [[0 17 :string] [18 22 :keyword]] state: nil})))
