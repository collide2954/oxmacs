;; ox json reads objects with keyword keys
(module (import (ox json))) (json/read "{{\"name\": \"Ana\", \"age\": 3, \"tags\": [\"a\", null, true, 1.5]}}") ;=> {name: "Ana" age: 3 tags: ["a" nil true 1.5]}
(module (import (ox json))) (json/read "[{{\"b\": 1, \"a\": 2}}]") ;=> [{b: 1 a: 2}]
(module (import (ox json))) (json/read "12345678901234567890") ;=> 1.2345678901234567e19
(module (import (ox json))) (json/read "\"caf\\u00e9\"") ;=> "café"
(module (import (ox json))) (json/read "{{") ;! json/read can't read this: EOF while parsing an object at line 1 column 1
(module (import (ox json))) (json/read 5) ;! json/read expects a string, got 5

;; and writes keywords back as strings
(module (import (ox json))) (json/write {name: "Ana" tags: [1 nil true] nested: {x: 1.5}}) ;=> "{{\"name\":\"Ana\",\"tags\":[1,null,true],\"nested\":{{\"x\":1.5}}}}"
(module (import (ox json))) (json/write ["a\"b\n" :metric '(1 2) #{3}]) ;=> "[\"a\\\"b\\n\",\"metric\",[1,2],[3]]"
(module (import (ox json))) (defrecord point [x y]) (json/write (point 1 2)) ;=> "{{\"x\":1,\"y\":2}}"
(module (import (ox json))) (json/read (json/write {deep: [{a: [1 2.5]}]})) ;=> {deep: [{a: [1 2.5]}]}
(module (import (ox json))) (json/write (fn [] 1)) ;! json/write can't write #<fn>
(module (import (ox json))) (json/write {1 2}) ;! json/write needs keyword or string keys, got 1
(module (import (ox json))) (let [inf (* 1e308 10.0)] (json/write (- inf inf))) ;! json/write can't write nan
