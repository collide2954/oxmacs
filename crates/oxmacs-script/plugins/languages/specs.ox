(module
  (import (languages lexer :as lexer))
  (export python go c cpp javascript typescript json))

(def slash-comment {"/*" {close: #"\*/" face: :comment}})

(def python
  {token: #"(?P<comment>#.*)|[rRbBuUfF]{0,2}(?P<block>\"\"\"|''')|(?P<string>[rRbBuUfF]{0,2}(?:\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*'))|(?P<attribute>@[A-Za-z_][\w.]*)|(?P<number>\b\d[\w.]*)|(?P<call>[A-Za-z_]\w*)\s*\(|(?P<word>[A-Za-z_]\w*)"
   blocks: {"\"\"\"" {close: #"^(?:[^\\]|\\.)*?\"\"\"" face: :string}
            "'''" {close: #"^(?:[^\\]|\\.)*?'''" face: :string}}
   keywords: #{"and" "as" "assert" "async" "await" "break" "case" "class" "continue" "def" "del" "elif" "else"
               "except" "finally" "for" "from" "global" "if" "import" "in" "is" "lambda" "match" "nonlocal"
               "not" "or" "pass" "raise" "return" "try" "while" "with" "yield"}
   constants: #{"True" "False" "None" "self" "cls"}
   types: #{"bool" "bytes" "dict" "float" "int" "list" "object" "set" "str" "tuple" "type"}})

(def go
  {token: #"(?P<comment>//.*)|(?P<block>/\*|`)|(?P<string>\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*')|(?P<number>\b\d[\w.]*)|(?P<call>[A-Za-z_]\w*)\s*\(|(?P<word>[A-Za-z_]\w*)"
   blocks: (assoc slash-comment "`" {close: #"`" face: :string})
   keywords: #{"break" "case" "chan" "const" "continue" "default" "defer" "else" "fallthrough" "for" "func" "go"
               "goto" "if" "import" "interface" "map" "package" "range" "return" "select" "struct" "switch"
               "type" "var"}
   constants: #{"true" "false" "nil" "iota"}
   types: #{"any" "bool" "byte" "comparable" "complex64" "complex128" "error" "float32" "float64" "int" "int8"
            "int16" "int32" "int64" "rune" "string" "uint" "uint8" "uint16" "uint32" "uint64" "uintptr"}})

(def c-keywords
  #{"auto" "break" "case" "const" "continue" "default" "do" "else" "enum" "extern" "for" "goto" "if" "inline"
    "register" "restrict" "return" "sizeof" "static" "struct" "switch" "typedef" "union" "volatile" "while"})

(def c-types
  #{"bool" "char" "double" "float" "int" "long" "short" "signed" "unsigned" "void" "size_t" "ptrdiff_t"
    "int8_t" "int16_t" "int32_t" "int64_t" "uint8_t" "uint16_t" "uint32_t" "uint64_t"})

(def c
  {token: #"(?P<comment>//.*)|(?P<block>/\*)|(?P<macro>^\s*#\s*include\s*[<\"][^>\"]*[>\"]?|^\s*#\s*\w+)|(?P<string>\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*')|(?P<number>\b\d[\w.']*)|(?P<call>[A-Za-z_]\w*)\s*\(|(?P<word>[A-Za-z_]\w*)"
   blocks: slash-comment
   keywords: c-keywords
   constants: #{"NULL" "true" "false"}
   types: c-types})

(def cpp
  (assoc c
         :keywords (into c-keywords
                         ["alignas" "alignof" "catch" "class" "concept" "consteval" "constexpr" "constinit" "co_await"
                          "co_return" "co_yield" "decltype" "delete" "explicit" "export" "final" "friend" "mutable"
                          "namespace" "new" "noexcept" "operator" "override" "private" "protected" "public" "requires"
                          "static_assert" "template" "this" "throw" "try" "typeid" "typename" "using" "virtual"])
         :constants #{"NULL" "nullptr" "true" "false"}
         :types (into c-types ["char8_t" "char16_t" "char32_t" "wchar_t"])))

(def javascript
  {token: #"(?P<comment>//.*)|(?P<block>/\*|`)|(?P<string>\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*')|(?P<attribute>@[A-Za-z_]\w*)|(?P<number>\b\d[\w.]*)|(?P<call>[A-Za-z_$][\w$]*)\s*\(|(?P<word>[A-Za-z_$][\w$]*)"
   blocks: (assoc slash-comment "`" {close: #"^(?:[^`\\]|\\.)*`" face: :string})
   keywords: #{"async" "await" "break" "case" "catch" "class" "const" "continue" "debugger" "default" "delete"
               "do" "else" "export" "extends" "finally" "for" "from" "function" "if" "import" "in" "instanceof"
               "let" "new" "of" "return" "static" "super" "switch" "this" "throw" "try" "typeof" "var" "void"
               "while" "with" "yield"}
   constants: #{"true" "false" "null" "undefined" "NaN" "Infinity"}
   types: #{}})

(def typescript
  (assoc javascript
         :keywords (into javascript.keywords
                         ["abstract" "as" "declare" "enum" "implements" "infer" "interface" "is" "keyof" "module"
                          "namespace" "override" "private" "protected" "public" "readonly" "satisfies" "type"])
         :types #{"any" "bigint" "boolean" "never" "number" "object" "string" "symbol" "unknown" "void"}))

(def json
  {token: #"(?P<key>\"(?:[^\"\\]|\\.)*\")\s*:|(?P<string>\"(?:[^\"\\]|\\.)*\")|(?P<comment>//.*)|(?P<block>/\*)|(?P<number>-?\b\d[\d.eE+-]*)|(?P<word>[A-Za-z_]\w*)"
   blocks: slash-comment
   keywords: #{}
   constants: #{"true" "false" "null"}
   types: #{}})

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

(test "Go shows raw strings across lines and its built-in types"
  (expect (= (faces go "func (r *Reader) Read(p []byte) (n int, err error) {{")
             [[:keyword 0 4] [:function 17 21] [:type 26 30] [:type 35 38] [:type 44 49]]))
  (expect (= (lexer/lex go "s := `raw" nil) {spans: [[5 9 :string]] state: "`"})))

(test "C and C++ show preprocessor lines and their own keywords"
  (expect (= (faces c "#include <stdio.h>") [[:macro 0 18]]))
  (expect (= (faces c "  # define N 10") [[:macro 0 10] [:constant 13 15]]))
  (expect (= (faces c "class A") []))
  (expect (= (faces cpp "class A : public B {{ auto p = nullptr; }};")
             [[:keyword 0 5] [:keyword 10 16] [:keyword 21 25] [:constant 30 37]])))

(test "TypeScript adds its keywords and types to JavaScript's"
  (expect (= (faces javascript "interface A") []))
  (expect (= (faces typescript "interface A {{ x: number }}") [[:keyword 0 9] [:type 17 23]]))
  (expect (= (lexer/lex javascript "const s = `a ${{b}}" nil) {spans: [[0 5 :keyword] [10 17 :string]] state: "`"})))

(test "JSON keys are attributes and other strings are strings"
  (expect (= (faces json "{{\"name\": \"ox\", \"n\": -1.5e3, \"ok\": true}} // note")
             [[:attribute 1 7] [:string 9 13] [:attribute 15 18] [:constant 20 26] [:attribute 28 32]
              [:constant 34 38] [:comment 40 47]])))
