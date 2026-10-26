;; promises hold a value that arrives later
(let [p (promise)] (deliver p 5) (await p)) ;=> 5
(let [p (promise)] [(done? p) (do (deliver p 1) (done? p))]) ;=> [false true]
(promise) ;=> #<promise>
(let [p (promise)] (deliver p [1 2]) p) ;=> #<promise [1 2]>
[(promise? (promise)) (promise? 1)] ;=> [true false]

;; spawn runs code as a task and gives a promise of its result
(await (spawn (+ 1 2))) ;=> 3
(done? (spawn 1)) ;=> false
(let [p (spawn 1)] (await p) (done? p)) ;=> true
(def log (atom [])) (let [p (promise)] (spawn (swap! log conj :worker) (deliver p :done)) [(await p) @log]) ;=> [:done [:worker]]
(let [ping (promise) pong (promise)] (spawn (deliver pong (+ (await ping) 1))) (spawn (deliver ping 1)) (await pong)) ;=> 2
(let [tasks (map (fn [n] (spawn (* n n))) [1 2 3])] (map await tasks)) ;=> [1 4 9]
(let [p (spawn 7) g (gen (yield (await p)))] (next g)) ;=> 7

;; failures travel through await
(try (await (spawn (raise (error "bad")))) (catch error e e.message)) ;=> "bad"
(let [p (promise)] (fail p (error "nope")) (try (await p) (catch error e e.message))) ;=> "nope"
(let [p (spawn (/ 1 0))] (try (await p) (catch zero-division-error e :zero))) ;=> :zero

;; mistakes are reported
(await (promise)) ;! Nothing can deliver this promise
(try (await (promise)) (catch error e :stuck)) ;=> :stuck
(let [p (promise)] (deliver p 1) (deliver p 2)) ;! This promise was already delivered
(await 5) ;=> 5
