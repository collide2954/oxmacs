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

;; moments in time zones, civil arithmetic and formatting
(module (import (ox time))) (time/instant "2026-10-23T05:43:15.656Z") ;=> 1792734195656
(module (import (ox time))) (time/instant "2026-10-23T09:30:00+02:00") ;=> 1792740600000
(module (import (ox time))) (time/instant "2026-10-23T09:30" :zone "Europe/Paris") ;=> 1792740600000
(module (import (ox time))) (time/instant {year: 2026 month: 10 day: 23 hour: 9 minute: 30} :zone "Europe/Paris") ;=> 1792740600000
(module (import (ox time))) (time/instant "soon") ;! time/instant can't read "soon" as a moment
(module (import (ox time))) (time/local 1792740600000 "UTC") ;=> {year: 2026 month: 10 day: 23 hour: 7 minute: 30 second: 0 weekday: :friday}
(module (import (ox time))) (time/local 0 "Mars/Base") ;! There is no time zone called Mars/Base
(module (import (ox time))) (time/add 1774744200000 {days: 1} :zone "Europe/Paris") ;=> 1774827000000
(module (import (ox time))) (time/add 1769860800000 {months: 1 hours: 2} :zone "UTC") ;=> 1772287200000
(module (import (ox time))) (time/add 0 {fortnights: 1}) ;! time/add takes years, months, weeks, days, hours, minutes and seconds, not :fortnights
(module (import (ox time))) (time/format 1792740600000 "%H:%M %A" :zone "Europe/Paris") ;=> "09:30 Friday"
(module (import (ox time))) [(time/days-in-month 2028 2) (time/days-in-month 2026 4)] ;=> [29 30]
