;; regexes match, find and capture
(module (import (ox regex))) [(regex/matches? #"\d+" "a1") (regex/matches? #"\d+" "abc") (regex/matches? "^a" "abc")] ;=> [true false true]
(module (import (ox regex))) (regex/find #"(\d+)-(\d+)" "on 2026-10 now") ;=> {text: "2026-10" start: 3 end: 10 groups: ["2026" "10"] named: {}}
(module (import (ox regex))) (get (regex/find #"(?<year>\d{4})-(?<month>\d\d)" "é 2026-10") :named) ;=> {year: "2026" month: "10"}
(module (import (ox regex))) (regex/find #"x" "abc") ;=> nil
(module (import (ox regex))) (map :text (regex/find-all #"\w+" "one, two three")) ;=> ["one" "two" "three"]

;; and replace and split
(module (import (ox regex))) [(regex/replace #"(\w+)@(\w+)" "ada@home bob@work" "$2:$1") (regex/split #",\s*" "a, b,c")] ;=> ["home:ada work:bob" ["a" "b" "c"]]
(module (import (ox regex))) (regex/find "(" "x") ;! Invalid regex (: unclosed group