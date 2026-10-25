(module
  (import (ox string) (ox time)
          (calendar ics :as ics))
  (export rule expand civil at-civil))

(def weekdays {"MO" 0 "TU" 1 "WE" 2 "TH" 3 "FR" 4 "SA" 5 "SU" 6})
(def most 2000)

(def (civil millis zone)
  (if zone
    (try (time/local millis zone) (catch error _ (time/local millis)))
    (time/local millis)))

(def (at-civil fields zone)
  (if zone
    (try (time/instant fields :zone zone) (catch error _ (time/instant fields)))
    (time/instant fields)))

(def (day-spec text)
  (let [size (count text)]
    {weekday: (get weekdays (string/upper (string/slice text (- size 2))))
     nth: (string/to-number (string/slice text 0 (- size 2)))}))

(def (numbers text)
  (vec (keep string/to-number (string/split text ","))))

(def (rule text)
  "An RRULE value as a map of :freq, :interval, :count, :until, :byday, :bymonthday and :bymonth."
  (let [parts (reduce (fn [found part]
                        (let [[key value] (string/split part "=" 2)]
                          (assoc found (string/upper key) value)))
                      {} (string/split text ";"))
        given (fn [key] (get parts key))]
    {freq: (string/upper (or (given "FREQ") "DAILY"))
     interval: (max 1 (or (string/to-number (or (given "INTERVAL") "")) 1))
     count: (when (given "COUNT") (string/to-number (given "COUNT")))
     until: (when (given "UNTIL") (get (ics/moment {value: (given "UNTIL") params: {}}) :time))
     byday: (when (given "BYDAY") (vec (map day-spec (string/split (given "BYDAY") ","))))
     bymonthday: (when (given "BYMONTHDAY") (numbers (given "BYMONTHDAY")))
     bymonth: (when (given "BYMONTH") (numbers (given "BYMONTH")))}))

(def (month-days year month spec)
  (let [first-day (time/day-number {year: year month: month day: 1})
        total (time/days-in-month year month)
        all (range first-day (+ first-day total))]
    (cond (spec.byday
           (vec (sort (mapcat (fn [wanted]
                                (let [matching (vec (filter (fn [days] (= (time/weekday-number days) wanted.weekday)) all))]
                                  (cond ((nil? wanted.nth) matching)
                                    ((> wanted.nth 0) (if (<= wanted.nth (count matching)) [(nth matching (dec wanted.nth))] []))
                                    (else (let [at (+ (count matching) wanted.nth)]
                                            (if (>= at 0) [(nth matching at)] []))))))
                              spec.byday))))
      (spec.bymonthday
       (vec (sort (keep (fn [day]
                          (let [day (if (< day 0) (+ total day 1) day)]
                            (when (and (>= day 1) (<= day total)) (+ first-day (dec day)))))
                        spec.bymonthday))))
      ((<= spec.day total) [(+ first-day (dec spec.day))])
      (else []))))

