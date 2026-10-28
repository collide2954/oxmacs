(module
  (import (ox lexer))
  (export json))

(def slash-comment {"/*" {close: #"\*/" face: :comment}})

(def json
  {token: #"(?P<key>\"(?:[^\"\\]|\\.)*\")\s*:|(?P<string>\"(?:[^\"\\]|\\.)*\")|(?P<comment>//.*)|(?P<block>/\*)|(?P<number>-?\b\d[\d.eE+-]*)|(?P<word>[A-Za-z_]\w*)"
   blocks: slash-comment
   keywords: #{}
   constants: #{"true" "false" "null"}
   types: #{}})

(def (faces spec text)
  (map (fn [span] [(nth span 2) (nth span 0) (nth span 1)]) (get (lexer/lex spec text nil) :spans)))

(test "JSON keys are attributes and other strings are strings"
  (expect (= (faces json "{{\"name\": \"ox\", \"n\": -1.5e3, \"ok\": true}} // note")
             [[:attribute 1 7] [:string 9 13] [:attribute 15 18] [:constant 20 26] [:attribute 28 32]
              [:constant 34 38] [:comment 40 47]])))
