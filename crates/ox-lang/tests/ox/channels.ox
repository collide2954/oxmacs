;; channels carry values between tasks
(let [ch (chan)] (send ch 1) (send ch 2) [(receive ch) (receive ch)]) ;=> [1 2]
(let [ch (chan)] (send ch 1) (close ch) [(receive ch) (done? ch) (receive ch) (done? ch)]) ;=> [1 false nil true]
(let [ch (chan)] (send ch nil) [(receive ch) (done? ch)]) ;=> [nil false]
(chan) ;=> #<chan>
[(chan? (chan)) (chan? [])] ;=> [true false]
(let [ch (chan)] (= ch ch)) ;=> true

;; a full channel makes send wait, and an empty one makes receive wait
(def log (atom [])) (let [ch (chan 1)] (spawn (send ch :a) (swap! log conj :sent-a) (send ch :b) (swap! log conj :sent-b)) [(receive ch) (receive ch) @log]) ;=> [:a :b [:sent-a]]
(let [ch (chan 2)] (spawn ((fn step [i] (when (< i 5) (send ch i) (step (+ i 1)))) 0) (close ch)) ((fn drain [out] (let [x (receive ch)] (if (done? ch) out (drain (conj out x))))) [])) ;=> [0 1 2 3 4]

;; channels carry data, copied as it would be for a worker
(let [ch (chan) v {a: [1 "two" #{:three}]}] (send ch v) (receive ch)) ;=> {a: [1 "two" #{:three}]}
(defrecord point [x y]) (let [ch (chan)] (send ch (point 1 2)) (receive ch)) ;=> #point{x: 1 y: 2}
(let [ch (chan)] (send ch (fn [] 1))) ;! #<fn> can't be sent through a channel
(let [ch (chan)] (send ch (atom 1))) ;! #<atom 1> can't be sent through a channel

;; mistakes are reported
(let [ch (chan)] (close ch) (send ch 1)) ;! This channel is closed
(receive (chan)) ;! Nothing can send to this channel
(let [ch (chan 1)] (send ch 1) (send ch 2)) ;! Nothing can receive from this channel
(chan 0) ;! chan expects a size of at least 1, got 0
(receive 5) ;! receive expects a channel, got 5
