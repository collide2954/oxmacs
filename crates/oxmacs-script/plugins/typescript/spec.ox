(module
  (import (ox lexer))
  (export typescript))

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

(def typescript
  (assoc javascript
         :keywords (into javascript.keywords
                         ["abstract" "as" "declare" "enum" "implements" "infer" "interface" "is" "keyof" "module"
                          "namespace" "override" "private" "protected" "public" "readonly" "satisfies" "type"])
         :types #{"any" "bigint" "boolean" "never" "number" "object" "string" "symbol" "unknown" "void"}))

(def (faces spec text)
  (map (fn [span] [(nth span 2) (nth span 0) (nth span 1)]) (get (lexer/lex spec text nil) :spans)))

(test "TypeScript adds its keywords and types to JavaScript's"
  (expect (= (faces typescript "interface A {{ x: number }}") [[:keyword 0 9] [:type 17 23]])))
