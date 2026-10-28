(module
  (import (ox lexer))
  (export cpp))

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

(def cpp
  (assoc c
         :keywords (into c-keywords
                         ["alignas" "alignof" "catch" "class" "concept" "consteval" "constexpr" "constinit" "co_await"
                          "co_return" "co_yield" "decltype" "delete" "explicit" "export" "final" "friend" "mutable"
                          "namespace" "new" "noexcept" "operator" "override" "private" "protected" "public" "requires"
                          "static_assert" "template" "this" "throw" "try" "typeid" "typename" "using" "virtual"])
         :constants #{"NULL" "nullptr" "true" "false"}
         :types (into c-types ["char8_t" "char16_t" "char32_t" "wchar_t"])))

(def (faces spec text)
  (map (fn [span] [(nth span 2) (nth span 0) (nth span 1)]) (get (lexer/lex spec text nil) :spans)))

(test "C++ adds its keywords and nullptr to C's"
  (expect (= (faces cpp "class A : public B {{ auto p = nullptr; }};")
             [[:keyword 0 5] [:keyword 10 16] [:keyword 21 25] [:constant 30 37]])))
