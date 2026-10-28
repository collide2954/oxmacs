(module
  (import (ox lexer))
  (export go))

(def slash-comment {"/*" {close: #"\*/" face: :comment}})

(def go
  {token: #"(?P<comment>//.*)|(?P<block>/\*|`)|(?P<string>\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*')|(?P<number>\b\d[\w.]*)|(?P<call>[A-Za-z_]\w*)\s*\(|(?P<word>[A-Za-z_]\w*)"
   blocks: (assoc slash-comment "`" {close: #"`" face: :string})
   keywords: #{"break" "case" "chan" "const" "continue" "default" "defer" "else" "fallthrough" "for" "func" "go"
               "goto" "if" "import" "interface" "map" "package" "range" "return" "select" "struct" "switch"
               "type" "var"}
   constants: #{"true" "false" "nil" "iota"}
   types: #{"any" "bool" "byte" "comparable" "complex64" "complex128" "error" "float32" "float64" "int" "int8"
            "int16" "int32" "int64" "rune" "string" "uint" "uint8" "uint16" "uint32" "uint64" "uintptr"}})

(def (faces spec text)
  (map (fn [span] [(nth span 2) (nth span 0) (nth span 1)]) (get (lexer/lex spec text nil) :spans)))

(test "Go shows raw strings across lines and its built-in types"
  (expect (= (faces go "func (r *Reader) Read(p []byte) (n int, err error) {{")
             [[:keyword 0 4] [:function 17 21] [:type 26 30] [:type 35 38] [:type 44 49]]))
  (expect (= (lexer/lex go "s := `raw" nil) {spans: [[5 9 :string]] state: "`"})))
