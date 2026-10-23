(module
  (import (ox string) (ox time)
          (calendar ics :as ics))
  (export rule expand day-number date-of weekday-of civil at-civil))

(def weekdays {"MO" 0 "TU" 1 "WE" 2 "TH" 3 "FR" 4 "SA" 5 "SU" 6})
(def most 2000)

(def (day-number year month day)
  "The days from 1970-01-01 to a date."
  (let [y (if (<= month 2) (dec year) year)
        era (// y 400)
        yoe (- y (* era 400))
        doy (+ (// (+ (* 153 (if (> month 2) (- month 3) (+ month 9))) 2) 5) (dec day))
        doe (+ (* yoe 365) (// yoe 4) (- (// yoe 100)) doy)]
    (+ (* era 146097) doe -719468)))

(def (date-of days)
  "The date that many days after 1970-01-01, as a map of :year, :month and :day."
  (let [z (+ days 719468)
        era (// z 146097)
        doe (- z (* era 146097))
        yoe (// (+ (- doe (// doe 1460)) (// doe 36524) (- (// doe 146096))) 365)
        doy (- doe (+ (* 365 yoe) (// yoe 4) (- (// yoe 100))))
        mp (// (+ (* 5 doy) 2) 153)
        day (+ (- doy (// (+ (* 153 mp) 2) 5)) 1)
        month (if (< mp 10) (+ mp 3) (- mp 9))]
    {year: (+ yoe (* era 400) (if (<= month 2) 1 0)) month: month day: day}))

(def (weekday-of days)
  "0 for Monday through 6 for Sunday."
  (mod (+ days 3) 7))

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
  (let [first-day (day-number year month 1)
        total (time/days-in-month year month)
        all (range first-day (+ first-day total))]
    (cond (spec.byday
           (vec (sort (mapcat (fn [wanted]
                                (let [matching (vec (filter (fn [days] (= (weekday-of days) wanted.weekday)) all))]
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
        start-date (date-of base)]
    (cond ((= spec.freq "DAILY") [(+ base step)])
      ((= spec.freq "WEEKLY")
       (let [week (+ (- base (weekday-of base)) (* 7 step))]
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
  (let [date (date-of days)]
    (and (or (nil? spec.bymonth) (= spec.freq "YEARLY") (some (fn [month] (= month date.month)) spec.bymonth))
         (or (nil? spec.byday) (not= spec.freq "DAILY") (some (fn [wanted] (= wanted.weekday (weekday-of days))) spec.byday)))))

(def (expand event from to)
  "The occurrences of event that overlap from up to to, each a map of :start and :end, at most 2000."
  (let [length (- event.end event.start)
        overlaps (fn [start] (and (< start to) (> (+ start (max length 1)) from)))]
    (if (nil? event.rrule)
      (if (overlaps event.start) [{start: event.start end: event.end}] [])
      (let [spec (rule event.rrule)
            begun (civil event.start event.zone)
            base (day-number begun.year begun.month begun.day)
            clock {hour: begun.hour minute: begun.minute second: begun.second}
            excluded (set (or event.exdates []))
            limit (if spec.until (min to (inc spec.until)) to)
            wanted (civil from event.zone)
            gap (- (day-number wanted.year wanted.month wanted.day) base)
            span (* spec.interval (get {"DAILY" 1 "WEEKLY" 7 "MONTHLY" 31 "YEARLY" 366} spec.freq 1))
            first-k (if (or spec.count (<= gap 0)) 0 (max 0 (dec (// gap span))))
            walk (fn walk [k made found]
                   (if (or (>= k (+ first-k most)) (and spec.count (>= made spec.count)))
                     found
                     (let [days (filter (fn [days] (and (>= days base) (allowed? spec days))) (period spec base k))
                           starts (map (fn [days] (at-civil (merge (date-of days) clock) event.zone)) days)
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
