;; quote gives the form as data
'a ;=> a
(quote a) ;=> a
'(1 a "s" [b :k] {c: d} #{e} 99999999999999999999) ;=> (1 a "s" [b :k] {c: d} #{e} 99999999999999999999)
'() ;=> nil
(= '(1 2) (list 1 2)) ;=> true
'a.b ;=> (. a :b)
'{a: 1 a: 2} ;! Duplicate key :a in a map literal
(quote) ;! quote takes one form
(quote a b) ;! quote takes one form

;; quasiquote fills in unquoted values
(def x 5) `(a ~x) ;=> (a 5)
(def xs (list 1 2)) `(a ~@xs b) ;=> (a 1 2 b)
(def xs [1 2]) `[0 ~@xs 3] ;=> [0 1 2 3]
(def x 1) `{k: ~x} ;=> {k: 1}
(def x 1) `#{~x 2} ;=> #{1 2}
`(a (b ~(+ 1 2))) ;=> (a (b 3))
`a ;=> a
`5 ;=> 5
(def x 1) `~x ;=> 1
(def xs nil) `(a ~@xs) ;=> (a)
`(a `(b ~(c ~(+ 1 2)))) ;=> (a (quasiquote (b (unquote (c 3)))))
`~@(list 1) ;! ~@ splices into a list or vector
`(a ~@5) ;! ~@ expects a collection, got 5
(unquote x) ;! ~ is only allowed inside a quasiquote
(unquote-splicing x) ;! ~@ is only allowed inside a quasiquote
