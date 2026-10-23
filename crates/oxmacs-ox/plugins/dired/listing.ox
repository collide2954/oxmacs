(module
  (import (ox path) (ox string) (ox time))
  (export human-size ls-time sorted next-order render name-column
          entry-at line-of first-entry-line last-entry-line has-parent))

(def units ["K" "M" "G" "T" "P" "E"])

(def (ceil-div a b)
  (// (+ a (- b 1)) b))

(def (human-size bytes)
  (if (< bytes 1024)
    "{bytes}B"
    ((fn step [i unit shown]
       (if (>= i (count units))
         shown
         (let [unit (* unit 1024)
               suffix (nth units i)
               tenths (ceil-div (* bytes 10) unit)
               whole (ceil-div bytes unit)]
           (cond
             [(< tenths 100) "{(// tenths 10)}.{(mod tenths 10)}{suffix}"]
             [(< whole 1024) "{whole}{suffix}"]
             [else (step (+ i 1) unit "{whole}{suffix}")]))))
     0 1 "")))

(def months ["Jan" "Feb" "Mar" "Apr" "May" "Jun" "Jul" "Aug" "Sep" "Oct" "Nov" "Dec"])
(def six-months 15778476000)

(def (two number)
  (string/pad-start (str number) 2 "0"))

(def (ls-time modified now)
  (if (nil? modified)
    ""
    (let [moment (time/local modified)
          month (nth months (- moment.month 1))
          day (string/pad-start (str moment.day) 2)
          age (- now modified)]
      (if (or (neg? age) (> age six-months))
        "{month} {day}  {moment.year}"
        "{month} {day} {(two moment.hour)}:{(two moment.minute)}"))))

(def (sort-key order entry)
  (let [name (string/lower entry.name)]
    (cond
      [(= order :newest) [(not entry.dir) (- (or entry.modified 0)) name]]
      [(= order :largest) [(not entry.dir) (- (or entry.size 0)) name]]
      [else [(not entry.dir) (string/starts-with? name ".") name]])))

(def (sorted entries order)
  (sort-by (fn [entry] (sort-key order entry)) entries))

(def (next-order order)
  (cond [(= order :name) :newest] [(= order :newest) :largest] [else :name]))

(def (order-label order)
  (cond [(= order :newest) "newest first"] [(= order :largest) "largest first"] [else nil]))

(def time-width 12)

(def (size-text entry)
  (if entry.dir "-" (human-size entry.size)))

(def (size-width listing)
  (apply max 1 (map (fn [entry] (string/length (size-text entry))) listing.entries)))

(def (name-column listing)
  (if listing.details (+ 2 (size-width listing) 2 time-width 2) 2))

(def (has-parent listing)
  (some? (path/parent listing.directory)))

(def (first-entry-line listing)
  (if (has-parent listing) 2 1))

(def (last-entry-line listing)
  (let [entries (count listing.entries)]
    (if (= entries 0)
      (if (has-parent listing) 1 0)
      (+ (first-entry-line listing) entries -1))))

(def (entry-at listing line)
  (let [index (- line (first-entry-line listing))]
    (when (and (>= index 0) (< index (count listing.entries)))
      (nth listing.entries index))))

(def (line-of listing name)
  (let [found (first (filter (fn [i] (= (get (nth listing.entries i) :name) name))
                             (range (count listing.entries))))]
    (when (some? found) (+ (first-entry-line listing) found))))

(def (header listing)
  (let [shown (path/abbreviate listing.directory)
        label (order-label listing.order)]
    (if label "  {shown} ({label}):" "  {shown}:")))

(def (entry-name entry)
  (cond
    [entry.dir (str entry.name (path/separator))]
    [entry.link "{entry.name} -> {entry.target}"]
    [else entry.name]))

(def (entry-line listing entry width now)
  (let [mark (get listing.marks entry.name)
        symbol (cond [(= mark :marked) "*"] [(= mark :flagged) "D"] [else " "])
        name (entry-name entry)
        face (cond [entry.dir :link] [entry.link :symlink] [else nil])
        prefix (if listing.details
                 "{symbol} {(string/pad-start (size-text entry) width)}  {(string/pad-end (ls-time entry.modified now) time-width)}  "
                 "{symbol} ")]
    {line: [prefix (if face [name face] name)]
     background: (cond [(= mark :marked) :marked] [(= mark :flagged) :flagged] [else nil])}))

(def (render listing now)
  (let [width (size-width listing)]
    (vec (concat
          [[[(header listing) :heading]]]
          (if (has-parent listing)
            [[(string/repeat " " (name-column listing)) [".." :link]]]
            [])
          (map (fn [entry] (entry-line listing entry width now)) listing.entries)))))
