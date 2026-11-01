;; deeply nested values are written, compared and read without running out of stack
(def (nest n) (loop [i 0 v []] (if (< i n) (again (inc i) [v]) v))) (count (pr-str (nest 20000))) ;=> 40002
(module (import (ox json))) (def (nest n) (loop [i 0 v []] (if (< i n) (again (inc i) [v]) v))) (count (json/write (nest 20000))) ;=> 40002
(module (import (ox json))) (def (nest n) (loop [i 0 v {a: 1}] (if (< i n) (again (inc i) {a: v}) v))) (count (json/write (nest 20000))) ;=> 120007
(def (nest n) (loop [i 0 v []] (if (< i n) (again (inc i) [v]) v))) [(count (sort [(nest 20000) (nest 20000)])) (compare (nest 20000) (nest 20000)) (compare (nest 20000) (nest 19999))] ;=> [2 0 1]
(module (import (ox xml))) (def (el n) (loop [i 0 e {tag: "a" attrs: {} children: ["x"]}] (if (< i n) (again (inc i) {tag: "a" attrs: {} children: [e]}) e))) [(count (xml/write (el 20000))) (xml/text (el 20000))] ;=> [140008 "x"]
(module (import (ox xml) (ox string))) (:tag (xml/read (str (string/repeat "<a>" 900) "x" (string/repeat "</a>" 900)))) ;=> "a"
(module (import (ox xml) (ox string))) (xml/read (str (string/repeat "<a>" 1500) (string/repeat "</a>" 1500))) ;! xml/read can't read elements nested more than 1000 deep

;; sizes that would take more memory than one value may are refused instead of crashing
(module (import (ox string))) (string/repeat "ab" 100000000000) ;! string/repeat would make 100000000000 copies, more than the 512 MB one value can take
(module (import (ox string))) (string/pad-start "a" 100000000000) ;! string/pad-start would make 100000000000 columns, more than the 512 MB one value can take
(range 100000000000) ;! range would make 100000000000 numbers, more than the 512 MB one value can take
[(count (range 3 100000000000 10000000000)) (count (range 10 0 -3)) (count (range 5 5))] ;=> [10 4 0]