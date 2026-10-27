(module
  (import (ox string) (ox time))
  (export rows runtime spark sorts next-sort))

(def columns [:name :kind :status :pid :time :cpu :memory :history :details])
(def titles {name: "Name" kind: "Kind" status: "Status" pid: "PID" time: "Time"
             cpu: "CPU" memory: "Memory" history: "History" details: "Details"})
(def dropped [:details :kind :history :pid :time :memory :cpu])
(def right #{:pid :time :cpu :memory})
(def gap 2)
(def least-details 12)
(def bars ["▁" "▂" "▃" "▄" "▅" "▆" "▇" "█"])
(def spark-width 12)
(def sorts [:started :name :kind :status :pid :time :cpu :memory])
(def help "RET visit  TAB details  k stop  K kill  s sort  S reverse  / filter  g refresh  q quit")

(def (next-sort current)
  (let [at (or (some (fn [i] (when (= (nth sorts i) current) i)) (range (count sorts))) 0)]
    (nth sorts (mod (+ at 1) (count sorts)))))

(def (two n) (string/pad-start (str n) 2 "0"))

(def (runtime ms)
  (let [seconds (max 0 (// ms 1000))
        hours (// seconds 3600)
        minutes (mod (// seconds 60) 60)
        left (mod seconds 60)]
    (cond ((> hours 0) (str hours "h " (two minutes) "m"))
      ((> minutes 0) (str minutes "m " (two left) "s"))
      (else (str left "s")))))

(def (megabytes bytes) (str (// (+ bytes 1048575) 1048576) " MB"))

(def (tenths x) (str (/ (round (* x 10)) 10.0)))

(def (recent samples n) (drop (max 0 (- (count samples) n)) samples))

(def (spark samples)
  (let [shown (recent samples spark-width)
        top (max 1.0 (apply max 0.0 shown))]
    (apply str (map (fn [x] (nth bars (min 7 (int (* 8 (/ x top)))))) shown))))

(def (live? p) (not (contains? #{:exited :failed} p.status)))

(def (status-text p)
  (cond ((= p.status :exited) (if (and p.exit (not= p.exit 0)) (str "exit " p.exit) "exited"))
    (else (name p.status))))

(def (status-face p)
  (cond ((= p.status :running) :success)
    ((contains? #{:starting :stopping} p.status) :warning)
    ((= (status-text p) "exited") :comment)
    (else :error)))

(def (command-line p) (string/join " " (concat [p.program] p.args)))

(def (cells p now)
  (let [latest (fn [samples] (when (and (live? p) (not (empty? samples))) (last samples)))
        cpu (latest p.cpu)
        memory (latest p.memory)]
    {name: p.name kind: p.kind status: (status-text p)
     pid: (if p.pid (str p.pid) "-")
     time: (runtime (- (or p.ended now) p.started))
     cpu: (if cpu (str (tenths cpu) "%") "-")
     memory: (if memory (megabytes memory) "-")
     history: (if (live? p) (spark p.cpu) "")
     details: (command-line p)}))

(def (span shown widths details)
  (+ (reduce + 0 (map (fn [column] (if (= column :details) details (get widths column))) shown))
     (* gap (max 0 (- (count shown) 1)))))

(def (fit widths room)
  ((fn step [shown details left]
     (if (or (<= (span shown widths details) room) (empty? left))
       [shown details]
       (let [cut (first left)
             kept (vec (remove (fn [column] (= column cut)) shown))]
         (if (= cut :details)
           (let [spare (- room (span kept widths 0) gap)]
             (if (>= spare least-details)
               [(conj kept :details) spare]
               (step kept details (rest left))))
           (step kept details (rest left))))))
   columns (get widths :details) dropped))

(def (sort-key p now sorting)
  (cond ((= sorting :time) (- (or p.ended now) p.started))
    ((= sorting :cpu) (or (last p.cpu) 0))
    ((= sorting :memory) (or (last p.memory) 0))
    ((= sorting :pid) (or p.pid 0))
    ((= sorting :status) (status-text p))
    (else (get p sorting))))

(def (matching processes query)
  (if (string/blank? query)
    processes
    (let [wanted (string/lower query)]
      (filter (fn [p]
                (some (fn [text] (string/includes? (string/lower text) wanted))
                      [p.name p.kind (command-line p)]))
              processes))))

(def (arranged processes now options)
  (let [sorted (sort-by (fn [p] (sort-key p now options.sort)) (matching processes options.filter))]
    (if options.descending (reverse sorted) sorted)))

(def (aligned column text width)
  (cond ((= column :details) (string/truncate text width))
    ((contains? right column) (string/pad-start text width))
    (else (string/pad-end text width))))

(def (face-of column p)
  (cond ((not (live? p)) (if (= column :status) (status-face p) :comment))
    ((= column :name) :link)
    ((= column :status) (status-face p))
    ((= column :history) :constant)
    ((= column :details) :comment)
    (else nil)))

(def (laid-out shown widths details texts faces)
  (vec (map (fn [i]
              (let [column (nth shown i)
                    text (aligned column (get texts column) (if (= column :details) details (get widths column)))
                    spaced (if (< (+ i 1) (count shown)) (str text "  ") text)
                    face (get faces column)]
                (if face [spaced face] spaced)))
            (range (count shown)))))

(def (title-text column options)
  (let [title (get titles column)]
    (if (= column options.sort) (str title (if options.descending " ▼" " ▲")) title)))

(def (details-rows p now)
  (let [field (fn [label value] {line: [(str "      " (string/pad-end label 13)) [value :comment]] id: p.id})
        env (keys (or p.env {}))]
    (vec (concat
          [(field "Command" (command-line p))
           (field "Folder" (or p.directory "-"))
           (field "Status" (str (status-text p) " after " (runtime (- (or p.ended now) p.started))))
           (field "Started" (time/format p.started "%Y-%m-%d %H:%M:%S"))
           (field "Buffer" (or p.buffer "-"))]
          (map (fn [key] (field (if (= key (first env)) "Environment" "") (str key "=" (get p.env key)))) env)))))

(def (rows processes now width options)
  (let [shown-processes (arranged processes now options)
        header [{line: [["Processes" :heading]]}
                {line: [[(string/truncate help width) :comment]]}
                {line: (if (string/blank? options.filter) [] [["Filter: " :comment] options.filter])}]]
    (if (empty? shown-processes)
      (conj header {line: [[(if (string/blank? options.filter) "Nothing running" "Nothing matches the filter") :comment]]})
      (let [table (map (fn [p] (cells p now)) shown-processes)
            heading (into {} (map (fn [column] [column (title-text column options)]) columns))
            widths (into {} (map (fn [column]
                                   [column (apply max (map (fn [texts] (count (get texts column))) (conj (vec table) heading)))])
                                 columns))
            [shown details] (fit widths width)
            title {line: [[(string/trim-end (apply str (map (fn [part] (if (vector? part) (first part) part))
                                                            (laid-out shown widths details heading {}))))
                           :comment]]}]
        (vec (concat header [title]
                     (mapcat (fn [pair]
                               (let [p (first pair)
                                     texts (second pair)
                                     faces (into {} (map (fn [column] [column (face-of column p)]) columns))
                                     row {line: (laid-out shown widths details texts faces) id: p.id}]
                                 (if (contains? options.open p.id)
                                   (concat [row] (details-rows p now))
                                   [row])))
                             (zip shown-processes table))))))))
