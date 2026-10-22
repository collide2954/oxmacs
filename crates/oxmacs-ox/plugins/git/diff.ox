(module
  (import (ox string))
  (export parse whole-file header))

(def (numbers text)
  (let [parts (string/split text ",")]
    [(string/to-number (nth parts 0))
     (if (> (count parts) 1) (string/to-number (nth parts 1)) 1)]))

(def (hunk-start line)
  (let [parts (string/split line " ")
        old (numbers (string/slice (nth parts 1) 1))
        new (numbers (string/slice (nth parts 2) 1))]
    {old-start: (nth old 0) old-count: (nth old 1)
     new-start: (nth new 0) new-count: (nth new 1)
     lines: []}))

(def (add-line hunk line)
  (let [mark (if (= line "") "" (string/slice line 0 1))
        kind (cond
               [(= mark "+") :added]
               [(= mark "-") :removed]
               [(= mark " ") :context]
               [(= mark "\\") :note]
               [else nil])]
    (if kind
      (update hunk :lines conj {kind: kind text: (if (= kind :note) line (string/slice line 1))})
      hunk)))

(def (parse text)
  (let [lines (string/split text "\n")
        n (count lines)]
    ((fn walk [i done current]
       (if (>= i n)
         (if current (conj done current) done)
         (let [line (nth lines i)]
           (cond
             [(string/starts-with? line "@@ ")
              (walk (+ i 1) (if current (conj done current) done) (hunk-start line))]
             [(nil? current) (walk (+ i 1) done current)]
             [else (walk (+ i 1) done (add-line current line))]))))
     0 [] nil)))

(def (whole-file text)
  (let [lines (string/lines text)]
    (if (empty? lines)
      []
      [{old-start: 0 old-count: 0 new-start: 1 new-count: (count lines)
        lines: (map (fn [line] {kind: :added text: line}) lines)}])))

(def (header hunk)
  "@@ -{hunk.old-start},{hunk.old-count} +{hunk.new-start},{hunk.new-count} @@")
