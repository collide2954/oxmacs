;; a rule rewrites a pattern into a template
(defrules swap [] [(swap a b) (let [tmp a] (set! a b) (set! b tmp))]) (let [tmp 1 other 2] (swap tmp other) [tmp other]) ;=> [2 1]
(defrules my-unless [] [(my-unless test body ...) (if test nil (do body ...))]) (my-unless false 1 2) ;=> 2
(defrules ignore [] [(ignore _ x) x]) (ignore 1 2) ;=> 2
(defrules pair-up [] [(pair-up [a b]) {first: a second: b}]) (pair-up [1 2]) ;=> {first: 1 second: 2}
(defrules my-let [] [(my-let ((name value) ...) body ...) ((fn [name ...] body ...) value ...)]) (my-let ((a 1) (b 2)) (+ a b)) ;=> 3
(defrules two [] [(two a b) [a b]]) (two 1) ;! No rule of two matches (two 1)

;; defrules tries its rules in order
(defrules my-or [] [(my-or) false] [(my-or x) x] [(my-or x y ...) (let [t x] (if t t (my-or y ...)))]) [(my-or) (my-or 1) (my-or nil 2) (let [t 5] (my-or nil t))] ;=> [false 1 2 5]
(defrules my-cond [else] [(my-cond (else e)) e] [(my-cond (c e) more ...) (if c e (my-cond more ...))]) (my-cond ((= 1 2) :a) (else :b)) ;=> :b