(def (period spec base k)
  (let [step (* k spec.interval)
        start-date (time/date-of base)]
    (cond ((= spec.freq "DAILY") [(+ base step)])
      ((= spec.freq "WEEKLY")
       (let [week (+ (- base (time/weekday-number base)) (* 7 step))]
         (if spec.byday
           (vec (sort (map (fn [wanted] (+ week wanted.weekday)) spec.byday)))
           [(+ base (* 7 step))])))
      ((= spec.freq "MONTHLY")
       (let [months (+ (* start-date.year 12) (dec start-date.month) step)]
         (month-days (// months 12) (inc (mod months 12)) (assoc spec :day start-date.day))))
      ((= spec.freq "YEARLY")
       (let [year (+ start-date.year step)]
         (vec (mapcat (fn [month] (month-days year month (assoc spec :day start-date.day)))
                      (or spec.bymonth [start-date.month])))))
      (else []))))

(def (allowed? spec days)
  (let [date (time/date-of days)]
    (and (or (nil? spec.bymonth) (= spec.freq "YEARLY") (some (fn [month] (= month date.month)) spec.bymonth))
         (or (nil? spec.byday) (not= spec.freq "DAILY") (some (fn [wanted] (= wanted.weekday (time/weekday-number days))) spec.byday)))))

(def (expand event from to)
  "The occurrences of event that overlap from up to to, each a map of :start and :end, at most 2000."
  (let [length (- event.end event.start)
        overlaps (fn [start] (and (< start to) (> (+ start (max length 1)) from)))]
    (if (nil? event.rrule)
      (if (overlaps event.start) [{start: event.start end: event.end}] [])
      (let [spec (rule event.rrule)
            begun (civil event.start event.zone)
            base (time/day-number begun)
            clock {hour: begun.hour minute: begun.minute second: begun.second}
            excluded (set (or event.exdates []))
            limit (if spec.until (min to (inc spec.until)) to)
            wanted (civil from event.zone)
            gap (- (time/day-number wanted) base)
            span (* spec.interval (get {"DAILY" 1 "WEEKLY" 7 "MONTHLY" 31 "YEARLY" 366} spec.freq 1))
            first-k (if (or spec.count (<= gap 0)) 0 (max 0 (dec (// gap span))))
            walk (fn walk [k made found]
                   (if (or (>= k (+ first-k most)) (and spec.count (>= made spec.count)))
                     found
                     (let [days (filter (fn [days] (and (>= days base) (allowed? spec days))) (period spec base k))
                           starts (map (fn [days] (at-civil (merge (time/date-of days) clock) event.zone)) days)
                           step (fn [[made found stopped] start]
                                  (cond (stopped [made found stopped])
                                    ((or (>= start limit) (and spec.count (>= made spec.count))) [made found true])
                                    (else [(inc made)
                                           (if (and (overlaps start) (not (contains? excluded start)))
                                             (conj found {start: start end: (+ start length)})
                                             found)
                                           false])))
                           [made found stopped] (reduce step [made found false] starts)]
                       (if stopped found (walk (inc k) made found)))))]
        (walk first-k 0 [])))))

(def sample-standup
  "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:standup@example.com\r\nDTSTART;TZID=Europe/Paris:20261019T090000\r\nDTEND;TZID=Europe/Paris:20261019T091500\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE\r\nEXDATE;TZID=Europe/Paris:20261021T090000\r\nSUMMARY:Standup\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n")

(test "repeating events follow their rules across daylight saving"
  (let [standup (ics/event-of (first (ics/components (ics/parse sample-standup) "VEVENT")))
        starts (fn [event from to]
                 (map (fn [found] (time/format found.start "%a %d %H:%M" :zone "UTC")) (expand event from to)))
        from (time/instant "2026-10-18T00:00:00Z")
        to (time/instant "2026-11-01T00:00:00Z")]
    (expect (= (starts standup from to) ["Mon 19 07:00" "Mon 26 08:00" "Wed 28 08:00"]))
    (expect (= (count (starts (assoc standup :rrule "FREQ=DAILY;COUNT=3" :exdates []) from to)) 3))
    (expect (= (starts (merge standup {rrule: "FREQ=MONTHLY;BYDAY=-1FR" exdates: []}) from (time/instant "2027-01-01T00:00:00Z"))
               ["Fri 30 08:00" "Fri 27 08:00" "Fri 25 08:00"]))
    (expect (= (starts (merge standup {start: (time/instant "2026-01-31T09:00:00Z") end: (time/instant "2026-01-31T10:00:00Z")
                                       zone: "UTC" rrule: "FREQ=MONTHLY" exdates: []})
                       (time/instant "2026-01-01T00:00:00Z") (time/instant "2026-06-01T00:00:00Z"))
               ["Sat 31 09:00" "Tue 31 09:00" "Sun 31 09:00"]))
    (expect (= (starts (assoc standup :rrule "FREQ=WEEKLY;BYDAY=MO;UNTIL=20261020T000000Z") from to) ["Mon 19 07:00"]))))
