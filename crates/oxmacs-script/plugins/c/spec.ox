(module
  (import (ox lexer))
  (export c))

(def slash-comment {"/*" {close: #"\*/" face: :comment}})

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

(def (faces spec text)
  (map (fn [span] [(nth span 2) (nth span 0) (nth span 1)]) (get (lexer/lex spec text nil) :spans)))

(test "C shows preprocessor lines, its keywords and its constants"
  (expect (= (faces c "#include <stdio.h>") [[:macro 0 18]]))
  (expect (= (faces c "  # define N 10") [[:macro 0 10] [:constant 13 15]]))
  (expect (= (faces c "class A") [])))
