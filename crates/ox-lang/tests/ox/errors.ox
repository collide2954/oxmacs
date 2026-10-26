;; raise and catch
(try (raise (error "boom")) (catch error e e.message)) ;=> "boom"
(try 1 (catch error e :caught)) ;=> 1
(try 1 2) ;=> 2
(try) ;=> nil
(try (raise 5) (catch _ e (+ e 1))) ;=> 6
(try (raise 5) (catch error e :error) (catch _ e :other)) ;=> :other
(try (raise (error "x")) (catch error e (try (raise (error "y")) (catch error f [e.message f.message])))) ;=> ["x" "y"]
(raise (error "uncaught")) ;! uncaught
(raise 5) ;! Raised 5
(error "boom") ;=> #error{message: "boom" traceback: nil}

;; Ox's own errors have types you can catch
(try (/ 1 0) (catch zero-division-error e e.message)) ;=> "Division by zero"
(try (/ 1 0) (catch error e :any-error)) ;=> :any-error
(try (+ 1 :a) (catch type-error e e.message)) ;=> "+ expects numbers, got :a"
(try (undefined-thing) (catch name-error e e.message)) ;=> "undefined-thing is not defined"
(try ((fn [x] x)) (catch arity-error e :arity)) ;=> :arity
(try (nth [1] 5) (catch range-error e :range)) ;=> :range
(try (match 1 [2 :two]) (catch no-match-error e e.message)) ;=> "No match for 1"
(try (/ 1 0) (catch type-error e :type)) ;! Division by zero

;; finally always runs
(def log (atom [])) (try (swap! log conj :body) (finally (swap! log conj :finally))) @log ;=> [:body :finally]
(def log (atom [])) (try (try (raise (error "x")) (finally (swap! log conj :cleanup))) (catch error e (swap! log conj :caught))) @log ;=> [:cleanup :caught]
(def log (atom [])) (try (try (raise (error "x")) (catch error e (raise (error "again"))) (finally (swap! log conj :cleanup))) (catch error e (swap! log conj e.message))) @log ;=> [:cleanup "again"]
(try 1 (finally 2)) ;=> 1

;; deferror defines error types with message templates
(deferror http-error [status url] "Request to {url} failed with {status}") (try (raise (http-error 404 "x.org")) (catch http-error e [e.status e.message])) ;=> [404 "Request to x.org failed with 404"]
(deferror oops []) (try (raise (oops)) (catch error e e.message)) ;=> "oops"
(deferror oops []) (try (raise (oops)) (catch zero-division-error e :wrong) (catch oops e :right)) ;=> :right
(deferror oops []) (oops) ;=> #oops{message: "oops" traceback: nil}
(deferror oops []) (raise (oops)) ;! oops
(deferror parse-error [line {column 0}] "Bad input at {line}:{column}") [(get (parse-error 3) :message) (get (parse-error 3 :column 7) :message)] ;=> ["Bad input at 3:0" "Bad input at 3:7"]
(deferror parse-error [line {column 0}]) (parse-error 3) ;=> #parse-error{line: 3 column: 0 message: "parse-error" traceback: nil}
(defrecord plain [x]) (try (raise (plain 1)) (catch error e :error) (catch _ e :plain)) ;=> :plain

;; errors unwind through calls
(def (inner) (raise (error "deep"))) (def (outer) (+ 1 (inner))) (try (outer) (catch error e e.message)) ;=> "deep"
(def (safe f) (try (f) (catch error e :failed))) (safe (fn [] (/ 1 0))) ;=> :failed
(def (down n) (if (< n 1) (raise (error "done")) (down (- n 1)))) (try (down 1000) (catch error e e.message)) ;=> "done"
(def (f) (try (raise (error "in f")) (catch error e :handled))) [(f) (f)] ;=> [:handled :handled]

;; maybe turns errors into nil
(maybe (/ 1 0)) ;=> nil
(maybe (+ 1 2)) ;=> 3

;; mistakes in try are reported
(try 1 (catch error)) ;! catch takes a type, a name and a body
(catch error e 1) ;! catch belongs inside try

;; errors carry a traceback, innermost frame first
(def (inner) (raise (error "deep"))) (def (outer) (+ 1 (inner))) (try (outer) (catch error e (map :fn e.traceback))) ;=> ["inner" "outer" nil]
(try (/ 1 0) (catch error e (first e.traceback))) ;=> {fn: nil file: "<input>" line: 1 column: 6}
(def (f)
  (/ 1 0))
(try (f) (catch error e (map :line e.traceback)))
;=> [2 3]
(def (f) (/ 1 0)) (try (try (f) (catch type-error e :no)) (catch error e (count e.traceback))) ;=> 2
(def (f) (/ 1 0)) (try (try (f) (catch error e (raise e))) (catch error e (count e.traceback))) ;=> 2
(deferror oops []) (try (raise (oops)) (catch oops e (vector? e.traceback))) ;=> true

;; expect passes quietly and explains what failed
[(expect (= 1 1)) (expect true)] ;=> [true true]
(try (expect (= (+ 1 1) 3)) (catch expectation-failed e e.message)) ;=> "(= (+ 1 1) 3)\n  (+ 1 1) => 2"
(try (expect (and true false)) (catch expectation-failed e e.message)) ;=> "(and true false)"
(pr-str ["a" :b 1]) ;=> "[\"a\" :b 1]"
