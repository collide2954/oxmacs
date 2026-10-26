;; ox string splits, joins and trims
(module (import (ox string))) [(string/split "a,b,,c" ",") (string/join ", " ["a" 1 :b]) (string/join ["x" "y"])] ;=> [["a" "b" "" "c"] "a, 1, :b" "xy"]
(module (import (ox string))) [(string/trim "  hi  ") (string/trim-start "  hi ") (string/trim-end " hi  ")] ;=> ["hi" "hi " " hi"]
(module (import (ox string))) [(string/upper "café") (string/lower "ÉCOLE") (string/capitalize "monday") (string/capitalize "")] ;=> ["CAFÉ" "école" "Monday" ""]
(module (import (ox string))) (string/lines "one\ntwo\r\nthree") ;=> ["one" "two" "three"]
(module (import (ox string))) [(string/split "1 .M N... a b.txt" " " 4) (string/split "a" "," 3)] ;=> [["1" ".M" "N..." "a b.txt"] ["a"]]
(module (import (ox string))) (string/chars "hé") ;=> [\h \é]

;; and asks about and changes them
(module (import (ox string))) [(string/starts-with? "weather" "wea") (string/ends-with? "weather" "her") (string/includes? "weather" "eat") (string/blank? " \t")] ;=> [true true true true]
(module (import (ox string))) [(string/index-of "banana" "an") (string/index-of "banana" "x")] ;=> [1 nil]
(module (import (ox string))) (string/replace "a-b-c" "-" "+") ;=> "a+b+c"
(module (import (ox string))) [(string/to-number "42") (string/to-number "+2") (string/to-number "-0") (string/to-number "-7") (string/to-number "1.5") (string/to-number "x") (string/to-number "") (string/to-number "inf")] ;=> [42 2 0 -7 1.5 nil nil nil]
(module (import (ox string))) [(string/length "héllo") (string/slice "weather" 1 4) (string/slice "weather" 4) (string/repeat "ab" 3)] ;=> [5 "eat" "her" "ababab"]
(module (import (ox string))) (string/slice "abc" 2 9) ;! string/slice goes past the end of a 3-character string

;; padding lines things up in columns
(module (import (ox string))) [(string/pad-start "7" 3) (string/pad-end "Mon" 5) (string/pad-start "7" 3 "0") (string/pad-start "long" 2)] ;=> ["  7" "Mon  " "007" "long"]
(module (import (ox string))) (string/pad-start 5 3) ;! string/pad-start expects a string, got 5

;; words, reversing and truncating
(module (import (ox string))) [(string/words "  one two	three  ") (string/reverse "héllo") (string/truncate "hello world" 8) (string/truncate "short" 8)] ;=> [["one" "two" "three"] "olléh" "hello w…" "short"]

;; raw strings keep braces, backslashes and quotes as they are
#r"C:\Users\{name}" ;=> "C:\\Users\\{{name}}"
#r#"{"a": "b"}"# ;=> "{{\"a\": \"b\"}}"
#r##"say "#hi"#"## ;=> "say \"#hi\"#"
(count #r"\n") ;=> 2

(module (import (ox time))) (time/format 0 "%Y" :place "UTC") ;! time/format has no keyword parameter :place
(module (import (ox time))) (time/format 0 "%Y" :zone "UTC") ;=> "1970"
