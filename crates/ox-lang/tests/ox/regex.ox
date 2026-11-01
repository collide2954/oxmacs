;; regexes find and capture, given the text first
(module (import (ox regex))) (regex/find "on 2026-10 now" #"(\d+)-(\d+)") ;=> {text: "2026-10" start: 3 end: 10 groups: ["2026" "10"] named: {}}
(module (import (ox regex))) (get (regex/find "é 2026-10" #"(?<year>\d{4})-(?<month>\d\d)") :named) ;=> {year: "2026" month: "10"}
(module (import (ox regex))) (regex/find "abc" #"x") ;=> nil
(module (import (ox regex))) (map :text (regex/find-all "one, two three" #"\w+")) ;=> ["one" "two" "three"]
(module (import (ox regex))) (map :start (regex/find-all "é1 é22 é3" #"\d+")) ;=> [1 4 8]
(module (import (ox regex))) (regex/find "x" "(") ;! Invalid regex (: unclosed group
(module (import (ox regex))) (regex/find #"\d" "1") ;! regex/find expects text to search, got #"\d"; it takes the text first, then the pattern

;; the string functions take a regex where they take text to look for
(module (import (ox string))) [(string/includes? "a1" #"\d+") (string/includes? "abc" #"\d+") (string/includes? "abc" "^a")] ;=> [true false false]
(module (import (ox string))) [(string/replace "ada@home bob@work" #"(\w+)@(\w+)" "$2:$1") (string/replace "a$1b" "$1" "-")] ;=> ["home:ada work:bob" "a-b"]
(module (import (ox string))) [(string/split "a, b,c" #",\s*") (string/split "a1b2c" #"\d" 2)] ;=> [["a" "b" "c"] ["a" "b2c"]]
(module (import (ox string))) (string/split "abc" 5) ;! string/split expects a string or a regex, got 5

;; a regex literal is made once, has the type :regex and prints as a literal
[(type #"\d+") (type (regex "a"))] ;=> [:regex :regex]
(pr-str [#"\d+" #"""a"b"""]) ;=> "[#\"\\d+\" #\"\"\"a\"b\"\"\"]"
(let [f (fn [] #"x")] (identical? (f) (f))) ;=> true
(fn [] #"(") ;! Invalid regex (: unclosed group