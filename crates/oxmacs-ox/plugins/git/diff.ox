(module
  (import (ox string))
  (export parse whole-file header patch words shown))

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
  (let [parts (string/split text "\n")
        lines (if (= (last parts) "")
                (map (fn [i] (nth parts i)) (range (- (count parts) 1)))
                parts)]
    (if (empty? lines)
      []
      [{old-start: 0 old-count: 0 new-start: 1 new-count: (count lines)
        lines: (map (fn [line] {kind: :added text: line}) lines)}])))

(def (header hunk)
  "@@ -{hunk.old-start},{hunk.old-count} +{hunk.new-start},{hunk.new-count} @@")

(def (shown text)
  (if (string/ends-with? text "\r") (string/slice text 0 (- (string/length text) 1)) text))

(def (patch path hunk selected reverse)
  (let [lines hunk.lines
        n (count lines)
        chosen (fn [i] (or (nil? selected) (some (fn [picked] (= picked i)) selected)))
        body ((fn walk [i body old new emitted]
                (if (>= i n)
                  [body old new]
                  (let [line (nth lines i)
                        kind line.kind
                        picked (chosen i)]
                    (cond
                      [(= kind :note)
                       (walk (+ i 1) (if emitted (conj body line.text) body) old new emitted)]
                      [(= kind :context)
                       (walk (+ i 1) (conj body " {line.text}") (+ old 1) (+ new 1) true)]
                      [(and (= kind :added) picked)
                       (walk (+ i 1) (conj body "+{line.text}") old (+ new 1) true)]
                      [(and (= kind :removed) picked)
                       (walk (+ i 1) (conj body "-{line.text}") (+ old 1) new true)]
                      [(or (and (= kind :added) reverse) (and (= kind :removed) (not reverse)))
                       (walk (+ i 1) (conj body " {line.text}") (+ old 1) (+ new 1) true)]
                      [else (walk (+ i 1) body old new false)]))))
              0 [] 0 0 false)]
    (str "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n"
         "@@ -{hunk.old-start},{(nth body 1)} +{hunk.new-start},{(nth body 2)} @@\n"
         (string/join "\n" (nth body 0)) "\n")))

(def word-chars "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_")

(def (class-of character)
  (let [text (str character)]
    (cond
      [(string/blank? text) :space]
      [(string/includes? word-chars text) :word]
      [else :other])))

(def (tokens text)
  (let [chars (string/chars text)
        n (count chars)]
    ((fn walk [i start class out]
       (if (>= i n)
         (if (< start n) (conj out [start n]) out)
         (let [found (class-of (nth chars i))]
           (if (and (> i start) (or (not (= found class)) (= found :other)))
             (walk (+ i 1) i found (conj out [start i]))
             (walk (+ i 1) start (if (= i start) found class) out)))))
     0 0 nil [])))

(def (lcs-row item others below)
  ((fn cols [j right acc]
     (if (< j 0)
       (vec acc)
       (let [value (if (= item (nth others j))
                     (+ 1 (nth below (+ j 1)))
                     (max (nth below j) right))]
         (cols (- j 1) value (cons value acc)))))
   (- (count others) 1) 0 (list 0)))

(def (lcs-table items others)
  (let [zeros (vec (map (fn [i] 0) (range (+ (count others) 1))))]
    ((fn rows [i acc]
       (if (< i 0)
         (vec acc)
         (rows (- i 1) (cons (lcs-row (nth items i) others (first acc)) acc))))
     (- (count items) 1) (list zeros))))

(def (matches items others)
  (let [table (lcs-table items others)
        n (count items)
        m (count others)]
    ((fn walk [i j left right]
       (cond
         [(or (>= i n) (>= j m)) [left right]]
         [(= (nth items i) (nth others j)) (walk (+ i 1) (+ j 1) (assoc left i true) (assoc right j true))]
         [(>= (nth (nth table (+ i 1)) j) (nth (nth table i) (+ j 1))) (walk (+ i 1) j left right)]
         [else (walk i (+ j 1) left right)]))
     0 0 {} {})))

(def (changed spans texts matched)
  ((fn walk [i current out]
     (if (>= i (count spans))
       (if current (conj out current) out)
       (let [span (nth spans i)]
         (cond
           [(get matched i) (walk (+ i 1) nil (if current (conj out current) out))]
           [(string/blank? (nth texts i)) (walk (+ i 1) current out)]
           [current (walk (+ i 1) [(nth current 0) (nth span 1)] out)]
           [else (walk (+ i 1) span out)]))))
   0 nil []))

(def word-limit 10000)

(def (words old new)
  (let [old-spans (tokens old)
        new-spans (tokens new)
        old-texts (map (fn [span] (string/slice old (nth span 0) (nth span 1))) old-spans)
        new-texts (map (fn [span] (string/slice new (nth span 0) (nth span 1))) new-spans)]
    (when (<= (* (count old-texts) (count new-texts)) word-limit)
      (let [found (matches old-texts new-texts)
            left (nth found 0)]
        (when (some (fn [i] (not (string/blank? (nth old-texts i)))) (keys left))
          [(changed old-spans old-texts left) (changed new-spans new-texts (nth found 1))])))))
