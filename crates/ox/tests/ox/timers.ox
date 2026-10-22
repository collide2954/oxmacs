;; sleep pauses a task without holding up the others
(do (sleep 5) :woke) ;=> :woke
(def log (atom [])) (let [late (spawn (sleep 20) (swap! log conj :late))] (spawn (swap! log conj :early)) (await late) @log) ;=> [:early :late]
(sleep -1) ;=> nil
(sleep 1.5) ;=> nil
(sleep "soon") ;! sleep expects a number of milliseconds, got "soon"

;; every repeats a function until it returns :stop
(def n (atom 0)) (await (every 5 (fn [] (if (< (swap! n inc) 3) nil :stop)))) @n ;=> 3

;; ox time measures and reads times
(module (import (ox time))) [(time/seconds 2) (time/minutes 30) (time/hours 1) (time/days 1) (time/millis 5)] ;=> [2000 1800000 3600000 86400000 5]
(module (import (ox time))) (time/seconds 1.5) ;=> 1500.0
(module (import (ox time))) (> (time/now) 1700000000000) ;=> true
(module (import (ox time))) (time/parse "2026-10-22T14:05") ;=> {year: 2026 month: 10 day: 22 hour: 14 minute: 5 second: 0 weekday: :thursday}
(module (import (ox time))) (time/parse "2026-10-25") ;=> {year: 2026 month: 10 day: 25 hour: 0 minute: 0 second: 0 weekday: :sunday}
(module (import (ox time))) (time/parse "soon") ;! time/parse can't read "soon" as a date or a time
(module (import (ox time))) (time/parse) ;! time/parse takes 1 argument, got 0
