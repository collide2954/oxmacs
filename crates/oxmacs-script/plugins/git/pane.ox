(module
  (import (ox string)
          (git diff :as diff)
          (git status :as status))
  (export render overview pairs nothing))

(def separator " │ ")
(def narrowest 12)
(def labels {untracked: "Untracked" unstaged: "Unstaged" staged: "Staged"})

(def (add out row line)
  {lines: (conj out.lines (if (map? line) (merge line row) (assoc row :line line)))})

(def (blank out)
  (add out {kind: :blank} ""))

(def (digits hunks)
  (string/length
   (str (apply max 1 (map (fn [hunk] (max (+ hunk.old-start hunk.old-count)
                                          (+ hunk.new-start hunk.new-count)))
                          hunks)))))

(def (numbered value width)
  (if (nil? value) (string/repeat " " width) (string/pad-start (str value) width)))

(def (flush found removed added)
  (reduce (fn [found row] (conj found [(get removed row) (get added row)]))
          found (range (max (count removed) (count added)))))

(def (pairs hunk)
  (let [lines hunk.lines
        n (count lines)]
    ((fn walk [i found removed added]
       (if (>= i n)
         (flush found removed added)
         (let [kind (get (nth lines i) :kind)]
           (cond
             [(and (= kind :removed) (empty? added)) (walk (+ i 1) found (conj removed i) added)]
             [(= kind :removed) (walk (+ i 1) (flush found removed added) [i] [])]
             [(= kind :added) (walk (+ i 1) found removed (conj added i))]
             [(= kind :context) (walk (+ i 1) (conj (flush found removed added) [i i]) [] [])]
             [else (walk (+ i 1) found removed added)]))))
     0 [] [] [])))

(def (word-marks hunk)
  (reduce (fn [marks pair]
            (let [left (nth pair 0)
                  right (nth pair 1)]
              (if (and (some? left) (some? right) (not (= left right)))
                (if-let [found (diff/words (diff/shown (get (nth hunk.lines left) :text))
                                           (diff/shown (get (nth hunk.lines right) :text)))]
                  (assoc marks left (nth found 0) right (nth found 1))
                  marks)
                marks)))
          {} (pairs hunk)))

(def (marked text ranges face)
  (let [n (string/length text)]
    ((fn walk [at i out]
       (if (>= i (count ranges))
         (if (< at n) (conj out (string/slice text at n)) out)
         (let [range (nth ranges i)
               start (min n (nth range 0))
               end (min n (nth range 1))
               out (if (< at start) (conj out (string/slice text at start)) out)]
           (walk (max at end) (+ i 1)
                 (if (< start end) (conj out [(string/slice text start end) face :pill]) out)))))
     0 0 [])))

(def (unified out file index hunk width)
  (let [marks (word-marks hunk)
        lines hunk.lines
        n (count lines)]
    ((fn walk [j old new out]
       (if (>= j n)
         out
         (let [line (nth lines j)
               kind line.kind
               text (diff/shown line.text)
               left (when (or (= kind :context) (= kind :removed)) old)
               right (when (or (= kind :context) (= kind :added)) new)
               gutter [[(numbered left width) :comment] " " [(numbered right width) :comment] " "]
               body (cond
                      [(= kind :note) [[text :comment]]]
                      [(= kind :removed) (vec (concat [["-" :error]] (marked text (get marks j []) :error)))]
                      [(= kind :added) (vec (concat [["+" :success]] (marked text (get marks j []) :success)))]
                      [else [" " text]])
               shown (vec (concat gutter body))]
           (walk (+ j 1)
                 (if (nil? left) old (+ old 1))
                 (if (nil? right) new (+ new 1))
                 (add out {kind: :line section: file.section path: file.path hunk: index at: j}
                      (cond
                        [(= kind :removed) {line: shown background: :removed}]
                        [(= kind :added) {line: shown background: :added}]
                        [else shown]))))))
     0 hunk.old-start hunk.new-start out)))

(def (clip text width)
  (if (<= (string/length text) width)
    (string/pad-end text width)
    (str (string/slice text 0 (max 0 (- width 1))) "…")))

(def (visible ranges text width)
  (if (<= (string/length text) width)
    ranges
    (keep (fn [range]
            (let [end (min (nth range 1) (- width 1))]
              (when (< (nth range 0) end) [(nth range 0) end])))
          ranges)))

(def (line-number hunk at old)
  (+ (if old hunk.old-start hunk.new-start)
     (count (filter (fn [i]
                      (let [kind (get (nth hunk.lines i) :kind)]
                        (or (= kind :context) (= kind (if old :removed :added)))))
                    (range at)))))

(def (side hunk at old marks width text-width)
  (if (nil? at)
    [(string/repeat " " (+ width 1 text-width))]
    (let [line (nth hunk.lines at)
          number (numbered (line-number hunk at old) width)
          text (diff/shown line.text)
          shown (clip text text-width)
          face (if old :error :success)
          words (get marks at)]
      (cond
        [(not (= line.kind (if old :removed :added))) [[number :comment] " " shown]]
        [(nil? words) [[$"{number} {shown}" face :pill]]]
        [else (vec (concat [[number :comment] " "]
                           (marked shown (visible words text text-width) face)))]))))

(def (split out file index hunk width columns)
  (let [half (// (- columns (string/length separator)) 2)
        text-width (max narrowest (- half (+ width 1)))
        marks (word-marks hunk)]
    (reduce (fn [out pair]
              (let [left (nth pair 0)
                    right (nth pair 1)]
                (add out {kind: :pair section: file.section path: file.path hunk: index left: left right: right}
                     (vec (concat (side hunk left true marks width text-width)
                                  [[separator :comment]]
                                  (side hunk right false marks width text-width))))))
            out (pairs hunk))))

(def (header file)
  (let [stats (if (and file.counted (not (= file.counted [0 0]))) (status/stats file.counted) [])]
    (vec (concat [[(get labels file.section) :link :pill] "  " [file.path :heading]]
                 (if (empty? stats) [] (concat ["  "] stats))))))

(def (render file hunks layout columns)
  (let [width (digits hunks)
        out (add {lines: []} {kind: :file section: file.section path: file.path} (header file))]
    (reduce (fn [out index]
              (let [hunk (nth hunks index)
                    out (add (blank out) {kind: :hunk section: file.section path: file.path hunk: index}
                             [[(diff/header hunk) :keyword]])]
                (if (= layout :split)
                  (split out file index hunk width columns)
                  (unified out file index hunk width))))
            out (range (count hunks)))))

(def (overview files layout columns)
  (if (empty? files)
    {lines: [{line: [["Nothing staged" :comment]] kind: :blank}]}
    (reduce (fn [out i]
              (let [item (nth files i)
                    shown (render item.file item.hunks layout columns)
                    out (if (> i 0) (blank out) out)]
                {lines: (vec (concat out.lines shown.lines))}))
            {lines: []} (range (count files)))))

(def (nothing path)
  {lines: [{line: [[$"No changes to {path}" :comment]] kind: :blank}]})
