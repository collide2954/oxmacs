(module
  (import (ox file) (ox path) (ox string) (ox time))
  (export ls-time next-order render name-column
          entry-at line-of first-entry-line last-entry-line has-parent))

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

(def (next-order order)
  (cond [(= order :name) :newest] [(= order :newest) :largest] [else :name]))

(def (order-label order)
  (cond [(= order :newest) "newest first"] [(= order :largest) "largest first"] [else nil]))

(def time-width 12)

(def (size-text entry)
  (if entry.dir "-" (file/size-text entry.size)))

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

(def ^:shadow (line-of listing name)
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
